//! Cross-runtime pairing approval channel.
//!
//! The RPC `pairing.request` runs on the tokio runtime (background
//! task) but the approval UI lives on the GPUI main thread. We bridge
//! the two with a flume request queue plus a `tokio::sync::oneshot`
//! response for each request — the handler awaits the response with a
//! bounded timeout so a missing / closed UI can't hang the peer
//! indefinitely.

use std::time::Duration;

use tokio::sync::oneshot;

/// How long `pairing.request` waits for a user decision before giving
/// up. 120 s is comfortable for a user walking back to the host and
/// short enough that clients don't hold an iroh connection forever.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// One pending approval — handed from the async handler to the GPUI
/// thread. `response` is consumed by the UI (Approve = `true`,
/// Reject = `false`). Dropping without sending a value is treated as
/// a reject because `tokio::oneshot` closes the channel.
pub struct PairingApprovalRequest {
    pub device_label: String,
    pub response: oneshot::Sender<bool>,
}

/// Handle used by `AppHandler` to submit requests to the UI.
#[derive(Clone)]
pub struct PairingApprover {
    tx: flume::Sender<PairingApprovalRequest>,
}

impl PairingApprover {
    pub fn new() -> (Self, flume::Receiver<PairingApprovalRequest>) {
        let (tx, rx) = flume::unbounded();
        (Self { tx }, rx)
    }

    /// Send a request and wait for the UI's decision. Returns `false`
    /// on timeout, channel closure, or an explicit reject — callers
    /// map `false` to `PAIRING_REJECTED`.
    pub async fn request_approval(&self, device_label: String) -> bool {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = PairingApprovalRequest {
            device_label,
            response: resp_tx,
        };
        if self.tx.send_async(req).await.is_err() {
            return false;
        }
        match tokio::time::timeout(APPROVAL_TIMEOUT, resp_rx).await {
            Ok(Ok(approved)) => approved,
            _ => false,
        }
    }
}
