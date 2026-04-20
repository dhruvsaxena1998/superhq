//! Title-bar button + popover for the remote-control feature.
//!
//! One small icon-only button that lives to the right of the sidebar
//! toggle. Shows a status dot (green = running, gray = off). Clicking
//! opens an anchored popover with the host's EndpointId, a copy button,
//! paired-device count, and the target URL to connect from.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use gpui::*;
use gpui::prelude::FluentBuilder as _;

use crate::db::Database;
use crate::ui::components::Toast;
use crate::ui::theme as t;

use super::{PairingStore, RemoteAccess};

/// SuperHQ logo overlaid in the center of the QR code. High error
/// correction (EcLevel::H) lets the code absorb up to ~30% occlusion,
/// so a ~20% center patch stays scannable.
static APP_ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../../../assets/app-icon-128.png").to_vec(),
    ))
});

/// Persistent state for the remote-control popover across re-renders.
#[derive(Clone, Default)]
pub struct RemotePopoverState {
    pub open: Rc<Cell<bool>>,
}

pub type ManageDevicesCallback = Arc<dyn Fn(&mut Window, &mut App) + 'static>;
pub type ToggleEnabledCallback = Arc<dyn Fn(bool, &mut App) + 'static>;

/// Render the title-bar button + optional popover.
///
/// Called from `AppView::render`.
pub fn render_titlebar_button<V: Render>(
    remote: &RemoteAccess,
    db: &Arc<Database>,
    state: &RemotePopoverState,
    toast: &Entity<Toast>,
    on_manage_devices: ManageDevicesCallback,
    on_toggle_enabled: ToggleEnabledCallback,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let running = remote.is_running();
    let endpoint_id = remote.endpoint_id();
    let paired_count = PairingStore::new(db.clone()).list().len();
    let is_open = state.open.get();

    // The button itself + the anchored popover (if open).
    div()
        .flex()
        .items_center()
        .relative()
        .child(render_button(running, state.open.clone(), cx))
        .when(is_open, |el| {
            el.child(
                deferred(
                    anchored()
                        .position(point(px(16.0), px(30.0)))
                        .anchor(Corner::TopLeft)
                        .snap_to_window_with_margin(px(16.0))
                        .child(render_popover(
                            running,
                            endpoint_id,
                            paired_count,
                            state.clone(),
                            toast.clone(),
                            on_manage_devices,
                            on_toggle_enabled,
                        )),
                )
                .with_priority(1),
            )
        })
}

fn render_button<V: Render>(
    running: bool,
    popover_open: Rc<Cell<bool>>,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let _ = cx;
    let dot_color = if running {
        rgb(0x3cc475) // green
    } else {
        t::text_ghost().into()
    };
    let toggle = popover_open.clone();
    div()
        .id("remote-control-btn")
        .p(px(5.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(|s: StyleRefinement| s.bg(t::bg_hover()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            toggle.set(!toggle.get());
            cx.stop_propagation();
        })
        .child(
            div()
                .relative()
                .child(
                    svg()
                        .path(SharedString::from("icons/network.svg"))
                        .size(px(14.0))
                        .text_color(t::text_secondary()),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(-1.0))
                        .right(px(-1.0))
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(dot_color),
                ),
        )
}

fn render_popover(
    running: bool,
    endpoint_id: Option<String>,
    paired_count: usize,
    state: RemotePopoverState,
    toast: Entity<Toast>,
    on_manage_devices: ManageDevicesCallback,
    on_toggle_enabled: ToggleEnabledCallback,
) -> impl IntoElement {
    let close = state.open.clone();
    let id_for_qr = endpoint_id.clone();
    // Backdrop + popover content.
    div()
        .child(
            // Backdrop: click outside closes.
            div()
                .id("remote-control-backdrop")
                .absolute()
                .top(px(-2000.0))
                .left(px(-2000.0))
                .w(px(8000.0))
                .h(px(8000.0))
                .occlude()
                .on_mouse_down(MouseButton::Left, {
                    let close = close.clone();
                    move |_, _, cx| {
                        close.set(false);
                        cx.stop_propagation();
                    }
                }),
        )
        .child(
            t::popover()
                .id("remote-control-popover")
                .w(px(340.0))
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t::text_secondary())
                        .child("Remote control"),
                )
                .child(render_status_row(running, on_toggle_enabled))
                .when(running, |el| match endpoint_id {
                    Some(id) => el.child(render_endpoint_id_box(id, toast.clone())),
                    None => el.child(
                        div()
                            .text_xs()
                            .text_color(t::text_ghost())
                            .child("starting..."),
                    ),
                })
                .when(running && id_for_qr.is_some(), |el| {
                    el.child(render_qr(&id_for_qr.unwrap()))
                })
                .child(render_connect_hint())
                .child(render_devices_row(paired_count, close, on_manage_devices)),
        )
}

fn render_status_row(
    running: bool,
    on_toggle_enabled: ToggleEnabledCallback,
) -> impl IntoElement {
    let toggle_label = if running { "Turn off" } else { "Turn on" };
    let target = !running;
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .text_xs()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(7.0))
                        .h(px(7.0))
                        .rounded_full()
                        .bg(if running {
                            rgb(0x3cc475)
                        } else {
                            t::text_ghost().into()
                        }),
                )
                .child(
                    div()
                        .text_color(t::text_secondary())
                        .child(if running { "Enabled" } else { "Stopped" }),
                ),
        )
        .child(
            div()
                .id("remote-control-toggle-btn")
                .px_2()
                .py_0p5()
                .rounded(px(4.0))
                .text_color(t::text_ghost())
                .cursor_pointer()
                .hover(|s: StyleRefinement| s.bg(t::bg_hover()).text_color(t::text_secondary()))
                .on_click(move |_, _window, cx| {
                    // Intentionally leaves the popover open so the user
                    // sees the status dot / label flip before dismissing
                    // it themselves.
                    on_toggle_enabled(target, cx);
                })
                .child(toggle_label),
        )
}

fn render_endpoint_id_box(id: String, toast: Entity<Toast>) -> impl IntoElement {
    let id_for_copy = id.clone();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(t::text_ghost())
                .child("Your host id:"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("endpoint-id-text")
                        .flex_grow()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(t::text_secondary())
                        .bg(t::bg_elevated())
                        .p_1p5()
                        .rounded(px(4.0))
                        .child(SharedString::from(id)),
                )
                .child(
                    div()
                        .id("endpoint-id-copy")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_2()
                        .py_1()
                        .rounded(px(4.0))
                        .bg(t::bg_elevated())
                        .text_xs()
                        .text_color(t::text_secondary())
                        .cursor_pointer()
                        .hover(|s: StyleRefinement| s.bg(t::bg_hover()))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                id_for_copy.clone(),
                            ));
                            toast.update(cx, |t, cx| t.show("Host id copied", cx));
                        })
                        .child(
                            svg()
                                .path(SharedString::from("icons/copy.svg"))
                                .size(px(12.0))
                                .text_color(t::text_secondary()),
                        )
                        .child("Copy"),
                ),
        )
}

/// Render a QR code for the given payload as a grid of tiny dark/light
/// rectangles, with the SuperHQ app icon overlaid in the center. High
/// error correction (EcLevel::H) lets the code survive the center patch.
fn render_qr(payload: &str) -> impl IntoElement {
    use qrcode::{EcLevel, QrCode};
    let code = match QrCode::with_error_correction_level(payload, EcLevel::H) {
        Ok(c) => c,
        Err(_) => {
            return div()
                .text_xs()
                .text_color(t::text_ghost())
                .child("QR encoding failed")
                .into_any_element();
        }
    };
    let width = code.width();
    let colors = code.to_colors();
    let cell_px = 6.0;
    let quiet_px = cell_px * 2.0;
    let qr_px = cell_px * width as f32;
    // Logo patch ~22% of the QR — well under the ~30% EcLevel::H budget.
    let logo_px = (qr_px * 0.22).round();

    // One row of cells is a horizontal flex; rows are stacked vertically.
    let mut grid = div().flex().flex_col();
    for row in 0..width {
        let mut row_el = div().flex().h(px(cell_px));
        for col in 0..width {
            let dark = colors[row * width + col] == qrcode::Color::Dark;
            let color: gpui::Rgba = if dark {
                rgb(0xeaeaea)
            } else {
                rgb(0x1e1e1e)
            };
            row_el = row_el.child(div().w(px(cell_px)).h(px(cell_px)).bg(color));
        }
        grid = grid.child(row_el);
    }

    // Stack the grid and a centered logo inside a fixed-size square so
    // the logo lines up exactly on the QR center regardless of the grid's
    // content-driven layout.
    let qr_stack = div()
        .relative()
        .w(px(qr_px))
        .h(px(qr_px))
        .child(grid)
        .child(
            div()
                .absolute()
                .top(px((qr_px - logo_px) / 2.0))
                .left(px((qr_px - logo_px) / 2.0))
                .w(px(logo_px))
                .h(px(logo_px))
                .p(px(4.0))
                .rounded(px(6.0))
                .bg(rgb(0xeaeaea))
                .child(
                    img(APP_ICON.clone())
                        .w(px(logo_px - 8.0))
                        .h(px(logo_px - 8.0)),
                ),
        );

    div()
        .flex()
        .justify_center()
        .py_2()
        .child(
            div()
                .p(px(quiet_px))
                .bg(rgb(0x1e1e1e))
                .rounded(px(6.0))
                .child(qr_stack),
        )
        .into_any_element()
}

fn render_connect_hint() -> impl IntoElement {
    div()
        .text_xs()
        .text_color(t::text_ghost())
        .child("Connect from a browser at remote.superhq.ai")
}

fn render_devices_row(
    count: usize,
    close: Rc<Cell<bool>>,
    on_manage_devices: ManageDevicesCallback,
) -> impl IntoElement {
    let label = match count {
        0 => "No paired devices".to_string(),
        1 => "1 paired device".to_string(),
        n => format!("{n} paired devices"),
    };
    div()
        .flex()
        .items_center()
        .justify_between()
        .pt_1p5()
        .mt_1()
        .border_t_1()
        .border_color(t::border())
        .text_xs()
        .child(div().text_color(t::text_ghost()).child(label))
        .child(
            div()
                .id("remote-control-manage-btn")
                .px_2()
                .py_0p5()
                .rounded(px(4.0))
                .text_color(t::text_ghost())
                .cursor_pointer()
                .hover(|s: StyleRefinement| s.bg(t::bg_hover()).text_color(t::text_secondary()))
                .on_click(move |_, window, cx| {
                    close.set(false);
                    on_manage_devices(window, cx);
                })
                .child("Manage →"),
        )
}
