//! Stream initialization framing.
//!
//! Every non-control stream opens with a `StreamInit` message that
//! identifies what the stream carries. Sent as a single JSON-RPC request
//! (method `stream.init`) — server replies with an ack, then the
//! stream-specific protocol takes over (for PTY: raw bytes both ways).

use serde::{Deserialize, Serialize};

use crate::types::{TabId, WorkspaceId};

pub const STREAM_INIT: &str = "stream.init";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamInit {
    /// A terminal stream attached to a specific tab.
    Pty {
        workspace_id: WorkspaceId,
        tab_id: TabId,
        cols: u16,
        rows: u16,
    },
    /// A status event stream (host → client, unidirectional).
    Status,
}
