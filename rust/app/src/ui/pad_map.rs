//! MIDI tab, picture view: the outline of the pad in hand with a number field
//! for every control around it. Clicking a part of the pad opens all settings
//! of its controls. Drawn with the painter, so it scales and follows the theme.

use std::collections::HashMap;
use std::f32::consts::TAU;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Popup, PopupCloseBehavior, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, UiBuilder, Vec2,
};
use padbridge_core::midi::{ButtonMode, MidiBinding, MidiMapping, MidiMessageKind, VelocitySource};
use padbridge_core::{Control, ControllerState, PadAxis, PadButton};

use super::midi::{kind_name, mode_name, velocity_name};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Look {
    Xbox,
    Ds4,
}

impl Look {
    pub(super) fn of_vendor(vendor_id: u16) -> Look {
        if vendor_id == 0x054c {
            Look::Ds4
        } else {
            Look::Xbox
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Look::Xbox => "Xbox",
            Look::Ds4 => "DualShock 4",
        }
    }
}

const FIELD_W: f32 = 150.0;
const FIELD_H: f32 = 22.0;
const ROW: f32 = 27.0;
/// Design units: the body spans about ±260 horizontally, -170..180 vertically.
const BODY_W: f32 = 540.0;
const BODY_H: f32 = 360.0;

#[derive(Clone, Copy)]
enum PartKind {
    Trigger(PadAxis),
    Bumper(PadButton),
    Stick(PadAxis, PadAxis, PadButton),
    Arrow(PadButton),
    Face(PadButton),
    Pill(PadButton),
    Round(PadButton),
}

/// One clickable part of the pad, in design units relative to the body centre.
#[derive(Clone, Copy)]
struct Part {
    kind: PartKind,
    at: (f32, f32),
    size: (f32, f32),
}

impl Part {
    fn controls(&self) -> Vec<Control> {
        match self.kind {
            PartKind::Trigger(a) => vec![Control::Axis(a)],
            PartKind::Stick(x, y, click) => vec![Control::Axis(x), Control::Axis(y), Control::Button(click)],
            PartKind::Bumper(b) | PartKind::Arrow(b) | PartKind::Face(b) | PartKind::Pill(b) | PartKind::Round(b) => {
                vec![Control::Button(b)]
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
    Bottom,
}

struct Layout {
    parts: Vec<Part>,
    /// (control, label, side); order within a side is top to bottom (left to right at the bottom).
    fields: Vec<(Control, &'static str, Side)>,
}

const fn p(kind: PartKind, x: f32, y: f32, w: f32, h: f32) -> Part {
    Part { kind, at: (x, y), size: (w, h) }
}

fn layout(look: Look) -> Layout {
    use PadAxis::*;
    use PadButton::*;
    use PartKind::*;
    let b = Control::Button;
    let a = Control::Axis;
    match look {
        Look::Xbox => Layout {
            parts: vec![
                p(Trigger(LeftTrigger), -140.0, -150.0, 70.0, 30.0),
                p(Trigger(RightTrigger), 140.0, -150.0, 70.0, 30.0),
                p(Bumper(LeftShoulder), -140.0, -113.0, 120.0, 16.0),
                p(Bumper(RightShoulder), 140.0, -113.0, 120.0, 16.0),
                p(Stick(LeftX, LeftY, LeftStick), -125.0, -40.0, 64.0, 64.0),
                p(Stick(RightX, RightY, RightStick), 65.0, 40.0, 64.0, 64.0),
                p(Arrow(DpadUp), -65.0, 21.0, 18.0, 18.0),
                p(Arrow(DpadLeft), -84.0, 40.0, 18.0, 18.0),
                p(Arrow(DpadRight), -46.0, 40.0, 18.0, 18.0),
                p(Arrow(DpadDown), -65.0, 59.0, 18.0, 18.0),
                p(Face(Y), 135.0, -70.0, 26.0, 26.0),
                p(Face(X), 105.0, -40.0, 26.0, 26.0),
                p(Face(B), 165.0, -40.0, 26.0, 26.0),
                p(Face(A), 135.0, -10.0, 26.0, 26.0),
                p(Pill(Back), -42.0, -40.0, 22.0, 12.0),
                p(Round(Guide), 0.0, -75.0, 32.0, 32.0),
                p(Pill(Start), 42.0, -40.0, 22.0, 12.0),
            ],
            fields: vec![
                (a(LeftTrigger), "LT", Side::Left),
                (b(LeftShoulder), "LB", Side::Left),
                (a(LeftX), "LS X", Side::Left),
                (a(LeftY), "LS Y", Side::Left),
                (b(LeftStick), "LS Klick", Side::Left),
                (b(DpadUp), "Hoch", Side::Left),
                (b(DpadLeft), "Links", Side::Left),
                (b(DpadRight), "Rechts", Side::Left),
                (b(DpadDown), "Runter", Side::Left),
                (a(RightTrigger), "RT", Side::Right),
                (b(RightShoulder), "RB", Side::Right),
                (b(Y), "Y", Side::Right),
                (b(B), "B", Side::Right),
                (b(X), "X", Side::Right),
                (b(A), "A", Side::Right),
                (a(RightX), "RS X", Side::Right),
                (a(RightY), "RS Y", Side::Right),
                (b(RightStick), "RS Klick", Side::Right),
                (b(Back), "Back", Side::Bottom),
                (b(Guide), "Guide", Side::Bottom),
                (b(Start), "Start", Side::Bottom),
            ],
        },
        Look::Ds4 => Layout {
            parts: vec![
                p(Trigger(LeftTrigger), -150.0, -150.0, 70.0, 30.0),
                p(Trigger(RightTrigger), 150.0, -150.0, 70.0, 30.0),
                p(Bumper(LeftShoulder), -150.0, -113.0, 110.0, 16.0),
                p(Bumper(RightShoulder), 150.0, -113.0, 110.0, 16.0),
                p(Arrow(DpadUp), -150.0, -62.0, 20.0, 20.0),
                p(Arrow(DpadLeft), -172.0, -40.0, 20.0, 20.0),
                p(Arrow(DpadRight), -128.0, -40.0, 20.0, 20.0),
                p(Arrow(DpadDown), -150.0, -18.0, 20.0, 20.0),
                p(Face(Y), 150.0, -70.0, 26.0, 26.0),
                p(Face(X), 120.0, -40.0, 26.0, 26.0),
                p(Face(B), 180.0, -40.0, 26.0, 26.0),
                p(Face(A), 150.0, -10.0, 26.0, 26.0),
                p(Stick(LeftX, LeftY, LeftStick), -68.0, 40.0, 60.0, 60.0),
                p(Stick(RightX, RightY, RightStick), 68.0, 40.0, 60.0, 60.0),
                p(Pill(Back), -100.0, -92.0, 12.0, 24.0),
                p(Pill(Start), 100.0, -92.0, 12.0, 24.0),
                p(Round(Guide), 0.0, 30.0, 24.0, 24.0),
            ],
            fields: vec![
                (a(LeftTrigger), "L2", Side::Left),
                (b(LeftShoulder), "L1", Side::Left),
                (b(DpadUp), "Hoch", Side::Left),
                (b(DpadLeft), "Links", Side::Left),
                (b(DpadRight), "Rechts", Side::Left),
                (b(DpadDown), "Runter", Side::Left),
                (a(LeftX), "L-Stick X", Side::Left),
                (a(LeftY), "L-Stick Y", Side::Left),
                (b(LeftStick), "L3", Side::Left),
                (a(RightTrigger), "R2", Side::Right),
                (b(RightShoulder), "R1", Side::Right),
                (b(Y), "Dreieck", Side::Right),
                (b(B), "Kreis", Side::Right),
                (b(X), "Viereck", Side::Right),
                (b(A), "Kreuz", Side::Right),
                (a(RightX), "R-Stick X", Side::Right),
                (a(RightY), "R-Stick Y", Side::Right),
                (b(RightStick), "R3", Side::Right),
                (b(Back), "Share", Side::Bottom),
                (b(Guide), "PS", Side::Bottom),
                (b(Start), "Options", Side::Bottom),
            ],
        },
    }
}

/// Draws the pad and its fields; returns true when a binding changed.
pub(super) fn show(
    ui: &mut egui::Ui,
    look: Look,
    midi: &mut MidiMapping,
    state: &ControllerState,
    live: &HashMap<Control, i32>,
) -> bool {
    let mut changed = false;
    let conflicts = midi.conflicts();
    let layout = layout(look);

    let width = ui.available_width().max(2.0 * FIELD_W + 300.0);
    let scale = ((width - 2.0 * FIELD_W - 30.0) / BODY_W).clamp(0.6, 1.3);
    let side_rows = layout.fields.iter().filter(|f| f.2 == Side::Left).count() as f32;
    let height = (BODY_H * scale).max(side_rows * ROW) + FIELD_H + 24.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let centre = Pos2::new(rect.center().x, rect.top() + 175.0 * scale);
    let at = |(x, y): (f32, f32)| centre + Vec2::new(x, y) * scale;

    let dark = ui.visuals().dark_mode;
    let body = if dark { Color32::from_gray(52) } else { Color32::from_gray(218) };
    let painter = ui.painter().clone();
    draw_body(&painter, look, &at, scale, body);

    // Where each control sits on the pad, for the leader lines.
    let mut anchor: HashMap<Control, Pos2> = HashMap::new();
    for part in &layout.parts {
        let c = at(part.at);
        match part.kind {
            PartKind::Stick(x, y, click) => {
                // On the side facing the field column, top to bottom like the fields.
                let r = part.size.0 / 2.0 * scale;
                let out = if part.at.0 < 0.0 { -r } else { r };
                anchor.insert(Control::Axis(x), c + Vec2::new(out * 0.7, -r * 0.7));
                anchor.insert(Control::Axis(y), c + Vec2::new(out, 0.0));
                anchor.insert(Control::Button(click), c + Vec2::new(out * 0.7, r * 0.7));
            }
            _ => {
                for control in part.controls() {
                    anchor.insert(control, c);
                }
            }
        }
    }

    // Field rectangles: left and right columns, bottom row.
    let top = rect.top() + ((height - FIELD_H - 24.0) - side_rows * ROW).max(0.0) / 2.0;
    let (mut left, mut right) = (0.0, 0.0);
    let bottom: Vec<_> = layout.fields.iter().filter(|f| f.2 == Side::Bottom).collect();
    let mut bottom_i = 0.0;
    let mut placed = Vec::new();
    for &(control, label, side) in &layout.fields {
        let r = match side {
            Side::Left => {
                left += 1.0;
                Rect::from_min_size(Pos2::new(rect.left(), top + (left - 1.0) * ROW), Vec2::new(FIELD_W, FIELD_H))
            }
            Side::Right => {
                right += 1.0;
                Rect::from_min_size(
                    Pos2::new(rect.right() - FIELD_W, top + (right - 1.0) * ROW),
                    Vec2::new(FIELD_W, FIELD_H),
                )
            }
            Side::Bottom => {
                let n = bottom.len() as f32;
                let x = rect.center().x + (bottom_i - (n - 1.0) / 2.0) * (FIELD_W + 8.0);
                bottom_i += 1.0;
                Rect::from_center_size(Pos2::new(x, rect.bottom() - FIELD_H / 2.0 - 2.0), Vec2::new(FIELD_W, FIELD_H))
            }
        };
        placed.push((control, label, side, r));
    }

    // Leader lines under the parts.
    let line = Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.6));
    for &(control, _, side, r) in &placed {
        let Some(&to) = anchor.get(&control) else { continue };
        let from = match side {
            Side::Left => r.right_center() + Vec2::new(4.0, 0.0),
            Side::Right => r.left_center() - Vec2::new(4.0, 0.0),
            Side::Bottom => r.center_top() - Vec2::new(0.0, 3.0),
        };
        painter.line_segment([from, to], line);
        painter.circle_filled(to, 2.0, line.color);
    }

    // Parts on top, each clickable for the full settings.
    for (i, part) in layout.parts.iter().enumerate() {
        let r = Rect::from_center_size(at(part.at), Vec2::new(part.size.0, part.size.1) * scale);
        let response = ui.interact(r.expand(3.0), Id::new(("pad-part", i)), Sense::click());
        let hovered = response.hovered();
        draw_part(ui, &painter, look, part, r, state, hovered);
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        Popup::from_toggle_button_response(&response).close_behavior(PopupCloseBehavior::CloseOnClickOutside).show(
            |ui| {
                ui.set_min_width(230.0);
                for (n, control) in part.controls().into_iter().enumerate() {
                    if n > 0 {
                        ui.separator();
                    }
                    let label = placed.iter().find(|f| f.0 == control).map_or(control.display_name(), |f| f.1);
                    if let Some(b) = midi.bindings.iter_mut().find(|b| b.control == control) {
                        changed |= binding_editor(ui, label, b);
                    }
                }
            },
        );
    }

    // Fields last, so they sit above everything.
    for &(control, label, _, r) in &placed {
        let Some(b) = midi.bindings.iter_mut().find(|b| b.control == control) else { continue };
        let active = match control {
            Control::Button(btn) => state.is_pressed(btn),
            Control::Axis(axis) => {
                let v = state.axis(axis);
                if axis.is_bipolar() {
                    v.abs() > 0.2
                } else {
                    v > 0.1
                }
            }
        };
        let conflict = conflicts.contains(&control);
        ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let mut text = egui::RichText::new(label).small();
                if conflict {
                    text = text.color(Color32::from_rgb(230, 80, 60));
                } else if active {
                    text = text.color(ui.visuals().selection.bg_fill).strong();
                }
                ui.add_sized([58.0, FIELD_H], egui::Label::new(text).truncate());
                let prefix = match (control, b.kind) {
                    (Control::Button(_), MidiMessageKind::Note) => "N ",
                    _ => "CC ",
                };
                changed |= ui
                    .add_enabled(b.enabled, egui::DragValue::new(&mut b.number).range(0..=127).prefix(prefix))
                    .on_disabled_hover_text("Aus – auf die Taste klicken zum Einschalten")
                    .changed();
                if let Some(v) = live.get(&control) {
                    ui.label(egui::RichText::new(v.to_string()).small().weak());
                }
            });
        });
    }
    changed
}

/// Every setting of one binding (the table's columns), for the popup.
fn binding_editor(ui: &mut egui::Ui, label: &str, b: &mut MidiBinding) -> bool {
    let mut changed = false;
    let is_axis = matches!(b.control, Control::Axis(_));
    ui.horizontal(|ui| {
        ui.strong(label);
        changed |= ui.checkbox(&mut b.enabled, "Aktiv").changed();
    });
    ui.add_enabled_ui(b.enabled, |ui| {
        ui.horizontal(|ui| {
            if is_axis {
                ui.label("CC");
            } else {
                for k in [MidiMessageKind::Cc, MidiMessageKind::Note] {
                    changed |= ui.selectable_value(&mut b.kind, k, kind_name(k)).changed();
                }
            }
            changed |= ui.add(egui::DragValue::new(&mut b.number).range(0..=127)).changed();
        });
        if is_axis {
            changed |= ui.checkbox(&mut b.invert, "Invertieren").changed();
        } else {
            ui.horizontal(|ui| {
                for m in [ButtonMode::Momentary, ButtonMode::Toggle] {
                    changed |= ui.selectable_value(&mut b.button_mode, m, mode_name(m)).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Velocity");
                for v in [VelocitySource::Fixed, VelocitySource::LeftTrigger, VelocitySource::RightTrigger] {
                    changed |= ui.selectable_value(&mut b.velocity_source, v, velocity_name(v)).changed();
                }
                if b.velocity_source == VelocitySource::Fixed {
                    changed |= ui.add(egui::DragValue::new(&mut b.velocity).range(1..=127)).changed();
                }
            });
        }
    });
    changed
}

fn ellipse(c: Pos2, rx: f32, ry: f32, angle: f32) -> Vec<Pos2> {
    let (sin, cos) = angle.sin_cos();
    (0..48)
        .map(|i| {
            let t = i as f32 / 48.0 * TAU;
            let (x, y) = (rx * t.cos(), ry * t.sin());
            Pos2::new(c.x + x * cos - y * sin, c.y + x * sin + y * cos)
        })
        .collect()
}

/// The silhouette: overlapping filled shapes in one colour.
fn draw_body(painter: &egui::Painter, look: Look, at: &impl Fn((f32, f32)) -> Pos2, s: f32, fill: Color32) {
    let blob = |x: f32, y: f32, rx: f32, ry: f32, angle: f32| {
        painter.add(Shape::convex_polygon(ellipse(at((x, y)), rx * s, ry * s, angle), fill, Stroke::NONE));
    };
    match look {
        Look::Xbox => {
            blob(0.0, -20.0, 205.0, 100.0, 0.0);
            blob(-150.0, 60.0, 80.0, 115.0, 0.45);
            blob(150.0, 60.0, 80.0, 115.0, -0.45);
            blob(-130.0, -88.0, 78.0, 30.0, 0.12);
            blob(130.0, -88.0, 78.0, 30.0, -0.12);
        }
        Look::Ds4 => {
            let r = Rect::from_center_size(at((0.0, -40.0)), Vec2::new(440.0, 150.0) * s);
            painter.rect_filled(r, CornerRadius::same((55.0 * s) as u8), fill);
            blob(-150.0, 55.0, 72.0, 112.0, 0.3);
            blob(150.0, 55.0, 72.0, 112.0, -0.3);
            // Touch pad (not a control, only for the eye).
            let pad = Rect::from_center_size(at((0.0, -62.0)), Vec2::new(150.0, 78.0) * s);
            let edge = fill.lerp_to_gamma(Color32::GRAY, 0.5);
            painter.rect_stroke(pad, CornerRadius::same((8.0 * s) as u8), Stroke::new(1.5, edge), StrokeKind::Inside);
        }
    }
}

fn face_colour(look: Look, b: PadButton) -> Color32 {
    match (look, b) {
        (Look::Xbox, PadButton::A) => Color32::from_rgb(96, 180, 80),
        (Look::Xbox, PadButton::B) => Color32::from_rgb(215, 70, 60),
        (Look::Xbox, PadButton::X) => Color32::from_rgb(60, 125, 215),
        (Look::Xbox, _) => Color32::from_rgb(230, 185, 40),
        (Look::Ds4, PadButton::A) => Color32::from_rgb(125, 155, 230),
        (Look::Ds4, PadButton::B) => Color32::from_rgb(230, 95, 95),
        (Look::Ds4, PadButton::X) => Color32::from_rgb(220, 130, 200),
        (Look::Ds4, _) => Color32::from_rgb(70, 190, 150),
    }
}

fn draw_part(
    ui: &egui::Ui,
    painter: &egui::Painter,
    look: Look,
    part: &Part,
    r: Rect,
    state: &ControllerState,
    hovered: bool,
) {
    let v = ui.visuals();
    let on = v.selection.bg_fill;
    let idle = if v.dark_mode { Color32::from_gray(30) } else { Color32::from_gray(245) };
    let edge =
        Stroke::new(if hovered { 2.0 } else { 1.0 }, if hovered { v.strong_text_color() } else { v.weak_text_color() });
    let text = v.text_color();
    let font = FontId::proportional((r.height() * 0.5).clamp(9.0, 14.0));
    let label = |s: &str, c: Color32| {
        painter.text(r.center(), Align2::CENTER_CENTER, s, font.clone(), c);
    };
    match part.kind {
        PartKind::Trigger(axis) => {
            painter.rect_filled(r, CornerRadius::same(6), idle);
            let value = state.axis(axis);
            let fill = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - r.height() * value), r.max);
            painter.rect_filled(fill, CornerRadius::same(6), on);
            painter.rect_stroke(r, CornerRadius::same(6), edge, StrokeKind::Inside);
            let name = match (look, axis) {
                (Look::Xbox, PadAxis::LeftTrigger) => "LT",
                (Look::Xbox, _) => "RT",
                (Look::Ds4, PadAxis::LeftTrigger) => "L2",
                (Look::Ds4, _) => "R2",
            };
            label(name, text);
        }
        PartKind::Bumper(b) => {
            let pressed = state.is_pressed(b);
            painter.rect_filled(r, CornerRadius::same(8), if pressed { on } else { idle });
            painter.rect_stroke(r, CornerRadius::same(8), edge, StrokeKind::Inside);
            let name = match (look, b) {
                (Look::Xbox, PadButton::LeftShoulder) => "LB",
                (Look::Xbox, _) => "RB",
                (Look::Ds4, PadButton::LeftShoulder) => "L1",
                (Look::Ds4, _) => "R1",
            };
            label(name, if pressed { Color32::WHITE } else { text });
        }
        PartKind::Stick(x, y, click) => {
            let c = r.center();
            let radius = r.width() / 2.0;
            painter.circle_filled(c, radius, idle);
            painter.circle_stroke(c, radius, edge);
            // Up is positive in the model, screen y grows downwards.
            let dot = c + Vec2::new(state.axis(x), -state.axis(y)) * (radius * 0.55);
            let pressed = state.is_pressed(click);
            painter.circle_filled(dot, radius * 0.45, if pressed { on } else { v.widgets.inactive.bg_fill });
            painter.circle_stroke(dot, radius * 0.45, edge);
        }
        PartKind::Arrow(b) => {
            let pressed = state.is_pressed(b);
            painter.rect_filled(r, CornerRadius::same(3), if pressed { on } else { idle });
            painter.rect_stroke(r, CornerRadius::same(3), edge, StrokeKind::Inside);
        }
        PartKind::Face(b) => {
            let pressed = state.is_pressed(b);
            let colour = face_colour(look, b);
            let c = r.center();
            let radius = r.width() / 2.0;
            painter.circle_filled(c, radius, if pressed { colour } else { idle });
            painter.circle_stroke(c, radius, Stroke::new(edge.width, colour));
            let mark = if pressed { Color32::WHITE } else { colour };
            match look {
                Look::Xbox => label(b.display_name(), mark),
                Look::Ds4 => ds4_symbol(painter, b, c, radius * 0.45, Stroke::new(2.0, mark)),
            }
        }
        PartKind::Pill(b) => {
            let pressed = state.is_pressed(b);
            let cr = CornerRadius::same((r.width().min(r.height()) / 2.0) as u8);
            painter.rect_filled(r, cr, if pressed { on } else { idle });
            painter.rect_stroke(r, cr, edge, StrokeKind::Inside);
        }
        PartKind::Round(b) => {
            let pressed = state.is_pressed(b);
            let c = r.center();
            painter.circle_filled(c, r.width() / 2.0, if pressed { on } else { idle });
            painter.circle_stroke(c, r.width() / 2.0, edge);
            if look == Look::Ds4 {
                label("PS", if pressed { Color32::WHITE } else { text });
            } else {
                // The Xbox guide button: a ring, not a letter (X is a face button).
                let mark = if pressed { Color32::WHITE } else { v.weak_text_color() };
                painter.circle_stroke(c, r.width() * 0.25, Stroke::new(2.0, mark));
            }
        }
    }
}

/// Cross, circle, square, triangle, drawn rather than taken from a font.
fn ds4_symbol(painter: &egui::Painter, b: PadButton, c: Pos2, r: f32, stroke: Stroke) {
    match b {
        PadButton::A => {
            painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], stroke);
            painter.line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], stroke);
        }
        PadButton::B => {
            painter.circle_stroke(c, r, stroke);
        }
        PadButton::X => {
            let sq = Rect::from_center_size(c, Vec2::splat(r * 1.8));
            painter.rect_stroke(sq, CornerRadius::ZERO, stroke, StrokeKind::Middle);
        }
        _ => {
            let pts = vec![c + Vec2::new(0.0, -r), c + Vec2::new(r, r * 0.8), c + Vec2::new(-r, r * 0.8)];
            painter.add(Shape::closed_line(pts, stroke));
        }
    }
}
