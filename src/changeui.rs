//! The "Change password" page: rules read from AD, a button to Windows' own prompt, and a
//! form that checks the new password against the rules before Windows changes it.

use crate::checker::{self, State};
use crate::policy::{self, Requirements};
use crate::winutil;
use eframe::egui::{self, Color32, RichText, Rounding, Stroke};
use std::sync::{Arc, Mutex};

type Slot<T> = Arc<Mutex<Option<T>>>;

const GOOD: Color32 = Color32::from_rgb(22, 163, 74);
const BAD: Color32 = Color32::from_rgb(220, 38, 38);
const MUTED: Color32 = Color32::from_rgb(100, 116, 139);
const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);

pub struct ChangePage {
    reqs: Slot<Result<Requirements, String>>,
    old: String,
    new: String,
    confirm: String,
    reveal: bool,
    working: bool,
    outcome: Slot<Result<(), String>>,
    last: Option<Result<(), String>>,
    hint: Option<String>,
}

fn load(slot: Slot<Result<Requirements, String>>, ctx: egui::Context) {
    *slot.lock().unwrap() = None;
    std::thread::spawn(move || {
        let r = policy::fetch();
        *slot.lock().unwrap() = Some(r);
        ctx.request_repaint();
    });
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(226, 232, 240)))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
}

impl ChangePage {
    pub fn new(ctx: &egui::Context) -> Self {
        let reqs: Slot<_> = Arc::new(Mutex::new(None));
        load(reqs.clone(), ctx.clone());
        Self {
            reqs,
            old: String::new(),
            new: String::new(),
            confirm: String::new(),
            reveal: false,
            working: false,
            outcome: Arc::new(Mutex::new(None)),
            last: None,
            hint: None,
        }
    }

    fn clear(&mut self) {
        policy::wipe(&mut self.old);
        policy::wipe(&mut self.new);
        policy::wipe(&mut self.confirm);
    }

    /// Returns true when the person wants to go back.
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &State) -> bool {
        let mut back = false;
        let finished = self.outcome.lock().unwrap().take();
        if let Some(r) = finished {
            self.working = false;
            if r.is_ok() {
                self.clear();
                checker::check_now(state);
            }
            self.last = Some(r);
        }
        let reqs = self.reqs.lock().unwrap().clone();

        ui.horizontal(|ui| {
            if ui.button("Back").clicked() {
                back = true;
            }
            ui.label(RichText::new("Change password").size(20.0).strong());
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // 1. the rules
            card(ui, |ui| {
                ui.label(RichText::new("Your password rules").size(15.0).strong());
                match &reqs {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reading the rules from Active Directory…");
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(format!("Couldn't read the rules: {e}")).color(BAD));
                        if ui.button("Try again").clicked() {
                            load(self.reqs.clone(), ui.ctx().clone());
                        }
                    }
                    Some(Ok(r)) => {
                        ui.label(RichText::new(format!("From: {}", r.source)).size(12.0).color(MUTED));
                        for line in r.lines() {
                            ui.label(format!("•  {line}"));
                        }
                    }
                }
            });

            // 2. Windows' own prompt
            card(ui, |ui| {
                ui.label(RichText::new("Change it in Windows").size(15.0).strong());
                let domain = matches!(&reqs, Some(Ok(r)) if r.domain_account);
                let local = matches!(&reqs, Some(Ok(r)) if !r.domain_account);
                if winutil::is_remote_session() {
                    if ui.button("Open Windows change password").clicked() {
                        self.hint = Some(if winutil::open_change_password() {
                            "Windows Security is opening — choose \"Change a password\".".into()
                        } else {
                            "Couldn't open Windows Security. Use Start → Windows Security.".into()
                        });
                    }
                } else if local {
                    if ui.button("Open Windows change password").clicked() {
                        winutil::open_signin_settings();
                        self.hint = Some("Settings is opening — under Password, choose Change.".into());
                    }
                } else {
                    let _ = domain;
                    ui.label(
                        "Press Ctrl+Alt+Del and choose \"Change a password\". Windows doesn't let apps open \
                         that screen on this computer, so you can also change it below.",
                    );
                }
                if let Some(h) = &self.hint {
                    ui.label(RichText::new(h).size(12.5).color(ACCENT));
                }
            });

            // 3. change it here, checked against the rules first
            card(ui, |ui| {
                ui.label(RichText::new("Or change it here").size(15.0).strong());
                egui::Grid::new("pw").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Current password");
                    ui.add(egui::TextEdit::singleline(&mut self.old).password(!self.reveal).desired_width(200.0));
                    ui.end_row();
                    ui.label("New password");
                    ui.add(egui::TextEdit::singleline(&mut self.new).password(!self.reveal).desired_width(200.0));
                    ui.end_row();
                    ui.label("Type it again");
                    ui.add(egui::TextEdit::singleline(&mut self.confirm).password(!self.reveal).desired_width(200.0));
                    ui.end_row();
                });
                ui.checkbox(&mut self.reveal, "Show passwords");

                let mut all_ok = false;
                if let Some(Ok(r)) = &reqs {
                    let checks = r.check(&self.new, &self.confirm);
                    all_ok = checks.iter().all(|(_, ok)| *ok);
                    for (rule, ok) in checks {
                        let (mark, color) = if ok { ("✔", GOOD) } else { ("✖", BAD) };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(mark).color(color));
                            ui.label(RichText::new(rule).size(12.5));
                        });
                    }
                    if r.history > 0 || r.min_age_days > 0.0 {
                        ui.label(
                            RichText::new("Windows checks the password history and minimum age when you press Change.")
                                .size(11.5)
                                .color(MUTED),
                        );
                    }
                }

                ui.horizontal(|ui| {
                    let ready = all_ok && !self.old.is_empty() && !self.working;
                    let btn = egui::Button::new(RichText::new("Change password").color(Color32::WHITE)).fill(ACCENT);
                    if ui.add_enabled(ready, btn).clicked() {
                        if let Some(Ok(r)) = &reqs {
                            self.working = true;
                            self.last = None;
                            let (dom, user) = (r.domain_account, r.sam.clone());
                            let (old, new) = (self.old.clone(), self.new.clone());
                            let slot = self.outcome.clone();
                            let ctx = ui.ctx().clone();
                            std::thread::spawn(move || {
                                let (mut old, mut new) = (old, new);
                                let res = policy::change(dom, &user, &old, &new);
                                policy::wipe(&mut old);
                                policy::wipe(&mut new);
                                *slot.lock().unwrap() = Some(res);
                                ctx.request_repaint();
                            });
                        }
                    }
                    if self.working {
                        ui.spinner();
                    }
                });
                match &self.last {
                    Some(Ok(())) => {
                        ui.label(
                            RichText::new("Password changed. Use the new one next time you sign in or unlock.").color(GOOD),
                        );
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(BAD));
                    }
                    None => {}
                }
            });
        });

        if back {
            self.clear();
        }
        back
    }
}
