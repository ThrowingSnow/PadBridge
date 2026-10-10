//! The Rust app must read and write the same settings files as the Swift app.
//! Fixtures under tests/fixtures were written by the Swift app's encoders.

use std::path::PathBuf;

use padbridge_core::key::NetKey;
use padbridge_core::keymouse::{EmulationTarget, KeyAction, KeyCode, KeyMouseMapping, Modifier};
use padbridge_core::midi::{ButtonMode, MidiMapping, MidiMessageKind, MidiTarget, VelocitySource};
use padbridge_core::settings::{self, HostPreset, NetSettings};
use padbridge_core::{Control, PadButton};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// Rust -> JSON -> Rust must be lossless, and Swift's own JSON must equal ours.
fn same_json(a: &impl serde::Serialize, b: &str) {
    let ours = serde_json::to_value(a).unwrap();
    let swift: serde_json::Value = serde_json::from_str(b).unwrap();
    assert_eq!(normalise(ours), normalise(swift));
}

/// Numbers compare by value at f32 precision: Swift writes Double 2.0 as `2` and
/// some Floats as `0.12`, Rust writes `2.0` and `0.11999999731779099`.
fn normalise(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Number(n) => serde_json::json!((n.as_f64().unwrap() as f32 * 1e5).round() / 1e5),
        Value::Array(a) => Value::Array(a.into_iter().map(normalise).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalise(v))).collect()),
        other => other,
    }
}

#[test]
fn reads_and_writes_swift_midi_mapping() {
    let m: MidiMapping = serde_json::from_str(&fixture("swift-midi-edited.json")).unwrap();
    assert_eq!(m.target, MidiTarget::Network);
    let edited = m.bindings[6];
    assert_eq!(edited.control, Control::Button(PadButton::A));
    assert_eq!(
        (edited.kind, edited.velocity_source, edited.button_mode),
        (MidiMessageKind::Note, VelocitySource::RightTrigger, ButtonMode::Toggle)
    );
    same_json(&m, &fixture("swift-midi-edited.json"));
    let real: MidiMapping = serde_json::from_str(&fixture("swift-midi-mapping.json")).unwrap();
    same_json(&real, &fixture("swift-midi-mapping.json"));
}

#[test]
fn reads_and_writes_swift_keymouse() {
    let m: KeyMouseMapping = serde_json::from_str(&fixture("swift-keymouse.json")).unwrap();
    assert!(m.enabled && m.target == EmulationTarget::Network);
    assert_eq!(
        m.action(Control::Button(PadButton::LeftShoulder)),
        &KeyAction::Key(KeyCode::Tab, vec![Modifier::Control, Modifier::Shift])
    );
    assert_eq!(m.action(Control::Button(PadButton::Back)), &KeyAction::None);
    let expected = KeyMouseMapping { enabled: true, target: EmulationTarget::Network, ..KeyMouseMapping::default() };
    assert_eq!(m, expected, "Swift default preset = Rust default preset");
    same_json(&m, &fixture("swift-keymouse.json"));
}

#[test]
fn reads_and_writes_swift_net_settings() {
    let s: NetSettings = serde_json::from_str(&fixture("swift-netpad.json")).unwrap();
    assert_eq!(s.presets, vec![HostPreset { name: "Desktop".into(), host: "192.0.2.10".into() }]);
    assert!(s.enabled && !s.rumble && s.port == 48100);
    // `listen` exists only in the Rust app; the Swift app ignores it.
    let mut ours = serde_json::to_value(&s).unwrap();
    ours.as_object_mut().unwrap().remove("listen");
    same_json(&ours, &fixture("swift-netpad.json"));
    let old: NetSettings = serde_json::from_str(r#"{"enabled":true,"host":"192.0.2.10","port":48100}"#).unwrap();
    assert!(old.rumble && old.presets.is_empty(), "fields added later take their defaults");
}

#[test]
fn save_and_load_roundtrip_in_a_directory() {
    let dir = std::env::temp_dir().join(format!("padbridge-settings-test-{}", std::process::id()));
    let m = MidiMapping { channel: 9, ..MidiMapping::default() };
    settings::save(&dir, settings::MIDI_FILE, &m).unwrap();
    assert_eq!(settings::load::<MidiMapping>(&dir, settings::MIDI_FILE), m);
    assert_eq!(settings::load::<NetSettings>(&dir, "missing.json"), NetSettings::default());
    std::fs::write(dir.join("broken.json"), "{not json").unwrap();
    assert_eq!(settings::load::<KeyMouseMapping>(&dir, "broken.json"), KeyMouseMapping::default());

    let key = NetKey::load_or_create(&dir).unwrap();
    assert_eq!(NetKey::load_or_create(&dir).unwrap(), key, "second call loads the same key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(settings::KEY_FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
