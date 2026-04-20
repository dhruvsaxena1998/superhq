-- Paired remote-control devices.
--
-- `encrypted_device_key` stores the 32-byte device secret encrypted with
-- the app's AES-256-GCM key (same pattern as `secrets.encrypted_value`).
CREATE TABLE IF NOT EXISTS paired_devices (
    device_id TEXT PRIMARY KEY,
    device_label TEXT NOT NULL,
    encrypted_device_key BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER,
    allowed_workspaces_json TEXT
);
