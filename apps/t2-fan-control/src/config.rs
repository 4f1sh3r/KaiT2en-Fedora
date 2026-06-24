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
pub struct AppConfig {
    pub active_preset: PresetKind,
    pub automatic_control_enabled: bool,
    pub autostart_enabled: bool,
    pub custom_curve: Vec<CurvePoint>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_preset: PresetKind::Balanced,
            automatic_control_enabled: true,
            autostart_enabled: false,
            custom_curve: default_curve_for(PresetKind::Custom),
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

        format!(
            "active_preset={}\nautomatic_control_enabled={}\nautostart_enabled={}\ncustom_curve={}\n",
            self.active_preset.as_str(),
            self.automatic_control_enabled,
            self.autostart_enabled,
            curve
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
            _ => {}
        }
    }

    Some(config)
}

pub fn normalize_curve(curve: &mut Vec<CurvePoint>) {
    curve.sort_by_key(|point| point.temp_c);
    for point in curve {
        point.temp_c = point.temp_c.clamp(20, 100);
        point.speed_percent = point.speed_percent.clamp(0, 100);
    }
}
