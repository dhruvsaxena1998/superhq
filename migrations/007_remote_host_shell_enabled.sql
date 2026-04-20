-- Remote access to the unsandboxed host shell is opt-in. A device
-- paired via remote control can still view agent/guest tabs by
-- default; host-shell access requires the user to flip this flag.
ALTER TABLE settings ADD COLUMN remote_host_shell_enabled BOOLEAN NOT NULL DEFAULT 0;
