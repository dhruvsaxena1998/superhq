//! Settings tab: Remote control — enable toggle + paired devices list.

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use super::card::{section_header, settings_card, settings_row};
use super::{RemoteControlToggle, SettingsPanel};
use crate::ui::components::switch::{Switch, SwitchEvent};
use crate::ui::remote::{PairedDevice, PairingStore};
use crate::ui::theme as t;

impl SettingsPanel {
    /// Switch bound to the `remote_control_enabled` DB setting. The
    /// toggle callback runs on `AppView` which persists + starts/stops
    /// the in-memory server; we intentionally don't write to the DB
    /// here so there's a single source of truth for that transition.
    pub(super) fn init_remote_control_switch(
        value: bool,
        on_toggled: RemoteControlToggle,
        cx: &mut Context<Self>,
    ) -> Entity<Switch> {
        let state = cx.new(|cx| Switch::new(value, cx));
        cx.subscribe(&state, move |_this, _, event: &SwitchEvent, cx| {
            let SwitchEvent::Change(value) = *event;
            on_toggled(value, cx);
        })
        .detach();
        state
    }

    pub(super) fn render_remote_control_tab(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let devices = PairingStore::new(self.db.clone()).list();
        let has_devices = !devices.is_empty();

        let toggle_card = settings_card(vec![settings_row(
            "Enable remote control",
            "When off, the host server is stopped and no devices can connect.",
            self.remote_control_switch.clone(),
        )
        .into_any_element()]);

        let host_id_block = self.render_host_id_block(cx);

        let devices_body: AnyElement = if has_devices {
            let rows: Vec<AnyElement> = devices
                .into_iter()
                .map(|d| Self::render_device_row(d, cx).into_any_element())
                .collect();
            settings_card(rows).into_any_element()
        } else {
            Self::render_empty_state().into_any_element()
        };

        let audit_path = self.audit_log_path.clone();
        let audit_link = div()
            .pt(px(14.0))
            .flex()
            .flex_col()
            .gap_1()
            .text_xs()
            .child(
                div()
                    .text_color(t::text_ghost())
                    .child("Audit log"),
            )
            .child(
                div()
                    .id("audit-log-path")
                    .font_family("monospace")
                    .text_color(t::text_secondary())
                    .cursor_pointer()
                    .hover(|s: StyleRefinement| s.text_color(t::text_tertiary()))
                    .on_click(move |_, _, _cx| {
                        let _ = open::that(audit_path.clone());
                    })
                    .child(SharedString::from(self.audit_log_path.display().to_string())),
            );

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(section_header("Remote control"))
            .child(
                div()
                    .pb(px(12.0))
                    .text_xs()
                    .text_color(t::text_ghost())
                    .child(
                        "Devices paired with this host can connect from a browser at \
                         remote.superhq.ai. Revoke a device to sign it out immediately.",
                    ),
            )
            .child(toggle_card)
            .child(div().h(px(16.0)))
            .child(host_id_block)
            .child(div().h(px(16.0)))
            .child(
                div()
                    .pb(px(6.0))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t::text_secondary())
                    .child("Paired devices"),
            )
            .child(devices_body)
            .child(audit_link)
    }

    fn render_host_id_block(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let toast = self.toast.clone();
        let id_str = self
            .host_id
            .clone()
            .unwrap_or_else(|| "—".to_string());
        let id_for_copy = id_str.clone();
        let has_id = self.host_id.is_some();
        let confirming = self.rotate_confirming;
        let rotate_cb = self.on_rotate_host_id.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t::text_secondary())
                    .child("Your host id"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(t::text_ghost())
                    .child(
                        "Paste or scan this into a remote device to connect. \
                         The id stays the same across restarts.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("host-id-text")
                            .flex_grow()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .font_family("monospace")
                            .text_color(t::text_secondary())
                            .bg(t::bg_elevated())
                            .border_1()
                            .border_color(t::border_subtle())
                            .p_2()
                            .rounded(px(4.0))
                            .child(SharedString::from(id_str.clone())),
                    )
                    .when(has_id, |el| {
                        el.child(
                            div()
                                .id("host-id-copy")
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .px_2p5()
                                .py_1p5()
                                .rounded(px(4.0))
                                .bg(t::bg_elevated())
                                .border_1()
                                .border_color(t::border_subtle())
                                .text_xs()
                                .text_color(t::text_secondary())
                                .cursor_pointer()
                                .hover(|s: StyleRefinement| s.bg(t::bg_hover()))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(
                                        ClipboardItem::new_string(id_for_copy.clone()),
                                    );
                                    toast.update(cx, |t, cx| {
                                        t.show("Host id copied", cx)
                                    });
                                })
                                .child(
                                    svg()
                                        .path(SharedString::from("icons/copy.svg"))
                                        .size(px(12.0))
                                        .text_color(t::text_secondary()),
                                )
                                .child("Copy"),
                        )
                    }),
            )
            .when(has_id, |el| {
                el.child(self.render_rotate_row(confirming, rotate_cb, cx))
            })
    }

    /// Row under the host id with either:
    ///   - a muted "Rotate host id" link (idle state), or
    ///   - an inline confirmation warning + [Cancel] [Rotate] when
    ///     `rotate_confirming` is true.
    /// Rotation is destructive — it unpairs every device — so we never
    /// fire it on a single click.
    fn render_rotate_row(
        &self,
        confirming: bool,
        rotate_cb: super::RotateHostIdCallback,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if !confirming {
            return div()
                .pt(px(4.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .id("host-id-rotate")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_3()
                        .py_1p5()
                        .rounded(px(4.0))
                        .bg(t::bg_surface())
                        .border_1()
                        .border_color(rgba(0xe5484d33))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xe5484d))
                        .cursor_pointer()
                        .hover(|s: StyleRefinement| s.bg(rgba(0xe5484d1a)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.rotate_confirming = true;
                            cx.notify();
                        }))
                        .child("Rotate host id"),
                )
                .into_any_element();
        }
        div()
            .pt(px(6.0))
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(t::border_subtle())
            .bg(t::bg_elevated())
            .p_3()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t::text_secondary())
                    .child("Rotate host id?"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(t::text_ghost())
                    .child(
                        "This generates a brand-new id and unpairs every \
                         device. You'll need to re-scan the QR code on \
                         each device to reconnect.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .id("host-id-rotate-cancel")
                            .px_3()
                            .py_1p5()
                            .rounded(px(4.0))
                            .bg(t::bg_surface())
                            .border_1()
                            .border_color(t::border_subtle())
                            .text_xs()
                            .text_color(t::text_secondary())
                            .cursor_pointer()
                            .hover(|s: StyleRefinement| s.bg(t::bg_hover()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rotate_confirming = false;
                                cx.notify();
                            }))
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .id("host-id-rotate-confirm")
                            .px_3()
                            .py_1p5()
                            .rounded(px(4.0))
                            .bg(rgb(0xe5484d))
                            .border_1()
                            .border_color(rgb(0xe5484d))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .cursor_pointer()
                            .hover(|s: StyleRefinement| s.bg(rgb(0xcc3c41)))
                            .on_click(move |_, _, cx| (rotate_cb)(cx))
                            .child("Rotate"),
                    ),
            )
            .into_any_element()
    }

    fn render_device_row(
        device: PairedDevice,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let device_id = device.device_id.clone();
        let short_id = short_id(&device.device_id);
        let paired_ago = relative_time(device.created_at);

        div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .min_w_0()
                    .flex_grow()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t::text_secondary())
                            .child(device.device_label.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(t::text_ghost())
                            .child(div().font_family("monospace").child(short_id))
                            .child(div().child("·"))
                            .child(div().child(paired_ago)),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("revoke-{}", device.device_id)))
                    .px_2p5()
                    .py_1()
                    .rounded(px(4.0))
                    .text_xs()
                    .text_color(rgb(0xe5484d))
                    .bg(t::bg_surface())
                    .border_1()
                    .border_color(t::border_subtle())
                    .cursor_pointer()
                    .hover(|s: StyleRefinement| s.bg(t::bg_hover()))
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        let removed = PairingStore::new(this.db.clone()).remove(&device_id);
                        if removed {
                            this.toast.update(cx, |tt, cx| tt.show("Device revoked", cx));
                        } else {
                            this.toast
                                .update(cx, |tt, cx| tt.show("Failed to revoke device", cx));
                        }
                        cx.notify();
                    }))
                    .child("Revoke"),
            )
    }

    fn render_empty_state() -> impl IntoElement {
        div()
            .rounded(px(8.0))
            .border_1()
            .border_color(t::border_subtle())
            .bg(t::bg_elevated())
            .px_4()
            .py_6()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t::text_secondary())
                    .child("No devices paired"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(t::text_ghost())
                    .child("Scan the QR code in the title-bar popover to pair a device."),
            )
    }
}

fn short_id(id: &str) -> String {
    if id.len() <= 16 {
        return id.to_string();
    }
    format!("{}…{}", &id[..8], &id[id.len() - 4..])
}

fn relative_time(created_at_secs: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(created_at_secs);
    let delta = now.saturating_sub(created_at_secs);
    match delta {
        0..=59 => "paired just now".into(),
        60..=3599 => format!("paired {} min ago", delta / 60),
        3600..=86399 => format!("paired {} hr ago", delta / 3600),
        _ => format!("paired {} days ago", delta / 86400),
    }
}
