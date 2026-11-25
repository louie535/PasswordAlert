#![windows_subsystem = "windows"]

mod changeui;
mod checker;
mod icon;
mod policy;
mod settings;
mod tray;
mod winutil;

use checker::{Source, State, Status};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};
use icon::Level;
use std::sync::Arc;
use std::time::Instant;

struct Args {
    screenshot: Option<String>,
    tray_only: bool,
    change_page: bool,
    show: bool,
}

fn parse_args() -> Args {
    let mut a = Args { screenshot: None, tray_only: false, change_page: false, show: false };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--screenshot" => a.screenshot = it.next(),
            "--tray" => a.tray_only = true,
            "--change-page" => a.change_page = true,
            "--show" => a.show = true,
            "--demo-days" => {
                if let Some(d) = it.next().and_then(|v| v.parse::<i64>().ok()) {
                    let _ = checker::DEMO_DAYS.set(d);
                }
            }
            _ => {}
        }
    }
    a
}

fn main() -> eframe::Result<()> {
    let args = parse_args();
    let shot = args.screenshot.is_some();

    if !shot && !winutil::single_instance() {
        // already running: bring it forward instead of starting twice
        // (a second start at sign-in, --tray, stays quiet)
        if !args.tray_only {
            winutil::show_main_window();
        }
        return Ok(());
    }

    let settings = settings::Settings::load();
    // start minimized to the tray (the setting, on by default) unless taking a picture or told --show
    let start_hidden = !shot && !args.show && !args.change_page && (args.tray_only || settings.start_minimized);
    let state = checker::new_state(settings, shot);

    if !shot {
        tray::spawn(state.clone());
    }
    {
        let st = state.clone();
        std::thread::spawn(move || {
            checker::run(
                st,
                Box::new(move |status, s| {
                    if !shot {
                        tray::set_status(checker::message(status, s), checker::level(status, s));
                    }
                    if let Some(r) = tray::REPAINT.get() {
                        r();
                    }
                }),
            )
        });
    }

    let icon = egui::IconData { rgba: icon::rgba(64, Level::Fine), width: 64, height: 64 };
    let mut vp = egui::ViewportBuilder::default()
        .with_title(winutil::WINDOW_TITLE)
        .with_inner_size([460.0, 600.0])
        .with_min_inner_size([400.0, 520.0])
        .with_icon(Arc::new(icon));
    if shot {
        vp = vp.with_active(false).with_window_level(egui::WindowLevel::AlwaysOnBottom);
    } else if start_hidden {
        vp = vp.with_visible(false);
        winutil::WINDOW_SHOWN.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };

    let result = eframe::run_native(
        winutil::WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let _ = tray::REPAINT.set(Box::new(move || ctx.request_repaint()));
            style(&cc.egui_ctx);
            let mut app = App::new(state, args.screenshot);
            app.start_hidden = start_hidden;
            if args.change_page {
                app.change = Some(changeui::ChangePage::new(&cc.egui_ctx));
            }
            Ok(Box::new(app))
        }),
    );
    if result.is_err() && !shot {
        // The window couldn't be drawn (e.g. a server without OpenGL 2). The tray icon, the
        // checks and the warning boxes don't need it, so keep running like the original did.
        winutil::NO_WINDOW.store(true, std::sync::atomic::Ordering::SeqCst);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    result
}

const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);
const INK: Color32 = Color32::from_rgb(15, 23, 42);
const MUTED: Color32 = Color32::from_rgb(100, 116, 139);
const PAGE: Color32 = Color32::from_rgb(241, 245, 249);

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = PAGE;
    v.window_fill = Color32::WHITE;
    v.override_text_color = Some(INK);
    v.selection.bg_fill = ACCENT;
    // text boxes: a light fill and a border so they show on white cards
    v.extreme_bg_color = Color32::from_rgb(248, 250, 252);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(203, 213, 225));
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(148, 163, 184));
    v.widgets.inactive.rounding = Rounding::same(6.0);
    v.widgets.hovered.rounding = Rounding::same(6.0);
    v.widgets.active.rounding = Rounding::same(6.0);
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(8.0, 8.0);
    s.spacing.button_padding = Vec2::new(12.0, 6.0);
    ctx.set_style(s);
}

struct App {
    state: State,
    screenshot: Option<String>,
    frames: u32,
    shot_sent: bool,
    autostart: bool,
    autostart_all: bool,
    started: Instant,
    note: Option<(String, Instant)>,
    hwnd_known: bool,
    start_hidden: bool,
    change: Option<changeui::ChangePage>,
}

impl App {
    fn new(state: State, screenshot: Option<String>) -> Self {
        Self {
            state,
            screenshot,
            frames: 0,
            shot_sent: false,
            autostart: winutil::autostart_enabled(),
            autostart_all: winutil::autostart_all_users(),
            started: Instant::now(),
            note: None,
            hwnd_known: false,
            start_hidden: false,
            change: None,
        }
    }

    fn flash(&mut self, s: impl Into<String>) {
        self.note = Some((s.into(), Instant::now()));
    }
}

fn level_colors(l: Level) -> (Color32, Color32) {
    let [r, g, b] = l.rgb();
    let strong = Color32::from_rgb(r, g, b);
    let tint = |c: u8| ((c as u16 + 255 * 7) / 8) as u8;
    let soft = Color32::from_rgb(tint(r), tint(g), tint(b));
    (strong, soft)
}

fn card(ui: &mut egui::Ui, fill: Color32, stroke: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
}

fn mins_text(m: u64) -> String {
    if m >= 24 * 60 && m % (24 * 60) == 0 {
        let d = m / (24 * 60);
        format!("{d} {}", if d == 1 { "day" } else { "days" })
    } else if m >= 60 && m % 60 == 0 {
        let h = m / 60;
        format!("{h} {}", if h == 1 { "hour" } else { "hours" })
    } else {
        format!("{m} min")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // remember our window so the tray can show and hide it
        if !self.hwnd_known {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    winutil::MAIN_HWND.store(w.hwnd.get(), std::sync::atomic::Ordering::SeqCst);
                    self.hwnd_known = true;
                }
            }
        }

        // started with --tray (from Windows start-up): stay in the tray until opened
        if self.start_hidden {
            self.start_hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            winutil::WINDOW_SHOWN.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // closing the window keeps it running in the tray, as the original only lives there
        if self.screenshot.is_none() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            winutil::hide_main_window();
            tray::show_balloon();
        }

        // --screenshot: draw ~120 frames, save the picture, quit
        if let Some(path) = self.screenshot.clone() {
            self.frames += 1;
            ctx.request_repaint();
            // ask for the picture at frame 120, and again every 60 frames until it arrives
            if self.frames >= 120 && (!self.shot_sent || self.frames % 60 == 0) {
                self.shot_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let img = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = img {
                save_png(&path, &img);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            // safety net: give up after 60 seconds (time, not frames: release builds draw very fast)
            if self.started.elapsed().as_secs() > 60 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // the Change password page replaces the status page while it's open
        if let Some(page) = &mut self.change {
            let mut back = false;
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(PAGE).inner_margin(egui::Margin::same(18.0)))
                .show(ctx, |ui| back = page.ui(ui, &self.state));
            if back {
                self.change = None;
            }
            return;
        }
        let mut open_change = false;

        let (status, checking, last, next_due, mut s) = {
            let g = self.state.0.lock().unwrap();
            (g.status.clone(), g.checking, g.last_lookup_ft, g.next_due, g.settings.clone())
        };
        let before = s.clone();
        let lvl = checker::level(&status, &s);
        let (strong, soft) = level_colors(lvl);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(PAGE).inner_margin(egui::Margin::same(18.0)))
            .show(ctx, |ui| {
                // header
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::hover());
                    draw_lock(ui.painter(), rect, strong);
                    ui.vertical(|ui| {
                        ui.add_space(-2.0);
                        ui.label(RichText::new("Password Alert").size(20.0).strong());
                        ui.label(RichText::new(winutil::account_name()).size(12.5).color(MUTED));
                    });
                });
                ui.add_space(6.0);

                // status card
                card(ui, soft, strong.gamma_multiply(0.45), |ui| {
                    match &status {
                        Status::Expires { days, at_ft, source } => {
                            ui.horizontal(|ui| {
                                let big = if *days < 0 { "0".to_string() } else { days.to_string() };
                                ui.label(RichText::new(big).size(54.0).strong().color(strong));
                                ui.vertical(|ui| {
                                    ui.add_space(14.0);
                                    let unit = if *days == 1 { "day left" } else { "days left" };
                                    let unit = if *days < 0 { "expired" } else { unit };
                                    ui.label(RichText::new(unit).size(18.0).color(strong));
                                    let kind = match source {
                                        Source::Domain => "Domain account",
                                        Source::Local => "This computer's account (estimated)",
                                    };
                                    ui.label(RichText::new(kind).size(12.0).color(MUTED));
                                });
                            });
                            ui.label(RichText::new(format!("Expires {}", winutil::format_date(*at_ft))).size(13.5));
                        }
                        Status::Never => {
                            ui.label(RichText::new("Never expires").size(30.0).strong().color(strong));
                        }
                        Status::NotChecked => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Checking…").size(26.0).strong().color(strong));
                            });
                        }
                        Status::Error(_) => {
                            ui.label(RichText::new("Couldn't check").size(28.0).strong().color(strong));
                        }
                    }
                    ui.add_space(2.0);
                    ui.label(RichText::new(checker::message(&status, &s)).size(13.5));
                });

                // last / next check
                ui.horizontal_wrapped(|ui| {
                    let last_t = last.map(winutil::format_clock).unwrap_or_else(|| "not yet".into());
                    let next_t = match next_due {
                        Some(d) => {
                            let secs = d.saturating_duration_since(Instant::now()).as_secs();
                            if secs < 60 { "under a minute".to_string() } else { mins_text((secs + 59) / 60) }
                        }
                        None => "—".into(),
                    };
                    ui.label(RichText::new(format!("Last checked {last_t}  ·  next check in {next_t}")).size(12.5).color(MUTED));
                    if checking {
                        ui.spinner();
                    }
                });

                // actions
                ui.horizontal(|ui| {
                    let primary = egui::Button::new(RichText::new("Check now").color(Color32::WHITE)).fill(ACCENT);
                    if ui.add_enabled(!checking, primary).clicked() {
                        checker::check_now(&self.state);
                    }
                    if ui.button("Change password…").clicked() {
                        open_change = true;
                    }
                    let live = self.screenshot.is_none();
                    if ui.button("Hide to tray").clicked() && live {
                        winutil::hide_main_window();
                    }
                    if ui.button("Exit").clicked() && live {
                        tray::exit_app();
                    }
                });
                ui.add_space(4.0);

                // settings
                card(ui, Color32::WHITE, Color32::from_rgb(226, 232, 240), |ui| {
                    ui.label(RichText::new("Settings").size(15.0).strong());
                    ui.add_space(2.0);
                    egui::Grid::new("settings").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                        ui.label("Warn me from");
                        ui.add(egui::Slider::new(&mut s.warn_days, 1..=30).suffix(" days left"));
                        ui.end_row();
                        ui.label("Check every hour from");
                        let max = s.warn_days.max(1);
                        ui.add(egui::Slider::new(&mut s.urgent_days, 0..=max).suffix(" days left"));
                        ui.end_row();
                    });
                    ui.checkbox(&mut s.popups, "Show a pop-up warning");
                    ui.checkbox(&mut s.start_minimized, "Start minimized to the tray");
                    let mut auto = self.autostart;
                    if self.autostart_all {
                        let mut on = true;
                        ui.add_enabled(false, egui::Checkbox::new(&mut on, "Start with Windows (set for everyone by the installer)"));
                    } else if ui.checkbox(&mut auto, "Start with Windows (in the tray)").changed() {
                        if winutil::set_autostart(auto) {
                            self.autostart = auto;
                        } else {
                            self.flash("Couldn't change the start-up setting.");
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Show test warning").clicked() {
                            let sample = Status::Expires { days: s.warn_days.min(5), at_ft: 0, source: Source::Domain };
                            winutil::warning_box(checker::message(&sample, &s), checker::PRODUCT.into());
                        }
                        if ui.button("Reset to defaults").clicked() {
                            s = settings::Settings::default();
                        }
                    });
                });

                if let Some((n, t)) = &self.note {
                    if t.elapsed().as_secs() < 8 {
                        ui.label(RichText::new(n).size(12.5).color(ACCENT));
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(
                        RichText::new("Closing this window keeps Password Alert running in the tray. Right-click the tray icon to exit.")
                            .size(11.5)
                            .color(MUTED),
                    );
                });
            });

        if open_change {
            self.change = Some(changeui::ChangePage::new(ctx));
        }

        if s != before {
            s.urgent_days = s.urgent_days.min(s.warn_days);
            s.save();
            let (m, cv) = &*self.state;
            m.lock().unwrap().settings = s.clone();
            cv.notify_all();
            if self.screenshot.is_none() {
                tray::set_status(checker::message(&status, &s), checker::level(&status, &s));
            }
        }
    }
}

fn draw_lock(p: &egui::Painter, rect: egui::Rect, color: Color32) {
    p.rect_filled(rect, Rounding::same(8.0), color);
    let c = rect.center();
    let w = rect.width();
    let body = egui::Rect::from_min_max(
        egui::pos2(rect.left() + w * 0.24, rect.top() + w * 0.46),
        egui::pos2(rect.left() + w * 0.76, rect.top() + w * 0.84),
    );
    // shackle
    let r = w * 0.17;
    let cy = rect.top() + w * 0.44;
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 16.0);
            egui::pos2(c.x - r * a.cos(), cy - r * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(w * 0.08, Color32::WHITE)));
    p.rect_filled(body, Rounding::same(2.0), Color32::WHITE);
    p.circle_filled(egui::pos2(c.x, rect.top() + w * 0.6), w * 0.06, color);
    p.rect_filled(
        egui::Rect::from_min_max(egui::pos2(c.x - w * 0.025, rect.top() + w * 0.6), egui::pos2(c.x + w * 0.025, rect.top() + w * 0.74)),
        Rounding::ZERO,
        color,
    );
}

fn save_png(path: &str, img: &egui::ColorImage) {
    if let Some(dir) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(file) = std::fs::File::create(path) else { return };
    let w = std::io::BufWriter::new(file);
    let mut enc = png::Encoder::new(w, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut wr) = enc.write_header() {
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        let _ = wr.write_image_data(&bytes);
    }
}
