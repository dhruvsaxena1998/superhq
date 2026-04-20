//! Paired-device store — thin wrapper over the existing SQLite + AES-GCM
//! infrastructure that secrets already use. Device keys are stored
//! encrypted at rest; the same app-wide encryption key protects both
//! API secrets and pairing credentials.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine};

use crate::db::Database;

#[derive(Debug, Clone)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_label: String,
    /// Base64-encoded 32-byte key. Only populated when a caller explicitly
    /// needs the key (e.g. for HMAC verification on `session.hello`).
    pub device_key_b64: String,
    pub created_at: u64,
}

pub struct PairingStore {
    db: Arc<Database>,
}

impl PairingStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn get(&self, device_id: &str) -> Option<PairedDevice> {
        let key_bytes = self.db.get_paired_device_key(device_id).ok().flatten()?;
        let row = self
            .db
            .list_paired_devices()
            .ok()?
            .into_iter()
            .find(|r| r.device_id == device_id)?;
        Some(PairedDevice {
            device_id: row.device_id,
            device_label: row.device_label,
            device_key_b64: STANDARD.encode(&key_bytes),
            created_at: row.created_at,
        })
    }

    pub fn insert(&self, device: PairedDevice) {
        let Ok(key_bytes) = STANDARD.decode(device.device_key_b64.as_bytes()) else {
            tracing::warn!(device = %device.device_id, "pairing: malformed base64 device key");
            return;
        };
        if let Err(e) = self.db.save_paired_device(
            &device.device_id,
            &device.device_label,
            &key_bytes,
            device.created_at,
        ) {
            tracing::warn!(error = %e, "pairing: save_paired_device failed");
        }
    }

    #[allow(dead_code)]
    pub fn remove(&self, device_id: &str) -> bool {
        self.db.remove_paired_device(device_id).is_ok()
    }

    /// Drop every paired device. Used by the "rotate host id" flow —
    /// a new NodeId invalidates every prior pairing's HMAC transcript,
    /// so keeping the rows around would only create ghost entries that
    /// can never authenticate.
    pub fn clear_all(&self) -> usize {
        let ids: Vec<String> = self
            .db
            .list_paired_devices()
            .map(|rows| rows.into_iter().map(|r| r.device_id).collect())
            .unwrap_or_default();
        let mut removed = 0;
        for id in &ids {
            if self.db.remove_paired_device(id).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    pub fn touch(&self, device_id: &str, now: u64) {
        let _ = self.db.touch_paired_device(device_id, now);
    }

    #[allow(dead_code)]
    pub fn list(&self) -> Vec<PairedDevice> {
        let Ok(rows) = self.db.list_paired_devices() else {
            return Vec::new();
        };
        rows.into_iter()
            .filter_map(|r| {
                let key = self.db.get_paired_device_key(&r.device_id).ok().flatten()?;
                Some(PairedDevice {
                    device_id: r.device_id,
                    device_label: r.device_label,
                    device_key_b64: STANDARD.encode(&key),
                    created_at: r.created_at,
                })
            })
            .collect()
    }
}
