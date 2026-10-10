//! Settings files, shared with the Swift app: same directory and JSON shape,
//! so switching between the two keeps every mapping.
//!
//! - macOS: `~/Library/Application Support/PadBridge/`
//! - Linux: `$XDG_CONFIG_HOME/padbridge/` (default `~/.config/padbridge/`)

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::protocol::DEFAULT_PORT;

pub const MIDI_FILE: &str = "midi-mapping.json";
pub const NET_FILE: &str = "netpad.json";
pub const KEYMOUSE_FILE: &str = "keymouse.json";
pub const KEY_FILE: &str = "netpad.key";

pub fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/PadBridge")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"))
            .join("padbridge")
    }
}

/// Loads `file` from `dir`; a missing or unreadable file gives the default,
/// so a broken file never stops the app from starting.
pub fn load<T: DeserializeOwned + Default>(dir: &Path, file: &str) -> T {
    fs::read(dir.join(file)).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default()
}

/// Writes pretty JSON with sorted keys (like the Swift app), atomically.
pub fn save<T: Serialize>(dir: &Path, file: &str, value: &T) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    // Going through Value sorts the keys (serde_json's map is a BTreeMap).
    let value = serde_json::to_value(value).map_err(io::Error::other)?;
    let text = serde_json::to_string_pretty(&value).map_err(io::Error::other)?;
    let tmp = dir.join(format!(".{file}.tmp"));
    fs::write(&tmp, text)?;
    fs::rename(tmp, dir.join(file))
}

/// A named host the user saved for quick switching (e.g. "Desktop LAN").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostPreset {
    pub name: String,
    pub host: String,
}

/// Network link settings (`netpad.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetSettings {
    /// Send the gamepad. The link also opens for MIDI or keyboard/mouse alone.
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// Let games on the peer drive this pad's motors.
    pub rumble: bool,
    pub presets: Vec<HostPreset>,
}

impl Default for NetSettings {
    fn default() -> Self {
        NetSettings { enabled: false, host: String::new(), port: DEFAULT_PORT, rumble: true, presets: Vec::new() }
    }
}
