-- Whether the remote-control server starts automatically. Users can
-- turn it off from the Settings → Remote control tab; we honor the
-- setting on next app launch, and changes at runtime start/stop the
-- in-memory RemoteServer immediately.
ALTER TABLE settings ADD COLUMN remote_control_enabled INTEGER NOT NULL DEFAULT 1;
