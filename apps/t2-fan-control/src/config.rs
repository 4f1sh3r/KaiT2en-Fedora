use std::{env, fs, path::{Path, PathBuf}};

pub const MIN_SOAK_TEMP_C: u8 = 30;
pub const MAX_SOAK_TEMP_C: u8 = 100;
pub const CURVE_POINT_COUNT: usize = 4;
const CONFIG_VERSION: u8 = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct CurvePoint { pub temp_c: u8, pub speed_percent: u8 }

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub automatic_control_enabled: bool,
    pub autostart_enabled: bool,
    pub soak_temp_c: u8,
    pub system_cooling_time_s: u16,
    pub any_sensor_enabled: bool,
    pub any_sensor_temp_c: u8,
    pub curve_sensor_key: Option<String>,
    pub custom_curve: Vec<CurvePoint>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            automatic_control_enabled: true,
            autostart_enabled: false,
            soak_temp_c: 45,
            system_cooling_time_s: 5,
            any_sensor_enabled: false,
            any_sensor_temp_c: 100,
            curve_sensor_key: None,
            custom_curve: vec![
                CurvePoint { temp_c: 0, speed_percent: 10 },
                CurvePoint { temp_c: 35, speed_percent: 15 },
                CurvePoint { temp_c: 70, speed_percent: 25 },
                CurvePoint { temp_c: 95, speed_percent: 65 },
            ],
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        for path in candidate_config_paths() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Some(config) = parse_config(&raw) { return config; }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = primary_config_path();
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        fs::write(path, self.to_disk_format())
    }

    fn to_disk_format(&self) -> String {
        let curve = self.custom_curve.iter()
            .map(|point| format!("{}:{}", point.temp_c, point.speed_percent))
            .collect::<Vec<_>>().join(",");
        format!("config_version={}\nautomatic_control_enabled={}\nautostart_enabled={}\nsystem_temp_target_c={}\nsystem_cooling_time_s={}\nany_sensor_enabled={}\nany_sensor_temp_c={}\ncurve_sensor_key={}\ncustom_curve={}\n",
            CONFIG_VERSION, self.automatic_control_enabled, self.autostart_enabled, self.soak_temp_c,
            self.system_cooling_time_s, self.any_sensor_enabled, self.any_sensor_temp_c,
            self.curve_sensor_key.as_deref().unwrap_or(""), curve)
    }
}

pub fn curve(config: &AppConfig) -> Vec<CurvePoint> { config.custom_curve.clone() }

pub fn normalize_curve(curve: &mut Vec<CurvePoint>) {
    if curve.len() != CURVE_POINT_COUNT { *curve = AppConfig::default().custom_curve; }
    curve.sort_by_key(|point| point.temp_c);
    curve[0].temp_c = curve[0].temp_c.min(MAX_SOAK_TEMP_C - 3);
    curve[1].temp_c = curve[1].temp_c.clamp(curve[0].temp_c + 1, MAX_SOAK_TEMP_C - 2);
    curve[2].temp_c = curve[2].temp_c.clamp(curve[1].temp_c + 1, MAX_SOAK_TEMP_C - 1);
    curve[3].temp_c = curve[3].temp_c.clamp(curve[2].temp_c + 1, MAX_SOAK_TEMP_C);
    for point in curve.iter_mut() { point.speed_percent = point.speed_percent.min(100); }
}

pub fn resize_soak(config: &mut AppConfig, new_soak_temp_c: u8) {
    config.soak_temp_c = new_soak_temp_c.clamp(MIN_SOAK_TEMP_C, MAX_SOAK_TEMP_C);
}

fn primary_config_path() -> PathBuf {
    env::var_os("T2_FANCONTROL_CONFIG").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/t2-fancontrol/config.txt"))
}

fn candidate_config_paths() -> Vec<PathBuf> {
    let mut paths = vec![primary_config_path()];
    let legacy_base = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".config")));
    if let Some(base) = legacy_base {
        let legacy = base.join("t2-fancontrol/config.txt");
        if !paths.contains(&legacy) { paths.push(legacy); }
    }
    paths
}

fn parse_config(raw: &str) -> Option<AppConfig> {
    let mut config = AppConfig::default();
    let mut version = None;
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((key, value)) = line.split_once('=') else { continue; };
        match key.trim() {
            "config_version" => version = value.trim().parse::<u8>().ok(),
            "automatic_control_enabled" => config.automatic_control_enabled = value.trim().parse().ok()?,
            "autostart_enabled" => config.autostart_enabled = value.trim().parse().ok()?,
            "system_temp_target_c" | "system_temp_limit_c" | "soak_temp_c" => config.soak_temp_c = value.trim().parse::<u8>().ok()?.clamp(MIN_SOAK_TEMP_C, MAX_SOAK_TEMP_C),
            "system_cooling_time_s" => config.system_cooling_time_s = value.trim().parse::<u16>().ok()?.min(600),
            "any_sensor_enabled" => config.any_sensor_enabled = value.trim().parse().ok()?,
            "any_sensor_temp_c" => config.any_sensor_temp_c = value.trim().parse::<u8>().ok()?.min(100),
            "curve_sensor_key" => config.curve_sensor_key = (!value.trim().is_empty()).then(|| value.trim().to_owned()),
            "custom_curve" => {
                let parsed = value.split(',').filter_map(|entry| {
                    let (temp, speed) = entry.split_once(':')?;
                    Some(CurvePoint { temp_c: temp.trim().parse().ok()?, speed_percent: speed.trim().parse().ok()? })
                }).collect::<Vec<_>>();
                if parsed.len() == CURVE_POINT_COUNT { config.custom_curve = parsed; }
            }
            _ => {}
        }
    }
    if version != Some(CONFIG_VERSION) {
        return Some(AppConfig::default());
    }
    normalize_curve(&mut config.custom_curve);
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_system_limit_does_not_change_curve() {
        let mut config = AppConfig::default();
        let curve = config.custom_curve.clone();
        resize_soak(&mut config, 100);
        assert_eq!(config.custom_curve, curve);
    }

    #[test]
    fn soak_cannot_move_below_thirty_degrees() {
        let mut config = AppConfig::default();
        resize_soak(&mut config, 5);
        assert_eq!(config.soak_temp_c, MIN_SOAK_TEMP_C);
        assert_eq!(config.custom_curve, AppConfig::default().custom_curve);
    }

    #[test]
    fn legacy_configuration_is_replaced_by_new_defaults() {
        let config = parse_config("system_temp_limit_c=100\ncustom_curve=0:0,30:20,70:80,100:100\n").unwrap();
        assert_eq!(config.soak_temp_c, 45);
        assert_eq!(config.custom_curve, AppConfig::default().custom_curve);
    }

    #[test]
    fn current_configuration_is_preserved() {
        let config = parse_config("config_version=2\nsystem_temp_limit_c=60\ncustom_curve=0:0,20:4,45:12,60:22\n").unwrap();
        assert_eq!(config.soak_temp_c, 60);
        assert_eq!(config.custom_curve[2].temp_c, 45);
    }

    #[test]
    fn first_curve_point_is_not_fixed() {
        let mut curve = vec![
            CurvePoint { temp_c: 5, speed_percent: 10 },
            CurvePoint { temp_c: 20, speed_percent: 20 },
            CurvePoint { temp_c: 40, speed_percent: 30 },
            CurvePoint { temp_c: 60, speed_percent: 40 },
        ];
        normalize_curve(&mut curve);
        assert_eq!(curve[0], CurvePoint { temp_c: 5, speed_percent: 10 });
    }

    #[test]
    fn curve_and_system_target_are_limited_to_one_hundred_degrees() {
        let config = parse_config("config_version=2\nsystem_temp_target_c=110\ncustom_curve=0:0,40:10,90:30,110:50\n").unwrap();
        assert_eq!(config.soak_temp_c, 100);
        assert_eq!(config.custom_curve[3].temp_c, 100);
    }
}
