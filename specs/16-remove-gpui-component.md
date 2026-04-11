# 16 — Remove gpui-component dependency (TODO)

## Problem

gpui-component controls its own theme (`Theme` global) with colors that
override our theme.rs. We can't fully control the UI appearance without
removing this dependency. It also adds significant binary size and
complexity for features we don't use.

## Current usage (8 features)

### 1. Resizable panels (`h_resizable`, `resizable_panel`)
- Used in: `main.rs` (outer layout: sidebar | terminal | review)
- Complexity: moderate (drag handles, size persistence, resize events)
- Plan: vendor the resizable module (~400 lines). It's self-contained.

### 2. Root / Theme / ThemeMode
- Used in: `main.rs` (init, dark mode, color overrides)
- Plan: remove entirely. We own our theme via theme.rs. The `Root`
  wrapper just sets up theme context — we replace with our own init.

### 3. SyntaxHighlighter + HighlightTheme
- Used in: `diff_view.rs` (tree-sitter syntax highlighting for diffs)
- Complexity: high (~2000 lines, tree-sitter integration)
- Plan: vendor the highlighter module. It wraps tree-sitter and
  gpui-component bundles the grammars via the `tree-sitter-languages`
  feature.

### 4. ScrollableElement (scroll extension trait)
- Used in: `changes_tab.rs` (`overflow_y_scrollbar()`)
- Plan: check if GPUI has this natively. If not, vendor the scroll
  module (~300 lines).

### 5. Input / InputState (old text input)
- Used in: `command_palette.rs` only (search input)
- Plan: replace with our `TextInput` component.

### 6. ActiveTheme trait
- Used in: `diff_view.rs` (access `cx.theme()` for font info)
- Plan: replace with direct font config from our theme.

### 7. v_flex / Sizable / WindowExt
- Used in: `command_palette.rs`
- Plan: `v_flex` is just `div().flex().flex_col()`. `Sizable` is
  `.small()` on inputs (replaced by our TextInput). `WindowExt`
  for popup positioning.

### 8. ContextMenuExt (terminal context menu)
- Used in: `gpui-terminal/view.rs`
- Plan: replace with our `ContextMenu` component.

### 9. gpui-component-assets
- Used in: `assets.rs` (fallback icon/font loading)
- Plan: bundle our own assets. Check which ones we actually use.

## Implementation phases

### Phase 1: Replace easy ones (no vendoring)
1. Command palette: replace `Input`/`InputState` with `TextInput`
2. Remove `v_flex`, `Sizable`, `WindowExt` — inline replacements
3. Remove `ActiveTheme` — use our own font config
4. Remove `Root`/`Theme`/`ThemeMode` — our own init
5. Remove theme overrides from `main.rs`

### Phase 2: Vendor resizable panels
1. Copy `gpui-component/src/resizable/` into our codebase
2. Strip dependencies on gpui-component internals
3. Wire into main.rs layout

### Phase 3: Vendor syntax highlighter
1. Copy `gpui-component/src/highlighter/` 
2. Keep `tree-sitter-languages` as a direct dependency
3. Wire into diff_view.rs

### Phase 4: Vendor scroll
1. Copy scroll module or implement `overflow_y_scrollbar` ourselves
2. Wire into changes_tab.rs

### Phase 5: Replace terminal context menu
1. Wire our `ContextMenu` component into gpui-terminal
2. Remove `ContextMenuExt` import

### Phase 6: Remove dependency
1. Remove `gpui-component` and `gpui-component-assets` from Cargo.toml
2. Bundle needed assets directly
3. Verify everything compiles and runs

## Risk

The syntax highlighter and resizable panels are the hardest — they have
deep internal dependencies. If vendoring gets too complex, we can keep
gpui-component as an optional dependency just for those two features
and strip everything else.
