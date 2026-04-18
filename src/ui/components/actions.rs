use gpui::{actions, App, KeyBinding};

actions!(superhq, [Cancel, Confirm, SelectUp, SelectDown]);

// Key name constants — GPUI doesn't export these.
pub const KEY_ESCAPE: &str = "escape";
pub const KEY_SPACE: &str = "space";
pub const KEY_TAB: &str = "tab";
pub const KEY_ENTER: &str = "enter";
pub const KEY_UP: &str = "up";
pub const KEY_DOWN: &str = "down";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        // Context menu
        KeyBinding::new(KEY_ESCAPE, Cancel, Some("ContextMenu")),
        KeyBinding::new(KEY_UP, SelectUp, Some("ContextMenu")),
        KeyBinding::new(KEY_DOWN, SelectDown, Some("ContextMenu")),
        KeyBinding::new(KEY_ENTER, Confirm, Some("ContextMenu")),
        // Select / dropdown
        KeyBinding::new(KEY_ESCAPE, Cancel, Some("Select")),
        KeyBinding::new(KEY_UP, SelectUp, Some("Select")),
        KeyBinding::new(KEY_DOWN, SelectDown, Some("Select")),
        KeyBinding::new(KEY_ENTER, Confirm, Some("Select")),
        // Text input
        KeyBinding::new(KEY_ESCAPE, Cancel, Some("TextInput")),
        KeyBinding::new(KEY_ENTER, Confirm, Some("TextInput")),
        // Button
        KeyBinding::new(KEY_ENTER, Confirm, Some("Button")),
        KeyBinding::new(KEY_SPACE, Confirm, Some("Button")),
        // Dialog
        KeyBinding::new(KEY_ESCAPE, Cancel, Some("Dialog")),
        KeyBinding::new(KEY_ENTER, Confirm, Some("Dialog")),
    ]);
}
