//! "MIDI-Geräte" tab: what external MIDI devices (control surfaces) send.
//! Known surfaces (Launch Control XL, MIDImix) get a drawing of their front
//! panel with live values; clicking a control sets what it sends through
//! "PadBridge" and its LED.
//! Every device gets a table of all controls seen and a message log.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Popup, PopupCloseBehavior, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Vec2,
};
use padbridge_core::midi::digitakt::{self, Page};
use padbridge_core::midi::models::{Ctl, CtlKind, Model};
use padbridge_core::midi::surface::{
    meter_colour, DeviceMapping, LedColour, LedMode, SurfaceBinding, SurfaceKey, SurfaceKind, SurfaceState,
};
use padbridge_core::midi::{ButtonMode, MidiMessageKind, MidiTarget};

use super::{hint, App, ORANGE};
use crate::engine::{Command, MidiInputView};

/// How long a changed control stays highlighted.
const FLASH: Duration = Duration::from_millis(400);

impl App {
    pub(super) fn surfaces_tab(&mut self, ui: &mut egui::Ui) {
        let inputs = self.snap.midi_inputs.clone();
        // A device plugged in while the app runs is shown right away.
        if !self.surfaces_seen.is_empty() {
            if let Some(new) = inputs.iter().find(|i| !self.surfaces_seen.contains(&i.name)) {
                self.surface = Some(new.name.clone());
            }
        }
        self.surfaces_seen = inputs.iter().map(|i| i.name.clone()).collect();
        if self.surface.as_ref().is_none_or(|n| !inputs.iter().any(|i| &i.name == n)) {
            // Prefer a known surface over e.g. the IAC bus.
            self.surface = inputs
                .iter()
                .find(|i| Model::of(&i.name).is_some())
                .or(inputs.first())
                .map(|i| i.name.clone());
        }
        let Some(input) = inputs.iter().find(|i| Some(&i.name) == self.surface.as_ref()) else {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.heading("Kein MIDI-Gerät gefunden");
                hint(ui, "Ein MIDI-Gerät (z.B. Launch Control XL) per USB einstecken.");
            });
            return;
        };
        let model = Model::of(&input.name);

        ui.horizontal(|ui| {
            if inputs.len() > 1 {
                egui::ComboBox::from_id_salt("surface").selected_text(&input.name).show_ui(ui, |ui| {
                    for i in &inputs {
                        ui.selectable_value(&mut self.surface, Some(i.name.clone()), &i.name);
                    }
                });
            } else {
                ui.strong(&input.name);
            }
            if let Some(ch) = input.state.channel {
                match model.and_then(|m| m.template_name(ch)) {
                    Some(t) => hint(ui, format!("Template {t} · Kanal {}", ch + 1)),
                    None => hint(ui, format!("zuletzt Kanal {}", ch + 1)),
                }
            }
        });

        let mut changed = false;
        // A peer's device is only shown; its settings live where it is plugged in.
        let mut scratch = DeviceMapping::default();
        let mapping = match &input.remote {
            Some(_) => &mut scratch,
            None => self.surfaces.devices.entry(input.name.clone()).or_default(),
        };
        if let Some(device) = &input.remote {
            hint(
                ui,
                format!(
                    "Gerät der Netzwerk-Gegenstelle, nur anzeigen. Es spielt hier auf den MIDI-Port „{}“; \
                     einstellen am Rechner, an dem es steckt.",
                    crate::platform::device_port_name(device)
                ),
            );
        } else {
            changed |= ui.checkbox(&mut mapping.enabled, "Umbelegen und weiterleiten").changed();
        }
        let local = input.remote.is_none();
        if local && mapping.enabled {
            ui.horizontal(|ui| {
                ui.label("Ziel");
                changed |= ui.selectable_value(&mut mapping.target, MidiTarget::Local, "Dieser Rechner").changed();
                changed |=
                    ui.selectable_value(&mut mapping.target, MidiTarget::Network, "Netzwerk-Gegenstelle").changed();
            });
            match mapping.target {
                MidiTarget::Local => hint(
                    ui,
                    "Im Programm (Bitwig, KodeLife…) den Port „PadBridge“ statt des Geräts verwenden, sonst kommt alles doppelt.",
                ),
                MidiTarget::Network => {
                    let port = crate::platform::device_port_name(&input.name);
                    hint(
                        ui,
                        &format!(
                            "Geht an den Host aus dem Netzwerk-Tab. Dort erscheint der MIDI-Port „{port}“ \
                             (Linux: über die nächste VirMIDI, dafür snd-virmidi mit midi_devs=4 laden)."
                        ),
                    );
                }
            }
            if model.is_some() {
                let per_template = model.is_some_and(|m| m.template_name(0).is_some());
                hint(
                    ui,
                    if per_template {
                        "Regler oder Taste anklicken zum Einstellen. Die Einstellungen gelten pro Template."
                    } else {
                        "Regler oder Taste anklicken zum Einstellen."
                    },
                );
            }
        } else if local {
            hint(ui, "Nur anzeigen. Andere Programme können das Gerät gleichzeitig direkt benutzen.");
        }
        ui.add_space(8.0);

        // Lists beside the drawing when there is room, else below it.
        const SIDE: f32 = 330.0;
        let drawn = model.is_some() || digitakt::is_digitakt(&input.name);
        let beside = drawn && ui.available_width() >= 560.0 + SIDE;
        if drawn {
            if beside {
                ui.horizontal_top(|ui| {
                    let width = ui.available_width() - SIDE - 16.0;
                    let height = ui
                        .allocate_ui(Vec2::new(width, 0.0), |ui| {
                            changed |= draw_device(ui, model, input, mapping);
                        })
                        .response
                        .rect
                        .height();
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.set_width(SIDE);
                        lists_stacked(ui, input, height.max(320.0));
                    });
                });
            } else {
                changed |= draw_device(ui, model, input, mapping);
                ui.add_space(12.0);
            }
        }
        if changed && input.remote.is_none() {
            self.handle.send(Command::SetSurfaces(self.surfaces.clone()));
        }
        if !beside {
            ui.columns(2, |cols| {
                cols[0].strong("Alle Werte");
                value_table(&mut cols[0], input, 220.0);
                cols[1].strong("Letzte Meldungen");
                message_log(&mut cols[1], input, 220.0);
            });
        }
    }
}

fn draw_device(ui: &mut egui::Ui, model: Option<Model>, input: &MidiInputView, mapping: &mut DeviceMapping) -> bool {
    match model {
        Some(m) => surface_view(ui, m, input, mapping),
        None => {
            digitakt_view(ui, input);
            false
        }
    }
}

/// "Letzte Meldungen" above "Alle Werte", together `height` tall.
fn lists_stacked(ui: &mut egui::Ui, input: &MidiInputView, height: f32) {
    let log = (height * 0.35).max(100.0);
    ui.strong("Letzte Meldungen");
    message_log(ui, input, log);
    ui.add_space(8.0);
    ui.strong("Alle Werte");
    value_table(ui, input, (height - log - 60.0).max(120.0));
}

fn message_log(ui: &mut egui::Ui, input: &MidiInputView, height: f32) {
    egui::ScrollArea::vertical()
        .id_salt("surface-log")
        .max_height(height)
        .auto_shrink([false, true])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if input.recent.is_empty() {
                hint(ui, "Noch nichts empfangen. Einen Regler bewegen.");
            }
            for line in &input.recent {
                ui.monospace(line);
            }
        });
}

fn value_table(ui: &mut egui::Ui, input: &MidiInputView, height: f32) {
    if input.state.values.is_empty() {
        hint(ui, "–");
        return;
    }
    egui::ScrollArea::vertical().id_salt("surface-values").max_height(height).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("surface-values-grid").num_columns(3).spacing([12.0, 2.0]).striped(true).show(ui, |ui| {
            for (key, v) in &input.state.values {
                ui.monospace(key_name(*key));
                let fraction = v.value as f32 / key.max_value() as f32;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(110.0, 10.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(2), ui.visuals().extreme_bg_color);
                let mut fill = rect;
                fill.set_width(rect.width() * fraction);
                let colour = if flashing(input, *key).is_some() { ORANGE } else { ui.visuals().selection.bg_fill };
                p.rect_filled(fill, CornerRadius::same(2), colour);
                let text = match key.kind {
                    SurfaceKind::Note if v.on => format!("an {}", v.value),
                    SurfaceKind::Note => "aus".into(),
                    SurfaceKind::Pitch => format!("{}", v.value as i32 - 8192),
                    _ => v.value.to_string(),
                };
                ui.monospace(text);
                ui.end_row();
            }
        });
    });
}

fn key_name(key: SurfaceKey) -> String {
    let ch = key.channel + 1;
    match key.kind {
        SurfaceKind::Note => format!("K{ch} Note {}", key.number),
        SurfaceKind::Cc => format!("K{ch} CC {}", key.number),
        SurfaceKind::Program => format!("K{ch} Programm"),
        SurfaceKind::Pressure if key.number == 128 => format!("K{ch} Druck"),
        SurfaceKind::Pressure => format!("K{ch} Druck {}", key.number),
        SurfaceKind::Pitch => format!("K{ch} Pitch"),
    }
}

/// 1.0 right after a change, fading to None.
fn flashing(input: &MidiInputView, key: SurfaceKey) -> Option<f32> {
    let age = Instant::now().saturating_duration_since(*input.changed.get(&key)?);
    (age < FLASH).then(|| 1.0 - age.as_secs_f32() / FLASH.as_secs_f32())
}

// MARK: Drawing

struct Painter<'a> {
    p: &'a egui::Painter,
    origin: Pos2,
    s: f32,
}

impl Painter<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.s
    }

    fn rect(&self, ctl: &Ctl) -> Rect {
        Rect::from_center_size(self.at(ctl.at.0, ctl.at.1), Vec2::new(ctl.size.0, ctl.size.1) * self.s)
    }

    fn font(&self, size: f32) -> FontId {
        FontId::proportional((size * self.s).max(7.0))
    }
}

/// The front panel; returns true when a setting changed.
fn surface_view(ui: &mut egui::Ui, model: Model, input: &MidiInputView, mapping: &mut DeviceMapping) -> bool {
    let body = Vec2::new(model.body().0, model.body().1);
    let mut s = ui.available_width() / body.x;
    if body.y > body.x {
        // Upright devices (Xone:K2) would grow taller than the window.
        s = s.min(720.0 / body.y);
    }
    let s = s.clamp(0.6, 1.5);
    let (rect, _) = ui.allocate_exact_size(body * s, Sense::hover());
    let painter = ui.painter_at(rect);
    let d = Painter { p: &painter, origin: rect.min, s };
    let ch = input.state.channel.unwrap_or(model.default_channel());

    let label = Color32::from_gray(150);
    painter.rect_filled(rect, CornerRadius::same((14.0 * s) as u8), Color32::from_gray(30));
    painter.rect_stroke(rect, CornerRadius::same((14.0 * s) as u8), Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Inside);
    for c in model.captions() {
        let align = if c.centred { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
        painter.text(d.at(c.x, c.y), align, c.text, d.font(8.0), label);
    }

    let mut changed = false;
    for ctl in model.controls() {
        let key = ctl.input(ch);
        let r = d.rect(&ctl);
        let response = ui.interact(r, Id::new(("surface", ch, key)), if mapping.enabled { Sense::click() } else { Sense::hover() });
        let binding = mapping.binding(key).copied().filter(|_| mapping.enabled);
        let look = Look {
            flash: flashing(input, key),
            binding,
            toggled: input.toggled.contains(&key),
            hovered: response.hovered() && mapping.enabled,
        };
        match ctl.kind {
            CtlKind::Knob => knob(&d, r, &input.state, key, &look),
            CtlKind::Encoder => encoder(&d, r, &input.state, key, &look),
            CtlKind::Fader | CtlKind::Strip => fader(&d, r, &input.state, key, &look),
            CtlKind::Button => button(&d, r, &input.state, key, &look, &ctl),
        }
        if mapping.enabled {
            if look.hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            Popup::from_toggle_button_response(&response).close_behavior(PopupCloseBehavior::CloseOnClickOutside).show(
                |ui| {
                    ui.set_min_width(240.0);
                    changed |= binding_editor(ui, model, &ctl, key, mapping);
                },
            );
        } else {
            response.on_hover_text(tip(&input.state, key));
        }
    }
    changed
}

fn tip(st: &SurfaceState, key: SurfaceKey) -> String {
    let value = match st.get(key) {
        None => "noch nichts empfangen".into(),
        Some(v) if key.kind == SurfaceKind::Note => {
            if v.on {
                format!("gedrückt ({})", v.value)
            } else {
                "losgelassen".into()
            }
        }
        Some(v) => format!("Wert {}", v.value),
    };
    format!("{} · {value}", key_name(key))
}

fn binding_editor(ui: &mut egui::Ui, model: Model, ctl: &Ctl, key: SurfaceKey, mapping: &mut DeviceMapping) -> bool {
    let mut changed = false;
    let button = ctl.is_button();
    ui.strong(&ctl.name);
    hint(ui, format!("Gerät sendet {}", key_name(key)));
    if ctl.kind == CtlKind::Encoder {
        hint(ui, "Endlos-Encoder: sendet Schritte (1–63 rechts, 127–64 links), die DAW muss ihn als relativ kennen.");
    }
    let b = mapping.binding_mut(key, button);
    ui.horizontal(|ui| {
        if button {
            for k in [MidiMessageKind::Cc, MidiMessageKind::Note] {
                changed |= ui.selectable_value(&mut b.kind, k, super::midi::kind_name(k)).changed();
            }
        } else {
            ui.label("CC");
        }
        changed |= ui.add(egui::DragValue::new(&mut b.number).range(0..=127)).changed();
        ui.label("Kanal");
        let mut channel = b.channel + 1;
        if ui.add(egui::DragValue::new(&mut channel).range(1..=16)).changed() {
            b.channel = channel - 1;
            changed = true;
        }
    });
    if button {
        ui.horizontal(|ui| {
            for m in [ButtonMode::Momentary, ButtonMode::Toggle] {
                changed |= ui.selectable_value(&mut b.mode, m, super::midi::mode_name(m)).changed();
            }
        });
    }
    let colour = ctl.led.is_some_and(|i| model.colour_led(i));
    if ctl.led.is_some() && !colour {
        ui.horizontal(|ui| {
            ui.label("LED");
            changed |= ui.selectable_value(&mut b.led, LedColour::Off, "aus").changed();
            // Any colour means "on"; the device has only one.
            let mut on = b.led != LedColour::Off;
            if ui.selectable_value(&mut on, true, "an").clicked() && b.led == LedColour::Off {
                b.led = LedColour::Yellow;
                changed = true;
            }
        });
        if let Some(text) = ctl.led_hint {
            hint(ui, text);
        }
    } else if ctl.led.is_some() {
        if !button {
            ui.horizontal(|ui| {
                ui.label("LED-Anzeige");
                for (m, name) in [
                    (LedMode::Fixed, "Fest"),
                    (LedMode::Brightness, "Helligkeit"),
                    (LedMode::Centre, "Mitte hell"),
                    (LedMode::Meter, "Ampel"),
                ] {
                    changed |= ui.selectable_value(&mut b.led_mode, m, name).changed();
                }
            });
        }
        if button || b.led_mode != LedMode::Meter {
            ui.horizontal(|ui| {
                ui.label("LED");
                for c in LedColour::ALL {
                    let text = egui::RichText::new(led_name(c)).color(led_colour(c, 3).unwrap_or(ui.visuals().text_color()));
                    changed |= ui.selectable_value(&mut b.led, c, text).changed();
                }
            });
        }
        match b.led_mode {
            LedMode::Meter if !button => hint(ui, "0 grün, 63 gelb, 127 rot, mit Übergängen dazwischen."),
            LedMode::Centre if !button => hint(ui, "Bei 64 voll, Richtung 0 und 127 dunkler, an den Enden aus."),
            _ => {}
        }
        if let Some(text) = ctl.led_hint {
            hint(ui, text);
        }
    }
    let default = SurfaceBinding::passthrough(key, button);
    if *b != default {
        ui.separator();
        if ui.button("Zurücksetzen (wie Gerät)").clicked() {
            mapping.bindings.retain(|x| x.input != key);
            changed = true;
        }
    }
    changed
}

fn led_name(c: LedColour) -> &'static str {
    match c {
        LedColour::Off => "aus",
        LedColour::Red => "rot",
        LedColour::Amber => "amber",
        LedColour::Yellow => "gelb",
        LedColour::Green => "grün",
    }
}

/// Screen colour of an LED at `level` 0..=3; None when dark.
fn led_colour(c: LedColour, level: u8) -> Option<Color32> {
    let (r, g) = c.red_green();
    if level == 0 {
        return None;
    }
    red_green_colour(r, g).map(|base| blend(Color32::from_gray(60), base, 0.35 + 0.65 * level as f32 / 3.0))
}

/// Screen colour of red and green LED levels 0..=3.
fn red_green_colour(r: u8, g: u8) -> Option<Color32> {
    (r > 0 || g > 0).then(|| Color32::from_rgb(r * 85, g * 80, 20))
}

struct Look {
    flash: Option<f32>,
    binding: Option<SurfaceBinding>,
    toggled: bool,
    hovered: bool,
}

impl Look {
    /// What the control sends, as a short label.
    fn out_label(&self, key: SurfaceKey) -> String {
        if key.kind == SurfaceKind::Pitch && self.binding.is_none() {
            return "PB".into();
        }
        let (kind, number, channel) = match self.binding {
            Some(b) => (b.kind, b.number, b.channel),
            None if key.kind == SurfaceKind::Note => (MidiMessageKind::Note, key.number, key.channel),
            None => (MidiMessageKind::Cc, key.number, key.channel),
        };
        let prefix = if kind == MidiMessageKind::Note { "N" } else { "" };
        let ch = if channel != key.channel { format!("/{}", channel + 1) } else { String::new() };
        format!("{prefix}{number}{ch}")
    }

    /// The LED colour for `value`, orange without one.
    fn accent(&self, value: u16) -> Color32 {
        match self.binding {
            Some(b) if b.led_mode == LedMode::Meter => {
                let (r, g) = meter_colour(value);
                red_green_colour(r, g).unwrap_or(ORANGE)
            }
            Some(b) => led_colour(b.led, 3).unwrap_or(ORANGE),
            None => ORANGE,
        }
    }

    fn edge(&self) -> Stroke {
        if self.hovered {
            Stroke::new(2.0, Color32::from_gray(230))
        } else {
            Stroke::new(1.0, Color32::from_gray(80))
        }
    }
}

/// Knob with a 270° value arc in its LED colour; the output number in the
/// middle and the value below.
fn knob(d: &Painter, r: Rect, st: &SurfaceState, key: SurfaceKey, look: &Look) {
    let c = r.center();
    let radius = r.width() / 2.0;
    let value = st.get(key).map(|v| v.value as f32 / 127.0);
    d.p.circle_filled(c, radius * 0.72, Color32::from_gray(50));
    if look.hovered {
        d.p.circle_stroke(c, radius * 0.72, look.edge());
    }
    let start = 135f32.to_radians(); // lower left, clockwise to lower right
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        let n = 32;
        (0..=n)
            .map(|i| {
                let a = start + (from + (to - from) * i as f32 / n as f32) * 270f32.to_radians();
                c + Vec2::new(a.cos(), a.sin()) * radius * 0.9
            })
            .collect()
    };
    d.p.add(Shape::line(arc(0.0, 1.0), Stroke::new(3.0 * d.s, Color32::from_gray(65))));
    if let Some(v) = value {
        let colour = blend(look.accent((v * 127.0) as u16), Color32::WHITE, look.flash.unwrap_or(0.0) * 0.6);
        if v > 0.0 {
            d.p.add(Shape::line(arc(0.0, v), Stroke::new(3.0 * d.s, colour)));
        }
        let a = start + v * 270f32.to_radians();
        let dir = Vec2::new(a.cos(), a.sin());
        d.p.line_segment([c + dir * radius * 0.5, c + dir * radius * 0.7], Stroke::new(2.0 * d.s, Color32::from_gray(220)));
    }
    d.p.text(c, Align2::CENTER_CENTER, look.out_label(key), d.font(8.0), Color32::from_gray(140));
    if let Some(v) = st.get(key) {
        d.p.text(c + Vec2::new(0.0, radius * 1.22), Align2::CENTER_CENTER, v.value.to_string(), d.font(8.5), Color32::from_gray(190));
    }
}

/// An endless encoder has no position: a closed ring that lights up while it
/// turns, and the last step ("+2", "−1") underneath.
fn encoder(d: &Painter, r: Rect, st: &SurfaceState, key: SurfaceKey, look: &Look) {
    let c = r.center();
    let radius = r.width() / 2.0;
    d.p.circle_filled(c, radius * 0.72, Color32::from_gray(50));
    if look.hovered {
        d.p.circle_stroke(c, radius * 0.72, look.edge());
    }
    let ring = match look.flash {
        Some(f) => blend(Color32::from_gray(65), look.accent(64), f),
        None => Color32::from_gray(65),
    };
    d.p.circle_stroke(c, radius * 0.9, Stroke::new(3.0 * d.s, ring));
    d.p.text(c, Align2::CENTER_CENTER, look.out_label(key), d.font(8.0), Color32::from_gray(140));
    if let Some(v) = st.get(key) {
        let step = match v.value {
            0 => "0".to_string(),
            n @ 1..=63 => format!("+{n}"),
            n => format!("−{}", 128 - n as u16),
        };
        d.p.text(c + Vec2::new(0.0, radius * 1.22), Align2::CENTER_CENTER, step, d.font(8.5), Color32::from_gray(190));
    }
}

fn fader(d: &Painter, r: Rect, st: &SurfaceState, key: SurfaceKey, look: &Look) {
    let x = r.center().x;
    let top = Pos2::new(x, r.top() + 7.0 * d.s);
    let bottom = Pos2::new(x, r.bottom() - 7.0 * d.s);
    let slot = Rect::from_min_max(top - Vec2::new(2.5 * d.s, 0.0), bottom + Vec2::new(2.5 * d.s, 0.0));
    d.p.rect_filled(slot, CornerRadius::same(2), Color32::from_gray(12));
    // Pitch bend (touch strip) has 14 bits, everything else 7.
    let v = st.get(key).map(|v| v.value as f32 / key.max_value() as f32);
    if let Some(v) = v {
        let y = bottom.y + (top.y - bottom.y) * v;
        let colour = blend(look.accent((v * 127.0) as u16), Color32::WHITE, look.flash.unwrap_or(0.0) * 0.6);
        d.p.rect_filled(Rect::from_min_max(Pos2::new(slot.left(), y), slot.max), CornerRadius::same(2), colour);
    }
    let y = bottom.y + (top.y - bottom.y) * v.unwrap_or(0.0);
    let cap = Rect::from_center_size(Pos2::new(x, y), Vec2::new(26.0, 12.0) * d.s);
    d.p.rect_filled(cap, CornerRadius::same(2), Color32::from_gray(if v.is_some() { 200 } else { 110 }));
    d.p.rect_stroke(cap, CornerRadius::same(2), look.edge(), StrokeKind::Outside);
    d.p.text(r.center_top() - Vec2::new(0.0, 5.0 * d.s), Align2::CENTER_CENTER, look.out_label(key), d.font(8.0), Color32::from_gray(140));
    if let Some(value) = st.get(key) {
        d.p.text(r.center_bottom() + Vec2::new(0.0, 6.0 * d.s), Align2::CENTER_CENTER, value.value.to_string(), d.font(8.5), Color32::from_gray(190));
    }
}

/// Arrow buttons are drawn as a triangle, others show caption and output.
fn button(d: &Painter, r: Rect, st: &SurfaceState, key: SurfaceKey, look: &Look, ctl: &Ctl) {
    let held = st.get(key).is_some_and(|v| v.on && v.value > 0);
    let lit = match look.binding {
        Some(b) if b.mode == ButtonMode::Toggle => look.toggled,
        _ => held,
    };
    let fill = if lit { look.accent(127) } else { Color32::from_gray(55) };
    let fill = blend(fill, Color32::WHITE, look.flash.unwrap_or(0.0) * 0.3);
    let corner = CornerRadius::same((4.0 * d.s) as u8);
    d.p.rect_filled(r, corner, fill);
    d.p.rect_stroke(r, corner, look.edge(), StrokeKind::Inside);
    let ink = if lit { Color32::from_gray(20) } else { Color32::from_gray(200) };
    if let Some(dir) = ctl.arrow {
        let c = r.center();
        let k = 5.0 * d.s;
        let pts = match dir {
            0 => vec![c + Vec2::new(0.0, -k), c + Vec2::new(k, k * 0.7), c + Vec2::new(-k, k * 0.7)],
            1 => vec![c + Vec2::new(0.0, k), c + Vec2::new(-k, -k * 0.7), c + Vec2::new(k, -k * 0.7)],
            2 => vec![c + Vec2::new(-k, 0.0), c + Vec2::new(k * 0.7, -k), c + Vec2::new(k * 0.7, k)],
            _ => vec![c + Vec2::new(k, 0.0), c + Vec2::new(-k * 0.7, k), c + Vec2::new(-k * 0.7, -k)],
        };
        d.p.add(Shape::convex_polygon(pts, ink, Stroke::NONE));
        return;
    }
    let mut text = look.out_label(key);
    if look.binding.is_some_and(|b| b.mode == ButtonMode::Toggle) {
        text.push_str(" T");
    }
    if let Some(caption) = ctl.caption {
        d.p.text(r.center_top() + Vec2::new(0.0, 8.0 * d.s), Align2::CENTER_CENTER, caption, d.font(7.5), ink);
    }
    let y = if ctl.caption.is_some() { r.center().y + 4.0 * d.s } else { r.center().y };
    d.p.text(Pos2::new(r.center().x, y), Align2::CENTER_CENTER, text, d.font(7.5), ink);
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

// MARK: Digitakt II

/// Page and track (channel) of the parameter that changed last.
fn digitakt_focus(input: &MidiInputView) -> (Page, u8, Option<SurfaceKey>) {
    input
        .changed
        .iter()
        .filter(|(k, _)| k.kind == SurfaceKind::Cc)
        .filter_map(|(k, t)| match digitakt::param(k.number) {
            Some((page, Some(_), _)) => Some((t, page, k.channel, *k)),
            _ => None,
        })
        .max_by_key(|(t, ..)| **t)
        .map_or((Page::Fltr, 0, None), |(_, page, ch, k)| (page, ch, Some(k)))
}

/// Front panel after the real one: the screen shows the page of the last
/// parameter turned, the encoders follow it, trig keys light per track.
fn digitakt_view(ui: &mut egui::Ui, input: &MidiInputView) {
    let body = Vec2::new(660.0, 370.0);
    let s = (ui.available_width() / body.x).clamp(0.6, 1.5);
    let (rect, _) = ui.allocate_exact_size(body * s, Sense::hover());
    let painter = ui.painter_at(rect);
    let d = Painter { p: &painter, origin: rect.min, s };
    let st = &input.state;
    let (page, ch, last) = digitakt_focus(input);
    let value = |cc: u8| st.get(SurfaceKey::cc(ch, cc)).map(|v| v.value);

    painter.rect_filled(rect, CornerRadius::same((14.0 * s) as u8), Color32::from_gray(26));
    painter.rect_stroke(rect, CornerRadius::same((14.0 * s) as u8), Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Inside);
    let label = Color32::from_gray(150);

    // Level encoder: track level of the shown track.
    let level_key = SurfaceKey::cc(ch, digitakt::TRACK_LEVEL);
    let level = Rect::from_center_size(d.at(110.0, 80.0), Vec2::splat(44.0 * s));
    let look = Look { flash: flashing(input, level_key), binding: None, toggled: false, hovered: false };
    knob(&d, level, st, level_key, &look);
    painter.text(d.at(110.0, 124.0), Align2::CENTER_CENTER, "LEVEL", d.font(8.0), label);

    // Screen.
    let screen = Rect::from_min_max(d.at(170.0, 28.0), d.at(420.0, 172.0));
    let ink = Color32::from_rgb(220, 225, 210);
    painter.rect_filled(screen, CornerRadius::same((4.0 * s) as u8), Color32::from_rgb(12, 14, 12));
    painter.rect_stroke(screen, CornerRadius::same((4.0 * s) as u8), Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);
    let muted = value(digitakt::TRACK_MUTE).is_some_and(|v| v > 0);
    let head = format!("SPUR {}  ·  {}{}", ch + 1, page.name(), if muted { "  ·  MUTE" } else { "" });
    painter.text(d.at(182.0, 44.0), Align2::LEFT_CENTER, head, d.font(10.0), ink);
    for (i, enc) in page.encoders().iter().enumerate() {
        let (col, row) = (i % 4, i / 4);
        let c = d.at(203.0 + col as f32 * 62.0, 82.0 + row as f32 * 52.0);
        let Some((cc, name)) = enc else {
            painter.text(c, Align2::CENTER_CENTER, "–", d.font(9.0), Color32::from_gray(70));
            continue;
        };
        let hot = last.is_some_and(|k| k.number == *cc);
        let colour = if hot { ORANGE } else { ink };
        painter.text(c - Vec2::new(0.0, 9.0 * s), Align2::CENTER_CENTER, *name, d.font(8.0), colour);
        let text = value(*cc).map_or("–".into(), |v| v.to_string());
        painter.text(c + Vec2::new(0.0, 8.0 * s), Align2::CENTER_CENTER, text, d.font(11.0), colour);
    }

    // Data encoders A-H, two rows of four.
    for (i, enc) in page.encoders().iter().enumerate() {
        let (col, row) = (i % 4, i / 4);
        let at = (470.0 + col as f32 * 52.0, 52.0 + row as f32 * 70.0);
        let r = Rect::from_center_size(d.at(at.0, at.1), Vec2::splat(38.0 * s));
        match enc {
            Some((cc, _)) => {
                let key = SurfaceKey::cc(ch, *cc);
                let look = Look { flash: flashing(input, key), binding: None, toggled: false, hovered: false };
                knob(&d, r, st, key, &look);
            }
            None => {
                painter.circle_filled(r.center(), r.width() * 0.36, Color32::from_gray(45));
            }
        }
        let letter = ["A", "B", "C", "D", "E", "F", "G", "H"][i];
        painter.text(r.center_bottom() + Vec2::new(0.0, 14.0 * s), Align2::CENTER_CENTER, letter, d.font(8.0), label);
    }

    // Page keys; the shown page is lit.
    for (i, (name, p)) in [("SRC", Page::Src), ("FLTR", Page::Fltr), ("AMP", Page::Amp), ("FX", Page::Fx)].into_iter().enumerate() {
        let r = Rect::from_center_size(d.at(470.0 + i as f32 * 52.0, 206.0), Vec2::new(42.0, 20.0) * s);
        let on = p == page;
        painter.rect_filled(r, CornerRadius::same((3.0 * s) as u8), if on { ORANGE } else { Color32::from_gray(55) });
        painter.text(r.center(), Align2::CENTER_CENTER, name, d.font(7.5), if on { Color32::from_gray(20) } else { Color32::from_gray(200) });
    }

    // Trig keys 1-16: lit for the key pressed (auto channel) and while the
    // track on channel n plays a note.
    let on_key = |k: &SurfaceKey, t: u8| k.kind == SurfaceKind::Note && digitakt::trig_key(k.channel, k.number) == Some(t);
    for t in 0..16u8 {
        let (col, row) = (t % 8, t / 8);
        let r = Rect::from_center_size(d.at(150.0 + col as f32 * 60.0, 270.0 + row as f32 * 56.0), Vec2::new(44.0, 38.0) * s);
        let held = st.values.iter().any(|(k, v)| on_key(k, t) && v.on);
        let flash = input
            .changed
            .iter()
            .filter(|(k, _)| on_key(k, t))
            .filter_map(|(k, _)| flashing(input, *k))
            .fold(0.0f32, f32::max);
        let base = if t == ch { Color32::from_gray(80) } else { Color32::from_gray(50) };
        let fill = if held { ORANGE } else { blend(base, ORANGE, flash) };
        painter.rect_filled(r, CornerRadius::same((5.0 * s) as u8), fill);
        let edge = if t == ch { Stroke::new(1.5, Color32::from_gray(200)) } else { Stroke::new(1.0, Color32::from_gray(80)) };
        painter.rect_stroke(r, CornerRadius::same((5.0 * s) as u8), edge, StrokeKind::Inside);
        let ink = if held || flash > 0.5 { Color32::from_gray(20) } else { Color32::from_gray(210) };
        painter.text(r.center(), Align2::CENTER_CENTER, format!("{}", t + 1), d.font(11.0), ink);
        if st.get(SurfaceKey::cc(t, digitakt::TRACK_MUTE)).is_some_and(|v| v.value > 0) {
            painter.circle_filled(r.right_top() + Vec2::new(-6.0, 6.0) * s, 3.0 * s, Color32::from_rgb(220, 60, 50));
        }
    }

    if let Some((_, _, name)) = last.and_then(|k| digitakt::param(k.number)) {
        let v = last.and_then(|k| st.get(k)).map_or(0, |v| v.value);
        painter.text(d.at(170.0, 190.0), Align2::LEFT_CENTER, format!("{name} · Spur {} = {v}", ch + 1), d.font(9.0), label);
    }
}
