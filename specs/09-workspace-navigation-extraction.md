# 09 — Extract workspace navigation into WorkspaceSession

## Problem

`terminal/mod.rs` is a giant file (~2700 lines) mixing navigation state,
sandbox lifecycle, agent setup, port forwarding, and UI rendering. Business
logic is deeply coupled with GPUI rendering code.

## How Zed does it

Zed separates concerns into layers:
- `Entity<Project>` — all business logic and services (no UI)
- `Entity<Workspace>` — UI orchestration, holds panes/panels/docks
- Communication via GPUI's `cx.emit()` + `cx.subscribe()` (typed events)
- `cx.notify()` + `cx.observe()` for state change propagation

## Proposed architecture

### New: `WorkspaceSession` (business logic, no GPUI rendering)

Pure state + operations. Implements `EventEmitter<SessionEvent>`.

```rust
pub struct WorkspaceSession {
    pub workspace_id: i64,
    pub tabs: Vec<TabEntry>,
    pub active_tab: usize,
}

pub struct TabEntry {
    pub tab_id: u64,
    pub label: SharedString,
    pub kind: TabKind,
    pub sandbox: Option<Arc<AsyncSandbox>>,
    pub terminal: Option<Entity<TerminalView>>,
    pub setup_steps: Option<Vec<SetupStep>>,
    pub setup_error: Option<String>,
    pub checkpoint_name: Option<String>,
    pub checkpointing: bool,
    // ... existing fields
}
```

### Events (replaces direct mutation + cx.notify)

```rust
pub enum SessionEvent {
    TabAdded { tab_id: u64 },
    TabRemoved { tab_id: u64 },
    TabActivated { tab_id: u64 },
    SandboxReady { tab_id: u64, sandbox: Arc<AsyncSandbox> },
    SandboxStopped { tab_id: u64 },
    SetupProgress { tab_id: u64, step: usize, status: StepStatus },
    SetupFailed { tab_id: u64, error: String },
    CheckpointStarted { tab_id: u64 },
    CheckpointCompleted { tab_id: u64, name: String },
}
```

### Navigation methods (pure state transitions)

```rust
impl WorkspaceSession {
    pub fn activate_tab(&mut self, idx: usize, cx: &mut Context<Self>) { ... cx.emit(...) }
    pub fn close_tab(&mut self, tab_id: u64, cx: &mut Context<Self>) { ... cx.emit(...) }
    pub fn add_tab(&mut self, entry: TabEntry, cx: &mut Context<Self>) { ... cx.emit(...) }
    pub fn reorder_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) { ... }
    pub fn next_tab(&mut self, cx: &mut Context<Self>) { ... }
    pub fn prev_tab(&mut self, cx: &mut Context<Self>) { ... }
    pub fn active_sandbox(&self) -> Option<(Arc<AsyncSandbox>, ...)> { ... }
}
```

### TerminalPanel becomes a thin UI shell

```rust
pub struct TerminalPanel {
    sessions: HashMap<i64, Entity<WorkspaceSession>>,
    active_workspace_id: Option<i64>,
    db: Arc<Database>,
    tokio_handle: Handle,
    // UI-only state
    pending_close: Option<(i64, u64)>,
    side_panel: Option<Entity<SidePanel>>,
    // callbacks
    on_open_settings: ...,
    on_open_port_dialog: ...,
}
```

TerminalPanel subscribes to session events:

```rust
cx.subscribe(&session, |panel, session, event, cx| {
    match event {
        SessionEvent::SandboxReady { .. } => {
            // notify side panel
        }
        SessionEvent::SandboxStopped { .. } => {
            // deactivate review panel
        }
        SessionEvent::TabRemoved { .. } => {
            // check if last tab, deactivate review
        }
        _ => {}
    }
    cx.notify(); // re-render
});
```

### What moves where

| Currently in TerminalPanel | Moves to |
|---|---|
| `sessions: HashMap<i64, Session>` | `sessions: HashMap<i64, Entity<WorkspaceSession>>` |
| Tab add/remove/switch/reorder | `WorkspaceSession` methods |
| `get_active_sandbox()` | `WorkspaceSession::active_sandbox()` |
| Setup step tracking | `WorkspaceSession` (emits SetupProgress) |
| Checkpoint logic | `WorkspaceSession` (emits CheckpointStarted/Completed) |
| Tab bar rendering | Stays in TerminalPanel (reads session state) |
| Setup view rendering | Stays in TerminalPanel |
| Status bar rendering | Stays in TerminalPanel |
| Boot sequence (`boot_agent_tab`) | Stays in TerminalPanel (orchestration) but calls session methods |

## Implementation plan

1. Create `src/ui/terminal/session.rs` with `WorkspaceSession` struct
2. Move tab state and navigation methods from TerminalPanel
3. Make WorkspaceSession an Entity with EventEmitter<SessionEvent>
4. TerminalPanel subscribes to events for side effects (review panel, etc.)
5. TerminalPanel render reads from session entities
6. Boot/checkpoint orchestration stays in TerminalPanel but delegates
   state mutations to session methods

## Not in scope

- Full Zed-style Panel/Item traits (too much abstraction for current scale)
- Extracting sandbox lifecycle into a separate service (future work)
- Splitting terminal/mod.rs into multiple render files (do after this)
