use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use crate::{
    config::{curve_for_preset, AppConfig, CurvePoint, PlatformBiasConfig},
    error::Result,
    sysfs::{FanEndpoint, SensorRole, TemperatureSnapshot, TemperatureSource},
};

const CONTROL_INTERVAL: Duration = Duration::from_secs(2);
const SMOOTHING_SAMPLES: usize = 10;
const SENSOR_LOG_INTERVAL: Duration = Duration::from_secs(30);
/// Upper end of the curve's temperature axis, so the bias can never point past it.
const MAX_EFFECTIVE_TEMP_C: f32 = 100.0;

pub struct Controller {
    samples: VecDeque<u8>,
    last_applied_percent: Option<u8>,
    last_tick: Instant,
    last_sensor_log: Option<Instant>,
}

#[derive(Clone, Debug, Default)]
pub struct ControlSnapshot {
    pub temperatures: TemperatureSnapshot,
    /// Smoothed max(CPU, GPU) before the platform bias is added.
    pub base_temp_c: Option<u8>,
    pub system_temp_c: Option<f32>,
    pub skin_temp_c: Option<f32>,
    pub battery_temp_c: Option<f32>,
    pub platform_bias_c: Option<f32>,
    pub effective_temp_c: Option<u8>,
    pub target_percent: Option<u8>,
    pub target_rpm_per_fan: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlatformBias {
    pub system_temp_c: Option<f32>,
    pub skin_temp_c: Option<f32>,
    pub battery_temp_c: Option<f32>,
    pub bias_c: f32,
}

/// Heat-soak bias from the system average plus the skin and battery groups. The
/// contributions are combined with max(), not summed, so the same heat soak is not
/// counted twice. Groups whose sensors the machine does not expose contribute nothing.
pub fn compute_platform_bias(
    config: &PlatformBiasConfig,
    temperatures: &TemperatureSnapshot,
) -> PlatformBias {
    let system_temp_c = temperatures.system_temp_c();
    let skin_temp_c = group_temp(temperatures, &config.skin_sensors, SensorRole::Skin);
    let battery_temp_c = group_temp(temperatures, &config.battery_sensors, SensorRole::Battery);

    if !config.enabled {
        return PlatformBias {
            system_temp_c,
            skin_temp_c,
            battery_temp_c,
            bias_c: 0.0,
        };
    }

    let system_bias = linear_bias(
        system_temp_c,
        config.system_threshold_c,
        config.system_factor,
        config.system_max_bias_c,
    );
    let skin_bias = linear_bias(
        skin_temp_c,
        config.skin_threshold_c,
        config.skin_factor,
        config.skin_max_bias_c,
    );
    let battery_bias = linear_bias(
        battery_temp_c,
        config.battery_threshold_c,
        config.battery_factor,
        config.battery_max_bias_c,
    );

    PlatformBias {
        system_temp_c,
        skin_temp_c,
        battery_temp_c,
        bias_c: system_bias.max(skin_bias).max(battery_bias),
    }
}

/// An empty label list means the group picks its sensors by role instead.
fn group_temp(
    temperatures: &TemperatureSnapshot,
    labels: &[String],
    role: SensorRole,
) -> Option<f32> {
    if labels.is_empty() {
        temperatures.max_by_role(role)
    } else {
        temperatures.max_sensor(labels)
    }
}

fn linear_bias(value: Option<f32>, threshold: f32, factor: f32, max_bias: f32) -> f32 {
    let Some(value) = value else {
        return 0.0;
    };
    if !value.is_finite() {
        return 0.0;
    }
    ((value - threshold) * factor).clamp(0.0, max_bias.max(0.0))
}

pub fn apply_platform_bias(base_temp_c: u8, bias_c: f32) -> u8 {
    (base_temp_c as f32 + bias_c).clamp(0.0, MAX_EFFECTIVE_TEMP_C).round() as u8
}

impl Controller {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(SMOOTHING_SAMPLES),
            last_applied_percent: None,
            last_tick: Instant::now() - CONTROL_INTERVAL,
            last_sensor_log: None,
        }
    }

    pub fn tick(
        &mut self,
        config: &AppConfig,
        fans: &mut [FanEndpoint],
        temperatures: &mut [TemperatureSource],
    ) -> Result<ControlSnapshot> {
        let snapshot = TemperatureSnapshot::read_from(temperatures);
        let base_temp = snapshot.base_temp_c();

        if let Some(temp) = base_temp {
            self.samples.push_back(temp);
            if self.samples.len() > SMOOTHING_SAMPLES {
                self.samples.pop_front();
            }
        }

        let smoothed_temp = self.smoothed_temp();
        let bias = compute_platform_bias(&config.platform_bias, &snapshot);
        let effective_temp = smoothed_temp.map(|temp| apply_platform_bias(temp, bias.bias_c));
        let curve = curve_for_preset(config);
        let target_percent = effective_temp.map(|temp| interpolate_percent(&curve, temp));

        let mut target_rpm_per_fan = Vec::with_capacity(fans.len());
        if config.automatic_control_enabled {
            let should_apply = should_apply_target(self.last_applied_percent, target_percent);

            for fan in fans {
                let rpm = target_percent
                    .map(|percent| fan.percent_to_rpm(percent))
                    .unwrap_or(fan.min_speed);

                if should_apply {
                    fan.set_target_speed(rpm)?;
                    fan.current_speed = Some(rpm);
                    fan.app_controlled = Some(true);
                }

                target_rpm_per_fan.push(rpm);
            }

            if should_apply {
                self.last_applied_percent = target_percent;
            }
        }

        self.last_tick = Instant::now();
        self.log_sensors(&snapshot, smoothed_temp, &bias, effective_temp);

        Ok(ControlSnapshot {
            temperatures: snapshot,
            base_temp_c: smoothed_temp,
            system_temp_c: bias.system_temp_c,
            skin_temp_c: bias.skin_temp_c,
            battery_temp_c: bias.battery_temp_c,
            platform_bias_c: Some(bias.bias_c),
            effective_temp_c: effective_temp,
            target_percent,
            target_rpm_per_fan,
        })
    }

    fn log_sensors(
        &mut self,
        temperatures: &TemperatureSnapshot,
        base_temp_c: Option<u8>,
        bias: &PlatformBias,
        effective_temp_c: Option<u8>,
    ) {
        if self
            .last_sensor_log
            .is_some_and(|last| last.elapsed() < SENSOR_LOG_INTERVAL)
        {
            return;
        }
        self.last_sensor_log = Some(Instant::now());

        let sensors = temperatures
            .sensors
            .iter()
            .map(|sensor| format!("{}={:.2}", sensor.label, sensor.temp_c))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "sensors: base={} system={} (n={}) skin={} battery={} bias={:.2} effective={} {}",
            format_opt_u8(base_temp_c),
            format_opt_f32(bias.system_temp_c),
            temperatures.system_sensor_count(),
            format_opt_f32(bias.skin_temp_c),
            format_opt_f32(bias.battery_temp_c),
            bias.bias_c,
            format_opt_u8(effective_temp_c),
            sensors
        );
    }

    pub fn release_to_system(&mut self, fans: &mut [FanEndpoint]) -> Result<()> {
        for fan in fans {
            fan.release_to_auto()?;
            fan.app_controlled = Some(false);
        }
        self.last_applied_percent = None;
        Ok(())
    }

    pub fn should_tick(&self) -> bool {
        self.last_tick.elapsed() >= CONTROL_INTERVAL
    }

    fn smoothed_temp(&self) -> Option<u8> {
        if self.samples.is_empty() {
            return None;
        }

        let sum: u16 = self.samples.iter().map(|value| *value as u16).sum();
        Some((sum / self.samples.len() as u16) as u8)
    }
}

fn format_opt_u8(value: Option<u8>) -> String {
    value.map_or_else(|| String::from("n/a"), |value| value.to_string())
}

fn format_opt_f32(value: Option<f32>) -> String {
    value.map_or_else(|| String::from("n/a"), |value| format!("{value:.2}"))
}

fn should_apply_target(last_applied_percent: Option<u8>, next_target_percent: Option<u8>) -> bool {
    match (last_applied_percent, next_target_percent) {
        (None, Some(_)) | (Some(_), None) => true,
        (None, None) => false,
        (Some(previous), Some(next)) => previous.abs_diff(next) >= 3,
    }
}

fn interpolate_percent(curve: &[CurvePoint], temp_c: u8) -> u8 {
    if curve.is_empty() {
        return 0;
    }
    if temp_c <= curve[0].temp_c {
        return curve[0].speed_percent;
    }
    for window in curve.windows(2) {
        let left = &window[0];
        let right = &window[1];
        if temp_c <= right.temp_c {
            let temp_span = (right.temp_c - left.temp_c) as f32;
            if temp_span <= f32::EPSILON {
                return right.speed_percent;
            }
            let progress = (temp_c - left.temp_c) as f32 / temp_span;
            let speed_span = right.speed_percent as f32 - left.speed_percent as f32;
            return (left.speed_percent as f32 + progress * speed_span).round() as u8;
        }
    }
    curve.last().map(|point| point.speed_percent).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::{classify_label, PlatformSensor};

    fn snapshot(sensors: &[(&str, f32)]) -> TemperatureSnapshot {
        TemperatureSnapshot {
            cpu_temp_c: Some(65),
            gpu_temp_c: Some(60),
            sensors: sensors
                .iter()
                .map(|(label, value)| PlatformSensor {
                    label: (*label).to_owned(),
                    temp_c: *value,
                    role: classify_label(label),
                })
                .collect(),
        }
    }

    fn no_system(config: PlatformBiasConfig) -> PlatformBiasConfig {
        PlatformBiasConfig {
            system_max_bias_c: 0.0,
            ..config
        }
    }

    #[test]
    fn base_temp_is_max_of_cpu_and_gpu() {
        assert_eq!(snapshot(&[]).base_temp_c(), Some(65));
        let gpu_only = TemperatureSnapshot {
            cpu_temp_c: None,
            gpu_temp_c: Some(70),
            ..TemperatureSnapshot::default()
        };
        assert_eq!(gpu_only.base_temp_c(), Some(70));
        assert_eq!(TemperatureSnapshot::default().base_temp_c(), None);
    }

    #[test]
    fn labels_are_classified_into_roles() {
        assert_eq!(classify_label("TCXC"), SensorRole::Cpu);
        assert_eq!(classify_label("TG0P"), SensorRole::Gpu);
        assert_eq!(classify_label("Ts1S"), SensorRole::Skin);
        assert_eq!(classify_label("TB2T"), SensorRole::Battery);
        assert_eq!(classify_label("TTXD"), SensorRole::System);
        assert_eq!(classify_label("Palmrest skin"), SensorRole::Skin);
        assert_eq!(classify_label("Unknown vendor sensor"), SensorRole::System);
    }

    #[test]
    fn system_temp_is_the_mean_without_cpu_and_gpu() {
        let temps = snapshot(&[("TCXC", 90.0), ("TG0P", 80.0), ("TTXD", 30.0), ("TW0P", 40.0)]);
        assert_eq!(temps.system_temp_c(), Some(35.0));
        assert_eq!(temps.system_sensor_count(), 2);
    }

    #[test]
    fn dead_sensors_are_kept_out_of_the_system_mean() {
        // TF0S reports 3.5 C on this machine and would drag the mean down
        let temps = snapshot(&[("TF0S", 3.5), ("TTXD", 30.0), ("TW0P", 40.0)]);
        assert_eq!(temps.system_temp_c(), Some(35.0));
        assert_eq!(temps.system_sensor_count(), 2);
    }

    #[test]
    fn system_temp_is_none_without_any_platform_sensor() {
        assert_eq!(snapshot(&[]).system_temp_c(), None);
        assert_eq!(compute_platform_bias(&PlatformBiasConfig::default(), &snapshot(&[])).bias_c, 0.0);
    }

    #[test]
    fn system_bias_follows_threshold_and_factor() {
        let config = PlatformBiasConfig::default();
        // mean 45 C -> (45 - 40) * 4 = 20
        let temps = snapshot(&[("TTXD", 44.0), ("TW0P", 46.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert_eq!(bias.system_temp_c, Some(45.0));
        assert!((bias.bias_c - 20.0).abs() < 1e-3, "got {}", bias.bias_c);
        assert_eq!(apply_platform_bias(60, bias.bias_c), 80);
    }

    #[test]
    fn system_bias_is_capped() {
        let config = PlatformBiasConfig::default();
        let temps = snapshot(&[("TTXD", 80.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert!((bias.bias_c - config.system_max_bias_c).abs() < 1e-3);
    }

    #[test]
    fn groups_auto_select_by_role_when_no_labels_are_configured() {
        let config = PlatformBiasConfig::default();
        assert!(config.skin_sensors.is_empty());
        let temps = snapshot(&[("Ts0S", 43.0), ("Ts1S", 45.2), ("TB0T", 36.0), ("TB2T", 39.5)]);
        let bias = compute_platform_bias(&config, &temps);
        assert_eq!(bias.skin_temp_c, Some(45.2));
        assert_eq!(bias.battery_temp_c, Some(39.5));
    }

    #[test]
    fn explicit_labels_override_the_auto_selection() {
        let config = PlatformBiasConfig {
            skin_sensors: vec![String::from("Ts0S")],
            ..PlatformBiasConfig::default()
        };
        let temps = snapshot(&[("Ts0S", 43.0), ("Ts1S", 45.2)]);
        assert_eq!(compute_platform_bias(&config, &temps).skin_temp_c, Some(43.0));
    }

    #[test]
    fn bias_is_max_not_sum() {
        let config = no_system(PlatformBiasConfig::default());
        // skin 44.0 -> 8.0, battery 40.0 -> 4.0; summing would give 12.0
        let temps = snapshot(&[("Ts0S", 44.0), ("TB0T", 40.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert!((bias.bias_c - 8.0).abs() < 1e-3, "got {}", bias.bias_c);
    }

    #[test]
    fn prochot_case_lifts_effective_temperature() {
        let config = no_system(PlatformBiasConfig::default());
        // Ts1S 45.2 C at the observed PROCHOT event: (45.2 - 42.0) * 4.0 = 12.8
        let temps = snapshot(&[("Ts0S", 44.0), ("Ts1S", 45.2)]);
        let bias = compute_platform_bias(&config, &temps);
        assert!((bias.bias_c - 12.8).abs() < 1e-3, "got {}", bias.bias_c);
        assert_eq!(apply_platform_bias(68, bias.bias_c), 81);
    }

    #[test]
    fn below_threshold_yields_no_bias() {
        let config = PlatformBiasConfig::default();
        let temps = snapshot(&[("Ts0S", 39.0), ("Ts1S", 30.0), ("TB0T", 38.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert_eq!(bias.bias_c, 0.0);
        // far below threshold must never pull the temperature down
        let cold = snapshot(&[("Ts0S", 20.0), ("TB0T", 15.0)]);
        assert_eq!(compute_platform_bias(&config, &cold).bias_c, 0.0);
    }

    #[test]
    fn missing_groups_are_ignored() {
        let config = PlatformBiasConfig::default();
        // no skin and no battery sensor, only an unclassified one
        let temps = snapshot(&[("TTXD", 30.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert_eq!(bias.skin_temp_c, None);
        assert_eq!(bias.battery_temp_c, None);
        assert_eq!(bias.system_temp_c, Some(30.0));
        assert_eq!(bias.bias_c, 0.0);
        assert_eq!(apply_platform_bias(65, bias.bias_c), 65);
    }

    #[test]
    fn battery_bias_is_capped() {
        let config = no_system(PlatformBiasConfig::default());
        let temps = snapshot(&[("TB0T", 60.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert!((bias.bias_c - config.battery_max_bias_c).abs() < 1e-3);
    }

    #[test]
    fn disabled_bias_keeps_sensor_readings_but_adds_nothing() {
        let config = PlatformBiasConfig {
            enabled: false,
            ..PlatformBiasConfig::default()
        };
        let temps = snapshot(&[("Ts1S", 50.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert_eq!(bias.skin_temp_c, Some(50.0));
        assert_eq!(bias.system_temp_c, Some(50.0));
        assert_eq!(bias.bias_c, 0.0);
    }

    #[test]
    fn cpu_sensors_never_reach_the_bias() {
        let config = PlatformBiasConfig::default();
        let temps = snapshot(&[("TCXC", 95.0), ("TC0P", 90.0), ("TG0P", 88.0)]);
        let bias = compute_platform_bias(&config, &temps);
        assert_eq!(bias.system_temp_c, None);
        assert_eq!(bias.bias_c, 0.0);
    }

    #[test]
    fn effective_temperature_is_clamped_to_the_curve_axis() {
        assert_eq!(apply_platform_bias(95, 25.0), 100);
        assert_eq!(apply_platform_bias(0, 0.0), 0);
        assert_eq!(apply_platform_bias(70, 4.4), 74);
        assert_eq!(apply_platform_bias(70, 4.5), 75);
    }

    #[test]
    fn custom_thresholds_and_factors_are_honoured() {
        let config = no_system(PlatformBiasConfig {
            skin_threshold_c: 42.0,
            skin_factor: 2.0,
            skin_max_bias_c: 3.0,
            ..PlatformBiasConfig::default()
        });
        let temps = snapshot(&[("Ts0S", 43.0)]);
        assert!((compute_platform_bias(&config, &temps).bias_c - 2.0).abs() < 1e-3);
        let hot = snapshot(&[("Ts0S", 50.0)]);
        assert!((compute_platform_bias(&config, &hot).bias_c - 3.0).abs() < 1e-3);
    }
}
