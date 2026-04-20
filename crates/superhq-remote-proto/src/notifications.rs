//! JSON-RPC notification names (host → client push) and their param shapes.

use serde::{Deserialize, Serialize};

use crate::types::{AgentState, BlobHandle, FileStatus, TabId, TabInfo, WorkspaceId, WorkspaceInfo};

// ── Snapshot ───────────────────────────────────────────────────

/// Catch-all "the workspaces/tabs snapshot you hold is stale — call
/// `workspaces.list` + `tabs.list`". Sent by the host any time its
/// snapshot changes shape (new tab, pty_ready flipped, agent state
/// moved, etc). No params.
pub const SNAPSHOT_INVALIDATED: &str = "snapshot.invalidated";

// ── Workspaces ─────────────────────────────────────────────────

pub const WORKSPACES_ADDED: &str = "workspaces.added";
pub const WORKSPACES_REMOVED: &str = "workspaces.removed";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspacesAddedParams {
    pub workspace: WorkspaceInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspacesRemovedParams {
    pub workspace_id: WorkspaceId,
}

// ── Tabs ───────────────────────────────────────────────────────

pub const TABS_ADDED: &str = "tabs.added";
pub const TABS_REMOVED: &str = "tabs.removed";
pub const TABS_UPDATED: &str = "tabs.updated";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabsAddedParams {
    pub tab: TabInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabsRemovedParams {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabsUpdatedParams {
    pub tab: TabInfo,
}

// ── Diff ───────────────────────────────────────────────────────

pub const DIFF_FILE_CHANGED: &str = "diff.file_changed";
pub const DIFF_FILE_REMOVED: &str = "diff.file_removed";
pub const DIFF_FULL_DIFF: &str = "diff.full_diff";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffFileChangedParams {
    pub workspace_id: WorkspaceId,
    pub path: String,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffFileRemovedParams {
    pub workspace_id: WorkspaceId,
    pub path: String,
}

/// Full diff content delivered as a blob (could be large).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffFullDiffParams {
    pub workspace_id: WorkspaceId,
    pub path: String,
    /// JSON-encoded hunks blob. Structure defined in the diff renderer spec.
    pub blob: BlobHandle,
}

// ── Status ─────────────────────────────────────────────────────

pub const STATUS_AGENT_STATE: &str = "status.agent_state";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusAgentStateParams {
    pub workspace_id: WorkspaceId,
    pub tab_id: TabId,
    #[serde(flatten)]
    pub state: AgentState,
}
