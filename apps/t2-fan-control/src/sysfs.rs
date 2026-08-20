use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{FanControlError, Result};

#[derive(Clone, Debug)]
pub struct FanEndpoint {
    pub name: String,
    pub base_path: PathBuf,
    pub min_speed: u32,
    pub max_speed: u32,
    pub current_speed: Option<u32>,
    pub app_controlled: Option<bool>,
    /// true = t2smc/macsmc hwmon (_target), false = applesmc (_output + _manual)
    uses_target_api: bool,
}

#[derive(Clone, Debug)]
pub struct TemperatureSource {
    pub name: String,
    pub path: PathBuf,
    pub last_temp_c: Option<u8>,
    pub role: TemperatureRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemperatureRole { Cpu, Gpu, System }

#[derive(Clone, Debug, Default)]
pub struct TemperatureSnapshot {
    pub cpu_temp_c: Option<u8>,
    pub gpu_temp_c: Option<u8>,
    pub hottest_temp_c: Option<u8>,
    pub hottest_sensor_name: Option<String>,
    pub overall_hottest_temp_c: Option<u8>,
    pub overall_hottest_sensor_name: Option<String>,
    pub system_temp_c: Option<u8>,
    pub system_sensor_count: usize,
    pub monitored_sensor_count: usize,
}

impl TemperatureSnapshot {
    pub fn read_from(sources: &mut [TemperatureSource]) -> Self {
        let mut snapshot = Self::default();
        let mut system_sum = 0_u32;
        let mut system_count = 0_usize;
        for source in sources {
            source.last_temp_c = read_temperature(&source.path).ok();
            let Some(temp) = source.last_temp_c.filter(|temp| *temp > 0) else { continue; };
            snapshot.monitored_sensor_count += 1;
            if snapshot.overall_hottest_temp_c.map_or(true, |hottest| temp > hottest) {
                snapshot.overall_hottest_temp_c = Some(temp);
                snapshot.overall_hottest_sensor_name = Some(source.name.clone());
            }
            if source.role != TemperatureRole::System
                && snapshot.hottest_temp_c.map_or(true, |hottest| temp > hottest)
            {
                snapshot.hottest_temp_c = Some(temp);
                snapshot.hottest_sensor_name = Some(source.name.clone());
            }
            record_temperature(&mut snapshot, source.role, temp, &mut system_sum, &mut system_count);
        }
        snapshot.system_sensor_count = system_count;
        snapshot.system_temp_c = (system_count > 0)
            .then(|| ((system_sum + system_count as u32 / 2) / system_count as u32) as u8);
        snapshot
    }

    pub fn effective_temp_c(&self) -> Option<u8> {
        self.hottest_temp_c
    }
}

fn record_temperature(snapshot: &mut TemperatureSnapshot, role: TemperatureRole, temp: u8, system_sum: &mut u32, system_count: &mut usize) {
    if temp == 0 { return; }
    snapshot.overall_hottest_temp_c = Some(snapshot.overall_hottest_temp_c.map_or(temp, |old| old.max(temp)));
    if role != TemperatureRole::System {
        snapshot.hottest_temp_c = Some(snapshot.hottest_temp_c.map_or(temp, |old| old.max(temp)));
    }
    match role {
        TemperatureRole::Cpu => snapshot.cpu_temp_c = Some(temp),
        TemperatureRole::Gpu => snapshot.gpu_temp_c = Some(snapshot.gpu_temp_c.map_or(temp, |old| old.max(temp))),
        TemperatureRole::System => {}
    }
    *system_sum += temp as u32;
    *system_count += 1;
}

impl FanEndpoint {
    pub fn refresh_state(&mut self) -> Result<()> {
        self.current_speed = Some(read_u32(&join_suffix(&self.base_path, "_input"))?);
        self.app_controlled = if self.uses_target_api {
            read_u32(&join_suffix(&self.base_path, "_target"))
                .ok()
                .map(|target| target != 0)
        } else {
            read_u32(&join_suffix(&self.base_path, "_manual"))
                .ok()
                .map(|manual| manual != 0)
        };
        Ok(())
    }

    pub fn set_target_speed(&self, requested_speed: u32) -> Result<()> {
        let clamped = requested_speed.clamp(self.min_speed, self.max_speed);
        if self.uses_target_api {
            write_string(
                &join_suffix(&self.base_path, "_target"),
                &clamped.to_string(),
            )
        } else {
            write_string(&join_suffix(&self.base_path, "_manual"), "1")?;
            write_string(
                &join_suffix(&self.base_path, "_output"),
                &clamped.to_string(),
            )
        }
    }

    pub fn release_to_auto(&self) -> Result<()> {
        if self.uses_target_api {
            write_string(&join_suffix(&self.base_path, "_target"), "0")
        } else {
            write_string(&join_suffix(&self.base_path, "_manual"), "0")
        }
    }

    pub fn percent_to_rpm(&self, percent: u8) -> u32 {
        let span = self.max_speed.saturating_sub(self.min_speed);
        self.min_speed + (span * percent as u32 / 100)
    }
}

/// Find the hwmon device under /sys/class/hwmon/ with the given name.
fn find_hwmon_by_name(name: &str) -> Option<PathBuf> {
    let pattern = "/sys/class/hwmon/hwmon*/name";
    for entry in glob::glob(pattern).ok()? {
        let Ok(path) = entry else {
            continue;
        };
        if fs::read_to_string(&path).ok().map_or(false, |n| n.trim() == name) {
            return path.parent().map(Path::to_path_buf);
        }
    }
    None
}

fn discover_fans_hwmon(name: &str) -> Option<Result<Vec<FanEndpoint>>> {
    let hwmon_dir = find_hwmon_by_name(name)?;
    let pattern = format!("{}/fan*_input", hwmon_dir.display());
    let mut fans = Vec::new();

    let entries = match glob::glob(&pattern) {
        Ok(e) => e,
        Err(_) => return None,
    };

    for entry in entries {
        let input_path = match entry {
            Ok(p) => p,
            Err(_) => continue,
        };
        let fan_path = match input_to_base_path(&input_path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let name = match fan_path
            .file_name()
            .and_then(|v| v.to_str())
        {
            Some(n) => n.to_owned(),
            None => continue,
        };

        let min_speed = read_u32(&join_suffix(&fan_path, "_min")).unwrap_or(0);
        let max_speed = match read_u32(&join_suffix(&fan_path, "_max")) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let current_speed = read_u32(&input_path).ok();
        let app_controlled = read_u32(&join_suffix(&fan_path, "_target"))
            .ok()
            .map(|target| target != 0);

        fans.push(FanEndpoint {
            name,
            base_path: fan_path,
            min_speed,
            max_speed,
            current_speed,
            app_controlled,
            uses_target_api: true,
        });
    }

    fans.sort_by(|left, right| left.name.cmp(&right.name));
    if fans.is_empty() {
        None
    } else {
        Some(Ok(fans))
    }
}

fn discover_fans_acpi() -> Option<Result<Vec<FanEndpoint>>> {
    let pattern = "/sys/devices/pci*/*/*/*/APP0001:00/fan*_input";
    let mut entries = match glob::glob(pattern) {
        Ok(e) => e,
        Err(_) => return None,
    };

    let first_fan = match entries.find_map(|e| e.ok()) {
        Some(p) => p,
        None => return None,
    };

    let fan_dir = first_fan.parent()?;
    let pattern = format!("{}/fan*_input", fan_dir.display());
    let mut fans = Vec::new();

    let entries = match glob::glob(&pattern) {
        Ok(e) => e,
        Err(_) => return None,
    };

    for entry in entries {
        let input_path = match entry {
            Ok(p) => p,
            Err(_) => continue,
        };
        let fan_path = match input_to_base_path(&input_path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let name = match fan_path
            .file_name()
            .and_then(|v| v.to_str())
        {
            Some(n) => n.to_owned(),
            None => continue,
        };

        let min_speed = read_u32(&join_suffix(&fan_path, "_min")).unwrap_or(0);
        let max_speed = match read_u32(&join_suffix(&fan_path, "_max")) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let current_speed = read_u32(&input_path).ok();
        let app_controlled = read_u32(&join_suffix(&fan_path, "_manual"))
            .ok()
            .map(|manual| manual != 0);

        fans.push(FanEndpoint {
            name,
            base_path: fan_path,
            min_speed,
            max_speed,
            current_speed,
            app_controlled,
            uses_target_api: false,
        });
    }

    fans.sort_by(|left, right| left.name.cmp(&right.name));
    if fans.is_empty() {
        None
    } else {
        Some(Ok(fans))
    }
}

pub fn discover_fans() -> Result<Vec<FanEndpoint>> {
    discover_fans_hwmon("t2smc")
        .or_else(|| discover_fans_hwmon("macsmc"))
        .or_else(discover_fans_acpi)
        .unwrap_or(Err(FanControlError::NoFans))
}

pub fn discover_temperature_sources() -> Vec<TemperatureSource> {
    let mut sources = Vec::new();

    // CPU from coretemp (Intel built-in)
    if let Some(path) = first_existing_path(
        "/sys/devices/platform/coretemp.0/hwmon/hwmon*/temp1_input",
    ) {
        sources.push(TemperatureSource {
            name: String::from("CPU package"),
            path,
            last_temp_c: None,
            role: TemperatureRole::Cpu,
        });
    }

    // GPU from SMC temperature sensor (t2smc, macsmc, or applesmc hwmon)
    let smc_hwmon = find_hwmon_by_name("t2smc")
        .or_else(|| find_hwmon_by_name("macsmc"))
        .or_else(|| find_hwmon_by_name("applesmc"));
    if let Some(hwmon_dir) = smc_hwmon {
        let pattern = format!("{}/temp*_label", hwmon_dir.display());
        if let Ok(entries) = glob::glob(&pattern) {
            for entry in entries.flatten() {
                let Ok(label) = fs::read_to_string(&entry) else {
                    continue;
                };
                let label = label.trim();
                let temp_path = entry.with_file_name(entry.file_name().unwrap_or_default().to_string_lossy().replace("_label", "_input"));
                if temp_path.exists() && read_temperature(&temp_path).is_ok() {
                    sources.push(TemperatureSource {
                        name: sensor_label(label),
                        path: temp_path,
                        last_temp_c: None,
                        role: if matches!(label, "TG0P" | "TGDD" | "TGDF" | "TGVP") {
                            TemperatureRole::Gpu
                        } else { TemperatureRole::System },
                    });
                }
            }
        }
    }

    sources
}

fn sensor_label(key: &str) -> String {
    let bytes = key.as_bytes();
    if bytes.len() == 4 && bytes[0] == b'T' && bytes[1] == b'C' && bytes[3] == b'C' {
        if let Some(index) = (bytes[2] as char).to_digit(16).filter(|index| *index > 0) {
            return format!("CPU Core {}", index - 1);
        }
    }
    match key {
        "TA0V" => String::from("Ambient"),
        "TB0T" => String::from("Battery 1"),
        "TB1T" => String::from("Battery 2"),
        "TB2T" => String::from("Battery 3"),
        "TC0P" => String::from("CPU Proximity"),
        "TC0E" => String::from("CPU Sensor 0E"),
        "TC0F" => String::from("CPU Sensor 0F"),
        "TCGC" => String::from("CPU Graphics Core"),
        "TCMX" => String::from("CPU Memory"),
        "TCSA" => String::from("CPU System Agent"),
        "TCXC" => String::from("CPU Package"),
        "TG0P" => String::from("GPU Proximity"),
        "TGDD" => String::from("GPU Die digital"),
        "TGDF" => String::from("GPU Die analog"),
        "TGVP" => String::from("GPU Voltage Regulator"),
        "TH0F" => String::from("SSD Heatsink"),
        "TH0X" => String::from("SSD Controller"),
        "TH0a" => String::from("SSD NAND"),
        "TH0b" => String::from("SSD NAND 2"),
        "TH1a" => String::from("Drive 1 Raw A"),
        "TH1b" => String::from("Drive 1 Raw B"),
        "TM0P" => String::from("Memory Bank A1"),
        "TM1P" => String::from("Memory Bank A2"),
        "TM2P" => String::from("Memory Bank A3"),
        "TM3P" => String::from("Memory Bank A4"),
        "TM8P" => String::from("Memory Bank B1"),
        "TM9P" => String::from("Memory Bank B2"),
        "Tm0P" => String::from("Mainboard Proximity"),
        "Tm1P" => String::from("Mainboard Bottom"),
        "Th0H" => String::from("CPU Heatpipe"),
        "Th1H" => String::from("Right Fin Stack"),
        "Th2H" => String::from("Left Fin Stack"),
        "TPCD" => String::from("PCH Die"),
        "TTLD" => String::from("Thunderbolt Left"),
        "TTRD" => String::from("Thunderbolt Right"),
        "TW0P" => String::from("WiFi"),
        "TaLC" => String::from("Audio Left"),
        "TaRC" => String::from("Audio Right"),
        "Ts0P" => String::from("Palmrest Left"),
        "Ts0S" => String::from("Palmrest Left skin"),
        "Ts1P" => String::from("Palmrest Right"),
        "Ts1S" => String::from("Palmrest Right skin"),
        "Ts2S" => String::from("Touchpad"),
        _ => format!("Unknown sensor ({key})"),
    }
}

fn first_existing_path(pattern: &str) -> Option<PathBuf> {
    let paths = glob::glob(pattern).ok()?;
    for entry in paths {
        let Ok(path) = entry else {
            continue;
        };
        if path.exists() && read_temperature(&path).is_ok() {
            return Some(path);
        }
    }
    None
}

fn input_to_base_path(input_path: &Path) -> Result<PathBuf> {
    let file_name = input_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| FanControlError::InvalidFanPath(input_path.to_path_buf()))?;
    let fan_name = file_name
        .strip_suffix("_input")
        .ok_or_else(|| FanControlError::InvalidFanPath(input_path.to_path_buf()))?;

    Ok(input_path.with_file_name(fan_name))
}

fn join_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("fan"));
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn read_u32(path: &Path) -> Result<u32> {
    let contents = fs::read_to_string(path).map_err(|source| FanControlError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    contents
        .trim()
        .parse::<u32>()
        .map_err(|source| FanControlError::ParseInt {
            path: path.to_path_buf(),
            source,
        })
}

fn read_temperature(path: &Path) -> Result<u8> {
    let contents = fs::read_to_string(path).map_err(|source| FanControlError::Io { path: path.to_path_buf(), source })?;
    let raw = contents.trim().parse::<i32>().map_err(|source| FanControlError::ParseInt { path: path.to_path_buf(), source })?;
    if raw <= 0 { return Ok(0); }
    Ok((raw / 1000).clamp(0, u8::MAX as i32) as u8)
}

fn write_string(path: &Path, value: &str) -> Result<()> {
    fs::write(path, value).map_err(|source| FanControlError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hottest_and_positive_smc_average_are_independent() {
        let mut snapshot = TemperatureSnapshot::default();
        let mut sum = 0;
        let mut count = 0;
        record_temperature(&mut snapshot, TemperatureRole::Cpu, 80, &mut sum, &mut count);
        record_temperature(&mut snapshot, TemperatureRole::System, 0, &mut sum, &mut count);
        record_temperature(&mut snapshot, TemperatureRole::System, 40, &mut sum, &mut count);
        record_temperature(&mut snapshot, TemperatureRole::Gpu, 100, &mut sum, &mut count);
        record_temperature(&mut snapshot, TemperatureRole::System, 110, &mut sum, &mut count);

        assert_eq!(snapshot.hottest_temp_c, Some(100));
        assert_eq!(snapshot.overall_hottest_temp_c, Some(110));
        assert_eq!(snapshot.cpu_temp_c, Some(80));
        assert_eq!(snapshot.gpu_temp_c, Some(100));
        assert_eq!((sum, count), (330, 4));
    }
}
