# 12 — Split terminal/mod.rs rendering into modules

## Problem

After specs 09-11, terminal/mod.rs still owns all rendering code (~1200
lines of GPUI element construction). One file with the tab bar, setup
view, status bar, close confirmation, context menus, drag-and-drop —
hard to navigate.

## Proposed structure

```
src/ui/terminal/
  mod.rs          — TerminalPanel struct, Render impl (thin orchestrator)
  session.rs      — WorkspaceSession entity (spec 09)
  tab_bar.rs      — Tab bar rendering + drag-and-drop
  setup_view.rs   — Agent setup/install progress view
  status_bar.rs   — Bottom status bar (ports, etc.)
  pane.rs         — Pane entity holding TabItems (spec 10)
```

### mod.rs (~200 lines after split)

```rust
impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col()
            .child(self.render_tab_bar(window, cx))
            .child(self.render_content(window, cx))
            .child(self.render_status_bar(cx))
    }
}
```

Each `render_*` method lives in its own file and is implemented as a
method on `TerminalPanel` (via `impl TerminalPanel` blocks in separate
files — Rust allows this).

### tab_bar.rs (~300 lines)

- Tab rendering with icons, badges, close buttons
- Drag-and-drop reordering (DraggedTab, DraggedTabView)
- Agent menu (+ button dropdown)
- Close confirmation bar

### setup_view.rs (~200 lines)

- `render_setup_view()` — the spinning icon + step list
- Setup step rendering

### status_bar.rs (~100 lines)

- Port count display
- Port dialog trigger

### pane.rs (~150 lines)

- `Pane` entity holding `Vec<Box<dyn TabItemHandle>>`
- Active item rendering
- Tab switching

## Implementation order

1. Spec 09 first (extract WorkspaceSession)
2. Spec 10 (Panel/Item traits, Pane, Dock)
3. Spec 11 (SandboxService)
4. This spec: move rendering into separate files
5. Each step compiles independently

## File moves (approximate)

| Current location | Lines | New file |
|---|---|---|
| DraggedTab, DraggedTabView, tab rendering | 1747-2013 | tab_bar.rs |
| render_setup_view, SetupStep | 1834-2012 | setup_view.rs |
| Status bar section in render() | 2640-2700 | status_bar.rs |
| Pane concept (implicit in session.tabs) | new | pane.rs |
| TerminalPanel struct + Render + remaining | rest | mod.rs |
