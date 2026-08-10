#!/usr/bin/env python3
import os
import signal
import statistics
import subprocess
import threading
import time
from collections import deque

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk

APP_ID = "org.t2cpucontrol.gtk"
HELPER = "/usr/local/libexec/t2-cpu-control-helper"
STATUS = "/usr/local/libexec/t2-cpu-control-status"
BENCHMARK = "/usr/local/libexec/t2-cpu-kernel-benchmark"
STATUS_CACHE = "/run/t2-cpu-control/status"
HISTORY = 180


def command(*args, check=True):
    return subprocess.run(args, text=True, capture_output=True, check=check)


def thermald_status():
    if command("systemctl", "is-active", "--quiet", "thermald.service", check=False).returncode == 0:
        return "active"
    if command("systemctl", "is-enabled", "--quiet", "thermald.service", check=False).returncode == 0:
        return "stopped"
    return "disabled"


def read_status():
    with open(STATUS_CACHE, encoding="ascii") as status_file:
        output = status_file.read()
    data, cores = {}, []
    for line in output.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if key.startswith("cpu."):
            mhz, temp, thermal, prochot = map(int, value.split(","))
            cores.append((int(key[4:]), mhz, temp, bool(thermal), bool(prochot)))
        else:
            data[key] = value
    data["cores"] = sorted(cores)
    return data


class HistoryGraph(Gtk.DrawingArea):
    def __init__(self, title, unit, colors, fixed_max=None, line_width=1.8):
        super().__init__()
        self.title, self.unit, self.colors, self.fixed_max = title, unit, colors, fixed_max
        self.line_width = line_width
        self.series = []
        self.current_value = None
        self.set_content_height(190)
        self.set_hexpand(True)
        self.set_draw_func(self.draw)

    def update(self, series, current_value=None):
        self.series = series
        self.current_value = current_value
        self.queue_draw()

    def draw(self, _area, cr, width, height):
        dark = Adw.StyleManager.get_default().get_dark()
        bg = (0.10, 0.10, 0.11) if dark else (0.96, 0.95, 0.92)
        fg = (0.90, 0.90, 0.90) if dark else (0.15, 0.15, 0.15)
        grid = (0.30, 0.30, 0.32) if dark else (0.78, 0.76, 0.72)
        cr.set_source_rgb(*bg); cr.paint()
        left, top, right, bottom = 48, 28, width - 12, height - 24
        cr.set_line_width(1); cr.set_source_rgb(*grid)
        for i in range(5):
            y = top + (bottom - top) * i / 4
            cr.move_to(left, y); cr.line_to(right, y)
        cr.stroke()
        values = [v for _, vals in self.series for v in vals if v is not None]
        maximum = self.fixed_max or (max(values, default=1) * 1.1)
        maximum = max(maximum, 1)
        cr.set_source_rgb(*fg); cr.select_font_face("Sans", 0, 0); cr.set_font_size(13)
        cr.move_to(12, 18); cr.show_text(self.title)
        if self.current_value is not None:
            value = f"{self.current_value:.0f}{self.unit}"
            extents = cr.text_extents(value)
            cr.move_to(width - extents.width - 14, 18)
            cr.show_text(value)
        cr.set_font_size(10); cr.move_to(8, top + 4); cr.show_text(f"{maximum:.0f}{self.unit}")
        cr.move_to(18, bottom); cr.show_text(f"0{self.unit}")
        for idx, (label, vals) in enumerate(self.series):
            if len(vals) < 2: continue
            cr.set_source_rgb(*self.colors[idx % len(self.colors)]); cr.set_line_width(self.line_width)
            started = False
            for i, value in enumerate(vals):
                if value is None: started = False; continue
                x = left + (right - left) * i / max(HISTORY - 1, 1)
                y = bottom - min(value / maximum, 1.0) * (bottom - top)
                (cr.line_to if started else cr.move_to)(x, y); started = True
            cr.stroke()
            if len(self.series) <= 6:
                cr.move_to(left + idx * 115, height - 7); cr.show_text(label)


class CpuControl(Adw.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID)
        self.connect("activate", self.activate)
        self.data = {}
        self.history_power = deque(maxlen=HISTORY)
        self.history_package_temp = deque(maxlen=HISTORY)
        self.last_energy = None
        self.last_energy_time = None
        self.calibrating = False
        self.calibration_persist = 0
        self.notice_until = 0
        self.control_update = False
        self.manual_apply_source = None
        self.cpu_rows = []
        self.controls_initialized = False
        self.cancel_event = threading.Event()
        self.status_monitor = None
        self.thermald_state = "unknown"

    def activate(self, _app):
        self.thermald_state = thermald_status()
        if self.status_monitor is None or self.status_monitor.poll() is not None:
            self.status_monitor = subprocess.Popen([
                "pkexec", "--disable-internal-agent", STATUS, "monitor", str(os.getpid())
            ])
        if self.manual_apply_source is not None:
            GLib.source_remove(self.manual_apply_source)
            self.manual_apply_source = None
        self.controls_initialized = False
        win = Adw.ApplicationWindow(application=self, title="T2 CPU Control")
        win.set_default_size(1080, 940)
        header = Adw.HeaderBar()
        title = Adw.WindowTitle(title="CPU Control", subtitle="Package power and thermal stability")
        header.set_title_widget(title)
        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        root.append(header)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        body.set_margin_top(14); body.set_margin_bottom(18); body.set_margin_start(18); body.set_margin_end(18)
        body.set_vexpand(True)
        root.append(body); win.set_content(root)

        self.summary = Gtk.Label(xalign=0); self.summary.add_css_class("title-3")
        self.status = Gtk.Label(xalign=0)
        self.status.add_css_class("dim-label")
        self.status.add_css_class("monospace")
        self.status.set_width_chars(100)
        self.status.set_max_width_chars(100)
        body.append(self.summary); body.append(self.status)

        controls = Gtk.Grid(column_spacing=14, column_homogeneous=True)
        auto_panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        manual_panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        auto_content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        manual_content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        for panel in (auto_panel, manual_panel):
            panel.add_css_class("card")
            panel.set_valign(Gtk.Align.FILL)
        for content in (auto_content, manual_content):
            content.set_margin_top(14)
            content.set_margin_bottom(14)
            content.set_margin_start(14)
            content.set_margin_end(14)
        auto_panel.append(auto_content)
        manual_panel.append(manual_content)
        controls.attach(auto_panel, 0, 0, 1, 1)
        controls.attach(manual_panel, 1, 0, 1, 1)
        body.append(controls)

        auto_title = Gtk.Label(label="Automatic tuning", xalign=0)
        auto_title.add_css_class("title-4")
        self.auto_hint = Gtk.Label(
            label="Tests PL2 while keeping PL1 at the detected processor base power.",
            xalign=0,
        )
        self.auto_hint.set_wrap(True)
        self.auto_hint.add_css_class("dim-label")
        auto_actions = Gtk.Box(spacing=8)
        self.cal_btn = Gtk.Button(label="Auto-tune power limits")
        self.cal_btn.add_css_class("suggested-action")
        self.cancel_btn = Gtk.Button(label="Cancel")
        self.cancel_btn.set_sensitive(False)
        auto_actions.append(self.cal_btn)
        auto_actions.append(self.cancel_btn)
        self.auto_result = Gtk.Label(label="No auto-tune result yet", xalign=0)
        self.auto_result.set_wrap(True)
        self.auto_result.add_css_class("dim-label")
        auto_content.append(auto_title); auto_content.append(self.auto_hint); auto_content.append(auto_actions)
        auto_content.append(self.auto_result)
        self.cal_btn.connect("clicked", self.confirm_calibration)
        self.cancel_btn.connect("clicked", lambda *_: self.cancel_event.set())

        manual_title = Gtk.Label(label="Manual power limits", xalign=0)
        manual_title.add_css_class("title-4")
        manual_hint = Gtk.Label(
            label="Slider changes apply immediately. Without persistence, current limits remain until reboot.",
            xalign=0,
        )
        manual_hint.set_wrap(True)
        manual_hint.add_css_class("dim-label")
        manual_content.append(manual_title); manual_content.append(manual_hint)

        grid = Gtk.Grid(column_spacing=18, row_spacing=10)
        manual_content.append(grid)
        self.pl1_label = Gtk.Label(xalign=0); self.pl2_label = Gtk.Label(xalign=0)
        for label in (self.pl1_label, self.pl2_label):
            label.set_width_chars(21)
            label.set_max_width_chars(21)
            label.add_css_class("monospace")
        self.pl1 = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 5, 125, 1)
        self.pl2 = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 5, 125, 1)
        for scale in (self.pl1, self.pl2): scale.set_hexpand(True); scale.set_draw_value(False)
        grid.attach(self.pl1_label, 0, 0, 1, 1); grid.attach(self.pl1, 1, 0, 1, 1)
        grid.attach(self.pl2_label, 0, 1, 1, 1); grid.attach(self.pl2, 1, 1, 1, 1)
        self.pl1.connect("value-changed", self.limit_changed, "pl1")
        self.pl2.connect("value-changed", self.limit_changed, "pl2")

        self.restore_btn = Gtk.Button(label="Restore system defaults")
        self.restore_btn.set_sensitive(False)
        manual_content.append(self.restore_btn)
        self.restore_btn.connect("clicked", self.restore_defaults)

        self.persist = Gtk.CheckButton(label="Reapply current limits after boot and resume")
        self.persist.set_margin_top(4)
        body.append(self.persist)
        self.persist.connect("toggled", self.persistence_changed)

        telemetry = Gtk.Grid(column_spacing=18, column_homogeneous=True)
        table_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        graph_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        telemetry.attach(table_box, 0, 0, 1, 1)
        telemetry.attach(graph_box, 1, 0, 1, 1)
        body.append(telemetry)

        table_title = Gtk.Label(label="Logical CPU status", xalign=0)
        table_title.add_css_class("title-4")
        table_box.append(table_title)
        self.cpu_table = Gtk.Grid(column_spacing=24, row_spacing=4)
        self.cpu_column_widths = (52, 76, 76, 52, 150)
        for column, heading in enumerate(("CPU", "MHz", "Temp", "dTj", "Status")):
            label = Gtk.Label(label=heading, xalign=1 if column < 4 else 0)
            label.set_size_request(self.cpu_column_widths[column], -1)
            label.add_css_class("heading")
            self.cpu_table.attach(label, column, 0, 1, 1)
        table_box.append(self.cpu_table)

        self.power_graph = HistoryGraph("Package power", " W", [(0.94, .55, .24), (.35, .68, .90), (.45, .75, .45)])
        self.temp_graph = HistoryGraph("CPU package temperature", " °C", [(.92, .30, .25)], 105)
        graph_box.append(self.power_graph); graph_box.append(self.temp_graph)
        self.throttle = Gtk.Label(xalign=0); self.throttle.set_wrap(True); body.append(self.throttle)
        win.connect("close-request", self.close_requested)
        win.present()
        GLib.timeout_add_seconds(1, self.poll)
        self.poll()

    def close_requested(self, _window):
        if not self.calibrating:
            return False
        self.cancel_event.set()
        self.status.set_text("Cancelling calibration and restoring hardware state")
        return True

    def limit_changed(self, _scale, which):
        if not self.control_update:
            self.control_update = True
            if which == "pl1" and self.pl1.get_value() > self.pl2.get_value():
                self.pl1.set_value(self.pl2.get_value())
            elif which == "pl2" and self.pl2.get_value() < self.pl1.get_value():
                self.pl2.set_value(self.pl1.get_value())
            self.control_update = False
        self.pl1_label.set_text(f"PL1 sustained  {self.pl1.get_value():3.0f} W")
        self.pl2_label.set_text(f"PL2 burst      {self.pl2.get_value():3.0f} W")
        if self.controls_initialized and not self.control_update and not self.calibrating:
            if self.manual_apply_source is not None:
                GLib.source_remove(self.manual_apply_source)
            self.manual_apply_source = GLib.timeout_add(700, self.apply_manual_limits)

    def run_helper(self, *args):
        result = command("pkexec", HELPER, *map(str, args))
        if args[0] == "apply":
            self.thermald_state = "disabled" if int(args[3]) else "stopped"
        elif args[0] in ("restore", "disable-persistence"):
            self.thermald_state = thermald_status()
        return result

    def show_notice(self, message, seconds=10):
        self.notice_until = time.monotonic() + seconds
        self.status.set_text(message)

    def apply_manual_limits(self):
        self.manual_apply_source = None
        if self.calibrating:
            return GLib.SOURCE_REMOVE
        try:
            self.run_helper("apply", int(self.pl1.get_value()*1e6), int(self.pl2.get_value()*1e6), int(self.persist.get_active()))
            persistence = "enabled" if self.persist.get_active() else "disabled"
            self.show_notice(
                f"Manual limits applied: {self.pl1.get_value():.0f}/{self.pl2.get_value():.0f} W · "
                f"auto-tuned recommendation replaced · persistence {persistence}"
            )
        except Exception as e: self.status.set_text(f"Apply failed: {e}")
        return GLib.SOURCE_REMOVE

    def persistence_changed(self, _button):
        if not self.controls_initialized or self.control_update or self.calibrating:
            return
        try:
            self.run_helper("apply", int(self.pl1.get_value()*1e6), int(self.pl2.get_value()*1e6), int(self.persist.get_active()))
            state = "enabled" if self.persist.get_active() else "disabled"
            self.show_notice(f"Persistence {state}; current limits remain active")
        except Exception as e:
            self.status.set_text(f"Persistence change failed: {e}")

    def set_sliders(self, pl1, pl2):
        self.control_update = True
        self.pl1.set_value(pl1)
        self.pl2.set_value(pl2)
        self.control_update = False

    def cancel_manual_apply(self):
        if self.manual_apply_source is not None:
            GLib.source_remove(self.manual_apply_source)
            self.manual_apply_source = None

    def set_persistence(self, active):
        self.control_update = True
        self.persist.set_active(active)
        self.control_update = False

    def update_cpu_table(self, cores, tjmax):
        while len(self.cpu_rows) < len(cores):
            row_number = len(self.cpu_rows) + 1
            labels = [Gtk.Label(xalign=1 if column < 4 else 0) for column in range(5)]
            for column, label in enumerate(labels):
                label.set_size_request(self.cpu_column_widths[column], -1)
                label.add_css_class("monospace")
                self.cpu_table.attach(label, column, row_number, 1, 1)
            self.cpu_rows.append(labels)
        for labels, core in zip(self.cpu_rows, cores):
            cpu, mhz, temp, thermal, prochot = core
            if prochot:
                state = "PROCHOT"
            elif thermal:
                state = "thermal"
            else:
                state = "ok"
            values = (str(cpu), str(mhz), f"{temp} °C", str(tjmax - temp), state)
            for label, value in zip(labels, values):
                label.set_text(value)
                label.remove_css_class("error")
                if state != "ok":
                    label.add_css_class("error")

    def restore_defaults(self, *_):
        try:
            self.cancel_manual_apply()
            self.run_helper("restore")
            data = read_status()
            self.data = data
            self.set_sliders(int(data["pl1_uw"]) / 1e6, int(data["pl2_uw"]) / 1e6)
            self.set_persistence(False)
            result = (
                f"System defaults restored: "
                f"PL1 {int(data['pl1_uw']) / 1e6:.0f} W, "
                f"PL2 {int(data['pl2_uw']) / 1e6:.0f} W; thermal management restored"
            )
            self.auto_result.set_text(result)
            self.show_notice("System power limits and thermal management restored.")
        except Exception as e: self.status.set_text(f"Could not restore system defaults: {e}")

    def poll(self):
        try:
            data = read_status(); now = time.monotonic(); energy = int(data.get("energy_uj", 0)); maxe = int(data.get("max_energy_uj", 1))
            watts = None
            if self.last_energy is not None:
                delta = energy - self.last_energy
                if delta < 0: delta += maxe
                watts = delta / 1e6 / (now - self.last_energy_time)
            self.last_energy, self.last_energy_time = energy, now
            self.data = data; cores = data["cores"]
            active_pl1 = int(data["pl1_uw"]) / 1e6
            active_pl2 = int(data["pl2_uw"]) / 1e6
            if not self.controls_initialized:
                base = max(5, int(data.get("base_uw", 5000000))/1e6)
                ceiling = max(
                    base + 10,
                    int(data.get("ceiling_uw", 0)) / 1e6,
                    int(data["pl1_uw"]) / 1e6,
                    int(data["pl2_uw"]) / 1e6,
                    int(data.get("defaults_pl1_uw", 0)) / 1e6,
                    int(data.get("defaults_pl2_uw", 0)) / 1e6,
                )
                for s in (self.pl1, self.pl2): s.set_range(5, ceiling)
                self.set_sliders(int(data["pl1_uw"])/1e6, int(data["pl2_uw"])/1e6)
                self.control_update = True
                self.persist.set_active(data.get("persistent") == "1")
                self.control_update = False
                self.controls_initialized = True
            elif self.manual_apply_source is None and not self.calibrating:
                self.set_sliders(active_pl1, active_pl2)
            self.summary.set_text(data.get("model", "Intel CPU"))
            avg_freq = statistics.mean([c[1] for c in cores]) if cores else 0
            max_temp = max([c[2] for c in cores], default=0)
            package_temp = int(data.get("package_temp", max_temp))
            base_name = "cTDP-down" if data.get("base_kind") == "ctdp-down" else "Package TDP"
            base_watts = int(data.get("base_uw", 0)) / 1e6
            if data.get("defaults_pl1_uw") and data.get("defaults_pl2_uw"):
                defaults_pl1 = int(data["defaults_pl1_uw"]) / 1e6
                defaults_pl2 = int(data["defaults_pl2_uw"]) / 1e6
                self.restore_btn.set_label(
                    f"Restore system defaults ({defaults_pl1:.0f}/{defaults_pl2:.0f} W)"
                )
                if not self.calibrating:
                    self.restore_btn.set_sensitive(True)
            self.auto_hint.set_text(
                f"PL1 stays at {base_watts:.0f} W ({base_name}); "
                f"PL2 is tested in 5 W steps."
            )
            self.update_cpu_table(cores, int(data.get("tjmax", 100)))
            if not self.calibrating and time.monotonic() >= self.notice_until:
                pending = ""
                if self.controls_initialized:
                    slider_pl1 = round(self.pl1.get_value())
                    slider_pl2 = round(self.pl2.get_value())
                    current_pl1 = round(active_pl1)
                    current_pl2 = round(active_pl2)
                    if (slider_pl1, slider_pl2) != (current_pl1, current_pl2):
                        pending = "  ·  Slider changes not applied"
                self.status.set_text(
                    f"Power {watts or 0:6.1f} W  |  Average {avg_freq:4.0f} MHz  |  "
                    f"Package {package_temp:3d} °C  |  thermald {self.thermald_state:<8}{pending}"
                )
            self.history_power.append(watts); self.history_package_temp.append(package_temp)
            pl1=float(data["pl1_uw"])/1e6; pl2=float(data["pl2_uw"])/1e6
            self.power_graph.update([("Power", list(self.history_power)), ("PL1", [pl1]*len(self.history_power)), ("PL2", [pl2]*len(self.history_power))])
            self.temp_graph.update([("Package", list(self.history_package_temp))], package_temp)
            reasons=[]; perf=int(data.get("perf_active",0))
            thermal_cores=[str(c[0]) for c in cores if c[3]]
            prochot_cores=[str(c[0]) for c in cores if c[4]]
            if data.get("bd_inferred")=="1": reasons.append("BD_PROCHOT inferred")
            elif prochot_cores: reasons.append("PROCHOT CPU " + ",".join(prochot_cores))
            if thermal_cores: reasons.append("thermal CPU " + ",".join(thermal_cores))
            if perf & (1<<10): reasons.append("PL1")
            if perf & (1<<11): reasons.append("PL2")
            self.throttle.set_markup("<b>Active throttle:</b> " + (", ".join(reasons) or "none") + f"    <b>Logged bits:</b> 0x{int(data.get('perf_log',0)):x}")
        except Exception as e: self.status.set_text(f"Telemetry unavailable: {e}")
        return True

    def set_calibration_ui(self, active):
        self.calibrating=active
        for w in (self.cal_btn,self.pl1,self.pl2,self.persist): w.set_sensitive(not active)
        self.restore_btn.set_sensitive(
            not active and bool(self.data.get("defaults_pl1_uw")) and bool(self.data.get("defaults_pl2_uw"))
        )
        self.cancel_btn.set_sensitive(active)

    def confirm_calibration(self, *_):
        dialog = Adw.MessageDialog.new(
            self.get_active_window(),
            "Auto-tune CPU power limits?",
            "PL1 remains fixed at the detected base power: cTDP-down when the processor exposes it, otherwise "
            "Package TDP. PL2 starts at the same value and increases in 5 W steps. "
            "Each PL2 value runs the same kernel compilation workload for 30 seconds. The matching Fedora "
            "kernel source is downloaded and cached before the first run. When a value triggers PROCHOT, the previous successful "
            "PL2 value is applied immediately. The result remains active until reboot. "
            "After auto-tuning completes, enable 'Reapply current limits after boot and resume' "
            "to keep the result. "
            "Administrator authentication is required to control the fans and power limits.",
        )
        dialog.set_default_size(480, 480)
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("calibrate", "Start auto-tuning")
        dialog.set_default_response("calibrate")
        dialog.set_close_response("cancel")
        dialog.set_response_appearance("calibrate", Adw.ResponseAppearance.SUGGESTED)
        dialog.connect("response", self.calibration_response)
        dialog.present()

    def calibration_response(self, _dialog, response):
        if response == "calibrate":
            self.start_calibration()

    def start_calibration(self):
        if self.calibrating or not self.data: return
        if self.manual_apply_source is not None:
            GLib.source_remove(self.manual_apply_source)
            self.manual_apply_source = None
        self.calibration_persist = int(self.persist.get_active())
        self.cancel_event.clear(); self.set_calibration_ui(True)
        self.auto_result.set_text("Auto-tuning in progress")
        self.status.set_text("Waiting for administrator authorization")
        threading.Thread(target=self.calibrate, daemon=True).start()

    def run_until_prochot(self, duration, label):
        proc = subprocess.Popen([
            BENCHMARK, "run",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        started = time.monotonic()
        next_sample = started
        prochot = False
        completed_window = False
        try:
            while proc.poll() is None:
                if self.cancel_event.is_set():
                    os.killpg(proc.pid, signal.SIGTERM)
                    break
                now = time.monotonic()
                if now - started >= duration:
                    completed_window = True
                    os.killpg(proc.pid, signal.SIGTERM)
                    break
                if now < next_sample:
                    time.sleep(min(next_sample - now, 0.05))
                    continue
                status = read_status()
                prochot = (
                    status.get("prochot") == "1" or
                    any(core[4] for core in status["cores"])
                )
                elapsed = min(duration, int(time.monotonic() - started) + 1)
                GLib.idle_add(self.status.set_text, f"{label} · {elapsed}/{duration} s")
                if prochot:
                    os.killpg(proc.pid, signal.SIGTERM)
                    break
                next_sample = time.monotonic() + .25
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
        if not completed_window and not prochot and not self.cancel_event.is_set():
            raise RuntimeError("kernel compilation benchmark failed")
        return prochot

    def prepare_benchmark(self):
        proc = subprocess.Popen(
            [BENCHMARK, "prepare"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        assert proc.stdout is not None
        last_message = ""
        for line in proc.stdout:
            message = line.strip()
            if message:
                last_message = message
                GLib.idle_add(self.status.set_text, message)
        if proc.wait() != 0:
            raise RuntimeError(last_message or "kernel source preparation failed")

    def calibrate(self):
        original=(int(self.data["pl1_uw"]),int(self.data["pl2_uw"]),int(self.data.get("persistent") == "1"))
        down=max(5,round(int(self.data.get("base_uw",35000000))/1e6))
        ceiling=max(down+10,round(int(self.data.get("ceiling_uw",(down+10)*1000000))/1e6))
        try:
            self.prepare_benchmark()
            self.run_helper("fans-max")
            candidates=list(range(down,ceiling+1,5))
            if candidates[-1] != ceiling:
                candidates.append(ceiling)
            selected_pl2=down
            last_passing_pl2=None
            for pl2 in candidates:
                if self.cancel_event.is_set(): break
                self.run_helper("apply",down*1000000,pl2*1000000,0)
                prochot=self.run_until_prochot(30,f"Testing {down}/{pl2} W")
                if self.cancel_event.is_set(): break
                if prochot:
                    selected_pl2=last_passing_pl2 if last_passing_pl2 is not None else down
                    break
                last_passing_pl2=pl2
                selected_pl2=pl2
            if not self.cancel_event.is_set():
                pl1=down; pl2=selected_pl2
                self.run_helper("apply",pl1*1000000,pl2*1000000,self.calibration_persist)
                GLib.idle_add(self.set_sliders,pl1,pl2)
                persistence="enabled" if self.calibration_persist else "disabled"
                GLib.idle_add(self.auto_result.set_text,f"Applied result: PL1 {pl1} W, PL2 {pl2} W; persistence {persistence}")
                GLib.idle_add(self.show_notice,f"Auto-tuning complete and applied: {pl1}/{pl2} W · persistence {persistence}",15)
            else:
                self.run_helper("apply",*original)
                GLib.idle_add(self.auto_result.set_text,"Auto-tuning cancelled; previous limits restored")
        except Exception as e:
            try: self.run_helper("apply",*original)
            except Exception: pass
            message=f"Auto-tuning failed: {e}. Previous limits restored"
            GLib.idle_add(self.auto_result.set_text,message)
            GLib.idle_add(self.show_notice,message,30)
        finally:
            try: self.run_helper("fans-restore")
            except Exception: pass
            GLib.idle_add(self.set_calibration_ui,False)


if __name__ == "__main__":
    raise SystemExit(CpuControl().run())
