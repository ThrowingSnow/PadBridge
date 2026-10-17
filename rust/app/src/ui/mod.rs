//! The window: five tabs over the engine's snapshot. Settings are edited on
//! local copies and every change is sent to the engine as a command; the
//! engine saves them.

mod keymouse;
mod midi;
mod monitor;
mod network;
mod pad_map;
mod surfaces;

use std::time::Duration;

use eframe::egui;
use padbridge_core::keymouse::KeyMouseMapping;
use padbridge_core::midi::surface::SurfaceMapping;
use padbridge_core::midi::MidiMapping;
use padbridge_core::settings::NetSettings;

use crate::engine::{Command, Handle, Snapshot};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Monitor,
    Midi,
    Surfaces,
    KeyMouse,
    Network,
}

pub struct App {
    handle: Handle,
    snap: Snapshot,
    tab: Tab,
    selected: Option<String>,
    midi: MidiMapping,
    keymouse: KeyMouseMapping,
    net: NetSettings,
    rumble: (f32, f32),
    preset_name: String,
    preset_open: bool,
    /// MIDI tab: pad picture (true) or table.
    midi_picture: bool,
    /// MIDI tab picture: forced outline, None follows the selected pad.
    pad_look: Option<pad_map::Look>,
    /// MIDI-Geräte tab: the device shown.
    surface: Option<String>,
    surfaces: SurfaceMapping,
}

pub fn run(handle: Handle) -> eframe::Result {
    let snap = handle.snapshot();
    let app = App {
        midi: snap.midi.clone(),
        keymouse: snap.keymouse.clone(),
        net: snap.net.clone(),
        surfaces: snap.surfaces.clone(),
        handle,
        snap,
        tab: Tab::Monitor,
        selected: None,
        rumble: (0.0, 0.0),
        preset_name: String::new(),
        preset_open: false,
        midi_picture: true,
        pad_look: None,
        surface: None,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PadBridge")
            .with_app_id("padbridge")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native("PadBridge", options, Box::new(|_cc| Ok(Box::new(app))))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.snap = self.handle.snapshot();
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("PadBridge");
                ui.add_space(16.0);
                for (tab, title) in [
                    (Tab::Monitor, "Monitor"),
                    (Tab::Midi, "MIDI"),
                    (Tab::Surfaces, "MIDI-Geräte"),
                    (Tab::KeyMouse, "Tastatur/Maus"),
                    (Tab::Network, "Netzwerk"),
                ] {
                    ui.selectable_value(&mut self.tab, tab, title);
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.tab {
                Tab::Monitor => self.monitor(ui),
                Tab::Midi => self.midi_tab(ui),
                Tab::Surfaces => self.surfaces_tab(ui),
                Tab::KeyMouse => self.keymouse_tab(ui),
                Tab::Network => self.network_tab(ui),
            });
        });
        // Live values: redraw at display rate while the window is open.
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.handle.send(Command::Quit);
        // Give the engine a moment to save and release held keys.
        std::thread::sleep(Duration::from_millis(150));
    }
}

impl App {
    fn set_midi(&mut self) {
        self.handle.send(Command::SetMidi(self.midi.clone()));
    }

    fn set_keymouse(&mut self) {
        self.handle.send(Command::SetKeyMouse(self.keymouse.clone()));
    }

    fn set_net(&mut self) {
        self.handle.send(Command::SetNet(self.net.clone()));
    }
}

/// Small grey explanatory text.
fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).small().weak());
}

/// A coloured status dot followed by text.
fn status(ui: &mut egui::Ui, color: egui::Color32, text: impl Into<String>) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, color);
        ui.label(text.into());
    });
}

const GREEN: egui::Color32 = egui::Color32::from_rgb(60, 190, 90);
const ORANGE: egui::Color32 = egui::Color32::from_rgb(240, 150, 40);
const GREY: egui::Color32 = egui::Color32::GRAY;
