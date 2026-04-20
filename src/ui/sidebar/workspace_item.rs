use crate::db::{CreateWorkspaceParams, Database, Workspace};
use crate::ui::components::HoldButton;
use crate::ui::review::SidePanel;
use crate::ui::terminal::TerminalPanel;
use crate::ui::theme as t;
use gpui::*;
use gpui::prelude::FluentBuilder as _;
use std::path::PathBuf;
use std::sync::Arc;

/// Renders a single workspace row in the sidebar.
pub struct WorkspaceItemView {
    pub workspace: Workspace,
    pub cloned_from_name: Option<String>,
    pub terminal_panel: Entity<TerminalPanel>,
    pub review_panel: Entity<SidePanel>,
    pub is_active: bool,
    pub db: Arc<Database>,
    pub on_refresh: std::rc::Rc<dyn Fn(&mut App) + 'static>,
    pub on_activate: std::rc::Rc<dyn Fn(i64, &mut App) + 'static>,
    pub badge_index: Option<usize>, // 1-based display number, e.g. Some(1) for ⌘1
    pub agent_names: Vec<String>,
    pub agent_status: crate::ui::terminal::session::AgentStatus,
    /// Current branch read from the repo's `.git/HEAD` at item-build time.
    /// Also serves as the runtime "is git repo" signal — presence means we
    /// successfully read HEAD, so the row renders as a repo. This overrides
    /// the persisted `workspace.is_git_repo`, which is a creation-time
    /// snapshot.
    live_branch: Option<String>,
    /// On-disk path to the cached GitHub owner avatar, if it exists.
    avatar: Option<PathBuf>,
    show_menu: bool,
    menu_position: Point<Pixels>,
    hold_button: Option<Entity<HoldButton>>,
}

impl WorkspaceItemView {
    pub fn new(
        workspace: Workspace,
        cloned_from_name: Option<String>,
        terminal_panel: Entity<TerminalPanel>,
        review_panel: Entity<SidePanel>,
        is_active: bool,
        db: Arc<Database>,
        on_refresh: std::rc::Rc<dyn Fn(&mut App) + 'static>,
        on_activate: std::rc::Rc<dyn Fn(i64, &mut App) + 'static>,
    ) -> Self {
        let (live_branch, avatar) = match workspace.mount_path.as_ref() {
            Some(path) if crate::git::is_git_repo(std::path::Path::new(path)) => {
                let repo = std::path::Path::new(path);
                let branch = crate::git::read_head_branch(repo);
                let owner = crate::git::github_owner_for_repo(repo);
                let avatar_path = owner.as_ref().map(|o| crate::avatar_cache::avatar_path(o));
                let avatar = avatar_path.as_ref().filter(|p| p.exists()).cloned();
                if avatar.is_none() {
                    if let Some(o) = owner {
                        crate::avatar_cache::prefetch(o);
                    }
                }
                (branch, avatar)
            }
            _ => (None, None),
        };

        Self {
            workspace,
            cloned_from_name,
            terminal_panel,
            review_panel,
            is_active,
            db,
            on_refresh,
            on_activate,
            badge_index: None,
            agent_names: Vec::new(),
            agent_status: Default::default(),
            live_branch,
            avatar,
            show_menu: false,
            menu_position: Point::default(),
            hold_button: None,
        }
    }

    fn render_subtitle(&self) -> AnyElement {
        let base = |text: String| -> AnyElement {
            div()
                .text_xs()
                .text_color(t::text_ghost())
                .overflow_hidden()
                .line_height(px(14.0))
                .child(text)
                .into_any_element()
        };

        if let Some(ref cloned_from) = self.cloned_from_name {
            return base(format!("cloned from {cloned_from}"));
        }

        match (&self.workspace.mount_path, self.live_branch.is_some()) {
            (Some(path), true) => {
                let repo_name = path.split('/').last().unwrap_or(path).to_string();
                let branch = self.live_branch.clone();
                let pr = self.workspace.pr_number;

                let mut row = div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_xs()
                    .text_color(t::text_ghost())
                    .line_height(px(14.0))
                    .overflow_hidden()
                    .child(div().flex_shrink_0().child(repo_name));

                if let Some(branch_name) = branch {
                    row = row
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(t::text_faint())
                                .child("\u{00B7}"),
                        )
                        .child(
                            svg()
                                .path(SharedString::from("icons/git-branch.svg"))
                                .size(px(11.0))
                                .flex_shrink_0()
                                .text_color(t::text_ghost()),
                        )
                        .child(div().flex_shrink_0().child(branch_name));
                }

                if let Some(n) = pr {
                    row = row
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(t::text_faint())
                                .child("\u{00B7}"),
                        )
                        .child(div().flex_shrink_0().child(format!("#{n}")));
                }

                row.into_any_element()
            }
            (Some(path), false) => {
                let dir_name = path.split('/').last().unwrap_or(path).to_string();
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_xs()
                    .text_color(t::text_ghost())
                    .line_height(px(14.0))
                    .overflow_hidden()
                    .child(
                        svg()
                            .path(SharedString::from("icons/folder.svg"))
                            .size(px(11.0))
                            .flex_shrink_0()
                            .text_color(t::text_ghost()),
                    )
                    .child(div().flex_shrink_0().child(dir_name))
                    .into_any_element()
            }
            (None, _) => base("scratch sandbox".to_string()),
        }
    }

    fn diff_label(&self) -> Option<String> {
        let stats = self.workspace.diff_stats.as_ref()?;
        if stats.additions == 0 && stats.deletions == 0 {
            return None;
        }
        Some(format!("+{} -{}", stats.additions, stats.deletions))
    }

    fn do_duplicate(&self, cx: &mut App) {
        let new_name = format!("{} (copy)", self.workspace.name);
        let _ = self.db.create_workspace(CreateWorkspaceParams {
            name: new_name,
            mount_path: self.workspace.mount_path.clone(),
            mount_read_only: true,
            is_git_repo: self.workspace.is_git_repo,
            branch_name: self.workspace.branch_name.clone(),
            base_branch: self.workspace.base_branch.clone(),
            initial_prompt: None,
            sandbox_cpus: self.workspace.sandbox_cpus,
            sandbox_memory_mb: self.workspace.sandbox_memory_mb,
            sandbox_disk_mb: self.workspace.sandbox_disk_mb,
            allowed_hosts: self.workspace.allowed_hosts.clone(),
            secrets_config: self.workspace.secrets_config.clone(),
            cloned_from_id: None,
        });
        (self.on_refresh)(cx);
    }

    fn open_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu_position = position;
        // Create the hold-to-delete button
        let on_refresh = self.on_refresh.clone();
        let db = self.db.clone();
        let ws_id = self.workspace.id;
        let terminal_panel = self.terminal_panel.clone();

        self.hold_button = Some(cx.new(|_| {
            HoldButton::new(
                "hold-delete",
                "Delete",
                gpui::rgba(0xFF4444AA), // red fill
                t::error_text(),
                move |cx| {
                    terminal_panel.update(cx, |panel, cx| {
                        panel.remove_session(ws_id, cx);
                    });
                    let _ = db.delete_workspace(ws_id);
                    (on_refresh)(cx);
                },
            )
            .icon("icons/trash.svg")
        }));
        self.show_menu = true;
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.show_menu = false;
        self.hold_button = None;
        cx.notify();
    }
}

impl Render for WorkspaceItemView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let diff_label = self.diff_label();
        let is_active = self.is_active;
        let workspace = self.workspace.clone();
        let terminal_panel = self.terminal_panel.clone();
        let review_panel = self.review_panel.clone();
        let on_activate = self.on_activate.clone();
        let show_menu = self.show_menu;

        // Build menu at mouse position
        let menu_el = if show_menu {
            let pos = self.menu_position;
            Some(deferred(
                anchored()
                    .position(pos)
                    .anchor(Corner::TopLeft)
                    .snap_to_window()
                    .child(
                        t::popover()
                            .w(px(160.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            // Duplicate
                            .child(
                                    t::menu_item()
                                    .id("menu-duplicate")
                                    .hover(|s| s.bg(t::bg_hover()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_menu(cx);
                                        this.do_duplicate(cx);
                                    }))
                                    .child(
                                        svg()
                                            .path(SharedString::from("icons/copy.svg"))
                                            .size(px(14.0))
                                            .text_color(t::text_dim()),
                                    )
                                    .child("Duplicate"),
                            )
                            .child(t::menu_separator())
                            // Hold-to-delete
                            .children(self.hold_button.as_ref().map(|btn| btn.clone())),
                    ),
            ).with_priority(1))
        } else {
            None
        };

        let badge_index = self.badge_index;

        let mut row = div()
            .id(("workspace-item", workspace.id as u64))
            .w_full()
            .px_2()
            .py(px(5.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .when(is_active, |s| s.bg(t::bg_active()))
            .hover(|s| s.bg(t::bg_hover()))
            .on_click(move |_event, window, cx| {
                if is_active {
                    return;
                }
                let ws = workspace.clone();
                terminal_panel.update(cx, |panel, cx| {
                    panel.activate_workspace(&ws, Some(window), cx);
                });
                // Show the side panel — use show_waiting to set visibility/git state,
                // then immediately try to connect a sandbox.
                let ws = workspace.clone();
                review_panel.update(cx, |panel, cx| {
                    panel.show_waiting(ws.id, "/workspace".to_string(), ws.mount_path.clone(), cx);
                });
                terminal_panel.update(cx, |panel, cx| {
                    panel.notify_side_panel_pub(ws.id, cx);
                });
                on_activate(workspace.id, cx);
            })
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.open_menu(event.position, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .overflow_hidden()
                    .gap(px(1.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child({
                                use crate::ui::terminal::session::AgentStatus;
                                let dot_color = match &self.agent_status {
                                    AgentStatus::Running { .. } => Some(t::agent_running()),
                                    AgentStatus::NeedsInput { .. } => Some(t::agent_needs_input()),
                                    AgentStatus::Idle | AgentStatus::Unknown => None,
                                };
                                let avatar = self.avatar.clone();
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .when_some(avatar, |el, path| {
                                        el.child(
                                            img(path)
                                                .w(px(12.0))
                                                .h(px(12.0))
                                                .rounded_full()
                                                .flex_shrink_0(),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(if is_active { t::text_primary() } else { t::text_secondary() })
                                            .overflow_hidden()
                                            .child(self.workspace.name.clone()),
                                    )
                                    .when_some(dot_color, |el, c| {
                                        el.child(
                                            div()
                                                .size(px(6.0))
                                                .rounded_full()
                                                .flex_shrink_0()
                                                .bg(c),
                                        )
                                    })
                            })
                            .children(diff_label.map(|label| {
                                div()
                                    .ml_auto()
                                    .text_xs()
                                    .text_color(t::status_green_dim())
                                    .child(label)
                            })),
                    )
                    .child(self.render_subtitle())
                    .when_some(self.agent_status.display_text(&self.agent_names), |el, status_text| {
                        use crate::ui::terminal::session::AgentStatus;
                        let color = match &self.agent_status {
                            AgentStatus::NeedsInput { .. } => t::agent_needs_input(),
                            AgentStatus::Running { .. } => t::agent_running(),
                            _ => t::text_ghost(),
                        };
                        el.child(
                            div()
                                .text_xs()
                                .text_color(color)
                                .overflow_hidden()
                                .line_height(px(14.0))
                                .child(status_text.to_string()),
                        )
                    }),
            )
            .when(badge_index.is_some(), |el| {
                let n = badge_index.unwrap();
                el.relative().child(
                    div()
                        .absolute()
                        .right(px(6.0))
                        .top(px(6.0))
                        .px(px(5.0))
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .bg(t::bg_selected())
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t::text_muted())
                        .child(format!("\u{2318}{}", n)),
                )
            })
            .children(menu_el);

        // Full-window backdrop to dismiss menu on any click or Esc
        if show_menu {
            row = row.child(deferred(
                div()
                    .id("ws-menu-backdrop")
                    .absolute()
                    .top(px(-2000.0))
                    .left(px(-2000.0))
                    .w(px(8000.0))
                    .h(px(8000.0))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.close_menu(cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.close_menu(cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        use crate::ui::components::actions::KEY_ESCAPE;
                        if event.keystroke.key.as_str() == KEY_ESCAPE {
                            this.close_menu(cx);
                        }
                    })),
            ).with_priority(0));
        }

        row
    }
}
