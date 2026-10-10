//! The shared secret of the network link: 32 random bytes, stored as 64 hex
//! characters in `netpad.key` (mode 600) in the settings directory. Same
//! format on every platform, so the file can simply be copied.

use std::fs;
use std::io;
use std::path::Path;

use crate::settings::KEY_FILE;

pub const KEY_LEN: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct NetKey(pub [u8; KEY_LEN]);

// Never print the secret by accident.
impl std::fmt::Debug for NetKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NetKey(…)")
    }
}

impl NetKey {
    pub fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; KEY_LEN];
        getrandom::getrandom(&mut bytes).map_err(io::Error::other)?;
        Ok(NetKey(bytes))
    }

    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim();
        if hex.len() != KEY_LEN * 2 || !hex.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; KEY_LEN];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(NetKey(bytes))
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn load(dir: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(dir.join(KEY_FILE))?;
        Self::parse(&text)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "netpad.key: expected 64 hex characters"))
    }

    /// Loads the key, creating one on first use.
    pub fn load_or_create(dir: &Path) -> io::Result<Self> {
        if let Ok(key) = Self::load(dir) {
            return Ok(key);
        }
        let key = Self::generate()?;
        key.save(dir)?;
        Ok(key)
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join(KEY_FILE);
        fs::write(&path, self.to_hex() + "\n")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
