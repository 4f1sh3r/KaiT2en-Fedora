//! Read-only view of the SMC zones and the limits the daemon applied

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    plimit::PLimits,
    sysfs::{glob_paths, read_string, read_value},
};

/// t2smc labels its hwmon channels with the raw SMC key
pub struct SmcSensors {
    t2smc: PathBuf,
    inputs: HashMap<String, PathBuf>,
}

impl SmcSensors {
    pub fn discover(t2smc: &Path) -> Self {
        let mut inputs = HashMap::new();
        for label in glob_paths(&format!("{}/*_label", t2smc.display())).unwrap_or_default() {
            let (Ok(key), Some(name)) = (read_string(&label), label.file_name()) else { continue };
            let input = name.to_string_lossy().replace("_label", "_input");
            inputs.insert(key, t2smc.join(input));
        }
        Self { t2smc: t2smc.to_owned(), inputs }
    }

    fn raw(&self, key: &str) -> Option<f64> {
        read_value::<i64>(self.inputs.get(key)?).ok().map(|value| value as f64)
    }

    /// Power keys and the zone throttle keys share the micro unit
    pub fn micro(&self, key: &str) -> Option<f64> {
        self.raw(key).map(|value| value / 1_000_000.0)
    }

    pub fn celsius(&self, key: &str) -> Option<f64> {
        self.raw(key).map(|value| value / 1000.0)
    }

    pub fn plimits(&self) -> Option<PLimits> {
        PLimits::read(&self.t2smc).ok()
    }
}

/// Limits as the daemon left them in sysfs
#[derive(Clone, Copy, Debug, Default)]
pub struct Applied {
    pub cpu_cap_mhz: Option<u32>,
    pub cpu_max_mhz: Option<u32>,
    pub idle_percent: Option<u32>,
    pub dgpu_cap_w: Option<u32>,
    pub dgpu_tdp_w: Option<u32>,
    pub igpu_cap_mhz: Option<u32>,
    pub igpu_max_mhz: Option<u32>,
}

impl Applied {
    pub fn read() -> Self {
        let policy = Path::new("/sys/devices/system/cpu/cpufreq/policy0");
        let khz = |file: &str| read_value::<u32>(&policy.join(file)).ok().map(|khz| khz / 1000);
        Self {
            cpu_cap_mhz: khz("scaling_max_freq"),
            cpu_max_mhz: khz("cpuinfo_max_freq"),
            idle_percent: powerclamp_state(),
            dgpu_cap_w: amdgpu_watts("power1_cap"),
            dgpu_tdp_w: amdgpu_watts("power1_cap_default"),
            igpu_cap_mhz: i915_mhz("gt_max_freq_mhz"),
            igpu_max_mhz: i915_mhz("gt_RP0_freq_mhz"),
        }
    }
}

fn i915_mhz(file: &str) -> Option<u32> {
    let path = glob_paths(&format!("/sys/class/drm/card*/{file}")).ok()?.into_iter().next()?;
    read_value(&path).ok()
}

fn powerclamp_state() -> Option<u32> {
    glob_paths("/sys/class/thermal/cooling_device*")
        .ok()?
        .into_iter()
        .find(|device| read_string(&device.join("type")).is_ok_and(|kind| kind == "intel_powerclamp"))
        .and_then(|device| read_value(&device.join("cur_state")).ok())
}

/// Reads nothing while the dGPU sleeps, so the GUI never wakes it
fn amdgpu_watts(file: &str) -> Option<u32> {
    for hwmon in glob_paths("/sys/class/drm/card*/device/hwmon/hwmon*").ok()? {
        if !read_string(&hwmon.join("name")).is_ok_and(|name| name == "amdgpu") {
            continue;
        }
        let active = read_string(&hwmon.join("device/power/runtime_status"))
            .is_ok_and(|status| status == "active");
        if !active {
            return None;
        }
        return read_value::<u64>(&hwmon.join(file)).ok().map(|uw| (uw / 1_000_000) as u32);
    }
    None
}
