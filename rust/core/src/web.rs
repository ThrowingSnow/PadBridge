//! The status web page: its settings and who may see it.
//!
//! Requests from the home network (private IPv4 ranges, loopback, link-local)
//! see the page right away. Anything else, e.g. over Tailscale (100.64.0.0/10)
//! or IPv6 from outside, needs the PIN; the browser then keeps a session token
//! derived from the network key and the PIN, so it survives restarts and ends
//! when the PIN changes.

use std::net::IpAddr;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const DEFAULT_PORT: u16 = 48180;
pub const PIN_LEN: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WebSettings {
    pub enabled: bool,
    pub port: u16,
    /// Six digits; made on first use.
    pub pin: String,
}

impl Default for WebSettings {
    fn default() -> Self {
        WebSettings { enabled: false, port: DEFAULT_PORT, pin: String::new() }
    }
}

/// A random six-digit PIN.
pub fn new_pin() -> String {
    let mut bytes = [0u8; 4];
    getrandom::getrandom(&mut bytes).expect("system random source");
    format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000)
}

/// The cookie value that proves the PIN was entered.
pub fn session_token(key: &[u8], pin: &str) -> String {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(b"padbridge-web-session:");
    m.update(pin.as_bytes());
    m.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Home network: no PIN needed.
pub fn is_home_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_home_address(IpAddr::V4(v4));
            }
            // Loopback, link-local fe80::/10 and unique local fc00::/7.
            v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80 || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_network_is_private_ranges_only() {
        for ip in ["192.168.1.10", "10.0.0.2", "172.16.5.1", "127.0.0.1", "169.254.1.1", "::1", "fe80::1", "fd00::5"] {
            assert!(is_home_address(ip.parse().unwrap()), "{ip}");
        }
        // Tailscale, the internet, public IPv6.
        for ip in ["100.64.0.7", "8.8.8.8", "172.32.0.1", "2001:db8::1"] {
            assert!(!is_home_address(ip.parse().unwrap()), "{ip}");
        }
        assert!(is_home_address("::ffff:192.168.1.5".parse().unwrap()));
    }

    #[test]
    fn pin_has_six_digits_and_token_follows_it() {
        let pin = new_pin();
        assert_eq!(pin.len(), PIN_LEN);
        assert!(pin.chars().all(|c| c.is_ascii_digit()));
        let key = [7u8; 32];
        assert_eq!(session_token(&key, "123456"), session_token(&key, "123456"));
        assert_ne!(session_token(&key, "123456"), session_token(&key, "123457"));
        assert_eq!(session_token(&key, "123456").len(), 64);
    }
}
