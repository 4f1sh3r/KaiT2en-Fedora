use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct CurvePoint {
    pub temp_c: u8,
    pub speed_percent: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetKind {
    Quiet,
    Balanced,
    Performance,
    Custom,
}

impl Default for PresetKind {
    fn default() -> Self {
        Self::Balanced
    }
}

impl PresetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quiet => "Quiet",
            Self::Balanced => "Balanced",
            Self::Performance => "Performance",
            Self::Custom => "Custom",
        }
    }

    pub fn ui_label(self) -> &'static str {
        match self {
            Self::Quiet => "Quiet",
            Self::Balanced => "Balanced",
            Self::Performance => "Power",
            Self::Custom => "Custom",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "Quiet" | "Silent" => Self::Quiet,
            "Balanced" | "All-round" => Self::Balanced,
            "Performance" | "High performance" => Self::Performance,
            "Custom" => Self::Custom,
            _ => return None,
        })
    }
}

/// Heat-soak bias inputs. Each group is either an explicit label list or, when the
/// list is empty, auto-discovered from the sensor roles of whatever the SMC exposes.
#[derive(Clone, Debug)]
pub struct PlatformBiasConfig {
    pub enabled: bool,
    /// Mean of all non-CPU/GPU sensors; the one group that exists on every machine.
    pub system_threshold_c: f32,
    pub system_factor: f32,
    pub system_max_bias_c: f32,
    /// Low-pass time constants for the system average, in seconds. Rising faster than
    /// falling keeps the fan from chasing the cooling it just caused itself.
    pub system_rise_time_s: f32,
    pub system_fall_time_s: f32,
    pub skin_sensors: Vec<String>,
    pub skin_threshold_c: f32,
    pub skin_factor: f32,
    pub skin_max_bias_c: f32,
    pub battery_sensors: Vec<String>,
    pub battery_threshold_c: f32,
    pub battery_factor: f32,
    pub battery_max_bias_c: f32,
    /// Last-resort latch: skin at or above this pins both fans to 100 %, whatever the
    /// die temperature says. Exists because a power clamp cools the die but not the chassis.
    pub emergency_enabled: bool,
    pub emergency_skin_c: f32,
    pub emergency_release_c: f32,
    pub emergency_hold_s: f32,
}

impl Default for PlatformBiasConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            system_threshold_c: 40.0,
            system_factor: 4.0,
            system_max_bias_c: 25.0,
            system_rise_time_s: 30.0,
            system_fall_time_s: 180.0,
            skin_sensors: Vec::new(),
            skin_threshold_c: 42.0,
            skin_factor: 4.0,
            skin_max_bias_c: 20.0,
            battery_sensors: Vec::new(),
            battery_threshold_c: 38.0,
            battery_factor: 2.0,
            battery_max_bias_c: 6.0,
            emergency_enabled: true,
            emergency_skin_c: 44.5,
            emergency_release_c: 43.5,
            emergency_hold_s: 300.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub active_preset: PresetKind,
    pub automatic_control_enabled: bool,
    pub autostart_enabled: bool,
    /// How fast the commanded fan speed may change, in percentage points per second.
    /// Falling slower than rising keeps the fan note steady instead of pumping.
    pub fan_rise_rate_pct_per_s: f32,
    pub fan_fall_rate_pct_per_s: f32,
    /// A large gap is closed within roughly this many seconds instead of at the base
    /// rate, so a load step is answered quickly without the fan ever jumping.
    pub fan_ramp_time_s: f32,
    /// Low-pass time constants for the CPU/GPU temperature that drives the curve.
    /// Rising within seconds and falling over a minute keeps the fan ahead of a load
    /// step without letting a short spike keep it up.
    pub base_rise_time_s: f32,
    pub base_fall_time_s: f32,
    pub custom_curve: Vec<CurvePoint>,
    pub platform_bias: PlatformBiasConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_preset: PresetKind::Balanced,
            automatic_control_enabled: true,
            autostart_enabled: false,
            fan_rise_rate_pct_per_s: 1.0,
            fan_fall_rate_pct_per_s: 0.35,
            fan_ramp_time_s: 1.5,
            base_rise_time_s: 3.0,
            base_fall_time_s: 45.0,
            custom_curve: default_curve_for(PresetKind::Custom),
            platform_bias: PlatformBiasConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        for path in candidate_config_paths() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Some(config) = parse_config(&raw) {
                    return config;
                }
            }
        }

        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = primary_config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_disk_format())
    }

    fn to_disk_format(&self) -> String {
        let curve = self
            .custom_curve
            .iter()
            .map(|point| format!("{}:{}", point.temp_c, point.speed_percent))
            .collect::<Vec<_>>()
            .join(",");

        let bias = &self.platform_bias;
        format!(
            "active_preset={}\nautomatic_control_enabled={}\nautostart_enabled={}\n\
             fan_rise_rate_pct_per_s={}\nfan_fall_rate_pct_per_s={}\n\
             fan_ramp_time_s={}\nbase_rise_time_s={}\nbase_fall_time_s={}\n\
             custom_curve={}\n\
             platform_bias_enabled={}\nsystem_threshold_c={}\nsystem_factor={}\n\
             system_max_bias_c={}\nsystem_rise_time_s={}\nsystem_fall_time_s={}\nskin_sensors={}\nskin_threshold_c={}\nskin_factor={}\n\
             skin_max_bias_c={}\nbattery_sensors={}\nbattery_threshold_c={}\nbattery_factor={}\n\
             battery_max_bias_c={}\nemergency_enabled={}\nemergency_skin_c={}\n\
             emergency_release_c={}\nemergency_hold_s={}\n",
            self.active_preset.as_str(),
            self.automatic_control_enabled,
            self.autostart_enabled,
            self.fan_rise_rate_pct_per_s,
            self.fan_fall_rate_pct_per_s,
            self.fan_ramp_time_s,
            self.base_rise_time_s,
            self.base_fall_time_s,
            curve,
            bias.enabled,
            bias.system_threshold_c,
            bias.system_factor,
            bias.system_max_bias_c,
            bias.system_rise_time_s,
            bias.system_fall_time_s,
            format_labels(&bias.skin_sensors),
            bias.skin_threshold_c,
            bias.skin_factor,
            bias.skin_max_bias_c,
            format_labels(&bias.battery_sensors),
            bias.battery_threshold_c,
            bias.battery_factor,
            bias.battery_max_bias_c,
            bias.emergency_enabled,
            bias.emergency_skin_c,
            bias.emergency_release_c,
            bias.emergency_hold_s
        )
    }
}

pub fn curve_for_preset(config: &AppConfig) -> Vec<CurvePoint> {
    match config.active_preset {
        PresetKind::Custom => config.custom_curve.clone(),
        preset => default_curve_for(preset),
    }
}

pub fn default_curve_for(preset: PresetKind) -> Vec<CurvePoint> {
    match preset {
        PresetKind::Quiet => vec![
            CurvePoint { temp_c: 20, speed_percent: 1 },
            CurvePoint { temp_c: 70, speed_percent: 10 },
            CurvePoint { temp_c: 85, speed_percent: 55 },
            CurvePoint { temp_c: 100, speed_percent: 100 },
        ],
        PresetKind::Balanced => vec![
            CurvePoint { temp_c: 20, speed_percent: 15 },
            CurvePoint { temp_c: 60, speed_percent: 30 },
            CurvePoint { temp_c: 80, speed_percent: 50 },
            CurvePoint { temp_c: 100, speed_percent: 100 },
        ],
        PresetKind::Performance => vec![
            CurvePoint { temp_c: 20, speed_percent: 15 },
            CurvePoint { temp_c: 40, speed_percent: 35 },
            CurvePoint { temp_c: 75, speed_percent: 75 },
            CurvePoint { temp_c: 85, speed_percent: 100 },
        ],
        PresetKind::Custom => vec![
            CurvePoint { temp_c: 35, speed_percent: 25 },
            CurvePoint { temp_c: 50, speed_percent: 45 },
            CurvePoint { temp_c: 70, speed_percent: 72 },
            CurvePoint { temp_c: 85, speed_percent: 100 },
        ],
    }
}

fn primary_config_path() -> PathBuf {
    if let Some(path) = env::var_os("T2_FANCONTROL_CONFIG").map(PathBuf::from) {
        return path;
    }
    PathBuf::from("/etc/t2-fancontrol/config.txt")
}

fn candidate_config_paths() -> Vec<PathBuf> {
    let mut paths = vec![primary_config_path()];
    let legacy_base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".config")));

    if let Some(base) = legacy_base {
        let legacy_path = base.join("t2-fancontrol").join("config.txt");
        if !paths.contains(&legacy_path) {
            paths.push(legacy_path);
        }
    }
    paths
}

fn parse_config(raw: &str) -> Option<AppConfig> {
    let mut config = AppConfig::default();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "active_preset" => {
                if let Some(preset) = PresetKind::from_str(value.trim()) {
                    config.active_preset = preset;
                }
            }
            "automatic_control_enabled" => {
                config.automatic_control_enabled = value.trim().parse().ok()?;
            }
            "autostart_enabled" => {
                config.autostart_enabled = value.trim().parse().ok()?;
            }
            "fan_rise_rate_pct_per_s" => assign_f32(&mut config.fan_rise_rate_pct_per_s, value),
            "fan_fall_rate_pct_per_s" => assign_f32(&mut config.fan_fall_rate_pct_per_s, value),
            "fan_ramp_time_s" => assign_f32(&mut config.fan_ramp_time_s, value),
            "base_rise_time_s" => assign_f32(&mut config.base_rise_time_s, value),
            "base_fall_time_s" => assign_f32(&mut config.base_fall_time_s, value),
            "custom_curve" => {
                let mut curve = Vec::new();
                for entry in value.split(',').filter(|entry| !entry.trim().is_empty()) {
                    let (temp, speed) = entry.split_once(':')?;
                    curve.push(CurvePoint {
                        temp_c: temp.trim().parse().ok()?,
                        speed_percent: speed.trim().parse().ok()?,
                    });
                }
                if !curve.is_empty() {
                    normalize_curve(&mut curve);
                    config.custom_curve = curve;
                }
            }
            "platform_bias_enabled" => {
                if let Ok(flag) = value.trim().parse() {
                    config.platform_bias.enabled = flag;
                }
            }
            "skin_sensors" => config.platform_bias.skin_sensors = parse_labels(value),
            "battery_sensors" => config.platform_bias.battery_sensors = parse_labels(value),
            "system_threshold_c" => assign_f32(&mut config.platform_bias.system_threshold_c, value),
            "system_factor" => assign_f32(&mut config.platform_bias.system_factor, value),
            "system_max_bias_c" => assign_f32(&mut config.platform_bias.system_max_bias_c, value),
            "system_rise_time_s" => assign_f32(&mut config.platform_bias.system_rise_time_s, value),
            "system_fall_time_s" => assign_f32(&mut config.platform_bias.system_fall_time_s, value),
            "skin_threshold_c" => assign_f32(&mut config.platform_bias.skin_threshold_c, value),
            "skin_factor" => assign_f32(&mut config.platform_bias.skin_factor, value),
            "skin_max_bias_c" => assign_f32(&mut config.platform_bias.skin_max_bias_c, value),
            "battery_threshold_c" => {
                assign_f32(&mut config.platform_bias.battery_threshold_c, value)
            }
            "battery_factor" => assign_f32(&mut config.platform_bias.battery_factor, value),
            "battery_max_bias_c" => assign_f32(&mut config.platform_bias.battery_max_bias_c, value),
            "emergency_enabled" => {
                if let Ok(flag) = value.trim().parse() {
                    config.platform_bias.emergency_enabled = flag;
                }
            }
            "emergency_skin_c" => assign_f32(&mut config.platform_bias.emergency_skin_c, value),
            "emergency_release_c" => {
                assign_f32(&mut config.platform_bias.emergency_release_c, value)
            }
            "emergency_hold_s" => assign_f32(&mut config.platform_bias.emergency_hold_s, value),
            _ => {}
        }
    }

    Some(config)
}

/// An empty list or the word `auto` means: pick the sensors by role at runtime.
fn parse_labels(value: &str) -> Vec<String> {
    if value.trim().eq_ignore_ascii_case("auto") {
        return Vec::new();
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
        .collect()
}

fn format_labels(labels: &[String]) -> String {
    if labels.is_empty() {
        String::from("auto")
    } else {
        labels.join(",")
    }
}

fn assign_f32(target: &mut f32, value: &str) {
    if let Ok(parsed) = value.trim().parse::<f32>() {
        if parsed.is_finite() {
            *target = parsed;
        }
    }
}

pub fn normalize_curve(curve: &mut Vec<CurvePoint>) {
    curve.sort_by_key(|point| point.temp_c);
    for point in curve {
        point.temp_c = point.temp_c.clamp(20, 100);
        point.speed_percent = point.speed_percent.clamp(0, 100);
    }
}
