//! The engine: one thread that owns every device, mapper and link and reacts
//! to messages (input reports, MIDI, network packets, UI commands) plus a few
//! timers. The UI only reads a `Snapshot` and sends `Command`s, so a slow
//! frame can never delay input, and headless mode is simply "no UI".

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver as ChannelRx, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use padbridge_core::key::NetKey;
use padbridge_core::keymouse::{EmulationTarget, KeyMouseMapper, KeyMouseMapping, KmEvent, WireInput};
use padbridge_core::midi::models::Model;
use padbridge_core::midi::surface::{self, SurfaceKey, SurfaceMapper, SurfaceMapping, SurfaceState};
use padbridge_core::midi::{ClockMessage, MidiClockTracker, MidiMapper, MidiMapping, MidiTarget};
use padbridge_core::protocol::{self, Kind, Packet};
use padbridge_core::ring::{Ring, RingMode};
use padbridge_core::settings::{self, NetSettings};
use padbridge_core::{Control, ControllerInfo, ControllerState, PlayerIndicator};

use crate::net::receiver::{Receiver, ReceiverStatus};
use crate::net::sender::Link;
use crate::platform::{self, DeviceOut, Event, KeyMouseOut, MidiPort, PadOutput};

pub enum Msg {
    Platform(Event),
    /// A packet from the peer this machine sends to, tagged with the link generation.
    Peer(u64, Packet),
    /// MIDI the receiver got from a peer, for the local port.
    NetMidiOut(Vec<u8>),
    Log(String),
    Cmd(Command),
}

pub enum Command {
    SetMidi(MidiMapping),
    SetKeyMouse(KeyMouseMapping),
    SetNet(NetSettings),
    SetSurfaces(SurfaceMapping),
    Rumble { id: String, low: f32, high: f32 },
    Indicator(PlayerIndicator),
    RingMode(RingMode),
    Quit,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClockStatus {
    pub running: bool,
    pub bpm: Option<f64>,
    pub bar: i64,
    pub beat: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LinkStatus {
    /// The link is open (something is being sent to the peer).
    pub active: bool,
    pub peer: Option<String>,
    pub answering: bool,
    pub latency_ms: Option<f64>,
    pub error: Option<String>,
    pub midi: bool,
    pub input: bool,
}

#[derive(Clone)]
pub struct DeviceView {
    pub info: ControllerInfo,
    pub state: ControllerState,
    pub midi_values: HashMap<Control, i32>,
    /// Index on the peer when the pad is sent over the network.
    pub net_pad: Option<u8>,
}

/// An external MIDI device (control surface) and what it has sent.
#[derive(Clone, Default)]
pub struct MidiInputView {
    pub name: String,
    pub state: SurfaceState,
    /// When each control last changed, for highlighting.
    pub changed: HashMap<SurfaceKey, Instant>,
    /// Latest messages, newest last.
    pub recent: VecDeque<String>,
    /// Toggle buttons that are on (when remapping).
    pub toggled: BTreeSet<SurfaceKey>,
}

/// Messages kept per device for the log.
const RECENT_MIDI: usize = 40;

/// Everything the UI shows. Copied out under a lock at most ~60 times a second.
#[derive(Clone)]
pub struct Snapshot {
    pub devices: Vec<DeviceView>,
    pub midi_inputs: Vec<MidiInputView>,
    pub midi_error: Option<String>,
    pub keymouse_error: Option<String>,
    pub clock: ClockStatus,
    pub ring_mode: RingMode,
    pub link: LinkStatus,
    pub receiver: Option<ReceiverStatus>,
    pub key_hex: Option<String>,
    pub log: VecDeque<String>,
    pub midi: MidiMapping,
    pub keymouse: KeyMouseMapping,
    pub net: NetSettings,
    pub surfaces: SurfaceMapping,
    pub listen_forced: bool,
}

/// The UI's (or main's) side of the engine.
#[derive(Clone)]
pub struct Handle {
    pub tx: Sender<Msg>,
    pub shared: Arc<Mutex<Snapshot>>,
}

impl Handle {
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(Msg::Cmd(command));
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.lock().unwrap().clone()
    }
}

struct Device {
    info: ControllerInfo,
    out: Box<dyn PadOutput>,
    state: ControllerState,
    midi: MidiMapper,
    keys: KeyMouseMapper,
    net_pad: Option<u8>,
    rumble_heard: Option<Instant>,
}

const KEYMOUSE_TICK: Duration = Duration::from_millis(10);
const KEYMOUSE_KEEPALIVE: Duration = Duration::from_millis(200);
const LINK_TICK: Duration = Duration::from_millis(100);
const LINK_RETRY: Duration = Duration::from_secs(3);
/// Without a refresh for this long the motors stop (a lost "off").
const RUMBLE_TIMEOUT: Duration = Duration::from_millis(500);
const SAVE_DELAY: Duration = Duration::from_millis(500);
const PUBLISH_INTERVAL: Duration = Duration::from_millis(16);

/// How the engine runs, from the command line.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Receive from a peer regardless of the `listen` setting.
    pub listen_forced: bool,
    /// Only receive (like the old padbridge-netpad): no local pads, no sending.
    /// For the background service, so a window opened next to it is the only
    /// one reading the local pads (no doubled MIDI).
    pub receive_only: bool,
}

pub struct Engine {
    rx: ChannelRx<Msg>,
    tx: Sender<Msg>,
    shared: Arc<Mutex<Snapshot>>,
    started: Instant,
    devices: BTreeMap<String, Device>,
    midi_inputs: BTreeMap<String, MidiInputView>,
    surfaces: SurfaceMapping,
    surface_mappers: HashMap<String, SurfaceMapper>,
    /// Per LCXL: template and LED values last sent.
    leds_sent: HashMap<String, (u8, Vec<u8>)>,
    device_out: Option<Box<dyn DeviceOut>>,
    midi_port: Option<Box<dyn MidiPort>>,
    keymouse_out: Option<Box<dyn KeyMouseOut>>,
    keymouse_error: Option<String>,
    /// Opening failed (e.g. macOS Accessibility not granted yet): try again then.
    keymouse_retry_at: Option<Instant>,
    key: Option<NetKey>,
    link: Option<Link>,
    link_generation: u64,
    link_error: Option<String>,
    link_retry_at: Option<Instant>,
    link_ticks: u64,
    receiver: Option<Receiver>,
    receiver_error: Option<String>,
    listen_forced: bool,
    receive_only: bool,
    clock: MidiClockTracker,
    clock_status: ClockStatus,
    ring: Ring,
    midi: MidiMapping,
    keymouse: KeyMouseMapping,
    net: NetSettings,
    dirty: Option<Instant>,
    log: VecDeque<String>,
    next_keymouse_tick: Instant,
    last_keymouse_tick: Instant,
    last_keepalive: Instant,
    next_link_tick: Instant,
    next_random: Option<Instant>,
    last_publish: Instant,
    changed: bool,
}

impl Engine {
    /// Starts the engine thread and returns its handle.
    pub fn spawn(options: Options) -> Handle {
        let listen_forced = options.listen_forced;
        let (tx, rx) = channel();
        let dir = settings::config_dir();
        let midi: MidiMapping = settings::load(&dir, settings::MIDI_FILE);
        let keymouse: KeyMouseMapping = settings::load(&dir, settings::KEYMOUSE_FILE);
        let net: NetSettings = settings::load(&dir, settings::NET_FILE);
        let surfaces: SurfaceMapping = settings::load(&dir, settings::SURFACES_FILE);
        let snapshot = Snapshot {
            devices: Vec::new(),
            midi_inputs: Vec::new(),
            midi_error: None,
            keymouse_error: None,
            clock: ClockStatus::default(),
            ring_mode: RingMode::Manual,
            link: LinkStatus::default(),
            receiver: None,
            key_hex: None,
            log: VecDeque::new(),
            midi: midi.clone(),
            keymouse: keymouse.clone(),
            net: net.clone(),
            surfaces: surfaces.clone(),
            listen_forced,
        };
        let shared = Arc::new(Mutex::new(snapshot));
        let handle = Handle { tx: tx.clone(), shared: shared.clone() };
        let now = Instant::now();
        let mut engine = Engine {
            rx,
            tx,
            shared,
            started: now,
            devices: BTreeMap::new(),
            midi_inputs: BTreeMap::new(),
            surfaces,
            surface_mappers: HashMap::new(),
            leds_sent: HashMap::new(),
            device_out: None,
            midi_port: None,
            keymouse_out: None,
            keymouse_error: None,
            keymouse_retry_at: None,
            key: None,
            link: None,
            link_generation: 0,
            link_error: None,
            link_retry_at: None,
            link_ticks: 0,
            receiver: None,
            receiver_error: None,
            listen_forced,
            receive_only: options.receive_only,
            clock: MidiClockTracker::new(),
            clock_status: ClockStatus::default(),
            ring: Ring::new(),
            midi,
            keymouse,
            net,
            dirty: None,
            log: VecDeque::new(),
            next_keymouse_tick: now,
            last_keymouse_tick: now,
            last_keepalive: now,
            next_link_tick: now,
            next_random: None,
            last_publish: now,
            changed: true,
        };
        std::thread::Builder::new().name("engine".into()).spawn(move || engine.run()).expect("engine thread");
        handle
    }

    fn run(&mut self) {
        let dir = settings::config_dir();
        match NetKey::load_or_create(&dir) {
            Ok(key) => {
                self.shared.lock().unwrap().key_hex = Some(key.to_hex());
                self.key = Some(key);
            }
            Err(e) => self.log(format!("Netzwerk-Schlüssel nicht lesbar ({}): {e}", dir.display())),
        }
        match platform::open_midi(&self.tx) {
            Ok(port) => {
                self.midi_port = Some(port);
                self.log("MIDI-Port „PadBridge“ bereit".into());
            }
            Err(e) => {
                self.log(format!("MIDI nicht verfügbar: {e}"));
                self.shared.lock().unwrap().midi_error = Some(e.to_string());
            }
        }
        if self.receive_only {
            self.log("Nur-Empfangen-Modus: lokale Pads werden nicht gelesen".into());
        } else {
            platform::start_pads(&self.tx);
            self.device_out = platform::start_midi_devices(&self.tx);
        }
        self.reconcile_link();
        self.reconcile_receiver();

        loop {
            let timeout = self.next_deadline().saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
            match self.rx.recv_timeout(timeout) {
                Ok(Msg::Cmd(Command::Quit)) => break,
                Ok(msg) => {
                    self.handle(msg);
                    // Drain whatever else is queued before running timers.
                    while let Ok(msg) = self.rx.try_recv() {
                        if matches!(msg, Msg::Cmd(Command::Quit)) {
                            return self.shutdown();
                        }
                        self.handle(msg);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.timers();
        }
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.save_now();
        let ids: Vec<String> = self.devices.keys().cloned().collect();
        for id in ids {
            self.release_keys(&id);
            if let (Some(link), Some(pad)) = (self.link.as_ref(), self.devices[&id].net_pad) {
                link.send_bye(pad);
            }
        }
        self.receiver = None;
    }

    fn next_deadline(&self) -> Instant {
        let mut next = Instant::now() + Duration::from_millis(100);
        if self.keymouse.enabled {
            next = next.min(self.next_keymouse_tick);
        }
        if self.link.is_some() {
            next = next.min(self.next_link_tick);
        }
        for t in [self.next_random, self.dirty.map(|t| t + SAVE_DELAY), self.link_retry_at].into_iter().flatten() {
            next = next.min(t);
        }
        if self.changed {
            next = next.min(self.last_publish + PUBLISH_INTERVAL);
        }
        next
    }

    fn log(&mut self, line: String) {
        let elapsed = self.started.elapsed().as_secs();
        let stamped = format!("{:02}:{:02}:{:02}  {line}", elapsed / 3600, elapsed / 60 % 60, elapsed % 60);
        eprintln!("padbridge: {line}");
        self.log.push_back(stamped);
        while self.log.len() > 200 {
            self.log.pop_front();
        }
        self.changed = true;
    }

    // MARK: Messages

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Platform(Event::PadConnected(info, out)) => self.pad_connected(info, out),
            Msg::Platform(Event::PadState(id, state)) => self.pad_state(&id, state),
            Msg::Platform(Event::PadGone(id)) => self.pad_gone(&id),
            Msg::Platform(Event::MidiIn(bytes)) => self.local_midi_in(bytes),
            Msg::Platform(Event::MidiDevices(names)) => self.midi_devices(names),
            Msg::Platform(Event::DeviceMidi(name, bytes)) => self.device_midi(&name, &bytes),
            Msg::Platform(Event::Error(e)) => self.log(e),
            Msg::Peer(generation, packet) => self.peer_packet(generation, packet),
            Msg::NetMidiOut(bytes) => {
                if let Some(port) = &self.midi_port {
                    port.send(&bytes);
                }
            }
            Msg::Log(line) => self.log(line),
            Msg::Cmd(cmd) => self.command(cmd),
        }
    }

    fn pad_connected(&mut self, info: ControllerInfo, mut out: Box<dyn PadOutput>) {
        self.log(format!("Verbunden: {} ({:04x}:{:04x})", info.name, info.vendor_id, info.product_id));
        if let Some(indicator) = self.ring.current() {
            out.set_indicator(indicator);
        }
        let used: Vec<u8> = self.devices.values().filter_map(|d| d.net_pad).collect();
        let net_pad = (0..4).find(|i| !used.contains(i));
        self.devices.insert(
            info.id.clone(),
            Device {
                info: info.clone(),
                out,
                state: ControllerState::default(),
                midi: MidiMapper::new(self.midi.clone()),
                keys: KeyMouseMapper::new(self.keymouse.clone()),
                net_pad,
                rumble_heard: None,
            },
        );
        self.changed = true;
    }

    fn pad_state(&mut self, id: &str, state: ControllerState) {
        let Some(dev) = self.devices.get_mut(id) else { return };
        dev.state = state;
        let messages: Vec<Vec<u8>> = dev.midi.process(&state).iter().map(|m| m.bytes().to_vec()).collect();
        let keys = if self.keymouse.enabled { dev.keys.process(&state) } else { Vec::new() };
        let net_pad = dev.net_pad;
        if let (true, Some(link), Some(pad)) = (self.net.enabled, self.link.as_ref(), net_pad) {
            link.send_state(pad, &state);
        }
        self.route_midi(&messages);
        self.emit_keys(&keys, self.keymouse.target);
        self.changed = true;
    }

    fn pad_gone(&mut self, id: &str) {
        self.release_keys(id);
        if let Some(dev) = self.devices.remove(id) {
            if let (Some(link), Some(pad)) = (self.link.as_ref(), dev.net_pad) {
                link.send_bye(pad);
            }
            self.log(format!("Getrennt: {}", dev.info.name));
        }
        self.changed = true;
    }

    fn midi_devices(&mut self, names: Vec<String>) {
        let gone: Vec<String> = self.midi_inputs.keys().filter(|n| !names.contains(n)).cloned().collect();
        for name in gone {
            self.midi_inputs.remove(&name);
            self.surface_mappers.remove(&name);
            self.leds_sent.remove(&name);
            self.log(format!("MIDI-Gerät getrennt: {name}"));
        }
        for name in names {
            if !self.midi_inputs.contains_key(&name) {
                self.log(format!("MIDI-Gerät: {name}"));
                self.midi_inputs.insert(name.clone(), MidiInputView { name: name.clone(), ..Default::default() });
                self.update_leds(&name);
            }
        }
        self.changed = true;
    }

    /// A message from an external MIDI device: remembered for display.
    fn device_midi(&mut self, name: &str, bytes: &[u8]) {
        // Clock and active sensing would flood the log.
        if bytes.first().is_some_and(|&b| b >= 0xF8) {
            return;
        }
        let Some(input) = self.midi_inputs.get_mut(name) else { return };
        if let Some(key) = input.state.apply(bytes) {
            input.changed.insert(key, Instant::now());
        }
        if let Some(template) = Model::of(name).and_then(|m| m.template_change(bytes)) {
            input.state.channel = Some(template);
        }
        input.recent.push_back(surface::describe(bytes));
        while input.recent.len() > RECENT_MIDI {
            input.recent.pop_front();
        }
        if let Some(mapping) = self.surfaces.devices.get(name).filter(|m| m.enabled) {
            let out = self.surface_mappers.entry(name.to_string()).or_default().process(mapping, bytes);
            if let Some(port) = &self.midi_port {
                for m in &out {
                    port.send(m);
                }
            }
        }
        self.update_leds(name);
        self.changed = true;
    }

    /// Brings the LEDs of a known surface in line with its mapping:
    /// only what changed is sent; switching remapping off clears them.
    fn update_leds(&mut self, name: &str) {
        let (Some(out), Some(input)) = (&self.device_out, self.midi_inputs.get(name)) else { return };
        let Some(model) = Model::of(name) else { return };
        let channel = input.state.channel.unwrap_or(model.default_channel());
        let Some(mapping) = self.surfaces.devices.get(name).filter(|m| m.enabled) else {
            if let Some((template, _)) = self.leds_sent.remove(name) {
                for m in model.reset(template) {
                    out.send(name, &m);
                }
            }
            return;
        };
        let none = BTreeSet::new();
        let toggled = self.surface_mappers.get(name).map_or(&none, |m| &m.toggled);
        let want = model.leds(mapping, &input.state, toggled, channel);
        let before = self.leds_sent.get(name).filter(|(t, _)| *t == channel).map(|(_, l)| l);
        for (i, &value) in want.iter().enumerate() {
            if before.is_none_or(|b| b[i] != value) {
                out.send(name, &model.led_message(channel, i, value));
            }
        }
        self.leds_sent.insert(name.to_string(), (channel, want));
    }

    /// MIDI arriving at the local port (from a DAW on this machine).
    fn local_midi_in(&mut self, bytes: Vec<u8>) {
        if let Some(receiver) = &self.receiver {
            receiver.send_midi_in(&bytes);
        }
        if self.midi.target == MidiTarget::Local {
            self.clock_message(&bytes);
        }
    }

    fn peer_packet(&mut self, generation: u64, packet: Packet) {
        let Some(link) = self.link.as_mut().filter(|l| l.generation == generation) else { return };
        let Some(packet) = link.inbound(packet) else {
            self.changed = true; // a pong: latency changed
            return;
        };
        match packet.kind {
            Kind::MidiIn => {
                if self.midi.target == MidiTarget::Network {
                    for message in protocol::midi_messages(&packet.payload) {
                        self.clock_message(&message);
                    }
                }
            }
            Kind::Rumble => {
                if !(self.net.rumble && self.net.enabled) {
                    return;
                }
                let (heavy, light) = protocol::rumble_levels(&packet.payload);
                if let Some(dev) = self.devices.values_mut().find(|d| d.net_pad == Some(packet.pad)) {
                    dev.out.set_rumble(heavy, light);
                    dev.rumble_heard = (heavy > 0.0 || light > 0.0).then(Instant::now);
                }
            }
            _ => {}
        }
    }

    fn command(&mut self, cmd: Command) {
        match cmd {
            Command::SetMidi(mapping) => {
                if mapping.target != self.midi.target {
                    // New clock source: forget position and tempo of the old one.
                    self.clock = MidiClockTracker::new();
                    self.clock_status = ClockStatus::default();
                }
                for dev in self.devices.values_mut() {
                    dev.midi.set_mapping(mapping.clone());
                }
                self.midi = mapping;
                self.reconcile_link();
            }
            Command::SetKeyMouse(mapping) => {
                let old = self.keymouse.clone();
                // Turning off or moving to the other target: let go on the old side first.
                if old.enabled && (!mapping.enabled || mapping.target != old.target) {
                    let ids: Vec<String> = self.devices.keys().cloned().collect();
                    for id in ids {
                        let released = self.devices.get_mut(&id).unwrap().keys.release_all();
                        self.emit_keys(&released, old.target);
                    }
                }
                for dev in self.devices.values_mut() {
                    dev.keys.mapping = mapping.clone();
                }
                self.keymouse = mapping;
                self.reconcile_link();
            }
            Command::SetNet(net) => {
                let endpoint_changed = net.host != self.net.host || net.port != self.net.port;
                if self.net.enabled && !net.enabled {
                    self.send_byes();
                }
                if !net.enabled || !net.rumble {
                    self.stop_rumble();
                }
                let listen_changed = net.listen != self.net.listen || net.port != self.net.port;
                self.net = net;
                if endpoint_changed {
                    self.close_link();
                }
                self.reconcile_link();
                if listen_changed {
                    self.receiver = None;
                    self.reconcile_receiver();
                }
            }
            Command::SetSurfaces(surfaces) => {
                self.surfaces = surfaces;
                for (name, mapper) in self.surface_mappers.iter_mut() {
                    match self.surfaces.devices.get(name) {
                        Some(m) if m.enabled => mapper.retain(m),
                        _ => mapper.toggled.clear(),
                    }
                }
                let names: Vec<String> = self.midi_inputs.keys().cloned().collect();
                for name in names {
                    self.update_leds(&name);
                }
            }
            Command::Rumble { id, low, high } => {
                if let Some(dev) = self.devices.get_mut(&id) {
                    dev.out.set_rumble(low, high);
                }
                return;
            }
            Command::Indicator(indicator) => {
                self.next_random = None;
                let shown = self.ring.set_manual(indicator);
                self.show_ring(shown);
                self.changed = true;
                return;
            }
            Command::RingMode(mode) => {
                self.next_random = (mode == RingMode::Random).then(Instant::now);
                if let Some(shown) = self.ring.set_mode(mode) {
                    self.show_ring(shown);
                }
                self.changed = true;
                return;
            }
            Command::Quit => {}
        }
        self.dirty = Some(Instant::now());
        self.changed = true;
    }

    // MARK: MIDI and clock

    fn route_midi(&self, messages: &[Vec<u8>]) {
        if messages.is_empty() {
            return;
        }
        match self.midi.target {
            MidiTarget::Local => {
                if let Some(port) = &self.midi_port {
                    for m in messages {
                        port.send(m);
                    }
                }
            }
            MidiTarget::Network => {
                if let Some(link) = &self.link {
                    link.send_midi(messages);
                }
            }
        }
    }

    fn clock_message(&mut self, bytes: &[u8]) {
        let Some(message) = ClockMessage::from_bytes(bytes) else { return };
        let cue = self.clock.handle(message, self.started.elapsed().as_secs_f64());
        if let Some(indicator) = cue.and_then(|c| self.ring.clock_cue(c)) {
            self.show_ring(indicator);
        }
        // Report on beat boundaries and transport changes, not 24x per beat.
        if message != ClockMessage::Tick || cue.is_some() {
            let (bar, beat) = self.clock.position();
            let status = ClockStatus {
                running: self.clock.running(),
                bpm: self.clock.bpm().map(|b| (b * 10.0).round() / 10.0),
                bar,
                beat,
            };
            if status != self.clock_status {
                self.clock_status = status;
                self.changed = true;
            }
        }
    }

    fn show_ring(&mut self, indicator: PlayerIndicator) {
        for dev in self.devices.values_mut() {
            dev.out.set_indicator(indicator);
        }
    }

    // MARK: Keyboard/mouse

    fn emit_keys(&mut self, events: &[KmEvent], target: EmulationTarget) {
        if events.is_empty() {
            return;
        }
        match target {
            EmulationTarget::Local => {
                if self.keymouse_out.is_none() && self.keymouse_retry_at.is_none_or(|t| Instant::now() >= t) {
                    match platform::open_keymouse() {
                        Ok(out) => {
                            self.keymouse_out = Some(out);
                            self.keymouse_error = None;
                            self.keymouse_retry_at = None;
                        }
                        Err(e) => {
                            if self.keymouse_error.is_none() {
                                self.log(format!("Tastatur/Maus nicht verfügbar: {e}"));
                            }
                            self.keymouse_error = Some(e.to_string());
                            self.keymouse_retry_at = Some(Instant::now() + Duration::from_secs(3));
                        }
                    }
                }
                if let Some(out) = self.keymouse_out.as_mut() {
                    out.post(events);
                }
            }
            EmulationTarget::Network => {
                if let Some(link) = &self.link {
                    self.last_keepalive = Instant::now();
                    link.send_input(&events.iter().map(|&e| WireInput::from(e)).collect::<Vec<_>>());
                }
            }
        }
    }

    fn release_keys(&mut self, id: &str) {
        if let Some(dev) = self.devices.get_mut(id) {
            let released = dev.keys.release_all();
            self.emit_keys(&released, self.keymouse.target);
        }
    }

    // MARK: Network

    fn link_wanted(&self) -> bool {
        self.net.enabled
            || (self.midi.enabled && self.midi.target == MidiTarget::Network)
            || (self.keymouse.enabled && self.keymouse.target == EmulationTarget::Network)
    }

    fn reconcile_link(&mut self) {
        let wanted = self.link_wanted() && !self.receive_only;
        if !wanted {
            if self.link.is_some() {
                self.close_link();
            }
            self.link_error = None;
            self.link_retry_at = None;
        } else if self.link.is_none() {
            self.open_link();
        }
        self.changed = true;
    }

    fn open_link(&mut self) {
        self.link_retry_at = None;
        let Some(key) = self.key.clone() else {
            self.link_error = Some("Kein Netzwerk-Schlüssel".into());
            return;
        };
        if self.net.host.trim().is_empty() {
            self.link_error = Some("Host eintragen".into());
            return;
        }
        self.link_generation += 1;
        match Link::open(&self.net.host, self.net.port, key, self.link_generation, self.tx.clone()) {
            Ok(link) => {
                self.log(format!("Netzwerk: sende an {}", link.peer));
                self.link = Some(link);
                self.link_error = None;
                self.next_link_tick = Instant::now();
                // Push current states right away so the peer's virtual pads appear.
                if self.net.enabled {
                    if let Some(link) = &self.link {
                        for dev in self.devices.values() {
                            if let Some(pad) = dev.net_pad {
                                link.send_state(pad, &dev.state);
                            }
                        }
                    }
                }
                // Re-send every mapped value so the peer's DAW starts in sync.
                for dev in self.devices.values_mut() {
                    dev.midi.reset();
                }
                let ids: Vec<String> = self.devices.keys().cloned().collect();
                for id in ids {
                    let state = self.devices[&id].state;
                    let messages: Vec<Vec<u8>> = self
                        .devices
                        .get_mut(&id)
                        .unwrap()
                        .midi
                        .process(&state)
                        .iter()
                        .map(|m| m.bytes().to_vec())
                        .collect();
                    self.route_midi(&messages);
                }
            }
            Err(e) => {
                self.link_error = Some(format!("{}: {e}", self.net.host));
                self.link_retry_at = Some(Instant::now() + LINK_RETRY);
            }
        }
    }

    fn close_link(&mut self) {
        if self.net.enabled {
            self.send_byes();
        }
        self.stop_rumble();
        self.link = None;
    }

    fn send_byes(&self) {
        if let Some(link) = &self.link {
            for dev in self.devices.values() {
                if let Some(pad) = dev.net_pad {
                    link.send_bye(pad);
                }
            }
        }
    }

    fn stop_rumble(&mut self) {
        for dev in self.devices.values_mut() {
            if dev.rumble_heard.take().is_some() {
                dev.out.set_rumble(0.0, 0.0);
            }
        }
    }

    fn reconcile_receiver(&mut self) {
        let wanted = self.net.listen || self.listen_forced;
        if !wanted {
            self.receiver = None;
            self.receiver_error = None;
            return;
        }
        if self.receiver.is_some() {
            return;
        }
        let Some(key) = self.key.clone() else { return };
        match Receiver::start(self.net.port, key, self.tx.clone()) {
            Ok(r) => {
                self.log(format!("Netzwerk: empfange auf Port {}", self.net.port));
                self.receiver = Some(r);
                self.receiver_error = None;
            }
            Err(e) => {
                let msg = format!("Port {} nicht verfügbar: {e}", self.net.port);
                self.log(format!("Netzwerk: {msg}"));
                self.receiver_error = Some(msg);
            }
        }
    }

    // MARK: Timers

    fn timers(&mut self) {
        let now = Instant::now();

        if self.keymouse.enabled && now >= self.next_keymouse_tick {
            let dt = now.duration_since(self.last_keymouse_tick).as_secs_f64().min(0.1);
            self.last_keymouse_tick = now;
            self.next_keymouse_tick = now + KEYMOUSE_TICK;
            let mut events = Vec::new();
            let mut holding = false;
            for dev in self.devices.values_mut() {
                events.extend(dev.keys.tick(&dev.state, dt));
                holding |= dev.keys.is_holding_anything();
            }
            self.emit_keys(&events, self.keymouse.target);
            if self.keymouse.target == EmulationTarget::Network
                && holding
                && now.duration_since(self.last_keepalive) > KEYMOUSE_KEEPALIVE
            {
                self.last_keepalive = now;
                if let Some(link) = &self.link {
                    link.send_input(&[]);
                }
            }
        }

        if self.link.is_some() && now >= self.next_link_tick {
            self.next_link_tick = now + LINK_TICK;
            self.link_ticks += 1;
            // Repeat states so a lost datagram heals quickly (the peer releases after 500 ms).
            if let Some(link) = &self.link {
                if self.net.enabled {
                    for dev in self.devices.values() {
                        if let Some(pad) = dev.net_pad {
                            link.send_state(pad, &dev.state);
                        }
                    }
                }
                if self.link_ticks.is_multiple_of(10) {
                    link.send_ping();
                    self.changed = true;
                }
            }
            for dev in self.devices.values_mut() {
                if dev.rumble_heard.is_some_and(|t| t.elapsed() > RUMBLE_TIMEOUT) {
                    dev.rumble_heard = None;
                    dev.out.set_rumble(0.0, 0.0);
                }
            }
        }

        if self.link_retry_at.is_some_and(|t| now >= t) {
            self.open_link();
        }

        if self.next_random.is_some_and(|t| now >= t) {
            let (indicator, wait) = self.ring.random_step();
            self.show_ring(indicator);
            self.next_random = Some(now + wait);
        }

        if self.dirty.is_some_and(|t| now >= t + SAVE_DELAY) {
            self.save_now();
        }

        if self.receiver.is_some() && now.duration_since(self.last_publish) > Duration::from_millis(200) {
            self.changed = true;
        }
        if self.changed && now.duration_since(self.last_publish) >= PUBLISH_INTERVAL {
            self.publish();
        }
    }

    fn save_now(&mut self) {
        if self.dirty.take().is_none() {
            return;
        }
        let dir = settings::config_dir();
        let results = [
            settings::save(&dir, settings::MIDI_FILE, &self.midi),
            settings::save(&dir, settings::KEYMOUSE_FILE, &self.keymouse),
            settings::save(&dir, settings::NET_FILE, &self.net),
            settings::save(&dir, settings::SURFACES_FILE, &self.surfaces),
        ];
        if let Some(Err(e)) = results.into_iter().find(|r| r.is_err()) {
            self.log(format!("Speichern fehlgeschlagen: {e}"));
        }
    }

    fn publish(&mut self) {
        self.changed = false;
        self.last_publish = Instant::now();
        let devices = self
            .devices
            .values()
            .map(|d| DeviceView {
                info: d.info.clone(),
                state: d.state,
                midi_values: d
                    .midi
                    .mapping()
                    .bindings
                    .iter()
                    .filter_map(|b| d.midi.last_sent(b.control).map(|v| (b.control, v)))
                    .collect(),
                net_pad: if self.net.enabled { d.net_pad } else { None },
            })
            .collect();
        let link = LinkStatus {
            active: self.link.is_some(),
            peer: self.link.as_ref().map(|l| l.peer.to_string()),
            answering: self.link.as_ref().is_some_and(|l| l.answering()),
            latency_ms: self.link.as_ref().and_then(|l| l.latency_ms()),
            error: self.link_error.clone(),
            midi: self.midi.enabled && self.midi.target == MidiTarget::Network,
            input: self.keymouse.enabled && self.keymouse.target == EmulationTarget::Network,
        };
        let receiver = match (&self.receiver, &self.receiver_error) {
            (Some(r), _) => Some(r.status.lock().unwrap().clone()),
            (None, Some(e)) => Some(ReceiverStatus { error: Some(e.clone()), ..Default::default() }),
            (None, None) => None,
        };
        let mut s = self.shared.lock().unwrap();
        s.devices = devices;
        s.midi_inputs = self
            .midi_inputs
            .values()
            .map(|i| MidiInputView {
                toggled: self.surface_mappers.get(&i.name).map(|m| m.toggled.clone()).unwrap_or_default(),
                ..i.clone()
            })
            .collect();
        s.surfaces = self.surfaces.clone();
        s.keymouse_error = self.keymouse_error.clone();
        s.clock = self.clock_status.clone();
        s.ring_mode = self.ring.mode();
        s.link = link;
        s.receiver = receiver;
        s.log = self.log.clone();
        s.midi = self.midi.clone();
        s.keymouse = self.keymouse.clone();
        s.net = self.net.clone();
    }
}
