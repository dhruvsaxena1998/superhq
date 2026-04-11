use gpui::*;
use gpui::prelude::FluentBuilder as _;

use super::session::TabKind;
use crate::ui::theme as t;

/// Drag payload for tab reordering.
#[derive(Clone)]
pub struct DraggedTab {
    pub ws_id: i64,
    pub tab_ix: usize,
    pub label: SharedString,
    pub icon_path: Option<SharedString>,
    pub color: Option<Rgba>,
}

/// Ghost view rendered while dragging a tab.
pub struct DraggedTabView {
    pub tab: DraggedTab,
}

impl Render for DraggedTabView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let tab = &self.tab;
        let mut el = div()
            .px_2()
            .py_1()
            .rounded(px(5.0))
            .bg(t::bg_surface())
            .text_xs()
            .text_color(t::text_secondary())
            .flex()
            .items_center()
            .gap_1();

        if let Some(ref path) = tab.icon_path {
            let icon_color = tab.color.unwrap_or(t::text_dim());
            el = el.child(
                svg().path(path.clone()).size(px(14.0)).text_color(icon_color),
            );
        }

        el.child(tab.label.clone())
    }
}

pub struct AgentTabInfo {
    pub tab_id: u64,
    pub name: String,
}

impl super::TerminalPanel {
    pub fn render_tab_icon(&self, icon_path: &Option<SharedString>, color: Option<Rgba>, is_active: bool) -> impl IntoElement {
        let icon_color = color.unwrap_or(if is_active { t::text_secondary() } else { t::text_dim() });
        div()
            .w(px(14.0))
            .h(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .children(icon_path.as_ref().map(|path| {
                svg()
                    .path(path.clone())
                    .size(px(12.0))
                    .text_color(icon_color)
            }))
    }

    pub fn render_menu_item(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        icon_path: Option<&str>,
        color: Option<Rgba>,
        enabled: bool,
        focused: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let icon_color = if enabled { color.unwrap_or(t::text_dim()) } else { t::text_dim() };
        let label_color = if enabled { t::text_secondary() } else { t::text_ghost() };
        let label = label.to_string();
        let icon = icon_path.map(|p| SharedString::from(p.to_string()));

        div()
            .id(id.into())
            .px_2p5()
            .py(px(5.0))
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.0))
            .when(!enabled, |s| s.opacity(0.4))
            .when(enabled && !focused, |s| s.cursor_pointer().hover(|s| s.bg(t::bg_hover())))
            .when(enabled && focused, |s| s.bg(t::bg_hover()).cursor_pointer())
            .when(enabled, |s| {
                s.on_click(cx.listener(move |this, _, _, cx| {
                    on_click(this, cx);
                    this.show_agent_menu = false;
                    cx.notify();
                }))
            })
            .children(icon.map(|path| {
                svg()
                    .path(path)
                    .size(px(14.0))
                    .text_color(icon_color)
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(label_color)
                    .child(label),
            )
    }

    pub fn active_agent_tabs(&self, cx: &App) -> Vec<AgentTabInfo> {
        let ws_id = match self.active_workspace_id {
            Some(id) => id,
            None => return vec![],
        };
        let session = match self.sessions.get(&ws_id) {
            Some(s) => s,
            None => return vec![],
        };
        let s = session.read(cx);
        s.tabs.iter()
            .filter_map(|tab| match &tab.kind {
                TabKind::Agent { sandbox: Some(_), .. } => Some(AgentTabInfo {
                    tab_id: tab.tab_id,
                    name: tab.label.to_string(),
                }),
                _ => None,
            })
            .collect()
    }
}
