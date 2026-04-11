# 11 — Extract sandbox lifecycle into SandboxService

## Problem

Sandbox boot, checkpoint, fork, and close logic is embedded in
TerminalPanel's boot_agent_tab method (~400 lines). This mixes VM
orchestration with UI setup step management. The boot sequence:

1. Check if checkpoint exists
2. Maybe boot install sandbox, run install steps, save checkpoint
3. Boot tab sandbox from checkpoint (or base)
4. Configure mounts, ports, auth gateway, secrets
5. Write auth config files
6. Create .bash_profile
7. Launch agent shell
8. Wire up terminal view

Steps 1-6 are pure sandbox orchestration with no UI dependency.
Steps 7-8 need GPUI.

## Proposed: SandboxService

A plain struct (not an Entity — no rendering) that handles sandbox
lifecycle operations. Async methods return results; the caller (session
or terminal panel) handles UI updates.

```rust
pub struct SandboxService {
    db: Arc<Database>,
    tokio_handle: tokio::runtime::Handle,
    agents: Vec<Agent>,
}

impl SandboxService {
    /// Boot a sandbox for an agent tab. Handles install if needed.
    /// Returns the booted sandbox + auth gateway handle.
    pub async fn boot_agent(
        &self,
        agent_id: i64,
        workspace_id: i64,
        checkpoint_from: Option<String>,
        mount_path: Option<String>,
        on_progress: impl Fn(BootProgress) + Send,
    ) -> Result<BootedAgent> { ... }

    /// Checkpoint a running sandbox.
    pub async fn checkpoint(
        sandbox: &Arc<AsyncSandbox>,
        name: &str,
    ) -> Result<()> { ... }

    /// Fork from a checkpoint.
    pub async fn fork(
        &self,
        checkpoint: &str,
        workspace_id: i64,
    ) -> Result<BootedAgent> { ... }
}

pub struct BootedAgent {
    pub sandbox: Arc<AsyncSandbox>,
    pub auth_gateway: Option<AuthGatewayHandle>,
    pub shell_argv: Vec<String>,
    pub agent_env: HashMap<String, String>,
}

pub enum BootProgress {
    Step { index: usize, label: String },
    Installing { step: usize, total: usize },
    Booting,
}
```

### What moves from terminal/mod.rs

| Code | Lines (approx) | Moves to |
|------|----------------|----------|
| Install sandbox boot + step execution | 630-890 | SandboxService::boot_agent |
| Tab sandbox config building | 990-1050 | SandboxService::boot_agent |
| Auth gateway setup | 1050-1090 | SandboxService::boot_agent |
| .bash_profile creation | 860-875 | SandboxService::boot_agent |
| Checkpoint save | 1435-1490 | SandboxService::checkpoint |
| Fork logic | 1586-1620 | SandboxService::fork |
| Boot retry | 1050-1070 | SandboxService (internal) |

### What stays in terminal/mod.rs

- Setup step UI updates (SetupStep rendering)
- Terminal view creation + wiring
- Tab bar rendering
- Close confirmation UI

### File location

`src/sandbox/service.rs` — next to existing `sandbox/agent_config.rs`,
`sandbox/agent_setup.rs`, etc. This is business logic, not UI.

## Implementation

1. Create `src/sandbox/service.rs` with `SandboxService`
2. Move boot sequence from `boot_agent_tab` into `SandboxService::boot_agent`
3. The progress callback replaces direct `cx.update()` calls for step tracking
4. TerminalPanel creates a `SandboxService` at init
5. `boot_agent_tab` becomes: spawn task, call `service.boot_agent(on_progress)`,
   on completion wire up terminal view via `cx.update`

## Dependencies

- Spec 09 (WorkspaceSession) should land first — boot_agent_tab currently
  mutates session state directly, which needs to go through session methods
