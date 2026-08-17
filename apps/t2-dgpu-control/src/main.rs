use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

#[allow(unused_imports)]
use adw::prelude::*;
use gtk4 as gtk;

const APP_ID: &str = "org.t2dgpucontrol.gtk";
const HELPER: &str = "/usr/local/libexec/t2-dgpu-control-helper";
const STATUS_HELPER: &str = "/usr/local/libexec/t2-dgpu-control-status";
const EFI_VAR: &str =
    "/sys/firmware/efi/efivars/gpu-power-prefs-fa4ce28d-b62f-4c99-9cc3-6815686e30f9";
const DGPU_OFF_SERVICE: &str = "kait2en-dgpu-off.service";
const DGPU_SUSPEND_SERVICE: &str = "kait2en-dgpu-suspend.service";
const POWER_PROFILE_SERVICE: &str = "kait2en-amdgpu-profile.service";
const POWER_PROFILE_RESUME_SERVICE: &str = "kait2en-amdgpu-profile-resume.service";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gpu {
    Integrated,
    Discrete,
    Unknown,
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

impl Gpu {
    fn label(self) -> &'static str {
        match self {
            Self::Integrated => "Integrated GPU",
            Self::Discrete => "Discrete GPU",
            Self::Unknown => "Unknown",
        }
    }
}

fn switcheroo_status() -> (Gpu, String) {
    let Ok(output) = Command::new("pkexec").arg(STATUS_HELPER).output() else {
        return (Gpu::Unknown, "Unavailable".into());
    };
    if !output.status.success() {
        return (Gpu::Unknown, "Unavailable".into());
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

    (
        active,
        discrete_state.unwrap_or_else(|| "Unavailable".into()),
    )
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

fn service_enabled(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-enabled", "--quiet", unit])
        .status()
        .is_ok_and(|status| status.success())
}

fn power_saving_enabled() -> bool {
    [POWER_PROFILE_SERVICE, POWER_PROFILE_RESUME_SERVICE]
        .iter()
        .all(|unit| service_enabled(unit))
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

fn build_ui(app: &adw::Application) {
    let (current, discrete_state) = switcheroo_status();
    let configured = configured_gpu();
    let configured_power_off = service_enabled(DGPU_OFF_SERVICE);
    let configured_suspend_restore = service_enabled(DGPU_SUSPEND_SERVICE);
    let configured_power_saving = power_saving_enabled();
    let initial_gpu = if configured == Gpu::Unknown {
        current
    } else {
        configured
    };

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("T2 GPU Control")
        .default_width(520)
        .default_height(720)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&adw::HeaderBar::new());

    let page = adw::PreferencesPage::new();
    let status_group = adw::PreferencesGroup::builder().title("GPU status").build();
    let current_row = adw::ActionRow::builder()
        .title("Current active GPU")
        .subtitle(current.label())
        .build();
    let next_row = adw::ActionRow::builder()
        .title("Configured boot GPU")
        .subtitle(configured.label())
        .build();
    let power_row = adw::ActionRow::builder()
        .title("Discrete GPU at next boot")
        .subtitle(if configured_power_off {
            "Will be powered off"
        } else {
            "Remains powered on"
        })
        .build();
    let discrete_state_row = adw::ActionRow::builder()
        .title("Discrete GPU state")
        .subtitle(discrete_state)
        .build();
    status_group.add(&current_row);
    status_group.add(&next_row);
    status_group.add(&power_row);
    status_group.add(&discrete_state_row);
    page.add(&status_group);

    let config_group = adw::PreferencesGroup::builder()
        .title("Next boot")
        .description("Choose the primary GPU and whether the discrete GPU remains available.")
        .build();

    let boot_row = adw::ActionRow::builder().title("Primary GPU").build();
    let choices = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    choices.add_css_class("linked");
    choices.set_valign(gtk::Align::Center);
    let integrated = gtk::ToggleButton::with_label("Integrated");
    let discrete = gtk::ToggleButton::with_label("Discrete");
    discrete.set_group(Some(&integrated));
    choices.append(&integrated);
    choices.append(&discrete);
    boot_row.add_suffix(&choices);
    config_group.add(&boot_row);

    let power_off_row = adw::ActionRow::builder()
        .title("Power off discrete GPU")
        .subtitle("Reduces idle power use; the dGPU remains unavailable until the next reboot.")
        .build();
    let power_off = gtk::Switch::new();
    power_off.set_valign(gtk::Align::Center);
    power_off_row.add_suffix(&power_off);
    power_off_row.set_activatable_widget(Some(&power_off));
    config_group.add(&power_off_row);

    let power_saving_row = adw::ActionRow::builder()
        .title("AMDGPU power-saving profile")
        .subtitle("Applies the driver power-saving profile after boot and resume.")
        .build();
    let power_saving = gtk::Switch::new();
    power_saving.set_valign(gtk::Align::Center);
    power_saving_row.add_suffix(&power_saving);
    power_saving_row.set_activatable_widget(Some(&power_saving));
    config_group.add(&power_saving_row);
    page.add(&config_group);

    let cap_group = adw::PreferencesGroup::builder()
        .title("AMDGPU power cap")
        .build();
    let power_cap = read_power_cap();
    let cap_row = adw::ActionRow::builder()
        .title("Maximum GPU power")
        .subtitle(if let Some(cap) = power_cap {
            format!("{:.0} W / {:.0} W default", cap.current_w, cap.default_w)
        } else {
            "Unavailable while the dGPU is off".into()
        })
        .build();
    let cap_scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        10.0,
        power_cap.map_or(10.0, |cap| cap.default_w),
        1.0,
    );
    cap_scale.set_draw_value(false);
    cap_scale.set_size_request(220, -1);
    cap_scale.set_valign(gtk::Align::Center);
    cap_scale.set_sensitive(power_cap.is_some());
    if let Some(cap) = power_cap {
        cap_scale.set_value(cap.current_w);
    }
    cap_row.add_suffix(&cap_scale);
    cap_group.add(&cap_row);
    page.add(&cap_group);

    let actions_group = adw::PreferencesGroup::new();
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::End);
    let apply = gtk::Button::with_label("Apply Changes");
    apply.add_css_class("suggested-action");
    let reboot = gtk::Button::with_label("Reboot");
    reboot.set_sensitive(false);
    buttons.append(&apply);
    buttons.append(&reboot);
    let message = gtk::Label::new(None);
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.set_visible(false);
    actions.append(&message);
    actions.append(&buttons);
    actions_group.add(&actions);
    page.add(&actions_group);

    {
        let generation = Rc::new(Cell::new(0_u64));
        let cap_row = cap_row.clone();
        let message = message.clone();
        cap_scale.connect_value_changed(move |scale| {
            let value = scale.value().round();
            cap_row.set_subtitle(&format!("{value:.0} W selected"));
            let next = generation.get() + 1;
            generation.set(next);
            let generation = generation.clone();
            let cap_row = cap_row.clone();
            let message = message.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(700), move || {
                if generation.get() != next {
                    return;
                }
                let microwatts = format!("{:.0}", value * 1_000_000.0);
                match run_helper(&["set-power-cap", &microwatts]) {
                    Ok(()) => {
                        cap_row.set_subtitle(&format!("{value:.0} W applied"));
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

    let selected_gpu = Rc::new(Cell::new(initial_gpu));
    let saved_gpu = Rc::new(Cell::new(configured));
    let saved_power_off = Rc::new(Cell::new(configured_power_off));
    let saved_suspend_restore = Rc::new(Cell::new(configured_suspend_restore));
    let saved_power_saving = Rc::new(Cell::new(configured_power_saving));

    match initial_gpu {
        Gpu::Discrete => discrete.set_active(true),
        _ => integrated.set_active(true),
    }
    power_off.set_active(configured_power_off && initial_gpu == Gpu::Integrated);
    power_off.set_sensitive(initial_gpu == Gpu::Integrated);
    power_saving.set_active(configured_power_saving && !power_off.is_active());
    power_saving.set_sensitive(!power_off.is_active());

    let update_apply = {
        let apply = apply.clone();
        let message = message.clone();
        let power_off = power_off.clone();
        let selected_gpu = selected_gpu.clone();
        let saved_gpu = saved_gpu.clone();
        let saved_power_off = saved_power_off.clone();
        let saved_suspend_restore = saved_suspend_restore.clone();
        let power_saving = power_saving.clone();
        let saved_power_saving = saved_power_saving.clone();
        let reboot = reboot.clone();
        move || {
            let gpu = selected_gpu.get();
            let changed = gpu != saved_gpu.get()
                || power_off.is_active() != saved_power_off.get()
                || power_off.is_active() != saved_suspend_restore.get()
                || power_saving.is_active() != saved_power_saving.get();
            apply.set_sensitive(changed);
            if changed {
                message.set_visible(false);
                reboot.set_sensitive(false);
            }
        }
    };

    {
        let selected_gpu = selected_gpu.clone();
        let power_off = power_off.clone();
        let update_apply = update_apply.clone();
        integrated.connect_toggled(move |button| {
            if button.is_active() {
                selected_gpu.set(Gpu::Integrated);
                power_off.set_sensitive(true);
                update_apply();
            }
        });
    }
    {
        let selected_gpu = selected_gpu.clone();
        let power_off = power_off.clone();
        let power_saving = power_saving.clone();
        let update_apply = update_apply.clone();
        discrete.connect_toggled(move |button| {
            if button.is_active() {
                selected_gpu.set(Gpu::Discrete);
                power_off.set_active(false);
                power_off.set_sensitive(false);
                power_saving.set_active(false);
                power_saving.set_sensitive(true);
                update_apply();
            }
        });
    }
    {
        let power_saving = power_saving.clone();
        let update_apply = update_apply.clone();
        power_off.connect_active_notify(move |button| {
            if button.is_active() {
                power_saving.set_active(false);
                power_saving.set_sensitive(false);
            } else {
                power_saving.set_sensitive(true);
            }
            update_apply();
        });
    }
    {
        let update_apply = update_apply.clone();
        power_saving.connect_active_notify(move |_| update_apply());
    }
    update_apply();

    {
        let selected_gpu = selected_gpu.clone();
        let saved_gpu = saved_gpu.clone();
        let saved_power_off = saved_power_off.clone();
        let saved_suspend_restore = saved_suspend_restore.clone();
        let saved_power_saving = saved_power_saving.clone();
        let power_off = power_off.clone();
        let power_saving = power_saving.clone();
        let message = message.clone();
        let reboot = reboot.clone();
        let next_row = next_row.clone();
        let power_row = power_row.clone();
        apply.connect_clicked(move |button| {
            let gpu = selected_gpu.get();
            let power = power_off.is_active();
            let profile = power_saving.is_active();
            button.set_sensitive(false);
            message.set_visible(true);
            message.set_label("Applying GPU configuration...");
            message.remove_css_class("error");
            message.remove_css_class("success");

            let result = match gpu {
                Gpu::Integrated => run_helper(&[
                    "apply",
                    "integrated",
                    if power { "power-off" } else { "keep-powered" },
                    if profile {
                        "power-saving"
                    } else {
                        "default-profile"
                    },
                ]),
                Gpu::Discrete => run_helper(&[
                    "apply",
                    "discrete",
                    "keep-powered",
                    if profile {
                        "power-saving"
                    } else {
                        "default-profile"
                    },
                ]),
                Gpu::Unknown => Err("No boot GPU was selected.".into()),
            };

            match result {
                Ok(()) => {
                    saved_gpu.set(gpu);
                    saved_power_off.set(power);
                    saved_suspend_restore.set(power);
                    saved_power_saving.set(profile);
                    next_row.set_subtitle(gpu.label());
                    power_row.set_subtitle(if power {
                        "Will be powered off"
                    } else {
                        "Remains powered on"
                    });
                    set_status(
                        &message,
                        "Changes applied. Reboot to use the new configuration.",
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
            message.set_visible(true);
            if let Err(error) = run_helper(&["reboot"]) {
                set_status(&message, &error, true);
            }
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
