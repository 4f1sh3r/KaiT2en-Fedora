use std::time::{Duration, Instant};

use crate::{
    config::{curve_for_preset, AppConfig, CurvePoint, PlatformBiasConfig},
    error::Result,
    sysfs::{FanEndpoint, PlatformSensor, SensorRole, TemperatureSnapshot, TemperatureSource},
};

const CONTROL_INTERVAL: Duration = Duration::from_secs(5);
/// Speed changes below this many percentage points are not worth a fan write.
const APPLY_DEADBAND_PCT: f32 = 3.0;
/// While a ramp is running the daemon only moves the fan, it reads no sensors, so it can
/// afford to do it often enough that the sweep is heard as a glide instead of steps.
const RAMP_INTERVAL: Duration = Duration::from_millis(100);
const SENSOR_LOG_INTERVAL: Duration = Duration::from_secs(30);
/// How often the full SMC sensor set is sampled. The heat-soak signals are low-passed
/// over minutes, so reading them on every tick would only cost wakeups.
const PLATFORM_SAMPLE_INTERVAL: Duration = Duration::from_secs(30);
/// Upper end of the curve's temperature axis, so the bias can never point past it.
const MAX_EFFECTIVE_TEMP_C: f32 = 100.0;

pub struct Controller {
    base_temp_avg: Option<f32>,
    system_temp_avg: Option<f32>,
    platform_sensors: Vec<PlatformSensor>,
    last_platform_read: Option<Instant>,
    /// Where the fan is being driven right now, kept fractional so a slow rate still
    /// accumulates instead of rounding away to nothing on every step.
    commanded_percent: Option<f32>,
    /// Where the curve wants it to end up.
    target_percent: Option<u8>,
    last_applied_percent: Option<u8>,
    last_ramp: Instant,
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
    system_temp_c: Option<f32>,
) -> PlatformBias {
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

/// First-order low-pass with separate rise and fall time constants. A time constant
/// of zero passes the sample straight through.
pub fn smooth_temp(
    previous: Option<f32>,
    sample: Option<f32>,
    rise_time_s: f32,
    fall_time_s: f32,
    step_s: f32,
) -> Option<f32> {
    let Some(sample) = sample else {
        return previous;
    };
    if !sample.is_finite() {
        return previous;
    }
    let Some(previous) = previous else {
        return Some(sample);
    };
    let time_constant = if sample >= previous { rise_time_s } else { fall_time_s };
    let step = step_s.max(0.0);
    let alpha = (step / (time_constant.max(0.0) + step)).clamp(0.0, 1.0);
    Some(previous + alpha * (sample - previous))
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
            base_temp_avg: None,
            system_temp_avg: None,
            platform_sensors: Vec::new(),
            last_platform_read: None,
            commanded_percent: None,
            target_percent: None,
            last_applied_percent: None,
            last_ramp: Instant::now(),
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
        self.last_tick = Instant::now();
        let platform_age = self
            .last_platform_read
            .map(|last| last.elapsed())
            .filter(|age| *age < PLATFORM_SAMPLE_INTERVAL);
        let snapshot = match platform_age {
            Some(_) => TemperatureSnapshot::read_cpu_gpu(temperatures, self.platform_sensors.clone()),
            None => {
                let snapshot = TemperatureSnapshot::read_from(temperatures);
                self.platform_sensors = snapshot.sensors.clone();
                self.last_platform_read = Some(Instant::now());
                snapshot
            }
        };
        let base_temp = snapshot.base_temp_c();

        self.base_temp_avg = smooth_temp(
            self.base_temp_avg,
            base_temp.map(f32::from),
            config.base_rise_time_s,
            config.base_fall_time_s,
            CONTROL_INTERVAL.as_secs_f32(),
        );
        let smoothed_temp = self.base_temp_avg.map(round_temp);
        let raw_system_temp = snapshot.system_temp_c();
        if platform_age.is_none() {
            self.system_temp_avg = smooth_temp(
                self.system_temp_avg,
                raw_system_temp,
                config.platform_bias.system_rise_time_s,
                config.platform_bias.system_fall_time_s,
                PLATFORM_SAMPLE_INTERVAL.as_secs_f32(),
            );
        }
        let bias = compute_platform_bias(&config.platform_bias, &snapshot, self.system_temp_avg);
        let effective_temp = smoothed_temp.map(|temp| apply_platform_bias(temp, bias.bias_c));
        let curve = curve_for_preset(config);
        let target_percent = effective_temp.map(|temp| interpolate_percent(&curve, temp));

        let mut commanded_percent = target_percent;
        if config.automatic_control_enabled {
            self.target_percent = target_percent;
            self.ramp(config, fans)?;
            commanded_percent = self.commanded_percent.map(round_percent);
        }
        let target_rpm_per_fan = fans
            .iter()
            .map(|fan| {
                commanded_percent
                    .map(|percent| fan.percent_to_rpm(percent))
                    .unwrap_or(fan.min_speed)
            })
            .collect();

        self.log_sensors(
            &snapshot,
            smoothed_temp,
            &bias,
            raw_system_temp,
            effective_temp,
            target_percent,
            commanded_percent,
        );

        Ok(ControlSnapshot {
            temperatures: snapshot,
            base_temp_c: smoothed_temp,
            system_temp_c: bias.system_temp_c,
            skin_temp_c: bias.skin_temp_c,
            battery_temp_c: bias.battery_temp_c,
            platform_bias_c: Some(bias.bias_c),
            effective_temp_c: effective_temp,
            target_percent: commanded_percent,
            target_rpm_per_fan,
        })
    }

    /// Moves the fan one step toward the target. Reads nothing, so it is cheap enough to
    /// run at RAMP_INTERVAL for as long as a ramp is actually in progress.
    pub fn ramp(&mut self, config: &AppConfig, fans: &mut [FanEndpoint]) -> Result<()> {
        let elapsed = self.last_ramp.elapsed().as_secs_f32();
        self.last_ramp = Instant::now();
        if !config.automatic_control_enabled {
            return Ok(());
        }

        let Some(target) = self.target_percent else {
            if self.last_applied_percent.is_some() {
                self.commanded_percent = None;
                self.last_applied_percent = None;
                for fan in fans {
                    let speed = fan.min_speed;
                    fan.set_target_speed(speed)?;
                    fan.current_speed = Some(speed);
                    fan.app_controlled = Some(true);
                }
            }
            return Ok(());
        };

        let next = match self.commanded_percent {
            None => target as f32,
            Some(current) => {
                // never spend more than one interval of movement in a single step, so a
                // target that jumps while the daemon was idle still leaves as a ramp
                let step_s = elapsed.min(RAMP_INTERVAL.as_secs_f32());
                advance_percent(current, target, ramp_rate(config, current, target), step_s)
            }
        };
        self.commanded_percent = Some(next);

        let percent = round_percent(next);
        if self.last_applied_percent == Some(percent) {
            return Ok(());
        }
        for fan in fans {
            let rpm = fan.percent_to_rpm(percent);
            fan.set_target_speed(rpm)?;
            fan.current_speed = Some(rpm);
            fan.app_controlled = Some(true);
        }
        self.last_applied_percent = Some(percent);
        Ok(())
    }

    /// True while the commanded speed is still meaningfully away from the target.
    fn ramping(&self) -> bool {
        match (self.commanded_percent, self.target_percent) {
            (Some(current), Some(target)) => {
                (target as f32 - current).abs() >= APPLY_DEADBAND_PCT
            }
            (None, Some(_)) => true,
            _ => false,
        }
    }

    pub fn should_ramp(&self) -> bool {
        self.ramping() && self.last_ramp.elapsed() >= RAMP_INTERVAL
    }

    fn log_sensors(
        &mut self,
        temperatures: &TemperatureSnapshot,
        base_temp_c: Option<u8>,
        bias: &PlatformBias,
        raw_system_temp_c: Option<f32>,
        effective_temp_c: Option<u8>,
        target_percent: Option<u8>,
        commanded_percent: Option<u8>,
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
            "sensors: base={} system={} (raw={}, n={}) skin={} battery={} bias={:.2} effective={} target={} fan={} {}",
            format_opt_u8(base_temp_c),
            format_opt_f32(bias.system_temp_c),
            format_opt_f32(raw_system_temp_c),
            temperatures.system_sensor_count(),
            format_opt_f32(bias.skin_temp_c),
            format_opt_f32(bias.battery_temp_c),
            bias.bias_c,
            format_opt_u8(effective_temp_c),
            format_opt_u8(target_percent),
            format_opt_u8(commanded_percent),
            sensors
        );
    }

    pub fn release_to_system(&mut self, fans: &mut [FanEndpoint]) -> Result<()> {
        for fan in fans {
            fan.release_to_auto()?;
            fan.app_controlled = Some(false);
        }
        self.commanded_percent = None;
        self.target_percent = None;
        self.last_applied_percent = None;
        Ok(())
    }

    pub fn should_tick(&self) -> bool {
        self.last_tick.elapsed() >= CONTROL_INTERVAL
    }

    /// How long the daemon may sleep before it has work again: the next control tick, or
    /// the next ramp step while the fan is still on its way.
    pub fn time_until_work(&self) -> Duration {
        let until_tick = CONTROL_INTERVAL.saturating_sub(self.last_tick.elapsed());
        if self.ramping() {
            until_tick.min(RAMP_INTERVAL.saturating_sub(self.last_ramp.elapsed()))
        } else {
            until_tick
        }
    }

}

fn format_opt_u8(value: Option<u8>) -> String {
    value.map_or_else(|| String::from("n/a"), |value| value.to_string())
}

fn format_opt_f32(value: Option<f32>) -> String {
    value.map_or_else(|| String::from("n/a"), |value| format!("{value:.2}"))
}

/// How fast the fan may move right now, in percentage points per second. Small
/// corrections crawl at the configured base rate; the bigger the gap, the closer the
/// rate gets to closing it within `fan_ramp_time_s`. Only speeding up is boosted --
/// coming down fast buys nothing and is what makes the fan pump.
pub fn ramp_rate(config: &AppConfig, current: f32, target: u8) -> f32 {
    let gap = target as f32 - current;
    if gap <= 0.0 {
        return config.fan_fall_rate_pct_per_s.max(0.0);
    }
    let ramp_time = config.fan_ramp_time_s.max(0.01);
    config
        .fan_rise_rate_pct_per_s
        .max(0.0)
        .max(gap.abs() / ramp_time)
}

/// One step of the ramp: move toward the target at `rate`, never past it.
pub fn advance_percent(current: f32, target: u8, rate: f32, step_s: f32) -> f32 {
    let gap = target as f32 - current;
    let step = rate.max(0.0) * step_s.max(0.0);
    if step >= gap.abs() {
        return target as f32;
    }
    (current + step * gap.signum()).clamp(0.0, 100.0)
}

fn round_temp(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

fn round_percent(value: f32) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
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
        assert_eq!(compute_platform_bias(&PlatformBiasConfig::default(), &snapshot(&[]), None).bias_c, 0.0);
    }

    #[test]
    fn system_bias_follows_threshold_and_factor() {
        let config = PlatformBiasConfig::default();
        // mean 45 C -> (45 - 40) * 4 = 20
        let temps = snapshot(&[("TTXD", 44.0), ("TW0P", 46.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert_eq!(bias.system_temp_c, Some(45.0));
        assert!((bias.bias_c - 20.0).abs() < 1e-3, "got {}", bias.bias_c);
        assert_eq!(apply_platform_bias(60, bias.bias_c), 80);
    }

    #[test]
    fn system_bias_is_capped() {
        let config = PlatformBiasConfig::default();
        let temps = snapshot(&[("TTXD", 80.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert!((bias.bias_c - config.system_max_bias_c).abs() < 1e-3);
    }

    #[test]
    fn groups_auto_select_by_role_when_no_labels_are_configured() {
        let config = PlatformBiasConfig::default();
        assert!(config.skin_sensors.is_empty());
        let temps = snapshot(&[("Ts0S", 43.0), ("Ts1S", 45.2), ("TB0T", 36.0), ("TB2T", 39.5)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
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
        assert_eq!(compute_platform_bias(&config, &temps, temps.system_temp_c()).skin_temp_c, Some(43.0));
    }

    #[test]
    fn bias_is_max_not_sum() {
        let config = no_system(PlatformBiasConfig::default());
        // skin 44.0 -> 8.0, battery 40.0 -> 4.0; summing would give 12.0
        let temps = snapshot(&[("Ts0S", 44.0), ("TB0T", 40.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert!((bias.bias_c - 8.0).abs() < 1e-3, "got {}", bias.bias_c);
    }

    #[test]
    fn prochot_case_lifts_effective_temperature() {
        let config = no_system(PlatformBiasConfig::default());
        // Ts1S 45.2 C at the observed PROCHOT event: (45.2 - 42.0) * 4.0 = 12.8
        let temps = snapshot(&[("Ts0S", 44.0), ("Ts1S", 45.2)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert!((bias.bias_c - 12.8).abs() < 1e-3, "got {}", bias.bias_c);
        assert_eq!(apply_platform_bias(68, bias.bias_c), 81);
    }

    #[test]
    fn below_threshold_yields_no_bias() {
        let config = PlatformBiasConfig::default();
        let temps = snapshot(&[("Ts0S", 39.0), ("Ts1S", 30.0), ("TB0T", 38.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert_eq!(bias.bias_c, 0.0);
        // far below threshold must never pull the temperature down
        let cold = snapshot(&[("Ts0S", 20.0), ("TB0T", 15.0)]);
        assert_eq!(compute_platform_bias(&config, &cold, cold.system_temp_c()).bias_c, 0.0);
    }

    #[test]
    fn missing_groups_are_ignored() {
        let config = PlatformBiasConfig::default();
        // no skin and no battery sensor, only an unclassified one
        let temps = snapshot(&[("TTXD", 30.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
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
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert!((bias.bias_c - config.battery_max_bias_c).abs() < 1e-3);
    }

    #[test]
    fn disabled_bias_keeps_sensor_readings_but_adds_nothing() {
        let config = PlatformBiasConfig {
            enabled: false,
            ..PlatformBiasConfig::default()
        };
        let temps = snapshot(&[("Ts1S", 50.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
        assert_eq!(bias.skin_temp_c, Some(50.0));
        assert_eq!(bias.system_temp_c, Some(50.0));
        assert_eq!(bias.bias_c, 0.0);
    }

    #[test]
    fn cpu_sensors_never_reach_the_bias() {
        let config = PlatformBiasConfig::default();
        let temps = snapshot(&[("TCXC", 95.0), ("TC0P", 90.0), ("TG0P", 88.0)]);
        let bias = compute_platform_bias(&config, &temps, temps.system_temp_c());
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
    fn smoothing_seeds_on_the_first_sample() {
        assert_eq!(smooth_temp(None, Some(40.0), 30.0, 180.0, 2.0), Some(40.0));
        assert_eq!(smooth_temp(None, None, 30.0, 180.0, 2.0), None);
        // a sensor that vanishes must not reset the average
        assert_eq!(smooth_temp(Some(40.0), None, 30.0, 180.0, 2.0), Some(40.0));
    }

    #[test]
    fn smoothing_rises_faster_than_it_falls() {
        let rise = smooth_temp(Some(40.0), Some(50.0), 30.0, 180.0, 2.0).unwrap();
        let fall = smooth_temp(Some(50.0), Some(40.0), 30.0, 180.0, 2.0).unwrap();
        assert!(rise - 40.0 > 50.0 - fall, "rise {rise} fall {fall}");
        // both stay between the previous value and the new sample
        assert!((40.0..50.0).contains(&rise));
        assert!((40.0..50.0).contains(&fall));
    }

    #[test]
    fn the_base_temperature_answers_a_load_step_within_seconds() {
        let config = AppConfig::default();
        // idle at 60 C, then a full load step to 95 C
        let mut avg = Some(60.0);
        for _ in 0..2 {
            avg = smooth_temp(avg, Some(95.0), config.base_rise_time_s, config.base_fall_time_s, 5.0);
        }
        assert!(avg.unwrap() > 90.0, "after 10 s: {avg:?}");
        // and takes minutes to come back down again
        let mut avg = Some(95.0);
        for _ in 0..2 {
            avg = smooth_temp(avg, Some(60.0), config.base_rise_time_s, config.base_fall_time_s, 5.0);
        }
        assert!(avg.unwrap() > 88.0, "after 10 s: {avg:?}");
    }

    #[test]
    fn a_longer_sampling_step_moves_the_average_further() {
        let slow = smooth_temp(Some(40.0), Some(50.0), 30.0, 180.0, 2.0).unwrap();
        let fast = smooth_temp(Some(40.0), Some(50.0), 30.0, 180.0, 30.0).unwrap();
        assert!(fast > slow, "slow {slow} fast {fast}");
        assert!((fast - 45.0).abs() < 1e-3, "got {fast}");
    }

    #[test]
    fn zero_time_constant_disables_smoothing() {
        assert_eq!(smooth_temp(Some(40.0), Some(50.0), 0.0, 0.0, 2.0), Some(50.0));
    }

    #[test]
    fn smoothing_converges_on_a_steady_sample() {
        let mut value = Some(30.0);
        for _ in 0..600 {
            value = smooth_temp(value, Some(45.0), 30.0, 180.0, 2.0);
        }
        assert!((value.unwrap() - 45.0).abs() < 0.1, "got {value:?}");
    }

    fn rates(rise: f32, fall: f32, ramp: f32) -> AppConfig {
        AppConfig {
            fan_rise_rate_pct_per_s: rise,
            fan_fall_rate_pct_per_s: fall,
            fan_ramp_time_s: ramp,
            ..AppConfig::default()
        }
    }

    #[test]
    fn the_base_rates_act_as_a_floor() {
        let config = rates(1.0, 0.35, 1.5);
        // falling is never boosted, whatever the gap
        assert!((ramp_rate(&config, 40.0, 38) - 0.35).abs() < 1e-3);
        assert!((ramp_rate(&config, 90.0, 20) - 0.35).abs() < 1e-3);
        // a rise floor above the gap-proportional rate wins
        let brisk = rates(10.0, 0.35, 1.5);
        assert!((ramp_rate(&brisk, 40.0, 42) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn a_large_gap_is_closed_within_the_ramp_time() {
        let config = rates(1.0, 0.35, 1.5);
        // 70 points to climb in 1.5 s is far more than the base rate allows
        assert!((ramp_rate(&config, 20.0, 90) - 70.0 / 1.5).abs() < 1e-3);
        // coming down is never boosted, that is what makes a fan pump
        assert!((ramp_rate(&config, 90.0, 20) - 0.35).abs() < 1e-3);
    }

    #[test]
    fn a_ramp_reaches_the_target_and_stops_there() {
        let config = rates(1.0, 0.35, 1.5);
        let mut current = 20.0f32;
        let mut steps = 0;
        let mut steps_to_deadband = None;
        while (90.0 - current).abs() > 0.05 && steps < 1000 {
            current = advance_percent(current, 90, ramp_rate(&config, current, 90), 0.1);
            steps += 1;
            if steps_to_deadband.is_none() && 90.0 - current < APPLY_DEADBAND_PCT {
                steps_to_deadband = Some(steps);
            }
        }
        assert_eq!(current, 90.0);
        // 20 -> 90 arrives within a few seconds, and never in a single jump
        let steps_to_deadband = steps_to_deadband.unwrap();
        assert!((10..60).contains(&steps_to_deadband), "took {steps_to_deadband} steps");
    }

    #[test]
    fn a_single_step_never_overshoots() {
        assert_eq!(advance_percent(88.0, 90, 100.0, 0.1), 90.0);
        assert_eq!(advance_percent(92.0, 90, 100.0, 0.1), 90.0);
    }

    #[test]
    fn a_slow_rate_still_accumulates_instead_of_rounding_away() {
        let mut current = 50.0f32;
        for _ in 0..30 {
            current = advance_percent(current, 20, 0.35, 0.1);
        }
        // 3 s at 0.35 points per second, which a u8 command would have dropped entirely
        assert!((current - 48.95).abs() < 1e-2, "got {current}");
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
        assert!((compute_platform_bias(&config, &temps, temps.system_temp_c()).bias_c - 2.0).abs() < 1e-3);
        let hot = snapshot(&[("Ts0S", 50.0)]);
        assert!((compute_platform_bias(&config, &hot, hot.system_temp_c()).bias_c - 3.0).abs() < 1e-3);
    }
}
