# Track 3 — Desktop Integration

Status: `draft`

## Context

Tracks 1 and 2 define the transport and the auth model. This track is how
the feature lives inside the existing SuperHQ GPUI app: where the user
toggles it on, what they see when they do, how pairings are managed, how
the app continues running when the main window is closed. In short: the
host-side UX.

## Goals

- **Discoverable**: the user finds it without reading docs.
- **Unambiguous**: "Remote control is ON" / "Remote control is OFF" is
  obvious at a glance.
- **Safe-by-default**: every new device requires an explicit approval
  gesture on the host.
- **Survives window close**: closing the SuperHQ main window does not kill
  running agents, and optionally keeps remote control alive in the background.

## Non-goals

- Web-client UX (Track 4).
- Automatic/CI headless mode (SuperHQ is a GUI app; headless is out of scope).
- Mobile-desktop synchronization features beyond pairing (no shared bookmarks,
  shared settings, etc. — those are product scope, not this track).

## UX surface — overview

Five distinct UI elements:

1. **Title-bar button + popover** — the primary control surface.
2. **Pairing approval dialog** — consent prompt when a new device connects.
3. **Paired-devices list** — manage what's connected (revoke, rename, see
   last-seen).
4. **TOTP enrollment flow** — one-time setup for the out-of-band fallback.
5. **Tray icon + daemon mode** — background operation after window close.

Phasing:

- **V1 must-haves**: 1, 2, 3 basic (label + revoke). Ship behind a
  Settings toggle that defaults off.
- **V1 nice-to-haves**: 4 (TOTP enrollment). Can be deferred one release.
- **V2**: 5 (tray + daemon mode). Big enough to merit its own iteration.

## 1. Title-bar button + popover

### Placement
Top-left area of the title bar, **to the right of the existing sidebar
toggle button**. Small icon-only button with a status indicator dot.

### States

| State | Visual |
|-------|--------|
| Off / disabled in Settings | Button hidden |
| On, no devices connected | Neutral icon, no dot |
| On, one or more devices connected | Icon + green dot |
| Pairing approval pending | Icon + pulsing amber dot |
| Error (server failed to start) | Icon + red dot |

### Popover content

Clicking the button opens a small popover (~320px wide) anchored to the
button:

```
┌─────────────────────────────────────────────┐
│  Remote control                              │
│  ─────────────────────────────────────────  │
│  [●] Enabled                                │
│                                             │
│  Your host id:                              │
│  ┌───────────────────────────────────────┐  │
│  │ 7d84d046...c8a3a2da4   [copy] [QR]    │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  Connect from: remote.superhq.ai            │
│                                             │
│  2 devices connected                        │
│    • iPhone 15   active now                 │
│    • MacBook     idle 3m                    │
│                                             │
│  [ Manage devices ]  [ Settings ]           │
└─────────────────────────────────────────────┘
```

- **Enabled toggle**: flips server on/off. When turning off, shows a brief
  confirmation ("Disconnect 2 devices and stop?") if any are connected.
- **Host id**: the iroh NodeId, shown truncated with a click-to-copy button
  and a "QR" button that expands an inline QR code encoding the NodeId.
- **remote.superhq.ai**: the hosted PWA URL, a hint for where to point a
  browser. (The URL itself does not carry the NodeId — it's just the app.)
- **Connected devices list**: shows device_label + "active now" / "idle Xs".
  Compact; "Manage devices" opens the full list.
- **Settings**: opens a Settings pane dedicated to remote control (TOTP
  enrollment, paired devices, connection log).

### Offline / not-yet-ready indicators

If the iroh endpoint hasn't reached "online" state yet (just started, waiting
on relays), show a subtle "Starting..." line instead of the host id, grayed
out. Disable the QR button until ready.

## 2. Pairing approval dialog

Triggered when a client sends `pairing.request` without TOTP.

### Design

Modal dialog (not a notification — this is a consent action that must have
user focus):

```
┌──────────────────────────────────────────────┐
│  Pair new device?                            │
│  ──────────────────────────────────────────  │
│                                              │
│  A device is requesting access to SuperHQ.   │
│                                              │
│  Device label: iPhone 15 (user-provided)     │
│  Source: via remote.superhq.ai               │
│                                              │
│  After approval, this device can:            │
│    • See your workspaces and agents          │
│    • Send keystrokes to your terminals       │
│    • Accept/discard diffs                    │
│                                              │
│  You can revoke this device at any time.     │
│                                              │
│          [ Reject ]       [ Approve ]        │
└──────────────────────────────────────────────┘
```

Timeout: 60 seconds. If no button clicked within 60s, treated as Reject
and `pairing.rejected { reason: "timeout" }` is sent.

**Approve** is not the default-focus button. Default focus is Reject — we
want intentional approval, not press-enter-to-approve.

### Rate limiting

If more than 3 `pairing.request` messages arrive within 60 seconds without
approval, additional requests are auto-rejected with `rate_limited` for the
next 60 seconds. Prevents spam-approval tricks.

## 3. Paired devices list

A dedicated area in Settings (Remote Control section). Also reachable from
the popover's "Manage devices" link.

### Design

```
┌─────────────────────────────────────────────────────┐
│  Paired devices                                     │
│  ─────────────────────────────────────────────────  │
│                                                     │
│  ┌─────────────────────────────────────────────┐   │
│  │ ● iPhone 15                                 │   │
│  │   active now                                │   │
│  │   paired Mar 14, 2026                       │   │
│  │                              [Rename] [×]   │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  ┌─────────────────────────────────────────────┐   │
│  │   MacBook (office)                          │   │
│  │   idle 3m                                   │   │
│  │   paired Feb 2, 2026                        │   │
│  │                              [Rename] [×]   │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  ┌─────────────────────────────────────────────┐   │
│  │   iPad                                      │   │
│  │   last seen 3 days ago                      │   │
│  │   paired Jan 9, 2026                        │   │
│  │                              [Rename] [×]   │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
└─────────────────────────────────────────────────────┘
```

Actions:

- **Rename**: local-only label change, stored in `paired_devices.device_label`.
- **Revoke (×)**: confirm-first, then delete pairing record + keychain entry
  + tear down any live session from that device. Non-recoverable.

### Live indicator

The solid green dot next to "iPhone 15" is driven by the actual session
state — present and active if there's a live connection with that
`device_id`, absent otherwise.

## 4. TOTP enrollment

Accessed from Settings → Remote Control → "Enable TOTP fallback".

### Flow

1. Generate 160-bit random secret, store in OS keychain (temporarily).
2. Render QR code for an authenticator app (otpauth URL).
3. Show: "Scan this in your authenticator app, then enter a code below."
4. Single text input for 6-digit code.
5. On submit, verify. If valid, mark enrollment complete (keychain entry
   persists); if invalid, show error, let them try again.
6. Once enrolled: Settings shows "TOTP is enabled" with a "Disable" button
   that wipes the secret after confirmation.

### Design

```
┌──────────────────────────────────────────────┐
│  Enable TOTP fallback                        │
│  ──────────────────────────────────────────  │
│                                              │
│  TOTP lets you pair devices when you're not  │
│  at this computer. Use any authenticator     │
│  app.                                        │
│                                              │
│   ┌──────────────┐                           │
│   │              │                           │
│   │   QR CODE    │                           │
│   │              │                           │
│   └──────────────┘                           │
│                                              │
│  Or enter manually: XXXX-XXXX-XXXX-XXXX      │
│                                              │
│  Verify: [ 6-digit code ]  [ Enable ]        │
│                                              │
└──────────────────────────────────────────────┘
```

## 5. Tray icon + daemon mode (V2)

### Why daemon mode matters

Agents run long. Users close the main window to reclaim screen space but
expect their agents to keep running. Tray-and-daemon makes this a first-class
feature instead of an accident.

### Design

- **Tray icon** (menu bar on macOS, system tray on Linux/Windows). Uses the
  `tray-icon` crate (Tauri's tray crate, standalone-usable, cross-platform).
- **Click behavior**:
  - Left click: show/hide main window.
  - Right click (or click on macOS): menu with:
    - "Show SuperHQ"
    - "Remote control: On/Off" (toggle)
    - Paired devices shortcut
    - "Quit"
- **App lifecycle**:
  - Closing main window → window hides, app stays running, tray icon remains.
  - On macOS, toggle `NSApp.setActivationPolicy(.accessory)` when main window
    closes so the Dock icon disappears (optional — user setting).
  - Quit from tray menu → full app shutdown (tears down agents, remote
    access, sandboxes).

### Caveat: sandbox longevity

Verify that closing the main window does not tear down the shuru sandboxes
(which host running agents). They're separate OS processes, so they should
survive, but this assumption needs a smoke test before we promise the UX.

## Settings

A new "Remote Control" section in the existing Settings pane:

- **Master toggle**: Enable/Disable remote control. Default off.
- **Auto-start**: on app launch, automatically enable remote control. Default off.
- **Keep running when window closes** (V2): pairs with tray/daemon mode.
- **TOTP fallback**: Enable / Disable / Reset.
- **Paired devices**: link to the devices list (or inline if space allows).
- **Connection history** (V2): last N connections with device_label,
  timestamp, duration. Useful for "did anyone connect while I was away?"

## Host-side wiring

Not a user-facing spec item but affects the integration shape.

### New module: `src/ui/remote/`

```
src/ui/remote/
├── mod.rs           # RemoteAccess singleton (owns RemoteServer lifecycle)
├── handler.rs       # AppHandler: real RemoteHandler impl reading app state
├── pairing.rs       # pairing dialog + rate limiting state
├── devices.rs       # paired-devices list UI
├── button.rs        # title-bar button + popover
└── settings.rs      # settings pane section
```

### Lifecycle

- `RemoteAccess` singleton lives in `AppView` or `TerminalPanel`.
- On app start, reads the master-toggle setting:
  - If on: spawn `RemoteServer::spawn(AppHandler::new(app_state))`.
  - If off: idle; no server, no port published.
- Master toggle in the UI calls `RemoteAccess::set_enabled(bool)` which
  spawns/shuts down the server.

### `AppHandler` — mapping protocol to app state

The spec for this is: `AppHandler` implements `RemoteHandler` by reading
the current `TerminalPanel` state:

- `workspaces_list` → enumerate `TerminalPanel.sessions` keys, return
  `WorkspaceInfo` for each (id + workspace_name).
- `tabs_list` → iterate every session's tabs, build `TabInfo` for each
  using the existing `TabKind`, `AgentStatus`, `label`.
- `pty_attach` → look up `(workspace_id, tab_id)`, return `(cols, rows,
  initial_buffer)` where the buffer is a scrollback snapshot from the
  terminal's content grid.
- `pty_stream` → **the tricky one**. We need PTY output to go to BOTH the
  local `TerminalView` AND the remote client. Options:
  - **Broadcast at the shell layer**: when we construct `ShuruPtyReader`
    in `boot.rs`, wrap the reader in a broadcast. Each subscriber (local
    view, remote stream) gets a clone.
  - **Tap inside TerminalView**: add a hook to TerminalView that emits
    every byte it receives. Remote clients subscribe to the hook.
  - Broadcast at the shell layer is cleaner — single point where bytes
    come in, then fan out. Requires touching `boot.rs` and `pty_adapter.rs`.

For client → PTY: simpler. Bytes from remote go straight to
`ShuruPtyWriter` (which is what the local keyboard also uses). The PTY
doesn't know or care who's typing.

### Notifications

`AppHandler` needs to push notifications (tabs added/removed, agent state
changed, diff events). Hook into existing SuperHQ event sources:

- Workspace/tab mutations already happen through `WorkspaceSession` methods.
  Add event emission there, let `AppHandler` subscribe.
- Agent status: the existing `AgentEventService` already publishes events;
  tap that stream.
- Diff events: the existing `WatchBridge` produces `DiffResult`; tap that.

## Settings schema migration

New keys in the app settings DB:

- `remote_access.enabled` (bool, default false)
- `remote_access.auto_start` (bool, default false)
- `remote_access.keep_running_when_closed` (bool, default false, V2)
- `remote_access.totp_enabled` (bool, default false)

## Accessibility

- Approval dialog must be keyboard-navigable; Tab cycles Reject/Approve.
  Escape = reject.
- Paired-devices list: each row fully navigable by keyboard.
- Screen reader announces pairing-request arrival as an alert, not a
  passive notification.

## Open questions

- **Tray crate vs direct OS APIs**: `tray-icon` crate works but adds a
  dependency. Direct `objc2` / `windows-rs` gives us control but more code.
  Decide at V2 time.
- **LSUIElement on macOS**: toggling Dock icon visibility. Standard but
  requires launch-arg handling.
- **Connection history granularity**: store every connection? last N? Never?
  Storage + privacy tradeoff.
- **Popover implementation**: GPUI has anchored-popover primitives
  (we've used them in the diff-view context menu). Use the same pattern.
- **QR rendering in GPUI**: no built-in QR widget. Options:
  - Use the `qrcode` crate to produce a bitmap, render as an image element.
  - Render a grid of dark/light rects. Both easy; pick at implementation.

## Verification

- **Manual acceptance**:
  - Toggle remote control on/off; confirm state matches title-bar indicator.
  - Pair a device (via the real web client from Track 1); approve;
    verify it connects.
  - Reject a pairing; verify the web client sees the rejection.
  - Let a pairing request time out; verify same.
  - Rename a paired device; reload app; confirm persistence.
  - Revoke a paired device while it's connected; verify web client gets
    disconnected.
  - Close main window; verify agents keep running (V2: verify tray shows).
  - Enroll TOTP; pair using TOTP; verify.
- **Automated**:
  - UI tests for pairing dialog state machine (approval / rejection /
    timeout paths).
  - Integration test pairing via the real client crate, bypassing the
    GPUI layer.

## Out of this spec, into others

- Web-side pairing UX (QR scanner, passphrase, credential bootstrap) — Track 4.
- Connection/event audit logs — Track 5.
