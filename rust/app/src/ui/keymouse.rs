use eframe::egui;
use padbridge_core::keymouse::{
    EmulationTarget, KeyAction, KeyCode, KeyMouseMapping, Modifier, MouseButton, StickRole,
};

use super::{hint, status, App, GREEN, ORANGE};

#[derive(PartialEq)]
enum Kind {
    None,
    Mouse,
    Key,
}

impl App {
    pub(super) fn keymouse_tab(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut self.keymouse.enabled, "Tastatur/Maus aktiv").changed();
                ui.horizontal(|ui| {
                    ui.label("Ziel");
                    changed |= ui
                        .selectable_value(&mut self.keymouse.target, EmulationTarget::Local, "Dieser Rechner")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut self.keymouse.target, EmulationTarget::Network, "Netzwerk-Gegenstelle")
                        .changed();
                });
                match self.keymouse.target {
                    EmulationTarget::Local => match &self.snap.keymouse_error {
                        Some(e) => status(ui, ORANGE, format!("Nicht verfügbar: {e}")),
                        None => hint(ui, "Virtuelle Tastatur/Maus „PadBridge Keyboard/Mouse“ auf diesem Rechner."),
                    },
                    EmulationTarget::Network => {
                        if self.snap.link.answering {
                            status(ui, GREEN, "Virtuelle Tastatur/Maus auf der Gegenstelle");
                        } else {
                            status(ui, ORANGE, "Keine Antwort von der Gegenstelle – Host im Netzwerk-Tab prüfen");
                        }
                    }
                }
                hint(ui, "Läuft parallel zum Gamepad: am besten Knöpfe belegen, die das Spiel nicht braucht.");
                if ui.button("Desktop-Preset laden").clicked() {
                    self.keymouse = KeyMouseMapping {
                        enabled: self.keymouse.enabled,
                        target: self.keymouse.target,
                        ..KeyMouseMapping::default()
                    };
                    changed = true;
                }
            });
            ui.add_space(32.0);
            egui::Grid::new("sticks").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                for (label, role) in
                    [("Linker Stick", &mut self.keymouse.left_stick), ("Rechter Stick", &mut self.keymouse.right_stick)]
                {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(label).selected_text(role.display_name()).show_ui(ui, |ui| {
                        for r in StickRole::ALL {
                            changed |= ui.selectable_value(role, r, r.display_name()).changed();
                        }
                    });
                    ui.end_row();
                }
                ui.label("Maus-Tempo");
                changed |=
                    ui.add(egui::Slider::new(&mut self.keymouse.mouse_speed, 200.0..=4000.0).suffix(" pt/s")).changed();
                ui.end_row();
                ui.label("Scroll-Tempo");
                changed |=
                    ui.add(egui::Slider::new(&mut self.keymouse.scroll_speed, 2.0..=40.0).suffix(" /s")).changed();
                ui.end_row();
                ui.label("Kurve");
                changed |= ui
                    .add(egui::Slider::new(&mut self.keymouse.curve, 1.0..=3.0))
                    .on_hover_text("1 = linear, höher = feiner um die Mitte, schnell am Rand")
                    .changed();
                ui.end_row();
                ui.label("Deadzone");
                changed |= ui.add(egui::Slider::new(&mut self.keymouse.stick_deadzone, 0.0..=0.4)).changed();
                ui.end_row();
            });
        });
        ui.separator();
        egui::Grid::new("key-bindings").striped(true).num_columns(4).spacing([12.0, 4.0]).show(ui, |ui| {
            for title in ["Control", "Aktion", "Belegung", "Modifier"] {
                ui.label(egui::RichText::new(title).small().strong());
            }
            ui.end_row();
            for (i, binding) in self.keymouse.bindings.iter_mut().enumerate() {
                ui.label(binding.control.display_name());
                let action = &mut binding.action;
                let mut kind = match action {
                    KeyAction::None => Kind::None,
                    KeyAction::Mouse(_) => Kind::Mouse,
                    KeyAction::Key(..) => Kind::Key,
                };
                let before = matches!(kind, Kind::None) as u8 + 2 * matches!(kind, Kind::Mouse) as u8;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut kind, Kind::None, "Aus");
                    ui.selectable_value(&mut kind, Kind::Mouse, "Maus");
                    ui.selectable_value(&mut kind, Kind::Key, "Taste");
                });
                let after = matches!(kind, Kind::None) as u8 + 2 * matches!(kind, Kind::Mouse) as u8;
                if before != after {
                    *action = match kind {
                        Kind::None => KeyAction::None,
                        Kind::Mouse => KeyAction::Mouse(MouseButton::Left),
                        Kind::Key => KeyAction::key(KeyCode::Space),
                    };
                    changed = true;
                }
                match action {
                    KeyAction::None => {
                        ui.label("–");
                        ui.label("");
                    }
                    KeyAction::Mouse(button) => {
                        egui::ComboBox::from_id_salt(("mouse", i)).selected_text(button.display_name()).show_ui(
                            ui,
                            |ui| {
                                for b in MouseButton::ALL {
                                    changed |= ui.selectable_value(button, b, b.display_name()).changed();
                                }
                            },
                        );
                        ui.label("");
                    }
                    KeyAction::Key(key, mods) => {
                        egui::ComboBox::from_id_salt(("key", i))
                            .height(320.0)
                            .selected_text(key.display_name())
                            .show_ui(ui, |ui| {
                                for k in KeyCode::ALL {
                                    changed |= ui.selectable_value(key, k, k.display_name()).changed();
                                }
                            });
                        ui.horizontal(|ui| {
                            for m in Modifier::ALL {
                                let on = mods.contains(&m);
                                if ui.selectable_label(on, m.symbol()).on_hover_text(modifier_name(m)).clicked() {
                                    if on {
                                        mods.retain(|x| *x != m);
                                    } else {
                                        mods.push(m);
                                    }
                                    // Stable order (⇧⌃⌥⌘) so combos press consistently.
                                    mods.sort();
                                    changed = true;
                                }
                            }
                        });
                    }
                }
                ui.end_row();
            }
        });
        if changed {
            self.set_keymouse();
        }
    }
}

fn modifier_name(m: Modifier) -> &'static str {
    match m {
        Modifier::Shift => "Shift",
        Modifier::Control => "Ctrl",
        Modifier::Option => "Alt / Option",
        Modifier::Command => "Super / Cmd",
    }
}
