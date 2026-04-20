//! Client-side HMAC proof generation for `session.hello`.
//!
//! Must produce the same bytes the host's `verify_proof` checks.

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const DOMAIN: &str = "superhq:v1:";

pub fn compute_proof(
    device_key: &[u8],
    host_node_id: &str,
    device_id: &str,
    timestamp: u64,
) -> Result<String, &'static str> {
    if device_key.len() != 32 {
        return Err("device key must be 32 bytes");
    }
    let mut mac = HmacSha256::new_from_slice(device_key).map_err(|_| "hmac init")?;
    mac.update(DOMAIN.as_bytes());
    mac.update(host_node_id.as_bytes());
    mac.update(b":");
    mac.update(device_id.as_bytes());
    mac.update(b":");
    mac.update(timestamp.to_string().as_bytes());
    let tag = mac.finalize().into_bytes();
    Ok(STANDARD.encode(tag))
}

pub fn decode_device_key(b64: &str) -> Result<Vec<u8>, &'static str> {
    STANDARD
        .decode(b64.as_bytes())
        .map_err(|_| "invalid base64 device key")
}

pub fn now_secs() -> u64 {
    // `SystemTime::now()` panics on wasm32-unknown-unknown (no OS clock).
    // Use the platform's JS `Date.now()` in WASM builds.
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}
