-- Persistent iroh endpoint secret key for the remote-control server.
--
-- Without this, iroh generates a fresh key on every launch, which
-- rotates the NodeId — breaking every paired device's cached id. With
-- this, the host keeps the same NodeId across restarts so paired
-- phones / browsers can reconnect without re-scanning the QR.
--
-- The `encrypted_secret` column uses the same AES-256-GCM wrapping as
-- `secrets.encrypted_value` / `paired_devices.encrypted_device_key`.
CREATE TABLE IF NOT EXISTS remote_endpoint_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    encrypted_secret BLOB NOT NULL
);
