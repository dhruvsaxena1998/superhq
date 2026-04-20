//! `RemoteStateSnapshot` — the point-in-time view of SuperHQ state that
//! the remote handler serves to clients. Built from `TerminalPanel` state
//! on the main thread, read by the handler from async tasks.

use std::sync::Arc;

use gpui::App;
use serde::Serialize;
use superhq_remote_proto::types::{AgentInfo, AgentState, TabInfo, TabKind, WorkspaceInfo};

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/agents/*.svg"]
struct AgentIconAssets;

use crate::db::Database;
use crate::ui::terminal::session::{AgentStatus as LocalAgentStatus, TabKind as LocalTabKind};
use crate::ui::terminal::TerminalPanel;

#[derive(Debug, Clone, Default, Serialize)]
pub struct RemoteStateSnapshot {
    pub workspaces: Vec<WorkspaceInfo>,
    pub tabs: Vec<TabInfo>,
    /// Whether the user has opted in to remote host-shell access.
    /// When false, remote clients cannot open a new host-shell tab
    /// nor attach to an existing one; the unsandboxed shell stays
    /// desktop-only by default.
    pub allow_host_shell: bool,
}

/// Snapshot the current workspaces + tabs.
///
/// **Workspaces** come from the database (the authoritative persistent list),
/// annotated with `is_active: true` for those currently loaded in the
/// `TerminalPanel`. **Tabs** come from the loaded sessions only — tabs of
/// inactive workspaces aren't live and don't make sense to expose yet.
/// Called from GPUI context; does not block on async work.
pub fn build_snapshot(
    terminal: &gpui::Entity<TerminalPanel>,
    db: &Arc<Database>,
    cx: &App,
) -> RemoteStateSnapshot {
    let panel = terminal.read(cx);
    let sessions = panel.sessions();

    let db_workspaces = db.list_workspaces().unwrap_or_default();
    let mut workspaces: Vec<WorkspaceInfo> = db_workspaces
        .into_iter()
        .map(|w| {
            let (repo_name, branch, github_owner) = match w.mount_path.as_deref() {
                Some(path) if crate::git::is_git_repo(std::path::Path::new(path)) => {
                    let repo = std::path::Path::new(path);
                    let repo_name = std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str().map(|s| s.to_string()));
                    let branch = crate::git::read_head_branch(repo);
                    let github_owner = crate::git::github_owner_for_repo(repo);
                    (repo_name, branch, github_owner)
                }
                _ => (None, None, None),
            };
            WorkspaceInfo {
                workspace_id: w.id,
                label: w.name,
                is_active: sessions.contains_key(&w.id),
                repo_name,
                branch,
                github_owner,
            }
        })
        .collect();

    // Live PTY registrations — a tab is only ready for remote attach
    // once its bus is in here. For agent tabs this means the sandbox
    // has booted; for shell tabs it happens almost immediately.
    let pty_map = panel.pty_map();
    let ready_ptys: std::collections::HashSet<(i64, u64)> = pty_map
        .read()
        .map(|m| m.keys().copied().collect())
        .unwrap_or_default();

    let mut tabs = Vec::new();
    for (workspace_id, session) in sessions {
        let s = session.read(cx);
        for tab in &s.tabs {
            tabs.push(TabInfo {
                workspace_id: *workspace_id,
                tab_id: tab.tab_id,
                label: tab
                    .dynamic_title
                    .borrow()
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| tab.label.to_string()),
                kind: map_tab_kind(&tab.kind),
                agent_state: map_agent_state(&tab.agent_status),
                pty_ready: ready_ptys.contains(&(*workspace_id, tab.tab_id)),
                setup_error: tab.setup_error.clone(),
            });
        }
    }

    // Stable ordering so clients see consistent lists between renders.
    workspaces.sort_by_key(|w| w.workspace_id);
    tabs.sort_by_key(|t| (t.workspace_id, t.tab_id));

    let allow_host_shell = db
        .get_settings()
        .map(|s| s.remote_host_shell_enabled)
        .unwrap_or(false);

    RemoteStateSnapshot {
        workspaces,
        tabs,
        allow_host_shell,
    }
}

fn map_tab_kind(kind: &LocalTabKind) -> TabKind {
    match kind {
        LocalTabKind::Agent { .. } => TabKind::Agent,
        LocalTabKind::Shell { .. } => TabKind::Shell,
        LocalTabKind::HostShell { .. } => TabKind::HostShell,
    }
}

/// Build the agents list to surface over the wire, with inlined SVG
/// icons so remote clients can render the desktop's +-menu exactly.
pub fn build_agent_infos(db: &Arc<Database>) -> Vec<AgentInfo> {
    let mut agents = db.list_agents().unwrap_or_default();
    agents.sort_by_key(|a| a.tab_order);
    agents
        .into_iter()
        .map(|a| {
            let slug = a
                .icon
                .as_deref()
                .and_then(|path| {
                    std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str().map(|s| s.to_string()))
                })
                .or_else(|| Some(a.name.clone()));
            let icon_svg = a.icon.as_deref().and_then(|p| {
                AgentIconAssets::get(p)
                    .and_then(|f| String::from_utf8(f.data.into_owned()).ok())
            });
            AgentInfo {
                id: a.id,
                display_name: a.display_name,
                slug,
                icon_svg,
                color: a.color,
            }
        })
        .collect()
}

fn map_agent_state(status: &LocalAgentStatus) -> AgentState {
    match status {
        LocalAgentStatus::Unknown => AgentState::Unknown,
        LocalAgentStatus::Idle => AgentState::Idle,
        LocalAgentStatus::Running { tool } => AgentState::Running { tool: tool.clone() },
        LocalAgentStatus::NeedsInput { message } => AgentState::NeedsInput {
            message: message.clone(),
        },
    }
}
