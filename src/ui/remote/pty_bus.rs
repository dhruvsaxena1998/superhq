//! `PtyBus` — the shared runtime handle to a single terminal's PTY that
//! bridges the local `TerminalView` and any remote clients.
//!
//! Lives outside GPUI entities (in a plain `Arc<RwLock<HashMap>>`) so both
//! the GPUI render thread and async tokio tasks can access it safely.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use bytes::Bytes;
use shuru_sdk::ShellWriter;
use superhq_remote_proto::types::{TabId, WorkspaceId};

/// Abstract input side of a PTY: send keystrokes, resize.
/// Implementations bridge to whatever concrete writer the tab uses —
/// `ShellWriter` for sandboxed agent / guest-shell tabs, a
/// `portable_pty` master for the host shell.
pub trait PtyInput: Send + Sync {
    fn send_input(&self, data: &[u8]) -> Result<(), String>;
    /// Resize using (rows, cols) — matches the shuru-sdk convention.
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String>;
}

impl PtyInput for ShellWriter {
    fn send_input(&self, data: &[u8]) -> Result<(), String> {
        self.send_input(data).map_err(|e| e.to_string())
    }
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.resize(rows, cols).map_err(|e| e.to_string())
    }
}

/// Runtime handle for one tab's PTY.
///
/// The `scrollback` mutex **also guards** the broadcast ordering: the
/// PTY-reader thread pushes to the ring and sends to the broadcast under
/// the same lock, so an attach handler holding the lock can snapshot +
/// subscribe atomically. See `snapshot_and_subscribe`.
#[derive(Clone)]
pub struct PtyBus {
    pub writer: Arc<dyn PtyInput>,
    pub output: tokio::sync::broadcast::Sender<Bytes>,
    pub dimensions: Arc<Mutex<(u16, u16)>>,
    pub scrollback: Arc<Mutex<crate::sandbox::pty_adapter::ScrollbackRing>>,
}

impl PtyBus {
    pub fn new(
        writer: impl PtyInput + 'static,
        output: tokio::sync::broadcast::Sender<Bytes>,
        scrollback: Arc<Mutex<crate::sandbox::pty_adapter::ScrollbackRing>>,
    ) -> Self {
        Self {
            writer: Arc::new(writer),
            output,
            dimensions: Arc::new(Mutex::new((80, 24))),
            scrollback,
        }
    }

    /// Atomically capture the current scrollback bytes AND subscribe to
    /// future live output. Any byte the PTY emits after this call goes
    /// to the returned receiver; everything before is in the snapshot.
    /// No duplicates, no gaps.
    pub fn snapshot_and_subscribe(
        &self,
    ) -> (Vec<u8>, tokio::sync::broadcast::Receiver<Bytes>) {
        let sb = self.scrollback.lock();
        let bytes = sb
            .as_ref()
            .map(|s| s.snapshot())
            .unwrap_or_default();
        let sub = self.output.subscribe();
        // Explicit drop of the MutexGuard *after* subscribe so the reader
        // thread can't interleave.
        drop(sb);
        (bytes, sub)
    }

    /// Apply a resize. Both pushes the new size to the PTY and updates
    /// the stored dimensions so future `pty.attach` responses reflect it.
    pub fn resize(&self, cols: u16, rows: u16) {
        // Writers historically take (rows, cols).
        let _ = self.writer.resize(rows, cols);
        if let Ok(mut g) = self.dimensions.lock() {
            *g = (cols, rows);
        }
    }

    pub fn current_dimensions(&self) -> (u16, u16) {
        self.dimensions
            .lock()
            .map(|g| *g)
            .unwrap_or((80, 24))
    }
}

/// Shared map keyed by (workspace_id, tab_id).
pub type PtyMap = Arc<RwLock<HashMap<(WorkspaceId, TabId), PtyBus>>>;

pub fn new_pty_map() -> PtyMap {
    Arc::new(RwLock::new(HashMap::new()))
}
