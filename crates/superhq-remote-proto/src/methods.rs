//! JSON-RPC method names and their typed param/result shapes.
//!
//! Method names are dot-separated: `namespace.action`.
//! Each method has a `PARAMS` type (request params) and `Result` alias.

use serde::{Deserialize, Serialize};

use crate::types::{AgentInfo, BlobHandle, TabId, TabInfo, WorkspaceId, WorkspaceInfo};

// ── Session ─────────────────────────────────────────────────────

pub const SESSION_HELLO: &str = "session.hello";
pub const SESSION_PING: &str = "session.ping";
pub const SESSION_CLOSE: &str = "session.close";
pub const PAIRING_REQUEST: &str = "pairing.request";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionHelloParams {
    /// Highest protocol version the client supports.
    pub protocol_version: u32,
    /// Human-readable device label (for the paired-devices UI).
    pub device_label: String,
    /// Resume token from a prior session, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
    /// Auth proof — `None` only on hosts that haven't opted into
    /// auth-required mode (V1 migration). Hosts that require auth will
    /// reject `None` with `PERMISSION_DENIED`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<SessionAuth>,
}

/// Proof of pairing, included on every `session.hello`.
///
/// The `proof` is HMAC-SHA256 over the ASCII transcript:
///   `"superhq:v1:" || host_node_id || ":" || device_id || ":" || timestamp`
/// using `device_key` as the HMAC key. `timestamp` is a UNIX second
/// count; hosts accept up to ±5 minutes of drift.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionAuth {
    pub device_id: String,
    pub timestamp: u64,
    /// Base64-encoded HMAC-SHA256 output (32 bytes).
    pub proof: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairingRequestParams {
    pub device_label: String,
    /// 6-digit TOTP code for the no-host-access pairing path. If set and
    /// valid, host issues credentials without a local approval dialog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairingRequestResult {
    pub device_id: String,
    /// Base64-encoded 32-byte random key. Client stores this securely.
    pub device_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionHelloResult {
    /// Protocol version both sides agreed to use.
    pub protocol_version: u32,
    /// Server-assigned session id (opaque).
    pub session_id: String,
    /// Resume token for this session.
    pub resume_token: String,
    /// Host app info (for display / compatibility).
    pub host_info: HostInfo,
    /// Initial snapshot of workspaces on the host.
    pub workspaces: Vec<WorkspaceInfo>,
    /// Initial snapshot of tabs across all workspaces.
    pub tabs: Vec<TabInfo>,
    /// Agents configured on the host, for the client's new-tab menu.
    #[serde(default)]
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostInfo {
    pub app_version: String,
    pub os: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionCloseParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Workspaces ─────────────────────────────────────────────────

pub const WORKSPACES_LIST: &str = "workspaces.list";
pub const WORKSPACE_ACTIVATE: &str = "workspace.activate";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkspacesListParams {}

pub type WorkspacesListResult = Vec<WorkspaceInfo>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceActivateParams {
    pub workspace_id: WorkspaceId,
}

/// Result of `workspace.activate` — the freshly-activated workspace
/// plus the tabs live inside it right after activation (auto-launched
/// default agent, restored checkpointed tabs, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceActivateResult {
    pub workspace: WorkspaceInfo,
    pub tabs: Vec<TabInfo>,
}

// ── Tabs ───────────────────────────────────────────────────────

pub const TABS_LIST: &str = "tabs.list";
pub const TABS_CREATE: &str = "tabs.create";
pub const TABS_CLOSE: &str = "tabs.close";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TabsListParams {}

pub type TabsListResult = Vec<TabInfo>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabsCreateParams {
    pub workspace_id: WorkspaceId,
    pub spec: TabCreateSpec,
}

/// What kind of tab to create. Mirrors the desktop's +-menu:
/// a host-machine shell, a guest (sandbox) shell attached to an
/// existing agent tab, or a fresh agent tab.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TabCreateSpec {
    /// Run a shell on the host machine (not the sandbox).
    HostShell,
    /// Attach a guest shell to an existing agent tab's sandbox.
    GuestShell { parent_tab_id: TabId },
    /// Spawn a new agent tab. `agent_id: None` → use the default.
    Agent {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_id: Option<i64>,
    },
}

/// Tab creation is inherently async on the host — the tab's PTY may
/// not exist yet when the RPC returns. We send back just the
/// identifiers; the client uses them to set the active tab, and the
/// full `TabInfo` shows up on a subsequent `snapshot.invalidated`
/// push.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabsCreateResult {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
}

/// How to close a tab. Mirrors the desktop's close-confirm prompt:
/// `Checkpoint` snapshots the sandbox + keeps a stopped row the user
/// can resume later; `Force` tears the tab down entirely.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TabCloseMode {
    Checkpoint,
    Force,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabsCloseParams {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
    pub mode: TabCloseMode,
}

// ── PTY ─────────────────────────────────────────────────────────

pub const PTY_ATTACH: &str = "pty.attach";
pub const PTY_DETACH: &str = "pty.detach";
pub const PTY_RESIZE: &str = "pty.resize";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PtyAttachParams {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PtyAttachResult {
    pub cols: u16,
    pub rows: u16,
    /// Recent scrollback delivered as a blob handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_buffer: Option<BlobHandle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PtyDetachParams {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PtyResizeParams {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
    pub cols: u16,
    pub rows: u16,
}

// ── Diff ───────────────────────────────────────────────────────

pub const DIFF_SUBSCRIBE: &str = "diff.subscribe";
pub const DIFF_UNSUBSCRIBE: &str = "diff.unsubscribe";
pub const DIFF_KEEP: &str = "diff.keep";
pub const DIFF_DISCARD: &str = "diff.discard";
pub const DIFF_APPLY_PARTIAL: &str = "diff.apply_partial";
pub const DIFF_ASK_AGENT: &str = "diff.ask_agent";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffSubscribeParams {
    pub workspace_id: WorkspaceId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffPathParams {
    pub workspace_id: WorkspaceId,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffApplyPartialParams {
    pub workspace_id: WorkspaceId,
    pub path: String,
    /// (hunk_idx, line_idx_within_hunk) pairs the user discarded.
    pub discarded_lines: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffAskAgentParams {
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub selected_text: String,
    pub instruction: String,
}

// ── Status ─────────────────────────────────────────────────────

pub const STATUS_SUBSCRIBE: &str = "status.subscribe";
pub const STATUS_UNSUBSCRIBE: &str = "status.unsubscribe";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StatusSubscribeParams {}

// ── Ack ────────────────────────────────────────────────────────

/// Generic ack used as the result for many methods that just confirm success.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ack {
    pub ok: bool,
}

impl Default for Ack {
    fn default() -> Self {
        Self { ok: true }
    }
}

impl Ack {
    pub fn ok() -> Self {
        Self { ok: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hello_params_roundtrip() {
        let p = SessionHelloParams {
            protocol_version: 1,
            device_label: "iPhone 15".into(),
            resume_token: None,
            auth: None,
        };
        let wire = serde_json::to_value(&p).unwrap();
        assert_eq!(
            wire,
            json!({"protocol_version": 1, "device_label": "iPhone 15"})
        );
    }

    #[test]
    fn ack_roundtrip() {
        let wire = serde_json::to_value(Ack::ok()).unwrap();
        assert_eq!(wire, json!({"ok": true}));
    }
}
