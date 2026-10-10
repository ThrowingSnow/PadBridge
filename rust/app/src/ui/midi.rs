use eframe::egui;
use padbridge_core::midi::{ButtonMode, MidiMapping, MidiMessageKind, MidiTarget, VelocitySource};
use padbridge_core::Control;

use super::{hint, status, App, GREEN, GREY, ORANGE};

impl App {
    pub(super) fn midi_tab(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        if let Some(e) = &self.snap.midi_error {
            status(ui, ORANGE, format!("MIDI-Port nicht verfügbar: {e}"));
        }
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut self.midi.enabled, "MIDI senden").changed();
            ui.add_space(16.0);
            ui.label("Ziel");
            changed |= ui.selectable_value(&mut self.midi.target, MidiTarget::Local, "Dieser Rechner").changed();
            changed |=
                ui.selectable_value(&mut self.midi.target, MidiTarget::Network, "Netzwerk-Gegenstelle").changed();
        });
        match self.midi.target {
            MidiTarget::Local => {
                hint(ui, "Port „PadBridge“ (unter Linux auch als VirMIDI, wenn snd-virmidi geladen ist).")
            }
            MidiTarget::Network => {
                let link = &self.snap.link;
                if link.answering {
                    status(ui, GREEN, format!("MIDI-Port „PadBridge“ auf {}", link.peer.clone().unwrap_or_default()));
                } else {
                    status(ui, ORANGE, "Keine Antwort von der Gegenstelle – Host im Netzwerk-Tab prüfen");
                }
            }
        }
        hint(ui, "MIDI-Clock an „PadBridge“ senden, damit der LED-Ring im Takt blinkt (Monitor-Tab).");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Kanal");
            changed |= ui.add(egui::DragValue::new(&mut self.midi.channel).range(1..=16)).changed();
            ui.add_space(16.0);
            ui.label("Stick-Deadzone");
            changed |= ui.add(egui::Slider::new(&mut self.midi.stick_deadzone, 0.0..=0.4)).changed();
            ui.label("Trigger-Deadzone");
            changed |= ui.add(egui::Slider::new(&mut self.midi.trigger_deadzone, 0.0..=0.2)).changed();
            if ui.button("Standard").clicked() {
                self.midi = MidiMapping { target: self.midi.target, ..MidiMapping::default() };
                changed = true;
            }
        });
        ui.separator();
        changed |= self.binding_table(ui);
        if changed {
            self.set_midi();
        }
    }

    fn binding_table(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let conflicts = self.midi.conflicts();
        let live = self
            .snap
            .devices
            .iter()
            .find(|d| Some(&d.info.id) == self.selected.as_ref())
            .or(self.snap.devices.first())
            .map(|d| d.midi_values.clone())
            .unwrap_or_default();
        egui::Grid::new("midi-bindings").striped(true).num_columns(9).spacing([12.0, 4.0]).show(ui, |ui| {
            for title in ["Control", "Aktiv", "Typ", "Nummer", "Modus", "Invert", "Velocity", "", "Live"] {
                ui.label(egui::RichText::new(title).small().strong());
            }
            ui.end_row();
            for (i, b) in self.midi.bindings.iter_mut().enumerate() {
                let is_axis = matches!(b.control, Control::Axis(_));
                let name = egui::RichText::new(b.control.display_name());
                ui.label(if conflicts.contains(&b.control) {
                    name.color(egui::Color32::from_rgb(230, 80, 60))
                } else {
                    name
                });
                changed |= ui.checkbox(&mut b.enabled, "").changed();
                ui.add_enabled_ui(b.enabled, |ui| {
                    if is_axis {
                        ui.label("CC");
                    } else {
                        egui::ComboBox::from_id_salt(("kind", i)).width(60.0).selected_text(kind_name(b.kind)).show_ui(
                            ui,
                            |ui| {
                                for k in [MidiMessageKind::Cc, MidiMessageKind::Note] {
                                    changed |= ui.selectable_value(&mut b.kind, k, kind_name(k)).changed();
                                }
                            },
                        );
                    }
                });
                changed |= ui.add_enabled(b.enabled, egui::DragValue::new(&mut b.number).range(0..=127)).changed();
                ui.add_enabled_ui(b.enabled && !is_axis, |ui| {
                    egui::ComboBox::from_id_salt(("mode", i))
                        .width(90.0)
                        .selected_text(mode_name(b.button_mode))
                        .show_ui(ui, |ui| {
                            for m in [ButtonMode::Momentary, ButtonMode::Toggle] {
                                changed |= ui.selectable_value(&mut b.button_mode, m, mode_name(m)).changed();
                            }
                        });
                });
                changed |= ui.add_enabled(b.enabled && is_axis, egui::Checkbox::without_text(&mut b.invert)).changed();
                ui.add_enabled_ui(b.enabled && !is_axis, |ui| {
                    egui::ComboBox::from_id_salt(("vel", i))
                        .width(80.0)
                        .selected_text(velocity_name(b.velocity_source))
                        .show_ui(ui, |ui| {
                            for v in [VelocitySource::Fixed, VelocitySource::LeftTrigger, VelocitySource::RightTrigger]
                            {
                                changed |= ui.selectable_value(&mut b.velocity_source, v, velocity_name(v)).changed();
                            }
                        });
                });
                let fixed = b.enabled && !is_axis && b.velocity_source == VelocitySource::Fixed;
                changed |= ui.add_enabled(fixed, egui::DragValue::new(&mut b.velocity).range(1..=127)).changed();
                match live.get(&b.control) {
                    Some(v) => {
                        ui.add(egui::ProgressBar::new(*v as f32 / 127.0).desired_width(90.0).text(v.to_string()))
                    }
                    None => ui.label(egui::RichText::new("–").color(GREY)),
                };
                ui.end_row();
            }
        });
        if !conflicts.is_empty() {
            hint(ui, "Rot: mehrere Controls senden dieselbe Nachricht.");
        }
        changed
    }
}

fn kind_name(k: MidiMessageKind) -> &'static str {
    match k {
        MidiMessageKind::Cc => "CC",
        MidiMessageKind::Note => "Note",
    }
}

fn mode_name(m: ButtonMode) -> &'static str {
    match m {
        ButtonMode::Momentary => "Momentary",
        ButtonMode::Toggle => "Toggle",
    }
}

fn velocity_name(v: VelocitySource) -> &'static str {
    match v {
        VelocitySource::Fixed => "Fix",
        VelocitySource::LeftTrigger => "LT",
        VelocitySource::RightTrigger => "RT",
    }
}
