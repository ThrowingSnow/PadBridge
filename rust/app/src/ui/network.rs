use eframe::egui;
use padbridge_core::settings::{self, HostPreset};
use qrcode::{Color, QrCode};

use super::{hint, status, App, GREEN, GREY, ORANGE};
use crate::engine::Command;

impl App {
    pub(super) fn network_tab(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;

        ui.heading("Senden");
        changed |= ui.checkbox(&mut self.net.enabled, "Gamepad an die Gegenstelle senden").changed();
        egui::Grid::new("net").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Host");
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.net.host).hint_text("IP oder Hostname").desired_width(200.0),
                );
                // Apply the host when editing ends, not on every keystroke.
                changed |= field.lost_focus();
                self.presets(ui, &mut changed);
            });
            ui.end_row();
            ui.label("Port");
            changed |= ui.add(egui::DragValue::new(&mut self.net.port).range(1024..=65535)).changed();
            ui.end_row();
            ui.label("Rumble");
            changed |= ui
                .checkbox(&mut self.net.rumble, "Spiele auf der Gegenstelle dürfen den Controller vibrieren lassen")
                .changed();
            ui.end_row();
        });
        self.link_status(ui);

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Empfangen");
        let forced = self.snap.listen_forced || self.snap.took_over;
        ui.add_enabled_ui(!forced, |ui| {
            changed |= ui
                .checkbox(
                    &mut self.net.listen,
                    format!("Pads, MIDI und Tastatur/Maus einer Gegenstelle annehmen (UDP {})", self.net.port),
                )
                .changed();
        });
        if self.snap.took_over {
            hint(ui, "Vom Hintergrund-Dienst übernommen, solange dieses Fenster offen ist; danach empfängt wieder der Dienst.");
        } else if forced {
            hint(ui, "Per Startoption --listen eingeschaltet.");
        }
        self.receiver_status(ui);

        ui.add_space(8.0);
        ui.separator();
        self.web_page(ui);

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Schlüssel");
        hint(
            ui,
            format!(
                "Beide Seiten brauchen denselben Schlüssel: {}/{}",
                settings::config_dir().display(),
                settings::KEY_FILE
            ),
        );
        ui.horizontal(|ui| {
            if let Some(hex) = &self.snap.key_hex {
                if ui.button("Schlüssel kopieren").clicked() {
                    ui.ctx().copy_text(hex.clone());
                }
            }
        });
        hint(ui, "Port nur im LAN oder per VPN (z.B. Tailscale) freigeben, nie ins Internet: signiert, aber nicht verschlüsselt.");
        hint(
            ui,
            "Moonlight: Gamepad-Support in den Einstellungen ausschalten, sonst legt Sunshine ein zweites Pad an.",
        );

        if changed {
            self.set_net();
        }
    }

    /// Saved hosts: click to use, right-click to delete, "+" saves the current host.
    fn presets(&mut self, ui: &mut egui::Ui, changed: &mut bool) {
        let mut remove = None;
        for (i, preset) in self.net.presets.iter().enumerate() {
            let active = preset.host == self.net.host;
            let button = ui.selectable_label(active, &preset.name).on_hover_text(&preset.host);
            if button.clicked() {
                self.net.host = preset.host.clone();
                *changed = true;
            }
            button.context_menu(|ui| {
                if ui.button(format!("Preset „{}“ löschen", preset.name)).clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
        if let Some(i) = remove {
            self.net.presets.remove(i);
            *changed = true;
        }
        let can_save = !self.net.host.trim().is_empty();
        let plus = ui
            .add_enabled(can_save, egui::Button::new("+"))
            .on_hover_text("Aktuellen Host als Preset speichern (Rechtsklick auf ein Preset löscht es)");
        if plus.clicked() {
            self.preset_name.clear();
            self.preset_open = true;
        }
        if self.preset_open {
            let mut open = true;
            egui::Window::new(format!("Preset für {}", self.net.host))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    let field =
                        ui.add(egui::TextEdit::singleline(&mut self.preset_name).hint_text("Name, z.B. Desktop LAN"));
                    field.request_focus();
                    let name = self.preset_name.trim().to_string();
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.add_enabled(!name.is_empty(), egui::Button::new("Speichern")).clicked() || enter)
                        && !name.is_empty()
                    {
                        // Same name again = update that preset's host.
                        self.net.presets.retain(|p| p.name != name);
                        self.net.presets.push(HostPreset { name, host: self.net.host.trim().to_string() });
                        self.preset_open = false;
                        *changed = true;
                    }
                });
            if !open {
                self.preset_open = false;
            }
        }
    }

    /// The status page for phones: switch, address with QR code, PIN.
    fn web_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Status-Webseite");
        let mut web = self.snap.web.clone();
        let mut changed = ui.checkbox(&mut web.enabled, "Status im Browser zeigen, z. B. auf dem Handy").changed();
        ui.horizontal(|ui| {
            ui.label("Port");
            changed |= ui.add(egui::DragValue::new(&mut web.port).range(1024..=65535)).changed();
        });
        if changed {
            self.handle.send(Command::SetWeb(web));
        }
        if let Some(e) = &self.snap.web_error {
            status(ui, ORANGE, e.clone());
        }
        let Some(url) = self.snap.web_url.clone() else { return };
        ui.horizontal_top(|ui| {
            qr_code(ui, &url);
            ui.vertical(|ui| {
                status(ui, GREEN, format!("Läuft: {url}"));
                if ui.button("Adresse kopieren").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("PIN");
                    ui.monospace(egui::RichText::new(&self.snap.web.pin).size(18.0));
                    if ui.button("Neue PIN").clicked() {
                        self.handle.send(Command::NewWebPin);
                    }
                });
                hint(ui, "Im Heimnetz ohne PIN. Von außerhalb, z. B. über Tailscale, fragt die Seite nach der PIN.");
                hint(ui, "Mit dem Handy den QR-Code scannen; „Zum Home-Bildschirm“ macht daraus eine App.");
                hint(ui, "Nie per Portweiterleitung ins Internet stellen: Die Seite ist unverschlüsselt.");
            });
        });
    }

    fn link_status(&self, ui: &mut egui::Ui) {
        let l = &self.snap.link;
        if !l.active {
            match &l.error {
                Some(e) => status(ui, ORANGE, e.clone()),
                None => status(ui, GREY, "Aus"),
            }
            return;
        }
        let peer = l.peer.clone().unwrap_or_default();
        if l.answering {
            let latency = l.latency_ms.map_or(String::new(), |ms| format!(" · Latenz {ms:.1} ms (Hin und zurück)"));
            status(ui, GREEN, format!("Verbunden mit {peer} – Gegenstelle antwortet{latency}"));
        } else {
            status(
                ui,
                ORANGE,
                format!("Sende an {peer} – keine Antwort (läuft PadBridge dort mit „Empfangen“? Firewall?)"),
            );
        }
        if l.midi {
            hint(ui, "MIDI läuft über diese Verbindung (MIDI-Tab, Ziel: Netzwerk-Gegenstelle).");
        }
        if l.input {
            hint(ui, "Tastatur/Maus läuft über diese Verbindung.");
        }
        for d in &self.snap.devices {
            if let Some(pad) = d.net_pad {
                hint(ui, format!("{} -> virtuelles Pad {}", d.info.name, pad + 1));
            }
        }
    }

    fn receiver_status(&self, ui: &mut egui::Ui) {
        let Some(r) = &self.snap.receiver else {
            status(ui, GREY, "Aus");
            return;
        };
        if let Some(e) = &r.error {
            status(ui, ORANGE, e.clone());
        }
        match (r.listening, &r.peer) {
            (Some(port), Some(peer)) => status(ui, GREEN, format!("Empfange auf Port {port} von {peer}")),
            (Some(port), None) => status(ui, GREY, format!("Bereit auf Port {port}, noch keine Gegenstelle")),
            _ => {}
        }
        for (index, name, active) in &r.pads {
            let state = if *active { "aktiv" } else { "still" };
            hint(ui, format!("Virtuelles Pad {} ({name}) · {state}", index + 1));
        }
        if r.keymouse {
            hint(ui, "Virtuelle Tastatur/Maus angelegt");
        }
        if r.midi_packets > 0 {
            hint(ui, format!("{} MIDI-Pakete empfangen", r.midi_packets));
        }
    }
}

/// Draws `text` as a QR code, black on white with the quiet zone.
fn qr_code(ui: &mut egui::Ui, text: &str) {
    let Ok(code) = QrCode::new(text.as_bytes()) else { return };
    let modules = code.width();
    let colors = code.to_colors();
    let quiet = 4;
    let cell = 4.0;
    let side = (modules + 2 * quiet) as f32 * cell;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::WHITE);
    for (i, color) in colors.iter().enumerate() {
        if *color == Color::Dark {
            let (x, y) = ((i % modules + quiet) as f32, (i / modules + quiet) as f32);
            let min = rect.min + egui::vec2(x * cell, y * cell);
            painter.rect_filled(egui::Rect::from_min_size(min, egui::vec2(cell, cell)), 0.0, egui::Color32::BLACK);
        }
    }
}
