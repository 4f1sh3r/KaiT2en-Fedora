use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

#[allow(unused_imports)]
use adw::prelude::*;
use gtk4 as gtk;

const APP_ID: &str = "org.t2hybridgpucontrol.gtk";
const HELPER: &str = "/usr/local/libexec/t2-hybrid-gpu-control-helper";
const STATUS_HELPER: &str = "/usr/local/libexec/t2-hybrid-gpu-control-status";
const EFI_VAR: &str =
    "/sys/firmware/efi/efivars/gpu-power-prefs-fa4ce28d-b62f-4c99-9cc3-6815686e30f9";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gpu {
    Integrated,
    Discrete,
    Unknown,
}

impl Gpu {
    fn label(self) -> &'static str {
        match self {
            Self::Integrated => "Integrated GPU",
            Self::Discrete => "Discrete GPU",
            Self::Unknown => "Unknown",
        }
    }
}

struct GpuStatus {
    active: Gpu,
    discrete_state: String,
    runtime_pm: bool,
}

#[derive(Clone, Copy)]
struct PowerCap {
    current_w: f64,
    default_w: f64,
}

fn amdgpu_device() -> Option<PathBuf> {
    for entry in fs::read_dir("/sys/class/drm").ok()? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name
            .strip_prefix("card")
            .is_some_and(|suffix| suffix.chars().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        let Ok(driver) = fs::canonicalize(path.join("device/driver")) else {
            continue;
        };
        if driver.file_name().and_then(|name| name.to_str()) == Some("amdgpu") {
            return Some(path.join("device"));
        }
    }
    None
}

fn read_power_cap() -> Option<PowerCap> {
    let hwmon = fs::read_dir(amdgpu_device()?.join("hwmon"))
        .ok()?
        .next()?
        .ok()?
        .path();
    let read = |name: &str| {
        fs::read_to_string(hwmon.join(name))
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()
    };
    let current = read("power1_cap")?;
    let default = read("power1_cap_default")?;
    (default >= 10_000_000).then_some(PowerCap {
        current_w: current as f64 / 1_000_000.0,
        default_w: default as f64 / 1_000_000.0,
    })
}

fn switcheroo_status() -> GpuStatus {
    let Ok(output) = Command::new("pkexec").arg(STATUS_HELPER).output() else {
        return GpuStatus {
            active: Gpu::Unknown,
            discrete_state: "Unavailable".into(),
            runtime_pm: false,
        };
    };
    if !output.status.success() {
        return GpuStatus {
            active: Gpu::Unknown,
            discrete_state: "Unavailable".into(),
            runtime_pm: false,
        };
    }

    let mut active = Gpu::Unknown;
    let mut discrete_state = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<_> = line.split(':').collect();
        if fields.len() < 4 {
            continue;
        }
        let gpu = match fields[1] {
            "IGD" => Gpu::Integrated,
            "DIS" => Gpu::Discrete,
            _ => continue,
        };
        if fields[2] == "+" {
            active = gpu;
        }
        if gpu == Gpu::Discrete {
            discrete_state = Some(fields[3].to_owned());
        }
    }

    let discrete_state = discrete_state.unwrap_or_else(|| "Unavailable".into());
    let runtime_pm = matches!(discrete_state.as_str(), "DynPwr" | "DynOff");
    GpuStatus {
        active,
        discrete_state,
        runtime_pm,
    }
}

fn configured_gpu() -> Gpu {
    let Ok(value) = fs::read(EFI_VAR) else {
        return Gpu::Unknown;
    };

    match value.get(4..8) {
        Some([1, 0, 0, 0]) => Gpu::Integrated,
        Some([0, 0, 0, 0]) => Gpu::Discrete,
        _ => Gpu::Unknown,
    }
}

fn run_helper(args: &[&str]) -> Result<(), String> {
    let output = Command::new("pkexec")
        .arg(HELPER)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start the privileged helper: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if message.is_empty() {
        Err("The operation was cancelled or failed.".into())
    } else {
        Err(message)
    }
}

fn set_status(label: &gtk::Label, message: &str, error: bool) {
    label.set_label(message);
    label.remove_css_class("error");
    label.remove_css_class("success");
    label.add_css_class(if error { "error" } else { "success" });
    label.set_visible(true);
}

fn update_runtime_status(
    current_row: &adw::ActionRow,
    runtime_row: &adw::ActionRow,
    discrete_state_row: &adw::ActionRow,
    hybrid_button: &gtk::Button,
) {
    let status = switcheroo_status();
    current_row.set_subtitle(status.active.label());
    runtime_row.set_subtitle(if status.runtime_pm {
        "Available"
    } else {
        "Required kernel patches are not active"
    });
    discrete_state_row.set_subtitle(&status.discrete_state);
    hybrid_button.set_sensitive(status.runtime_pm);
}

fn build_ui(app: &adw::Application) {
    let status = switcheroo_status();
    let configured = configured_gpu();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("T2 Hybrid GPU Control")
        .default_width(520)
        .default_height(700)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&adw::HeaderBar::new());

    let page = adw::PreferencesPage::new();
    let status_group = adw::PreferencesGroup::builder().title("GPU status").build();
    let current_row = adw::ActionRow::builder()
        .title("Current display GPU")
        .subtitle(status.active.label())
        .build();
    let next_row = adw::ActionRow::builder()
        .title("Configured boot GPU")
        .subtitle(configured.label())
        .build();
    let runtime_row = adw::ActionRow::builder()
        .title("Hybrid graphics support")
        .subtitle(if status.runtime_pm {
            "Available"
        } else {
            "Required kernel patches are not active"
        })
        .build();
    let discrete_state_row = adw::ActionRow::builder()
        .title("Discrete GPU runtime state")
        .subtitle(&status.discrete_state)
        .build();
    status_group.add(&current_row);
    status_group.add(&next_row);
    status_group.add(&runtime_row);
    status_group.add(&discrete_state_row);
    page.add(&status_group);

    let config_group = adw::PreferencesGroup::builder()
        .title("Configuration")
        .description("Hybrid graphics uses the integrated GPU for the display and wakes the AMD GPU automatically for PRIME offload.")
        .build();

    let hybrid_row = adw::ActionRow::builder()
        .title("Hybrid graphics")
        .subtitle("Recommended for performance, battery life, and suspend")
        .build();
    let hybrid_button = gtk::Button::with_label(if configured == Gpu::Integrated {
        "Reapply"
    } else {
        "Enable"
    });
    hybrid_button.add_css_class("suggested-action");
    hybrid_button.set_valign(gtk::Align::Center);
    hybrid_button.set_sensitive(status.runtime_pm);
    hybrid_row.add_suffix(&hybrid_button);
    config_group.add(&hybrid_row);

    let discrete_row = adw::ActionRow::builder()
        .title("Discrete GPU boot")
        .subtitle("Recovery option for configurations that cannot use hybrid graphics")
        .build();
    let discrete_button = gtk::Button::with_label("Use Next Boot");
    discrete_button.set_valign(gtk::Align::Center);
    discrete_row.add_suffix(&discrete_button);
    config_group.add(&discrete_row);
    page.add(&config_group);

    let power_group = adw::PreferencesGroup::builder()
        .title("AMDGPU power cap")
        .build();
    let power_cap = read_power_cap();
    let power_row = adw::ActionRow::builder()
        .title("Maximum GPU power")
        .subtitle(if let Some(cap) = power_cap {
            format!("{:.0} W / {:.0} W default", cap.current_w, cap.default_w)
        } else {
            "Unavailable while the dGPU is off".into()
        })
        .build();
    let power_scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        10.0,
        power_cap.map_or(10.0, |cap| cap.default_w),
        1.0,
    );
    power_scale.set_draw_value(false);
    power_scale.set_size_request(220, -1);
    power_scale.set_valign(gtk::Align::Center);
    power_scale.set_sensitive(power_cap.is_some());
    if let Some(cap) = power_cap {
        power_scale.set_value(cap.current_w);
    }
    power_row.add_suffix(&power_scale);
    power_group.add(&power_row);
    page.add(&power_group);

    let actions_group = adw::PreferencesGroup::new();
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let message = gtk::Label::new(None);
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.set_visible(false);
    let reboot = gtk::Button::with_label("Reboot");
    reboot.add_css_class("suggested-action");
    reboot.set_halign(gtk::Align::End);
    reboot.set_sensitive(false);
    actions.append(&message);
    actions.append(&reboot);
    actions_group.add(&actions);
    page.add(&actions_group);

    {
        let generation = Rc::new(Cell::new(0_u64));
        let power_row = power_row.clone();
        let message = message.clone();
        power_scale.connect_value_changed(move |scale| {
            let value = scale.value().round();
            power_row.set_subtitle(&format!("{value:.0} W selected"));
            let next = generation.get() + 1;
            generation.set(next);
            let generation = generation.clone();
            let power_row = power_row.clone();
            let message = message.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(700), move || {
                if generation.get() != next {
                    return;
                }
                let microwatts = format!("{:.0}", value * 1_000_000.0);
                match run_helper(&["set-power-cap", &microwatts]) {
                    Ok(()) => {
                        power_row.set_subtitle(&format!("{value:.0} W applied"));
                        set_status(
                            &message,
                            &format!("AMDGPU power cap set to {value:.0} W."),
                            false,
                        );
                    }
                    Err(error) => set_status(&message, &error, true),
                }
            });
        });
    }

    {
        let message = message.clone();
        let next_row = next_row.clone();
        let reboot = reboot.clone();
        hybrid_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            match run_helper(&["enable-hybrid"]) {
                Ok(()) => {
                    next_row.set_subtitle(Gpu::Integrated.label());
                    set_status(
                        &message,
                        "Hybrid graphics configured. Reboot to use the integrated display GPU.",
                        false,
                    );
                    reboot.set_sensitive(true);
                }
                Err(error) => {
                    set_status(&message, &error, true);
                    button.set_sensitive(true);
                }
            }
        });
    }

    {
        let message = message.clone();
        let next_row = next_row.clone();
        let reboot = reboot.clone();
        discrete_button.connect_clicked(move |button| {
            button.set_sensitive(false);
            match run_helper(&["use-discrete"]) {
                Ok(()) => {
                    next_row.set_subtitle(Gpu::Discrete.label());
                    set_status(
                        &message,
                        "Discrete GPU boot configured. Reboot to apply the recovery configuration.",
                        false,
                    );
                    reboot.set_sensitive(true);
                }
                Err(error) => {
                    set_status(&message, &error, true);
                    button.set_sensitive(true);
                }
            }
        });
    }

    {
        let message = message.clone();
        reboot.connect_clicked(move |_| {
            if let Err(error) = run_helper(&["reboot"]) {
                set_status(&message, &error, true);
            }
        });
    }

    {
        let current_row = current_row.clone();
        let runtime_row = runtime_row.clone();
        let discrete_state_row = discrete_state_row.clone();
        let hybrid_button = hybrid_button.clone();
        gtk::glib::timeout_add_seconds_local(3, move || {
            update_runtime_status(
                &current_row,
                &runtime_row,
                &discrete_state_row,
                &hybrid_button,
            );
            gtk::glib::ControlFlow::Continue
        });
    }

    content.append(&page);
    window.set_content(Some(&content));
    window.present();
}

fn main() {
    if !Path::new(HELPER).exists() {
        eprintln!("Privileged helper is not installed at {HELPER}");
    }
    if !Path::new(STATUS_HELPER).exists() {
        eprintln!("Status helper is not installed at {STATUS_HELPER}");
    }

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}
