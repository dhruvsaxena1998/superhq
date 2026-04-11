# 10 — Panel and Item traits

## Problem

All panel types (review, terminal, settings) are hardcoded in the main
layout. Adding a new panel means editing main.rs layout code. Tab content
types (agent, shell) are mixed into TerminalPanel via TabKind enum with
different rendering branches in one giant match.

## Zed's approach

- `Panel` trait: persistent_name, position, icon, size, toggle_action
- `Item` trait: tab_content, is_dirty, save, navigate
- `PanelHandle` / `ItemHandle`: type-erased wrappers for heterogeneous storage
- `Dock` holds `Vec<PanelEntry>`, renders whichever panel is active
- `Pane` holds `Vec<Box<dyn ItemHandle>>`, renders active item's tab bar + content

## Proposed traits for SuperHQ

### Panel trait (simplified from Zed)

```rust
pub trait Panel: Render + EventEmitter<PanelEvent> {
    fn name(&self) -> &'static str;
    fn icon(&self) -> Option<&'static str>;   // SVG path
    fn position(&self) -> PanelPosition;       // Right (review panel)
}

pub enum PanelPosition { Right }

pub enum PanelEvent {
    Activate,
    Deactivate,
}
```

Currently only one panel (Review). But this trait makes it trivial to add
panels (file tree, git log, search) without touching layout code.

### Item trait (for tab content)

```rust
pub trait TabItem: Render + EventEmitter<TabItemEvent> {
    fn tab_label(&self) -> SharedString;
    fn tab_icon(&self) -> Option<SharedString>;
    fn tab_color(&self) -> Option<Rgba>;
    fn can_close(&self) -> bool { true }
    fn is_dirty(&self) -> bool { false }
}

pub enum TabItemEvent {
    UpdateLabel(SharedString),
    Close,
    Ready,       // agent finished booting
}
```

### What this enables

- `AgentTab` and `ShellTab` become separate entities implementing `TabItem`
- `TerminalPanel`'s `Pane` holds `Vec<Box<dyn TabItemHandle>>` instead of
  the current `Vec<TerminalTab>` with `TabKind` enum
- Adding a new tab type (e.g., LogViewer, FileEditor) means implementing
  `TabItem` on a new struct, no changes to TerminalPanel

## Implementation

1. Create `src/ui/traits.rs` with `Panel` and `TabItem` traits
2. Make SidePanel implement Panel
3. Split AgentTab / ShellTab from TerminalTab into separate entities
4. Each implements TabItem
5. TerminalPanel renders tabs generically via the trait

## Future: Docks and Pane Splitting

Design the traits with these in mind from day one:

### Docks

```rust
pub enum PanelPosition { Left, Right, Bottom }

pub struct Dock {
    position: PanelPosition,
    panels: Vec<Box<dyn PanelHandle>>,
    active_panel: Option<usize>,
    size: Pixels,
    visible: bool,
}
```

Even though we only have Right dock now, Panel trait includes `position()`
so panels can be moved between docks later. The main layout should use
`Dock` entities instead of hardcoding panel placement.

### Pane Splitting

```rust
pub enum PaneLayout {
    Single(Entity<Pane>),
    Split {
        axis: Axis,
        first: Box<PaneLayout>,
        second: Box<PaneLayout>,
        ratio: f32,
    },
}

pub struct Pane {
    items: Vec<Box<dyn TabItemHandle>>,
    active_item: usize,
    // ...
}
```

A `Pane` holds tabs. `PaneLayout` is a recursive tree that allows
horizontal/vertical splits. The current single-pane setup is just
`PaneLayout::Single(pane)`. Splitting creates a new pane in the tree.

### TabItemHandle (type-erased)

```rust
pub trait TabItemHandle: 'static {
    fn tab_label(&self, cx: &App) -> SharedString;
    fn tab_icon(&self, cx: &App) -> Option<SharedString>;
    fn tab_color(&self, cx: &App) -> Option<Rgba>;
    fn can_close(&self, cx: &App) -> bool;
    fn to_any_element(&self, window: &mut Window, cx: &mut App) -> AnyElement;
    fn entity_id(&self) -> EntityId;
}

// Blanket impl for Entity<T: TabItem>
impl<T: TabItem> TabItemHandle for Entity<T> { ... }
```

This allows `Pane` to hold heterogeneous tab types without generics.

### Implementation note

Implement Dock + PaneLayout as data structures now (specs 09-10),
even if the UI only uses single-pane + right-dock. The structures
are cheap and making the traits aware of them from the start avoids
breaking changes later.
