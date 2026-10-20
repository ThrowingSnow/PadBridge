use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use padbridge_core::ring::RingMode;
use padbridge_core::{ControllerState, PadAxis, PadButton, PlayerIndicator};

use super::{hint, App};
use crate::engine::Command;

impl App {
    pub(super) fn monitor(&mut self, ui: &mut egui::Ui) {
        let devices = self.snap.devices.clone();
        let receiver = self.snap.receiver.clone().unwrap_or_default();
        // Local pads first, then the peer's pads (shown only).
        let mut choices: Vec<(String, String)> =
            devices.iter().map(|d| (d.info.id.clone(), d.info.name.clone())).collect();
        choices.extend(
            receiver.remote_pads.iter().map(|(i, _, _)| (remote_id(*i), format!("Pad {} der Gegenstelle", i + 1))),
        );
        if self.selected.as_ref().is_none_or(|id| !choices.iter().any(|(c, _)| c == id)) {
            self.selected = choices.first().map(|(id, _)| id.clone());
        }
        let remote = receiver.remote_pads.iter().find(|(i, _, _)| Some(remote_id(*i)) == self.selected);
        if let Some((_, state, live)) = remote {
            ui.horizontal(|ui| {
                self.pad_picker(ui, &choices);
                let from = receiver.peer.map_or(String::new(), |p| format!(" von {}", p.ip()));
                let still = if *live { "" } else { " · still" };
                hint(ui, format!("· Netzwerk{from} · nur Anzeige{still}"));
            });
            ui.add_space(8.0);
            pad_view(ui, state);
            hint(ui, "Rumble und LEDs stellt die Gegenstelle ein, an der der Pad hängt.");
        }
        match devices.iter().find(|d| Some(&d.info.id) == self.selected.as_ref()) {
            None if remote.is_some() => {}
            None => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Kein Controller gefunden");
                    hint(ui, "Xbox-360-Controller (oder ein anderes Gamepad) per USB einstecken.");
                });
            }
            Some(device) => {
                ui.horizontal(|ui| {
                    self.pad_picker(ui, &choices);
                    hint(
                        ui,
                        format!(
                            "{:04x}:{:04x} · {}",
                            device.info.vendor_id, device.info.product_id, device.info.backend
                        ),
                    );
                    if let Some(pad) = device.net_pad {
                        hint(ui, format!("· im Netzwerk Pad {}", pad + 1));
                    }
                });
                ui.add_space(8.0);
                pad_view(ui, &device.state);
                ui.add_space(12.0);
                ui.separator();
                let caps = device.info.capabilities;
                let id = device.info.id.clone();
                ui.horizontal_top(|ui| {
                    if caps.rumble {
                        ui.vertical(|ui| {
                            ui.strong("Rumble");
                            let before = self.rumble;
                            ui.add(egui::Slider::new(&mut self.rumble.0, 0.0..=1.0).text("Links (schwer)"));
                            ui.add(egui::Slider::new(&mut self.rumble.1, 0.0..=1.0).text("Rechts (leicht)"));
                            if ui.button("Stopp").clicked() {
                                self.rumble = (0.0, 0.0);
                            }
                            if self.rumble != before {
                                let (low, high) = self.rumble;
                                self.handle.send(Command::Rumble { id: id.clone(), low, high });
                            }
                        });
                        ui.add_space(32.0);
                    }
                    ui.vertical(|ui| self.ring_controls(ui, caps.player_indicator));
                });
            }
        }
        ui.add_space(12.0);
        ui.separator();
        ui.strong("Protokoll");
        egui::ScrollArea::vertical().id_salt("log").max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
            for line in &self.snap.log {
                ui.monospace(line);
            }
        });
    }

    /// A choice of all pads when there are several, else just the name.
    fn pad_picker(&mut self, ui: &mut egui::Ui, choices: &[(String, String)]) {
        let name = choices.iter().find(|(id, _)| Some(id) == self.selected.as_ref()).map_or("", |(_, n)| n.as_str());
        if choices.len() > 1 {
            egui::ComboBox::from_id_salt("device").selected_text(name).show_ui(ui, |ui| {
                for (id, n) in choices {
                    ui.selectable_value(&mut self.selected, Some(id.clone()), n);
                }
            });
        } else {
            ui.strong(name);
        }
    }

    fn ring_controls(&mut self, ui: &mut egui::Ui, available: bool) {
        ui.strong("LED-Ring");
        if !available {
            hint(ui, "Nicht verfügbar (unter Linux: Schreibrecht auf die xpad-LED fehlt, siehe DOC.md).");
        }
        ui.add_enabled_ui(available, |ui| {
            ui.horizontal(|ui| {
                for n in 1..=4 {
                    if ui.button(format!("P{n}")).clicked() {
                        self.handle.send(Command::Indicator(PlayerIndicator::Player(n)));
                    }
                }
            });
            ui.horizontal(|ui| {
                for (title, indicator) in [
                    ("Rotieren", PlayerIndicator::Rotate),
                    ("Blinken", PlayerIndicator::BlinkAll),
                    ("Aus", PlayerIndicator::Off),
                ] {
                    if ui.button(title).clicked() {
                        self.handle.send(Command::Indicator(indicator));
                    }
                }
            });
            ui.horizontal(|ui| {
                for (title, mode) in [
                    ("Zufall", RingMode::Random),
                    ("Clock: Takt", RingMode::ClockBar),
                    ("Clock: Beat", RingMode::ClockBeat),
                ] {
                    let active = self.snap.ring_mode == mode;
                    if ui.selectable_label(active, title).clicked() {
                        // Pressing an active mode again returns to manual (P1).
                        let cmd = if active {
                            Command::Indicator(PlayerIndicator::Player(1))
                        } else {
                            Command::RingMode(mode)
                        };
                        self.handle.send(cmd);
                    }
                }
            });
            if self.snap.ring_mode.is_clock() {
                let c = &self.snap.clock;
                let bpm = c.bpm.map_or("– BPM".to_string(), |b| format!("{b:.1} BPM"));
                let state = if c.running { "läuft" } else { "gestoppt" };
                hint(ui, format!("Clock {state} · {bpm} · Takt {} · Schlag {}", c.bar, c.beat));
            }
        });
    }
}

/// Sticks, triggers and every button of one pad.
fn pad_view(ui: &mut egui::Ui, s: &ControllerState) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            trigger(ui, "LT", s.axis(PadAxis::LeftTrigger));
            stick(ui, "Links", s.axis(PadAxis::LeftX), s.axis(PadAxis::LeftY), s.is_pressed(PadButton::LeftStick));
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            buttons(ui, s, &[PadButton::DpadUp, PadButton::DpadDown, PadButton::DpadLeft, PadButton::DpadRight]);
            buttons(ui, s, &[PadButton::Back, PadButton::Guide, PadButton::Start]);
            buttons(ui, s, &[PadButton::LeftShoulder, PadButton::RightShoulder]);
            buttons(ui, s, &[PadButton::A, PadButton::B, PadButton::X, PadButton::Y]);
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            trigger(ui, "RT", s.axis(PadAxis::RightTrigger));
            stick(ui, "Rechts", s.axis(PadAxis::RightX), s.axis(PadAxis::RightY), s.is_pressed(PadButton::RightStick));
        });
    });
}

fn stick(ui: &mut egui::Ui, label: &str, x: f32, y: f32, clicked: bool) {
    let size = 130.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let line = Stroke::new(1.0, ui.visuals().weak_text_color());
    p.circle_stroke(c, size / 2.0 - 1.0, line);
    p.line_segment([Pos2::new(c.x, rect.top()), Pos2::new(c.x, rect.bottom())], line);
    p.line_segment([Pos2::new(rect.left(), c.y), Pos2::new(rect.right(), c.y)], line);
    let r = size / 2.0 - 8.0;
    // Up is positive in the model, screen y grows downwards.
    let dot = Pos2::new(c.x + x * r, c.y - y * r);
    let color = if clicked { ui.visuals().selection.bg_fill } else { ui.visuals().strong_text_color() };
    p.circle_filled(dot, 7.0, color);
    ui.label(format!("{label}  x {x:+.2}  y {y:+.2}"));
}

fn trigger(ui: &mut egui::Ui, label: &str, value: f32) {
    ui.add(egui::ProgressBar::new(value).desired_width(130.0).text(format!("{label} {:.0} %", value * 100.0)));
}

fn buttons(ui: &mut egui::Ui, s: &ControllerState, list: &[PadButton]) {
    ui.horizontal(|ui| {
        for &b in list {
            let on = s.is_pressed(b);
            let text = egui::RichText::new(b.display_name()).monospace();
            let (fill, text) = if on {
                (ui.visuals().selection.bg_fill, text.color(Color32::WHITE))
            } else {
                (ui.visuals().faint_bg_color, text)
            };
            egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 3)).show(
                ui,
                |ui| {
                    ui.label(text);
                },
            );
        }
    });
}

/// Monitor id of the peer's pad `index` (local ids come from the backends).
fn remote_id(index: u8) -> String {
    format!("net-{index}")
}
