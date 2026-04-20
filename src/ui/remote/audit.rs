//! Append-only audit log for remote-control RPC activity.
//!
//! Every control-stream method call the host receives is recorded as a
//! single JSON line with: timestamp, method name, authenticated device
//! id (when known), and ok/err. Logs live at
//! `<data_dir>/logs/remote-audit.log` and are rotated when the active
//! file crosses ~5 MB, keeping the last [`MAX_FILES`] backups. Rotation
//! is in-process only — we do not trust external log shippers.
//!
//! The writer is shared across tokio tasks via `Arc<Mutex<_>>`. Audit
//! writes happen on the control-stream task so the mutex contention is
//! low (sequential per connection). Failure to write does not propagate
//! to clients — we warn and move on.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Roll over the active log at this size (bytes). 5 MB keeps files
/// openable in any editor and keeps the retained set under ~30 MB.
const ROTATE_BYTES: u64 = 5 * 1024 * 1024;

/// Keep the active log plus this many rotated backups
/// (`remote-audit.log.1` … `remote-audit.log.N`).
const MAX_FILES: usize = 5;

#[derive(Clone)]
pub struct AuditLog {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    path: PathBuf,
    file: Option<File>,
    bytes_written: u64,
}

#[derive(Serialize)]
struct Entry<'a> {
    ts: u64,
    method: &'a str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<&'a str>,
}

impl AuditLog {
    /// Open (or create) the audit log at the standard data-dir location.
    /// A failure to open is logged via `tracing::warn!` and the returned
    /// instance becomes a no-op — we never want to crash the remote
    /// server because audit can't write.
    pub fn open() -> Self {
        let path = crate::runtime::data_dir().join("logs").join("remote-audit.log");
        let (file, bytes) = match Self::open_file(&path) {
            Ok((f, b)) => (Some(f), b),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "audit: failed to open log");
                (None, 0)
            }
        };
        Self {
            inner: Arc::new(Mutex::new(Inner {
                path,
                file,
                bytes_written: bytes,
            })),
        }
    }

    /// Absolute path of the active log file — surfaced in the UI so
    /// users can open it in their editor.
    pub fn path(&self) -> PathBuf {
        self.inner.lock().map(|g| g.path.clone()).unwrap_or_default()
    }

    /// Record a single RPC dispatch. `device_id` is the authenticated
    /// device (after `session.hello`), or `None` for pre-auth methods.
    pub fn log(&self, method: &str, ok: bool, device_id: Option<&str>) {
        let entry = Entry {
            ts: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            method,
            ok,
            device: device_id,
        };
        let Ok(mut line) = serde_json::to_string(&entry) else {
            return;
        };
        line.push('\n');

        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(file) = inner.file.as_mut() {
            if let Err(e) = file.write_all(line.as_bytes()) {
                tracing::warn!(error = %e, "audit: write failed");
                return;
            }
            inner.bytes_written += line.len() as u64;
            if inner.bytes_written >= ROTATE_BYTES {
                if let Err(e) = inner.rotate() {
                    tracing::warn!(error = %e, "audit: rotation failed");
                }
            }
        }
    }

    fn open_file(path: &std::path::Path) -> std::io::Result<(File, u64)> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok((file, size))
    }
}

impl Inner {
    fn rotate(&mut self) -> std::io::Result<()> {
        // Close the current handle before renaming so Windows / macOS
        // can always complete the rename. On Unix rename-while-open is
        // fine, but dropping first is cheap and makes the code portable.
        self.file = None;

        // Shift .N-1 → .N, dropping any .N beyond MAX_FILES.
        for i in (1..MAX_FILES).rev() {
            let src = sidecar_path(&self.path, i);
            let dst = sidecar_path(&self.path, i + 1);
            if src.exists() {
                let _ = std::fs::rename(&src, &dst);
            }
        }
        // Move active → .1, then reopen active for append.
        let dot_one = sidecar_path(&self.path, 1);
        let _ = std::fs::rename(&self.path, &dot_one);

        let (f, bytes) = AuditLog::open_file(&self.path)?;
        self.file = Some(f);
        self.bytes_written = bytes;
        Ok(())
    }
}

fn sidecar_path(base: &std::path::Path, n: usize) -> PathBuf {
    let mut os = base.as_os_str().to_os_string();
    os.push(format!(".{n}"));
    PathBuf::from(os)
}
