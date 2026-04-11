use gpui::*;

use super::session::{SetupStep, StepStatus};
use crate::ui::animation;
use crate::ui::theme as t;

impl super::TerminalPanel {
    pub fn render_setup_view(
        &self,
        steps: &[SetupStep],
        error: &Option<String>,
        agent_color: Option<Rgba>,
        icon_path: &Option<SharedString>,
    ) -> impl IntoElement {
        let accent = agent_color.unwrap_or(t::text_secondary());

        let mut step_list = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .w(px(280.0));

        for (i, step) in steps.iter().enumerate() {
            let is_active = step.status == StepStatus::Active;

            let (indicator, indicator_color, label_color) = match step.status {
                StepStatus::Done => (
                    "\u{2713}",
                    t::status_green_dim(),
                    t::text_ghost(),
                ),
                StepStatus::Active => (
                    "\u{25CF}",
                    accent,
                    t::text_secondary(),
                ),
                StepStatus::Pending => (
                    "\u{25CB}",
                    t::text_invisible(),
                    t::text_invisible(),
                ),
                StepStatus::Failed => (
                    "\u{2717}",
                    t::error_text(),
                    t::error_text(),
                ),
            };

            let indicator_el = div()
                .w(px(14.0))
                .text_xs()
                .text_color(indicator_color)
                .flex()
                .justify_center()
                .child(indicator);

            let step_row = if is_active {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py(px(2.0))
                    .child(
                        indicator_el
                            .with_animation(
                                SharedString::from(format!("step-pulse-{i}")),
                                animation::breathing(2.0),
                                |el, t| el.opacity(t),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(label_color)
                            .child(step.label.clone()),
                    )
            } else {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py(px(2.0))
                    .child(indicator_el)
                    .child(
                        div()
                            .text_xs()
                            .text_color(label_color)
                            .child(step.label.clone()),
                    )
            };

            step_list = step_list.child(step_row);
        }

        let icon_el = icon_path.as_ref().map(|path| {
            div()
                .w(px(32.0))
                .h(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(path.clone())
                        .size(px(28.0))
                        .text_color(accent),
                )
        });

        let mut view = div()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .children(icon_el)
                    .child(step_list),
            );

        if let Some(err) = error {
            let (msg, log_path) = if let Some(idx) = err.find("logs saved to ") {
                let path = err[idx + 14..].trim().to_string();
                let msg = err[..idx].trim_end_matches(" \u{2014} ").to_string();
                (msg, Some(path))
            } else {
                (err.clone(), None)
            };

            let mut error_bar = div()
                .max_w(px(500.0))
                .px_3()
                .py_2()
                .rounded(px(6.0))
                .bg(t::error_bg())
                .border_1()
                .border_color(t::error_border())
                .text_xs()
                .text_color(t::error_text())
                .flex()
                .flex_col()
                .gap_1()
                .child(msg);

            if let Some(path) = log_path {
                let path_for_click = path.clone();
                error_bar = error_bar.child(
                    div()
                        .id("open-log-file")
                        .text_color(t::text_ghost())
                        .cursor_pointer()
                        .hover(|s| s.text_color(t::text_secondary()))
                        .on_click(move |_, _, _cx| {
                            let _ = std::process::Command::new("open")
                                .arg(&path_for_click)
                                .spawn();
                        })
                        .child(format!("View logs: {path}")),
                );
            }

            view = view.child(
                div()
                    .absolute()
                    .bottom(px(24.0))
                    .left_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(error_bar),
            );
        }

        view
    }
}
