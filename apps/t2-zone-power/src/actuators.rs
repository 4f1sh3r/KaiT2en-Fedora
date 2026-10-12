//! Linux counterparts of the macOS power limit listeners

use std::path::{Path, PathBuf};

use crate::{
    error::Result,
    sysfs::{glob_paths, read_string, read_value, write_value},
};

/// X86PlatformShim caps XCPM at the core frequency of the P-state the CPU
/// step indexes. Step 0 is the turbo entry and leaves the CPU uncapped.
pub struct CpuCap {
    policies: Vec<PathBuf>,
    pstates_khz: Vec<u32>,
    min_khz: u32,
    max_khz: u32,
}

impl CpuCap {
    pub fn discover(t2smc: &Path) -> Result<Option<Self>> {
        // Older t2smc builds lack the list, then the CPU step is left alone
        let Ok(pstates) = read_string(&t2smc.join("cpu_pstates_mhz")) else { return Ok(None) };
        let pstates_khz: Vec<u32> = pstates
            .split_whitespace()
            .filter_map(|mhz| mhz.parse::<u32>().ok())
            .map(|mhz| mhz * 1000)
            .collect();
        let policies = glob_paths("/sys/devices/system/cpu/cpufreq/policy*")?;
        let Some(first) = policies.first() else { return Ok(None) };
        if pstates_khz.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            min_khz: read_value(&first.join("cpuinfo_min_freq"))?,
            max_khz: read_value(&first.join("cpuinfo_max_freq"))?,
            policies,
            pstates_khz,
        }))
    }

    pub fn target_khz(&self, step: u8) -> u32 {
        let index = usize::from(step).min(self.pstates_khz.len() - 1);
        if index == 0 {
            return self.max_khz;
        }
        self.pstates_khz[index].clamp(self.min_khz, self.max_khz)
    }

    pub fn apply(&self, khz: u32) -> Result<()> {
        for policy in &self.policies {
            let path = policy.join("scaling_max_freq");
            if read_value::<u32>(&path).ok() != Some(khz) {
                write_value(&path, khz)?;
            }
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        self.apply(self.max_khz)
    }
}

/// X86PlatformShim turns the idle step into a run ratio of 100 - step
/// percent, at least 30 %. intel_powerclamp takes the idle share.
pub struct ForcedIdle {
    cur_state: PathBuf,
    max_state: u32,
}

impl ForcedIdle {
    pub fn discover() -> Result<Option<Self>> {
        for device in glob_paths("/sys/class/thermal/cooling_device*")? {
            if read_string(&device.join("type")).is_ok_and(|kind| kind == "intel_powerclamp") {
                return Ok(Some(Self {
                    max_state: read_value(&device.join("max_state"))?,
                    cur_state: device.join("cur_state"),
                }));
            }
        }
        Ok(None)
    }

    pub fn target_percent(&self, step: u8) -> u32 {
        if step == 0 {
            return 0;
        }
        let run = 100u32.saturating_sub(u32::from(step)).max(30);
        (100 - run).min(self.max_state)
    }

    pub fn apply(&self, percent: u32) -> Result<()> {
        if read_value::<u32>(&self.cur_state).ok() != Some(percent) {
            write_value(&self.cur_state, percent)?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        self.apply(0)
    }
}

/// AGPM passes the dGPU step as index into the PowerTune table of the AMD
/// driver. Both drivers fill it linearly from the TDP (step 0) down to a
/// floor in 16 steps.
pub struct DGpuCap {
    device: PathBuf,
    hwmon: PathBuf,
    tdp_w: u32,
    floor_w: u32,
}

/// AGPM drives the dGPU through power states 0 to 15 on every Mac with one
pub const DGPU_STEPS: u32 = 16;

impl DGpuCap {
    pub fn discover() -> Result<Option<Self>> {
        for hwmon in glob_paths("/sys/class/drm/card*/device/hwmon/hwmon*")? {
            if !read_string(&hwmon.join("name")).is_ok_and(|name| name == "amdgpu") {
                continue;
            }
            let device = hwmon.join("device");
            let Some(floor_ratio) = floor_ratio(&device) else { continue };
            // Reads fail with EBUSY while the GPU is runtime suspended
            let Ok(tdp_uw) = read_value::<u64>(&hwmon.join("power1_cap_default")) else { continue };
            let tdp_w = u32::try_from(tdp_uw / 1_000_000).unwrap_or(u32::MAX);
            let floor_w = (f64::from(tdp_w) * floor_ratio).ceil() as u32;
            return Ok(Some(Self { device, hwmon, tdp_w, floor_w }));
        }
        Ok(None)
    }

    /// generatePowerTuneTable and populatePowerTuneTable
    pub fn target_w(&self, step: u8) -> u32 {
        let index = u32::from(step).min(DGPU_STEPS - 1);
        let span = f64::from(self.tdp_w - self.floor_w);
        let above_floor = span * f64::from(DGPU_STEPS - 1 - index) / f64::from(DGPU_STEPS - 1);
        self.floor_w + above_floor as u32
    }

    /// Leaves a runtime suspended GPU alone. The cap is applied once it wakes.
    pub fn apply(&self, watts: u32) -> Result<()> {
        if !self.is_active() {
            return Ok(());
        }
        let min: u64 = read_value(&self.hwmon.join("power1_cap_min"))?;
        let max: u64 = read_value(&self.hwmon.join("power1_cap_max"))?;
        let microwatts = (u64::from(watts) * 1_000_000).clamp(min, max);
        let path = self.hwmon.join("power1_cap");
        if read_value::<u64>(&path).ok() != Some(microwatts) {
            write_value(&path, microwatts)?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        self.apply(self.tdp_w)
    }

    fn is_active(&self) -> bool {
        read_string(&self.device.join("power/runtime_status")).is_ok_and(|status| status == "active")
    }
}

/// Floor of the PowerTune table in parts of the TDP, per macOS driver
fn floor_ratio(device: &Path) -> Option<f64> {
    let id = read_string(&device.join("device")).ok()?;
    let id = u16::from_str_radix(id.trim_start_matches("0x"), 16).ok()?;
    match id {
        // Navi 1x, AMDRadeonX6000Framebuffer: ceil(TDP * 0.4)
        0x7310..=0x73ff => Some(0.4),
        // Polaris, AMD9500Controller: CFG_PTPL2_MIN defaults to 0 W
        0x67c0..=0x67ff | 0x6980..=0x699f => Some(0.0),
        _ => None,
    }
}

/// AGPM passes the iGPU step to the Intel framebuffer, whose P-state table
/// runs from RP0 down to RPn one ratio per state (PMOBJECT::initPState2Freq).
/// A Gen9 ratio is 50 MHz.
pub struct IGpuCap {
    max_freq: PathBuf,
    rp0_mhz: u32,
    rpn_mhz: u32,
}

const IGPU_RATIO_MHZ: u32 = 50;

impl IGpuCap {
    pub fn discover() -> Result<Option<Self>> {
        for rp0 in glob_paths("/sys/class/drm/card*/gt_RP0_freq_mhz")? {
            let Some(card) = rp0.parent() else { continue };
            return Ok(Some(Self {
                rp0_mhz: read_value(&rp0)?,
                rpn_mhz: read_value(&card.join("gt_RPn_freq_mhz"))?,
                max_freq: card.join("gt_max_freq_mhz"),
            }));
        }
        Ok(None)
    }

    /// Number of P-states AGPM reports to the SMC
    pub fn steps(&self) -> u32 {
        (self.rp0_mhz - self.rpn_mhz) / IGPU_RATIO_MHZ + 1
    }

    pub fn target_mhz(&self, step: u8) -> u32 {
        let index = u32::from(step).min(self.steps() - 1);
        (self.rp0_mhz - index * IGPU_RATIO_MHZ).max(self.rpn_mhz)
    }

    pub fn apply(&self, mhz: u32) -> Result<()> {
        if read_value::<u32>(&self.max_freq).ok() != Some(mhz) {
            write_value(&self.max_freq, mhz)?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        self.apply(self.rp0_mhz)
    }
}
