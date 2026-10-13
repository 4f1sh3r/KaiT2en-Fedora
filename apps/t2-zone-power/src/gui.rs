use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Duration};

use adw::StyleManager;
use gtk4::{
    cairo, gdk, gio, glib, prelude::*, Align, Application, ApplicationWindow, Box as GtkBox, CssProvider,
    DrawingArea, Grid, Label, LinkButton, Orientation,
};

use crate::{
    plimit::PLimits,
    sensors::{Applied, SmcSensors},
    sysfs::{find_t2smc, glob_paths, read_string},
};

const APP_ID: &str = "org.t2zonepower.gtk";
const APP_VERSION: &str = "0.01";
const HISTORY_CAPACITY: usize = 90;

fn kait2en_brand() -> DrawingArea {
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file("/usr/local/share/kait2en/kait2en-wordmark.png")
        .expect("failed to load kait2en wordmark");
    let brand = DrawingArea::new();
    brand.set_content_width(80); brand.set_content_height(21); brand.set_size_request(80, 21);
    brand.set_draw_func(move |_area, context, width, height| {
        let scale = f64::min(width as f64 / pixbuf.width() as f64, height as f64 / pixbuf.height() as f64);
        let _ = context.save();
        context.translate((width as f64 - pixbuf.width() as f64 * scale) / 2.0, (height as f64 - pixbuf.height() as f64 * scale) / 2.0);
        context.scale(scale, scale); context.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = context.paint(); let _ = context.restore();
    });
    brand
}

#[derive(Clone, Copy)]
struct ThemePalette {
    window_bg: &'static str,
    window_fg: &'static str,
    meta_fg: &'static str,
    panel_fill: (f64, f64, f64),
    grid: (f64, f64, f64),
    heatpipe: (f64, f64, f64),
    cpu: (f64, f64, f64),
    gpu: (f64, f64, f64),
    idle: (f64, f64, f64),
    fin: (f64, f64, f64),
    igpu: (f64, f64, f64),
    label: (f64, f64, f64),
}

const DARK_PALETTE: ThemePalette = ThemePalette {
    window_bg: "#161616",
    window_fg: "#e8e8e8",
    meta_fg: "rgba(230,230,230,0.62)",
    panel_fill: (0.063, 0.063, 0.063),
    grid: (0.28, 0.30, 0.32),
    heatpipe: (0.68, 0.82, 0.42),
    cpu: (0.92, 0.54, 0.28),
    gpu: (0.34, 0.60, 0.86),
    idle: (0.90, 0.28, 0.22),
    fin: (0.78, 0.52, 0.88),
    igpu: (0.40, 0.78, 0.72),
    label: (0.56, 0.56, 0.56),
};

const LIGHT_PALETTE: ThemePalette = ThemePalette {
    window_bg: "#f2f2f2",
    window_fg: "#242424",
    meta_fg: "rgba(36,36,36,0.68)",
    panel_fill: (0.91, 0.91, 0.91),
    grid: (0.72, 0.72, 0.72),
    heatpipe: (0.42, 0.62, 0.18),
    cpu: (0.86, 0.43, 0.18),
    gpu: (0.20, 0.45, 0.75),
    idle: (0.80, 0.18, 0.14),
    fin: (0.55, 0.30, 0.68),
    igpu: (0.15, 0.55, 0.50),
    label: (0.096, 0.096, 0.096),
};

/// One second of SMC zones and applied limits
#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    /// PZ*G is the short average, PZ*F the filtered value the SMC holds at the target
    heatpipe_w: Option<f64>,
    heatpipe_filtered_w: Option<f64>,
    heatpipe_target_w: Option<f64>,
    cpu_w: Option<f64>,
    cpu_filtered_w: Option<f64>,
    cpu_target_w: Option<f64>,
    gpu_w: Option<f64>,
    gpu_filtered_w: Option<f64>,
    gpu_target_w: Option<f64>,
    heatpipe_throttle: Option<f64>,
    cpu_throttle: Option<f64>,
    gpu_throttle: Option<f64>,
    cpu_temp_c: Option<f64>,
    gpu_temp_c: Option<f64>,
    fin_temp_c: Option<f64>,
    plimits: Option<PLimits>,
    applied: Applied,
}

impl Sample {
    fn read(sensors: &SmcSensors) -> Self {
        let plimits = sensors.plimits();
        let fin = [sensors.celsius("Th1H"), sensors.celsius("Th2H")].into_iter().flatten().reduce(f64::max);
        Self {
            heatpipe_w: sensors.micro("PZ0G"),
            heatpipe_filtered_w: sensors.micro("PZ0F"),
            heatpipe_target_w: sensors.micro("PZ0E"),
            cpu_w: sensors.micro("PZ1G"),
            cpu_filtered_w: sensors.micro("PZ1F"),
            cpu_target_w: sensors.micro("PZ1E"),
            gpu_w: sensors.micro("PZ2G"),
            gpu_filtered_w: sensors.micro("PZ2F"),
            gpu_target_w: sensors.micro("PZ2E"),
            heatpipe_throttle: sensors.micro("PZ0T"),
            cpu_throttle: sensors.micro("PZ1T"),
            gpu_throttle: sensors.micro("PZ2T"),
            cpu_temp_c: sensors.celsius("TCXC"),
            gpu_temp_c: sensors.celsius("TG0P"),
            fin_temp_c: fin,
            plimits,
            applied: Applied::read(sensors.dgpu(), plimits.map(|plimits| plimits.dgpu.step)),
        }
    }

    fn cpu_limit_percent(&self) -> Option<f64> {
        let applied = self.applied;
        Some(f64::from(applied.cpu_cap_mhz?) * 100.0 / f64::from(applied.cpu_max_mhz?))
    }

    fn igpu_limit_percent(&self) -> Option<f64> {
        let applied = self.applied;
        Some(f64::from(applied.igpu_cap_mhz?) * 100.0 / f64::from(applied.igpu_max_mhz?))
    }

    fn dgpu_limit_percent(&self) -> Option<f64> {
        let applied = self.applied;
        Some(f64::from(applied.dgpu_cap_w?) * 100.0 / f64::from(applied.dgpu_tdp_w?))
    }
}

struct AppModel {
    sensors: Option<SmcSensors>,
    history: VecDeque<Sample>,
}

impl AppModel {
    fn new() -> Self {
        let mut model = Self {
            sensors: find_t2smc().ok().map(|t2smc| SmcSensors::discover(&t2smc)),
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        };
        model.tick();
        model
    }

    fn tick(&mut self) {
        let Some(sensors) = &self.sensors else { return };
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(Sample::read(sensors));
    }

    fn latest(&self) -> Sample {
        self.history.back().copied().unwrap_or_default()
    }

    fn series(&self, value: impl Fn(&Sample) -> Option<f64>) -> Vec<Option<f64>> {
        self.history.iter().map(value).collect()
    }
}

struct UiRefs {
    heatpipe: Label,
    cpu_zone: Label,
    gpu_zone: Label,
    fin_stack: Label,
    cpu_step: Label,
    dgpu_step: Label,
    idle_step: Label,
    igpu_step: Label,
    power_graph: DrawingArea,
    throttle_graph: DrawingArea,
    limit_graph: DrawingArea,
    temperature_graph: DrawingArea,
}

pub fn run() -> glib::ExitCode {
    register_embedded_resources();
    let app = Application::builder()
        .application_id(APP_ID)
        .resource_base_path("/org/t2zonepower/gtk")
        .build();
    app.connect_activate(build_ui);
    app.run_with_args::<&str>(&[])
}

fn build_ui(app: &Application) {
    let css_provider = install_css();
    let model = Rc::new(RefCell::new(AppModel::new()));
    let ui = Rc::new(build_widgets());

    let window = ApplicationWindow::builder()
        .application(app)
        .title("T2 Zone Power")
        .default_width(480)
        .default_height(720)
        .build();
    window.set_resizable(true);

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.set_margin_start(8);
    root.set_margin_end(8);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Zone Power", "")));
    let brand = kait2en_brand();
    brand.set_margin_start(10);
    brand.set_margin_end(10);
    header.pack_start(&brand);
    window.set_titlebar(Some(&header));

    let summary = Grid::builder()
        .column_spacing(12)
        .row_spacing(4)
        .column_homogeneous(true)
        .hexpand(true)
        .build();
    attach_pair_at(&summary, 0, 0, "Heatpipe", &ui.heatpipe);
    attach_pair_at(&summary, 2, 0, "CPU zone", &ui.cpu_zone);
    // Zone 2 follows the dGPU rail on Macs with a dGPU, on others it is something else
    let (zone2, zone2_key) = if has_dgpu() { ("dGPU", "dGPU zone") } else { ("Zone 2", "Zone 2") };
    attach_pair_at(&summary, 0, 1, zone2_key, &ui.gpu_zone);
    attach_pair_at(&summary, 2, 1, "Fin stack", &ui.fin_stack);
    attach_pair_at(&summary, 0, 2, "CPU step", &ui.cpu_step);
    attach_pair_at(&summary, 2, 2, "dGPU step", &ui.dgpu_step);
    attach_pair_at(&summary, 0, 3, "Idle step", &ui.idle_step);
    attach_pair_at(&summary, 2, 3, "iGPU step", &ui.igpu_step);
    root.append(&summary);

    let zone_legend = format!("Heatpipe  •  CPU  •  {zone2}");
    root.append(&panel("Zone power", &ui.power_graph, &zone_legend));
    root.append(&panel("Zone throttle", &ui.throttle_graph, &zone_legend));
    root.append(&panel("Applied limits", &ui.limit_graph, "CPU clock  •  iGPU clock  •  dGPU power  •  Forced idle"));
    root.append(&panel("Temperatures", &ui.temperature_graph, "CPU  •  dGPU  •  Fin stack"));

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    let donate = LinkButton::builder()
        .uri("https://donate.stripe.com/eVq14n8a7agh2lQdqq14400")
        .label("Fund our bugs")
        .build();
    donate.set_halign(Align::Start);
    donate.add_css_class("footer-link");
    footer.append(&donate);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    footer.append(&spacer);
    let version = Label::new(Some(&format!("v{APP_VERSION}")));
    version.set_halign(Align::End);
    version.add_css_class("footer-version");
    footer.append(&version);
    root.append(&footer);

    window.set_child(Some(&root));
    watch_theme_changes(&css_provider, &ui);
    connect_drawings(&model, &ui);
    sync_ui(&model.borrow(), &ui);

    {
        let model = model.clone();
        let ui = ui.clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let mut model = model.borrow_mut();
            model.tick();
            sync_ui(&model, &ui);
            glib::ControlFlow::Continue
        });
    }

    window.present();
}

fn build_widgets() -> UiRefs {
    let make_value = || {
        let label = Label::new(None);
        label.set_halign(Align::Start);
        label.set_xalign(0.0);
        label.set_max_width_chars(18);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.add_css_class("metric-value");
        label
    };
    let make_graph = || {
        let graph = DrawingArea::new();
        graph.set_content_width(248);
        graph.set_content_height(88);
        graph.set_vexpand(false);
        graph.set_hexpand(true);
        graph
    };

    UiRefs {
        heatpipe: make_value(),
        cpu_zone: make_value(),
        gpu_zone: make_value(),
        fin_stack: make_value(),
        cpu_step: make_value(),
        dgpu_step: make_value(),
        idle_step: make_value(),
        igpu_step: make_value(),
        power_graph: make_graph(),
        throttle_graph: make_graph(),
        limit_graph: make_graph(),
        temperature_graph: make_graph(),
    }
}

fn connect_drawings(model: &Rc<RefCell<AppModel>>, ui: &Rc<UiRefs>) {
    {
        let model = model.clone();
        ui.power_graph.set_draw_func(move |_area, cr, width, height| {
            let model = model.borrow();
            let palette = current_palette();
            let latest = model.latest();
            let plot = draw_history_panel(cr, width as f64, height as f64, "W");
            draw_target(cr, plot, latest.heatpipe_target_w, palette.heatpipe);
            draw_target(cr, plot, latest.cpu_target_w, palette.cpu);
            draw_target(cr, plot, latest.gpu_target_w, palette.gpu);
            draw_history_line(cr, plot, &model.series(|s| s.heatpipe_w), palette.heatpipe);
            draw_history_line(cr, plot, &model.series(|s| s.cpu_w), palette.cpu);
            draw_history_line(cr, plot, &model.series(|s| s.gpu_w), palette.gpu);
            draw_history_series(cr, plot, &model.series(|s| s.heatpipe_filtered_w), palette.heatpipe);
            draw_history_series(cr, plot, &model.series(|s| s.cpu_filtered_w), palette.cpu);
            draw_history_series(cr, plot, &model.series(|s| s.gpu_filtered_w), palette.gpu);
        });
    }
    {
        let model = model.clone();
        ui.throttle_graph.set_draw_func(move |_area, cr, width, height| {
            let model = model.borrow();
            let palette = current_palette();
            let plot = draw_history_panel(cr, width as f64, height as f64, "%");
            draw_history_series(cr, plot, &model.series(|s| s.heatpipe_throttle), palette.heatpipe);
            draw_history_series(cr, plot, &model.series(|s| s.cpu_throttle), palette.cpu);
            draw_history_series(cr, plot, &model.series(|s| s.gpu_throttle), palette.gpu);
        });
    }
    {
        let model = model.clone();
        ui.limit_graph.set_draw_func(move |_area, cr, width, height| {
            let model = model.borrow();
            let palette = current_palette();
            let plot = draw_history_panel(cr, width as f64, height as f64, "%");
            draw_history_series(cr, plot, &model.series(Sample::cpu_limit_percent), palette.cpu);
            draw_history_series(cr, plot, &model.series(Sample::igpu_limit_percent), palette.igpu);
            draw_history_series(cr, plot, &model.series(Sample::dgpu_limit_percent), palette.gpu);
            draw_history_series(cr, plot, &model.series(|s| s.applied.idle_percent.map(f64::from)), palette.idle);
        });
    }
    {
        let model = model.clone();
        ui.temperature_graph.set_draw_func(move |_area, cr, width, height| {
            let model = model.borrow();
            let palette = current_palette();
            let plot = draw_history_panel(cr, width as f64, height as f64, " C");
            draw_history_series(cr, plot, &model.series(|s| s.cpu_temp_c), palette.cpu);
            draw_history_series(cr, plot, &model.series(|s| s.gpu_temp_c), palette.gpu);
            draw_history_series(cr, plot, &model.series(|s| s.fin_temp_c), palette.fin);
        });
    }
}

fn sync_ui(model: &AppModel, ui: &UiRefs) {
    let sample = model.latest();
    let applied = sample.applied;
    ui.heatpipe.set_label(&format_zone(sample.heatpipe_filtered_w, sample.heatpipe_target_w));
    ui.cpu_zone.set_label(&format_zone(sample.cpu_filtered_w, sample.cpu_target_w));
    ui.gpu_zone.set_label(&format_zone(sample.gpu_filtered_w, sample.gpu_target_w));
    ui.fin_stack.set_label(&format_value(sample.fin_temp_c, " C"));

    let plimits = sample.plimits.unwrap_or_default();
    ui.cpu_step.set_label(&format_step(plimits.cpu.step, plimits.cpu.hard, applied.cpu_cap_mhz, " MHz"));
    ui.dgpu_step.set_label(&format_step(plimits.dgpu.step, plimits.dgpu.hard, applied.dgpu_cap_w, " W"));
    ui.idle_step.set_label(&format_step(plimits.idle.step, plimits.idle.hard, applied.idle_percent, " %"));
    ui.igpu_step.set_label(&format_step(plimits.igpu.step, plimits.igpu.hard, applied.igpu_cap_mhz, " MHz"));

    ui.power_graph.queue_draw();
    ui.throttle_graph.queue_draw();
    ui.limit_graph.queue_draw();
    ui.temperature_graph.queue_draw();
}

fn format_value(value: Option<f64>, unit: &str) -> String {
    value.map(|value| format!("{value:.0}{unit}")).unwrap_or_else(|| String::from("unavailable"))
}

fn format_zone(value: Option<f64>, target: Option<f64>) -> String {
    match (value, target) {
        (Some(value), Some(target)) => format!("{value:.0} / {target:.0} W"),
        (Some(value), None) => format!("{value:.0} W"),
        _ => String::from("unavailable"),
    }
}

fn format_step(step: u8, hard: bool, applied: Option<u32>, unit: &str) -> String {
    let hard = if hard { "!" } else { "" };
    match applied {
        Some(applied) => format!("{step}{hard} → {applied}{unit}"),
        None => format!("{step}{hard}"),
    }
}

fn has_dgpu() -> bool {
    glob_paths("/sys/class/drm/card*/device/hwmon/hwmon*/name")
        .unwrap_or_default()
        .iter()
        .any(|name| read_string(name).is_ok_and(|name| name == "amdgpu"))
}

fn panel(title: &str, widget: &impl IsA<gtk4::Widget>, legend: &str) -> GtkBox {
    let box_ = GtkBox::new(Orientation::Vertical, 3);
    let label = Label::new(Some(title));
    label.set_halign(Align::Start);
    label.add_css_class("panel-title");
    box_.append(&label);
    box_.append(widget);
    let legend_label = Label::new(None);
    legend_label.set_halign(Align::Start);
    legend_label.set_xalign(0.0);
    legend_label.add_css_class("meta-text");
    legend_label.set_markup(&legend_markup(legend));
    box_.append(&legend_label);
    box_
}

fn attach_pair_at(grid: &Grid, column: i32, row: i32, title: &str, value: &Label) {
    let key = Label::new(Some(title));
    key.set_halign(Align::Start);
    key.set_xalign(0.0);
    key.add_css_class("metric-key");
    key.set_hexpand(true);
    key.set_max_width_chars(12);
    key.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    key.set_tooltip_text(Some(title));
    value.set_hexpand(true);
    value.set_max_width_chars(14);
    value.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    grid.attach(&key, column, row, 1, 1);
    grid.attach(value, column + 1, row, 1, 1);
}

/// Fills the panel, draws the grid and returns the plot area. Every graph
/// runs from 0 to 100 in its unit.
fn draw_history_panel(cr: &cairo::Context, width: f64, height: f64, unit: &str) -> (f64, f64, f64, f64) {
    let palette = current_palette();
    cr.set_source_rgb(palette.panel_fill.0, palette.panel_fill.1, palette.panel_fill.2);
    cr.rectangle(0.5, 0.5, width - 1.0, height - 1.0);
    let _ = cr.fill();
    let plot = plot_rect(width, height);
    draw_grid(cr, plot);
    draw_label(cr, plot.0, plot.1 + 8.0, &format!("100{unit}"));
    draw_label(cr, plot.0, plot.1 + plot.3 - 2.0, &format!("0{unit}"));
    plot
}

fn draw_grid(cr: &cairo::Context, plot: (f64, f64, f64, f64)) {
    set_color(cr, current_palette().grid);
    cr.set_line_width(0.8);
    for step in 1..4 {
        let x = plot.0 + plot.2 * (step as f64 / 4.0);
        let y = plot.1 + plot.3 * (step as f64 / 4.0);
        cr.move_to(x, plot.1);
        cr.line_to(x, plot.1 + plot.3);
        cr.move_to(plot.0, y);
        cr.line_to(plot.0 + plot.2, y);
    }
    let _ = cr.stroke();
}

/// Dashed line for a zone target
fn draw_target(cr: &cairo::Context, plot: (f64, f64, f64, f64), target: Option<f64>, color: (f64, f64, f64)) {
    let Some(target) = target else { return };
    let y = remap(target.clamp(0.0, 100.0), 0.0, 100.0, plot.1 + plot.3, plot.1);
    cr.set_source_rgba(color.0, color.1, color.2, 0.7);
    cr.set_line_width(1.0);
    cr.set_dash(&[4.0, 3.0], 0.0);
    cr.move_to(plot.0, y);
    cr.line_to(plot.0 + plot.2, y);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
}

fn draw_history_series(cr: &cairo::Context, plot: (f64, f64, f64, f64), values: &[Option<f64>], color: (f64, f64, f64)) {
    let denominator = (HISTORY_CAPACITY.saturating_sub(1)).max(1) as f64;
    let start_index = HISTORY_CAPACITY.saturating_sub(values.len());
    let point = |index: usize, value: f64| {
        (
            plot.0 + plot.2 * ((start_index + index) as f64 / denominator),
            remap(value.clamp(0.0, 100.0), 0.0, 100.0, plot.1 + plot.3, plot.1),
        )
    };

    cr.set_source_rgba(color.0, color.1, color.2, 0.18);
    let mut filled = false;
    for (index, value) in values.iter().enumerate() {
        let Some(value) = value else { continue };
        let (x, y) = point(index, *value);
        if !filled {
            cr.move_to(x, plot.1 + plot.3);
            filled = true;
        }
        cr.line_to(x, y);
    }
    if filled {
        cr.line_to(plot.0 + plot.2, plot.1 + plot.3);
        cr.close_path();
        let _ = cr.fill();
    }

    set_color(cr, color);
    cr.set_line_width(2.0);
    let mut first = true;
    for (index, value) in values.iter().enumerate() {
        let Some(value) = value else {
            first = true;
            continue;
        };
        let (x, y) = point(index, *value);
        if first {
            cr.move_to(x, y);
            first = false;
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();
}

/// Thin pale line without fill for the short average behind the filtered value
fn draw_history_line(cr: &cairo::Context, plot: (f64, f64, f64, f64), values: &[Option<f64>], color: (f64, f64, f64)) {
    let denominator = (HISTORY_CAPACITY.saturating_sub(1)).max(1) as f64;
    let start_index = HISTORY_CAPACITY.saturating_sub(values.len());
    cr.set_source_rgba(color.0, color.1, color.2, 0.45);
    cr.set_line_width(1.0);
    let mut first = true;
    for (index, value) in values.iter().enumerate() {
        let Some(value) = value else {
            first = true;
            continue;
        };
        let x = plot.0 + plot.2 * ((start_index + index) as f64 / denominator);
        let y = remap(value.clamp(0.0, 100.0), 0.0, 100.0, plot.1 + plot.3, plot.1);
        if first {
            cr.move_to(x, y);
            first = false;
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();
}

fn plot_rect(width: f64, height: f64) -> (f64, f64, f64, f64) {
    (10.0, 10.0, (width - 20.0).max(10.0), (height - 20.0).max(10.0))
}

fn remap(value: f64, src_min: f64, src_max: f64, dst_min: f64, dst_max: f64) -> f64 {
    if (src_max - src_min).abs() < f64::EPSILON {
        return dst_min;
    }
    dst_min + (value - src_min) * (dst_max - dst_min) / (src_max - src_min)
}

fn set_color(cr: &cairo::Context, color: (f64, f64, f64)) {
    cr.set_source_rgb(color.0, color.1, color.2);
}

fn legend_markup(legend: &str) -> String {
    let palette = current_palette();
    legend
        .split("  •  ")
        .map(|item| {
            let dot = match item {
                "Heatpipe" => palette.heatpipe,
                "CPU" | "CPU clock" => palette.cpu,
                "dGPU" | "dGPU power" | "Zone 2" => palette.gpu,
                "Forced idle" => palette.idle,
                "iGPU clock" => palette.igpu,
                "Fin stack" => palette.fin,
                _ => palette.label,
            };
            format!("<span foreground=\"{}\">●</span> {}", rgb_to_hex(dot), glib::markup_escape_text(item))
        })
        .collect::<Vec<_>>()
        .join("    ")
}

fn rgb_to_hex(color: (f64, f64, f64)) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (color.0 * 255.0).round() as u8,
        (color.1 * 255.0).round() as u8,
        (color.2 * 255.0).round() as u8
    )
}

fn draw_label(cr: &cairo::Context, x: f64, y: f64, text: &str) {
    let palette = current_palette();
    cr.select_font_face("JetBrains Mono", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(11.0);
    cr.set_source_rgba(palette.label.0, palette.label.1, palette.label.2, 1.0);
    cr.move_to(x, y);
    let _ = cr.show_text(text);
}

fn install_css() -> CssProvider {
    let provider = CssProvider::new();
    provider.load_from_data(&build_css(current_palette()));
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    provider
}

fn register_embedded_resources() {
    gio::resources_register_include!("t2-zonepower.gresource").expect("failed to register embedded GTK resources");
}

fn current_palette() -> ThemePalette {
    if StyleManager::default().is_dark() { DARK_PALETTE } else { LIGHT_PALETTE }
}

fn build_css(palette: ThemePalette) -> String {
    format!(
        "window, popover {{
  background: {};
  color: {};
  font-family: \"JetBrains Mono\";
  font-size: 11pt;
  font-weight: 400;
}}
headerbar {{
  background: @headerbar_bg_color;
}}
.panel-title {{
  color: {};
  font-size: 11pt;
  font-weight: 400;
}}
.meta-text {{
  color: {};
  font-size: 11pt;
}}
.metric-key,
.metric-value {{
  color: {};
}}
.metric-value {{
  font-weight: 400;
  font-feature-settings: \"tnum\" 1;
}}
.footer-link,
.footer-version {{
  color: {};
  font-size: 11pt;
  font-weight: 400;
}}",
        palette.window_bg,
        palette.meta_fg,
        palette.window_fg,
        palette.meta_fg,
        palette.meta_fg,
        palette.meta_fg,
    )
}

fn watch_theme_changes(provider: &CssProvider, ui: &Rc<UiRefs>) {
    let provider = provider.clone();
    let ui = ui.clone();
    StyleManager::default().connect_dark_notify(move |_| {
        provider.load_from_data(&build_css(current_palette()));
        ui.power_graph.queue_draw();
        ui.throttle_graph.queue_draw();
        ui.limit_graph.queue_draw();
        ui.temperature_graph.queue_draw();
    });
}
