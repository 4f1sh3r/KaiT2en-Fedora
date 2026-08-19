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

#[derive(Clone, Debug)]
pub struct PlatformBiasConfig {
    pub enabled: bool,
    pub skin_sensors: Vec<String>,
    pub skin_threshold_c: f32,
    pub skin_factor: f32,
    pub skin_max_bias_c: f32,
    pub battery_sensors: Vec<String>,
    pub battery_threshold_c: f32,
    pub battery_factor: f32,
    pub battery_max_bias_c: f32,
    pub max_effective_temp_c: f32,
    /// Read and logged only, never part of the bias.
    pub log_sensors: Vec<String>,
}

impl Default for PlatformBiasConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            skin_sensors: labels(&["Ts0S", "Ts1S"]),
            skin_threshold_c: 40.5,
            skin_factor: 6.0,
            skin_max_bias_c: 25.0,
            battery_sensors: labels(&["TB0T", "TB1T", "TB2T"]),
            battery_threshold_c: 38.0,
            battery_factor: 2.0,
            battery_max_bias_c: 6.0,
            max_effective_temp_c: 100.0,
            log_sensors: labels(&["TaLC", "TaRC", "TH0X", "TTRD", "TTTD", "TTWD", "TTXD"]),
        }
    }
}

impl PlatformBiasConfig {
    /// Every SMC label the daemon has to sample, bias inputs plus log-only ones.
    pub fn sensors_of_interest(&self) -> Vec<String> {
        let mut all = Vec::new();
        for label in self
            .skin_sensors
            .iter()
            .chain(&self.battery_sensors)
            .chain(&self.log_sensors)
        {
            if !all.contains(label) {
                all.push(label.clone());
            }
        }
        all
    }
}

fn labels(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub active_preset: PresetKind,
    pub automatic_control_enabled: bool,
    pub autostart_enabled: bool,
    pub custom_curve: Vec<CurvePoint>,
    pub platform_bias: PlatformBiasConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_preset: PresetKind::Balanced,
            automatic_control_enabled: true,
            autostart_enabled: false,
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
            "active_preset={}\nautomatic_control_enabled={}\nautostart_enabled={}\ncustom_curve={}\n\
             platform_bias_enabled={}\nskin_sensors={}\nskin_threshold_c={}\nskin_factor={}\n\
             skin_max_bias_c={}\nbattery_sensors={}\nbattery_threshold_c={}\nbattery_factor={}\n\
             battery_max_bias_c={}\nmax_effective_temp_c={}\nlog_sensors={}\n",
            self.active_preset.as_str(),
            self.automatic_control_enabled,
            self.autostart_enabled,
            curve,
            bias.enabled,
            bias.skin_sensors.join(","),
            bias.skin_threshold_c,
            bias.skin_factor,
            bias.skin_max_bias_c,
            bias.battery_sensors.join(","),
            bias.battery_threshold_c,
            bias.battery_factor,
            bias.battery_max_bias_c,
            bias.max_effective_temp_c,
            bias.log_sensors.join(",")
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
            "log_sensors" => config.platform_bias.log_sensors = parse_labels(value),
            "skin_threshold_c" => assign_f32(&mut config.platform_bias.skin_threshold_c, value),
            "skin_factor" => assign_f32(&mut config.platform_bias.skin_factor, value),
            "skin_max_bias_c" => assign_f32(&mut config.platform_bias.skin_max_bias_c, value),
            "battery_threshold_c" => {
                assign_f32(&mut config.platform_bias.battery_threshold_c, value)
            }
            "battery_factor" => assign_f32(&mut config.platform_bias.battery_factor, value),
            "battery_max_bias_c" => assign_f32(&mut config.platform_bias.battery_max_bias_c, value),
            "max_effective_temp_c" => {
                assign_f32(&mut config.platform_bias.max_effective_temp_c, value)
            }
            _ => {}
        }
    }

    Some(config)
}

fn parse_labels(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
        .collect()
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
