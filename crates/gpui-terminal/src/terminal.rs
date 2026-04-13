//! Terminal state management with deferred event processing.
//!
//! Inspired by Zed editor's terminal architecture: events are queued via
//! `InternalEvent` and batch-processed in `sync()`, which produces a
//! `TerminalContent` snapshot for rendering without holding the lock.

use crate::event::GpuiEventProxy;
use alacritty_terminal::grid::{Dimensions, Scroll as AlacScroll};
use alacritty_terminal::index::Point as AlacPoint;
use alacritty_terminal::selection::Selection;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape as AlacCursorShape, Processor};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

/// Simple dimensions implementation for terminal initialization.
struct TermDimensions {
    columns: usize,
    screen_lines: usize,
}

impl TermDimensions {
    fn new(columns: usize, screen_lines: usize) -> Self {
        Self {
            columns,
            screen_lines,
        }
    }
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.columns
    }
    fn last_column(&self) -> alacritty_terminal::index::Column {
        alacritty_terminal::index::Column(self.columns.saturating_sub(1))
    }
}

// ---------------------------------------------------------------------------
// Internal event queue (Zed pattern)
// ---------------------------------------------------------------------------

/// Deferred events queued by input handlers, processed in batch by `sync()`.
#[derive(Debug)]
pub(crate) enum InternalEvent {
    /// Scroll by delta lines (positive = up, negative = down).
    Scroll(AlacScroll),
    /// Set or clear the selection.
    SetSelection(Option<(Selection, AlacPoint)>),
    /// Update an in-progress selection to a new pixel position.
    UpdateSelection(AlacPoint),
}

// ---------------------------------------------------------------------------
// Cursor shape
// ---------------------------------------------------------------------------

/// Cursor shape for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
    Hollow,
}

/// Renderable cursor info.
#[derive(Debug, Clone)]
pub struct RenderableCursor {
    pub point: AlacPoint,
    pub shape: CursorShape,
    pub cursor_char: char,
}

// ---------------------------------------------------------------------------
// Terminal content snapshot (for rendering without holding the lock)
// ---------------------------------------------------------------------------

/// Snapshot of terminal state for rendering.
/// Produced by `sync()`, consumed by the render pipeline.
/// Note: cells are read directly from the Term lock during paint,
/// not from this snapshot (avoids O(cols*rows) clone per frame).
pub struct TerminalContent {
    /// Current terminal mode flags.
    pub mode: TermMode,
    /// Current scroll offset (0 = bottom, >0 = scrolled up).
    pub display_offset: usize,
    /// Currently selected text (if any).
    pub selection_text: Option<String>,
    /// Selection range for highlight rendering.
    pub selection_range: Option<SelectionRange>,
    /// Cursor info for rendering.
    pub cursor: RenderableCursor,
}

/// A selection range in grid coordinates.
#[derive(Debug, Clone)]
pub struct SelectionRange {
    pub start: AlacPoint,
    pub end: AlacPoint,
}

impl Default for TerminalContent {
    fn default() -> Self {
        Self {
            mode: TermMode::empty(),
            display_offset: 0,
            selection_text: None,
            selection_range: None,
            cursor: RenderableCursor {
                point: AlacPoint::new(alacritty_terminal::index::Line(0), alacritty_terminal::index::Column(0)),
                shape: CursorShape::Block,
                cursor_char: ' ',
            },
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalState: the core state wrapper
// ---------------------------------------------------------------------------

/// Thread-safe terminal state with deferred event processing.
pub struct TerminalState {
    /// The underlying alacritty terminal emulator.
    term: Arc<Mutex<Term<GpuiEventProxy>>>,
    /// VTE parser for converting byte streams into terminal actions.
    parser: Processor,
    /// Number of columns.
    cols: usize,
    /// Number of rows.
    rows: usize,
    /// Deferred event queue (processed in sync()).
    events: VecDeque<InternalEvent>,
    /// Cached content snapshot for rendering.
    pub(crate) content: TerminalContent,
    /// Whether we're in a selection drag.
    pub(crate) selection_phase: SelectionPhase,
    /// Scroll pixel accumulator for smooth scrolling.
    pub(crate) scroll_px: f32,
}

/// Selection state machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SelectionPhase {
    None,
    Selecting,
    Selected,
}

impl TerminalState {
    /// Create a new terminal state with the given dimensions.
    pub fn new(cols: usize, rows: usize, scrollback: usize, event_proxy: GpuiEventProxy) -> Self {
        let mut config = Config::default();
        config.scrolling_history = scrollback;

        let dimensions = TermDimensions::new(cols, rows);
        let term = Term::new(config, &dimensions, event_proxy);
        let parser = Processor::new();

        Self {
            term: Arc::new(Mutex::new(term)),
            parser,
            cols,
            rows,
            events: VecDeque::new(),
            content: TerminalContent::default(),
            selection_phase: SelectionPhase::None,
            scroll_px: 0.0,
        }
    }

    /// Process incoming bytes from the PTY through the VTE parser.
    ///
    /// Normalizes readline's bracketed paste redisplay pattern:
    /// `ESC[27m \r ESC[7m` → `ESC[27m \r \n ESC[7m`
    /// Readline uses inverse video + bare CR (no LF) for multi-line paste
    /// redisplay, causing all lines to overlap on the same row. Inserting
    /// a LF after the CR makes each line advance properly.
    pub fn process_bytes(&mut self, bytes: &[u8]) {
        // Pattern: 1b 5b 32 37 6d 0d 1b 5b 37 6d
        //          ESC [  2  7  m  CR ESC [  7  m
        const PATTERN: &[u8] = b"\x1b[27m\r\x1b[7m";
        const REPLACEMENT: &[u8] = b"\x1b[27m\r\n\x1b[7m";

        if bytes.len() < PATTERN.len() || !bytes.windows(PATTERN.len()).any(|w| w == PATTERN) {
            // Fast path: no pattern found, process directly
            let mut term = self.term.lock();
            self.parser.advance(&mut *term, bytes);
            return;
        }

        // Replace all occurrences of the pattern
        let mut normalized = Vec::with_capacity(bytes.len() + 64);
        let mut i = 0;
        while i < bytes.len() {
            if i + PATTERN.len() <= bytes.len() && &bytes[i..i + PATTERN.len()] == PATTERN {
                normalized.extend_from_slice(REPLACEMENT);
                i += PATTERN.len();
            } else {
                normalized.push(bytes[i]);
                i += 1;
            }
        }

        let mut term = self.term.lock();
        self.parser.advance(&mut *term, &normalized);
    }

    /// Resize the terminal to new dimensions.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        let mut term = self.term.lock();
        let dimensions = TermDimensions::new(cols, rows);
        term.resize(dimensions);
    }

    /// Get the current terminal mode.
    pub fn mode(&self) -> TermMode {
        let term = self.term.lock();
        *term.mode()
    }

    /// Queue an internal event for deferred processing.
    pub(crate) fn push_event(&mut self, event: InternalEvent) {
        self.events.push_back(event);
    }

    /// Check if the terminal is in mouse reporting mode.
    pub fn mouse_mode(&self, shift: bool) -> bool {
        self.content.mode.intersects(
            TermMode::MOUSE_REPORT_CLICK
                | TermMode::MOUSE_MOTION
                | TermMode::MOUSE_DRAG,
        ) && !shift
    }

    /// Check if on alternate screen (vim, less, etc.).
    pub fn alt_screen(&self) -> bool {
        self.content.mode.contains(TermMode::ALT_SCREEN)
    }


    /// Synchronize: drain the event queue, process everything, rebuild the content snapshot.
    /// Call this once per frame before rendering.
    pub fn sync(&mut self) {
        let mut term = self.term.lock();

        // Process all queued events
        while let Some(event) = self.events.pop_front() {
            match event {
                InternalEvent::Scroll(scroll) => {
                    term.scroll_display(scroll);
                }
                InternalEvent::SetSelection(sel) => {
                    if let Some((selection, _point)) = sel {
                        term.selection = Some(selection);
                    } else {
                        term.selection = None;
                    }
                }
                InternalEvent::UpdateSelection(point) => {
                    if let Some(ref mut selection) = term.selection {
                        selection.update(point, alacritty_terminal::index::Side::Left);
                    }
                }
            }
        }

        // Build content snapshot
        self.content = Self::make_content(&term);
    }

    /// Build a TerminalContent snapshot from the current term state.
    fn make_content(term: &Term<GpuiEventProxy>) -> TerminalContent {
        let grid = term.grid();
        let display_offset = grid.display_offset();
        let mode = *term.mode();

        // Selection: compute range once, derive both text and range from it
        let sel_range = term.selection.as_ref().and_then(|sel| sel.to_range(term));

        let selection_text = sel_range.as_ref().map(|range| {
            term.bounds_to_string(range.start, range.end)
        });

        let selection_range = sel_range.map(|range| {
            SelectionRange { start: range.start, end: range.end }
        });

        // Cursor
        let cursor = grid.cursor.point;
        let cursor_cell = &grid[cursor];
        let cursor_char = cursor_cell.c;
        let cursor_style = term.cursor_style();

        let shape = match cursor_style.shape {
            AlacCursorShape::Block => CursorShape::Block,
            AlacCursorShape::Underline => CursorShape::Underline,
            AlacCursorShape::Beam => CursorShape::Bar,
            AlacCursorShape::HollowBlock => CursorShape::Hollow,
            AlacCursorShape::Hidden => CursorShape::Block, // hidden handled in render
        };

        TerminalContent {
            mode,
            display_offset,
            selection_text,
            selection_range,
            cursor: RenderableCursor {
                point: cursor,
                shape,
                cursor_char,
            },
        }
    }

    /// Execute a function with read access to the terminal.
    pub fn with_term<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Term<GpuiEventProxy>) -> R,
    {
        let term = self.term.lock();
        f(&term)
    }

    /// Execute a function with mutable access to the terminal.
    pub fn with_term_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Term<GpuiEventProxy>) -> R,
    {
        let mut term = self.term.lock();
        f(&mut term)
    }

    /// Get the number of columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Get the number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Get a cloned Arc reference to the underlying terminal.
    pub fn term_arc(&self) -> Arc<Mutex<Term<GpuiEventProxy>>> {
        Arc::clone(&self.term)
    }
}
