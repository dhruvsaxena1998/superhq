use gpui::*;
use gpui_terminal::TerminalView;
use shuru_sdk::{AsyncSandbox, SandboxConfig};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::session::{AgentStatus, TabKind, TerminalTab};
use crate::agents;
use crate::sandbox::agent_setup;
use crate::sandbox::auth_gateway::{AuthGateway, AuthGatewayConfig};
use crate::sandbox::pty_adapter::{ShuruPtyReader, ShuruPtyWriter};
use crate::sandbox::secrets;

impl super::TerminalPanel {
    /// Spawn the async boot sequence for an existing tab.
    pub(super) fn boot_agent_tab(
        &mut self,
        ws_id: i64,
        tab_id: u64,
        agent_id: i64,
        agent_command: String,
        checkpoint_from: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let agent_info = self.agents.iter().find(|a| a.id == agent_id);
        let required_secrets = agent_info
            .map(|a| a.required_secrets.clone())
            .unwrap_or_default();
        let agent_slug = agent_info.map(|a| a.name.clone()).unwrap_or_default();
        let agent_for_config = agent_info.cloned();

        // Get install steps from static config
        let static_cfg = agents::builtin_agents()
            .into_iter()
            .find(|c| c.name == agent_slug);
        let install_steps: Vec<agents::InstallStep> = static_cfg
            .map(|c| c.install_steps)
            .unwrap_or_default();

        let needs_install = checkpoint_from.is_none()
            && !install_steps.is_empty()
            && !agent_setup::checkpoint_exists(&agent_setup::agent_checkpoint_name(&agent_slug));

        // The "Starting workspace" step index depends on whether install is needed
        let start_ws_step = if needs_install { install_steps.len() + 2 } else { 0 };

        let db_for_secrets = self.db.clone();
        let mount_path: Option<String> = self.sessions.get(&ws_id)
            .and_then(|s| s.read(cx).mount_path.clone());

        let tokio_handle = self.tokio_handle.clone();
        let this = cx.entity().downgrade();

        cx.spawn(async move |_, cx| {
            // === CHECKPOINT PHASE ===
            let boot_from = if checkpoint_from.is_some() {
                checkpoint_from
            } else if needs_install {
                // Step 0: Preparing sandbox (already Active)
                let sb_settings_install = db_for_secrets.get_settings().ok();
                let mut install_config = SandboxConfig::default();
                install_config.allow_net = true;
                install_config.env.insert("HOME".into(), "/root".into());
                install_config.storage = shuru_sdk::StorageMode::Direct;
                install_config.memory_mb = sb_settings_install.as_ref().map(|s| s.sandbox_memory_mb as u64).unwrap_or(8192);
                install_config.disk_size_mb = sb_settings_install.as_ref().map(|s| s.sandbox_disk_mb as u64).unwrap_or(16384);

                let install_sb = tokio_handle
                    .spawn(async move { AsyncSandbox::boot(install_config).await })
                    .await
                    .unwrap();

                let install_sb = match install_sb {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        cx.update(|cx| {
                            this.update(cx, |p, cx| {
                                p.fail_setup(ws_id, tab_id, 0, format!("{e}"), None, cx);
                                cx.notify();
                            }).ok();
                        }).ok();
                        return;
                    }
                };

                // Run each install step, advancing the UI after each
                for (step_ix, step) in install_steps.iter().enumerate() {
                    let ui_ix = step_ix + 1; // step 0 is "Preparing sandbox"

                    cx.update(|cx| {
                        this.update(cx, |p, cx| {
                            p.advance_step(ws_id, tab_id, ui_ix.saturating_sub(1), ui_ix, cx);
                            cx.notify();
                        }).ok();
                    }).ok();

                    // Check skip_if
                    if let Some(skip_cmd) = step.skip_if() {
                        let sb = install_sb.clone();
                        let cmd = skip_cmd.to_string();
                        if let Ok(Ok(r)) = tokio_handle
                            .spawn(async move { sb.exec_in("bash", &cmd).await })
                            .await
                        {
                            if r.exit_code == 0 {
                                continue;
                            }
                        }
                    }

                    // Execute the step
                    let step_result: Result<(), String> = match step {
                        agents::InstallStep::Cmd { command, .. } => {
                            let sb = install_sb.clone();
                            let cmd = command.to_string();
                            match tokio_handle
                                .spawn(async move { sb.exec_in("bash", &cmd).await })
                                .await
                                .unwrap()
                            {
                                Ok(r) if r.exit_code != 0 => {
                                    Err(format!("exit code {} — {}{}", r.exit_code, r.stdout, r.stderr))
                                }
                                Err(e) => Err(format!("{e}")),
                                _ => Ok(()),
                            }
                        }
                        agents::InstallStep::Group { steps: sub_steps, .. } => {
                            let mut group_result = Ok(());
                            for sub in sub_steps {
                                let r: Result<(), String> = match sub {
                                    agents::InstallStep::Cmd { command, .. } => {
                                        let sb = install_sb.clone();
                                        let cmd = command.to_string();
                                        match tokio_handle.spawn(async move { sb.exec_in("bash", &cmd).await }).await.unwrap() {
                                            Ok(r) if r.exit_code != 0 => Err(format!("exit code {} — {}{}", r.exit_code, r.stdout, r.stderr)),
                                            Err(e) => Err(format!("{e}")),
                                            _ => Ok(()),
                                        }
                                    }
                                    agents::InstallStep::Download { label: dl_label, url, path, extract, strip_components, .. } => {
                                        let sb = install_sb.clone();
                                        let url = url.to_string();
                                        let path = path.to_string();
                                        let extract = *extract;
                                        let strip = *strip_components;
                                        let base_label = *dl_label;
                                        match tokio_handle.spawn(async move { sb.download(&url, &path, extract, strip).await }).await.unwrap() {
                                            Ok((mut reply_rx, progress_rx)) => {
                                                loop {
                                                    let mut last_progress = None;
                                                    while let Ok(p) = progress_rx.try_recv() {
                                                        last_progress = Some(p);
                                                    }
                                                    if let Some(p) = last_progress {
                                                        let mb = p.bytes_downloaded / (1024 * 1024);
                                                        let label = if let Some(total) = p.total_bytes {
                                                            let total_mb = total / (1024 * 1024);
                                                            SharedString::from(format!("{base_label} ({mb}/{total_mb} MB)"))
                                                        } else {
                                                            SharedString::from(format!("{base_label} ({mb} MB)"))
                                                        };
                                                        cx.update(|cx| {
                                                            this.update(cx, |panel, cx| {
                                                                panel.update_step_label(ws_id, tab_id, ui_ix, label, cx);
                                                                cx.notify();
                                                            }).ok();
                                                        }).ok();
                                                    }
                                                    match reply_rx.try_recv() {
                                                        Ok(Ok(())) => break Ok(()),
                                                        Ok(Err(e)) => break Err(format!("{e}")),
                                                        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => break Err("closed".into()),
                                                        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                                                            cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => Err(format!("{e}")),
                                        }
                                    }
                                    agents::InstallStep::Rename { from, to, .. } => {
                                        let sb = install_sb.clone();
                                        let f = from.to_string();
                                        let t = to.to_string();
                                        tokio_handle.spawn(async move { sb.rename(&f, &t).await }).await.unwrap().map_err(|e| format!("{e}"))
                                    }
                                    agents::InstallStep::Chmod { path, mode, .. } => {
                                        let sb = install_sb.clone();
                                        let p = path.to_string();
                                        let m = *mode;
                                        tokio_handle.spawn(async move { sb.chmod(&p, m).await }).await.unwrap().map_err(|e| format!("{e}"))
                                    }
                                    _ => Ok(()),
                                };
                                if let Err(e) = r {
                                    group_result = Err(e);
                                    break;
                                }
                            }
                            group_result
                        }
                        agents::InstallStep::Download { label, url, path, extract, strip_components, .. } => {
                            let sb = install_sb.clone();
                            let url = url.to_string();
                            let path = path.to_string();
                            let extract = *extract;
                            let strip = *strip_components;
                            let base_label = *label;
                            match tokio_handle
                                .spawn(async move { sb.download(&url, &path, extract, strip).await })
                                .await
                                .unwrap()
                            {
                                Ok((mut reply_rx, progress_rx)) => {
                                    loop {
                                        // Drain progress updates
                                        let mut last_progress = None;
                                        while let Ok(p) = progress_rx.try_recv() {
                                            last_progress = Some(p);
                                        }
                                        if let Some(p) = last_progress {
                                            let mb = p.bytes_downloaded / (1024 * 1024);
                                            let label = if let Some(total) = p.total_bytes {
                                                let total_mb = total / (1024 * 1024);
                                                SharedString::from(format!("{base_label} ({mb}/{total_mb} MB)"))
                                            } else {
                                                SharedString::from(format!("{base_label} ({mb} MB)"))
                                            };
                                            cx.update(|cx| {
                                                this.update(cx, |panel, cx| {
                                                    panel.update_step_label(ws_id, tab_id, ui_ix, label, cx);
                                                    cx.notify();
                                                }).ok();
                                            }).ok();
                                        }

                                        match reply_rx.try_recv() {
                                            Ok(Ok(())) => break Ok(()),
                                            Ok(Err(e)) => break Err(format!("{e}")),
                                            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                                                break Err("download channel closed".into())
                                            }
                                            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                                                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                                            }
                                        }
                                    }
                                }
                                Err(e) => Err(format!("{e}")),
                            }
                        }
                        agents::InstallStep::Rename { from, to, .. } => {
                            let sb = install_sb.clone();
                            let f = from.to_string();
                            let t = to.to_string();
                            tokio_handle.spawn(async move { sb.rename(&f, &t).await }).await.unwrap().map_err(|e| format!("{e}"))
                        }
                        agents::InstallStep::Chmod { path, mode, .. } => {
                            let sb = install_sb.clone();
                            let p = path.to_string();
                            let m = *mode;
                            tokio_handle.spawn(async move { sb.chmod(&p, m).await }).await.unwrap().map_err(|e| format!("{e}"))
                        }
                    };

                    if let Err(msg) = step_result {
                        cx.update(|cx| {
                            this.update(cx, |p, cx| {
                                p.fail_setup(ws_id, tab_id, ui_ix, msg, None, cx);
                                cx.notify();
                            }).ok();
                        }).ok();
                        return;
                    }
                }

                // Ensure login shells source .bashrc (tool installers write PATH there,
                // but bash -l only reads .bash_profile/.profile by default).
                {
                    let sb = install_sb.clone();
                    let _ = tokio_handle
                        .spawn(async move {
                            // Only create if it doesn't exist — don't overwrite
                            // an installer-created .bash_profile
                            if sb.read_file("/root/.bash_profile").await.is_err() {
                                let _ = sb.write_file("/root/.bash_profile", b"[ -f ~/.bashrc ] && . ~/.bashrc\n").await;
                            }
                        })
                        .await;
                }

                // All install steps done → Saving checkpoint
                let save_step_ix = install_steps.len() + 1;
                cx.update(|cx| {
                    this.update(cx, |p, cx| {
                        p.advance_step(ws_id, tab_id, save_step_ix - 1, save_step_ix, cx);
                        cx.notify();
                    }).ok();
                }).ok();

                let cp_name = agent_setup::agent_checkpoint_name(&agent_slug);
                let sb = install_sb.clone();
                let n = cp_name.clone();
                let cp_result = tokio_handle
                    .spawn(async move { sb.checkpoint(&n).await })
                    .await
                    .unwrap();

                if let Err(e) = cp_result {
                    eprintln!("Checkpoint save failed (non-fatal): {e}");
                }

                // Stop install sandbox
                let sb = install_sb;
                let _ = tokio_handle.spawn(async move { sb.stop().await }).await;

                // Checkpoint done → Starting workspace
                let start_step_ix = save_step_ix + 1;
                cx.update(|cx| {
                    this.update(cx, |p, cx| {
                        p.advance_step(ws_id, tab_id, save_step_ix, start_step_ix, cx);
                        cx.notify();
                    }).ok();
                }).ok();

                Some(cp_name)
            } else if !install_steps.is_empty() {
                // Checkpoint exists — boot from it
                Some(agent_setup::agent_checkpoint_name(&agent_slug))
            } else {
                // No install script — boot from base
                None
            };

            // === LOOK UP STATIC AGENT CONFIG (for auth gateway spec) ===
            let static_config = agents::builtin_agents()
                .into_iter()
                .find(|c| c.name == agent_slug);
            let gateway_spec =
                agents::auth_gateway_spec_for_agent(&agent_slug, &db_for_secrets, static_config.as_ref());

            // === REFRESH OAUTH TOKENS & BUILD SECRETS ===
            {
                let db = db_for_secrets.clone();
                let required = required_secrets.clone();
                let _ = tokio_handle
                    .spawn(async move { secrets::refresh_oauth_tokens(&db, &required).await })
                    .await;
            }
            // Skip MITM proxy setup for secrets handled by the auth gateway
            let gateway_env_vars: HashSet<&str> = gateway_spec
                .map(|s| [s.secret_env_var].into_iter().collect())
                .unwrap_or_default();
            let secrets_map = secrets::build_secrets_map(&db_for_secrets, &required_secrets, &gateway_env_vars)
                .map(|r| r.secrets)
                .unwrap_or_default();

            // === START AUTH GATEWAY (if agent uses one) ===
            let mut auth_gateway_handle: Option<AuthGateway> = None;
            let mut gateway_env: HashMap<String, String> = HashMap::new();
            if let Some(spec) = gateway_spec {
                let gw_config = AuthGatewayConfig {
                    db: db_for_secrets.clone(),
                    secret_env_var: spec.secret_env_var.to_string(),
                    upstream_base: spec.upstream_base.to_string(),
                };
                match tokio_handle
                    .spawn(async move { AuthGateway::start(gw_config).await })
                    .await
                    .unwrap()
                {
                    Ok(gw) => {
                        let gw_url = format!("http://host.shuru.internal:{}/v1", spec.guest_port);
                        if let Some(env_var) = spec.base_url_env {
                            gateway_env.insert(env_var.to_string(), gw_url.clone());
                        }
                        // Pass the gateway URL so auth_setup can write agent
                        // config files. Don't inject the real env var name
                        // (e.g. OPENAI_API_KEY) — that activates built-in
                        // providers that bypass the gateway.
                        gateway_env.insert("_GATEWAY_BASE_URL".to_string(), gw_url);
                        // Pass auth method so auth_setup can pick the right API type + models
                        let auth_method = db_for_secrets
                            .get_secret_auth_method(spec.secret_env_var)
                            .unwrap_or_else(|_| "api_key".into());
                        gateway_env.insert("_GATEWAY_AUTH_METHOD".to_string(), auth_method.clone());
                        if agent_slug == "codex" {
                            let provider = if spec.secret_env_var == "OPENROUTER_API_KEY" {
                                "openrouter"
                            } else {
                                "openai"
                            };
                            gateway_env.insert("_CODEX_PROVIDER".to_string(), provider.to_string());
                            if spec.secret_env_var == "OPENROUTER_API_KEY" {
                                gateway_env.insert(
                                    "OPENROUTER_API_KEY".to_string(),
                                    agents::GATEWAY_DUMMY_KEY.to_string(),
                                );
                            }
                        }
                        // For OAuth, build a stub JWT with just the accountId so Pi's
                        // openai-codex-responses can parse it without real credentials.
                        if auth_method == "oauth" {
                            if let Some(jwt) = crate::sandbox::auth_gateway::build_stub_jwt(
                                &db_for_secrets, spec.secret_env_var,
                            ) {
                                gateway_env.insert("_GATEWAY_STUB_JWT".to_string(), jwt);
                            }
                        }
                        auth_gateway_handle = Some(gw);
                    }
                    Err(e) => {
                        eprintln!("[boot] auth gateway start failed: {e}");
                    }
                }
            }

            // === BOOT TAB SANDBOX ===
            use crate::sandbox::service as svc;
            let config = svc::build_config(
                &db_for_secrets,
                ws_id,
                mount_path.as_deref(),
                secrets_map,
                boot_from.as_deref(),
                gateway_spec.as_ref(),
                auth_gateway_handle.as_ref(),
            );

            let sandbox = match tokio_handle.spawn(svc::boot_with_retry(config, 3)).await.unwrap() {
                Ok(s) => s,
                Err(last_err) => {
                    let step_idx = start_ws_step;
                    cx.update(|cx| {
                        this.update(cx, |p, cx| {
                            p.fail_setup(ws_id, tab_id, step_idx, last_err, None, cx);
                            cx.notify();
                        }).ok();
                    }).ok();
                    return;
                }
            };

            svc::post_boot_setup(
                &sandbox,
                mount_path.as_deref(),
                agent_for_config.as_ref(),
                &gateway_env,
            ).await;

            let agent_argv: Vec<String> = agent_command
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            let agent_env = svc::build_agent_env(&gateway_env);
            let shell = {
                let sb = sandbox.clone();
                let argv = agent_argv;
                tokio_handle
                    .spawn(async move {
                        let argv_refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
                        sb.open_shell(24, 80, Some("/workspace"), Some(&argv_refs), agent_env).await
                    })
                    .await
                    .unwrap()
            };

            let shell = match shell {
                Ok(s) => s,
                Err(e) => {
                    let step_idx = start_ws_step;
                    cx.update(|cx| {
                        this.update(cx, |p, cx| {
                            p.fail_setup(ws_id, tab_id, step_idx, format!("{e}"), None, cx);
                            cx.notify();
                        }).ok();
                    }).ok();
                    return;
                }
            };

            let (writer, reader) = shell.split();
            let pty_writer = ShuruPtyWriter::new(writer.clone());
            let pty_reader = ShuruPtyReader::new(reader, tokio_handle.clone());
            let pty_output_tap = pty_reader.tap();
            let pty_scrollback = pty_reader.scrollback();
            let pty_bus_writer = writer.clone();

            // Build the PtyBus up front so the local resize callback can
            // route through it — otherwise `PtyBus.dimensions` drifts
            // from the real PTY size and remote `pty.attach` responses
            // report stale geometry.
            let pty_bus = crate::ui::remote::PtyBus::new(
                pty_bus_writer,
                pty_output_tap,
                pty_scrollback,
            );
            let bus_for_resize = pty_bus.clone();

            let terminal_config = Self::make_terminal_config();
            let resize_callback = move |cols: usize, rows: usize| {
                bus_for_resize.resize(cols as u16, rows as u16);
            };

            cx.update(|cx| {
                this.update(cx, |panel, cx| {
                    // Get the dynamic_title Rc from the existing tab
                    let dyn_title_cb = panel.sessions.get(&ws_id)
                        .and_then(|s| s.read(cx).find_tab(tab_id).map(|t| t.dynamic_title.clone()))
                        .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(None)));

                    let terminal = cx.new(|cx| {
                        TerminalView::new(
                            Box::new(pty_writer),
                            Box::new(pty_reader),
                            terminal_config,
                            cx,
                        )
                        .with_resize_callback(resize_callback)
                        .with_title_callback(move |_window, cx, title| {
                            *dyn_title_cb.borrow_mut() = Some(SharedString::from(title.to_string()));
                            cx.notify();
                        })
                    });

                    // Start agent event service for lifecycle hooks
                    let event_service = crate::sandbox::event_watcher::AgentEventService::start(
                        sandbox.clone(),
                        panel.tokio_handle.clone(),
                    );

                    // Store terminal + sandbox but keep setup view visible.
                    // The terminal buffers output in the background.
                    if let Some(session) = panel.sessions.get(&ws_id) {
                        session.update(cx, |s, _cx| {
                            if let Some(tab) = s.find_tab_mut(tab_id) {
                                tab.terminal = Some(terminal.clone());
                                if let TabKind::Agent { sandbox: ref mut sb, auth_gateway: ref mut ag, .. } = tab.kind {
                                    *sb = Some(sandbox);
                                    *ag = auth_gateway_handle;
                                }
                            }
                        });
                    }

                    // Register this PTY on the shared bus map so the
                    // remote-control handler can attach to it.
                    if let Ok(mut map) = panel.pty_map.write() {
                        map.insert((ws_id, tab_id), pty_bus);
                    }

                    // Bridge agent status updates to GPUI
                    if let Some((service, status_rx)) = event_service {
                        if let Some(session) = panel.sessions.get(&ws_id) {
                            session.update(cx, |s, _cx| {
                                if let Some(tab) = s.find_tab_mut(tab_id) {
                                    tab.event_service = Some(service);
                                }
                            });
                        }
                        let weak_panel = cx.entity().downgrade();
                        cx.spawn(async move |_, cx| {
                            while let Ok(status) = status_rx.recv_async().await {
                                cx.update(|cx| {
                                    weak_panel.update(cx, |panel, cx| {
                                        if let Some(session) = panel.sessions.get(&ws_id) {
                                            session.update(cx, |s, _cx| {
                                                if let Some(tab) = s.find_tab_mut(tab_id) {
                                                    tab.agent_status = status;
                                                }
                                            });
                                        }
                                        cx.notify();
                                    }).ok();
                                }).ok();
                            }
                        }).detach();
                    }

                    panel.notify_side_panel(ws_id, cx);

                    // Wait for TUI to take over (cursor hidden or alt screen).
                    let weak_panel = cx.entity().downgrade();
                    cx.spawn(async move |_, cx| {
                        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                        loop {
                            let ready = cx.update(|cx| {
                                terminal.read(cx).tui_active()
                            }).unwrap_or(false);

                            if ready || std::time::Instant::now() >= deadline {
                                cx.update(|cx| {
                                    weak_panel.update(cx, |panel, cx| {
                                        if let Some(session) = panel.sessions.get(&ws_id) {
                                            let focus_handle = session.update(cx, |s, cx| {
                                                if let Some(tab) = s.find_tab_mut(tab_id) {
                                                    tab.setup_steps = None;
                                                    tab.setup_error = None;
                                                }
                                                // Auto-focus the terminal if this is the active tab
                                                if let Some(active) = s.tabs.get(s.active_tab) {
                                                    if active.tab_id == tab_id {
                                                        if let Some(ref terminal) = active.terminal {
                                                            return Some(terminal.read(cx).focus_handle().clone());
                                                        }
                                                    }
                                                }
                                                None
                                            });
                                            if let Some(fh) = focus_handle {
                                                cx.defer(move |cx| {
                                                    if let Some(window) = cx.active_window() {
                                                        window.update(cx, |_, window, _cx| {
                                                            fh.focus(window);
                                                        }).ok();
                                                    }
                                                });
                                            }
                                        }
                                        cx.notify();
                                    }).ok();
                                }).ok();
                                break;
                            }

                            cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                        }
                    }).detach();

                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
    }

    /// Open a shell tab on an existing agent's sandbox (like `docker exec`).
    pub fn open_shell_tab(
        &mut self,
        parent_agent_tab_id: u64,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let ws_id = match self.active_workspace_id {
            Some(id) => id,
            None => return None,
        };

        let session = match self.sessions.get(&ws_id) {
            Some(s) => s,
            None => return None,
        };

        // Find the parent agent tab — must have a ready sandbox
        let (sandbox, parent_label) = {
            let s = session.read(cx);
            match s.tabs.iter().find(|t| t.tab_id == parent_agent_tab_id) {
                Some(tab) => match &tab.kind {
                    TabKind::Agent { sandbox: Some(sandbox), .. } => (sandbox.clone(), tab.label.clone()),
                    _ => return None, // Still setting up or not an agent
                },
                None => return None,
            }
        };

        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        let tokio_handle = self.tokio_handle.clone();
        let this = cx.entity().downgrade();
        let sandbox_for_kind = sandbox.clone();

        cx.spawn(async move |_, cx| {
            let shell = {
                let sb = sandbox.clone();
                let mut env = HashMap::new();
                env.insert("TERM".to_string(), "xterm-256color".to_string());
                env.insert("COLORTERM".to_string(), "truecolor".to_string());
                env.insert(
                    "COLORFGBG".to_string(),
                    if crate::ui::theme::is_dark() { "15;0".into() } else { "0;15".into() },
                );
                env.insert("PROMPT_COMMAND".to_string(),
                    r#"printf "\033]0;%s@%s:%s\007" "${USER}" "${HOSTNAME%%.*}" "${PWD/#$HOME/~}""#.to_string());
                tokio_handle
                    .spawn(async move { sb.open_shell(24, 80, Some("/workspace"), None, env).await })
                    .await
                    .unwrap()
            };

            let shell = match shell {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to open shell: {e}");
                    return;
                }
            };

            let (writer, reader) = shell.split();
            let pty_writer = ShuruPtyWriter::new(writer.clone());
            let pty_reader = ShuruPtyReader::new(reader, tokio_handle.clone());
            let pty_output_tap = pty_reader.tap();
            let pty_scrollback = pty_reader.scrollback();
            let pty_bus_writer = writer.clone();

            let pty_bus = crate::ui::remote::PtyBus::new(
                pty_bus_writer,
                pty_output_tap,
                pty_scrollback,
            );
            let bus_for_resize = pty_bus.clone();

            let terminal_config = Self::make_terminal_config();
            let resize_callback = move |cols: usize, rows: usize| {
                bus_for_resize.resize(cols as u16, rows as u16);
            };

            cx.update(|cx| {
                this.update(cx, |panel, cx| {
                    let dyn_title: std::rc::Rc<std::cell::RefCell<Option<SharedString>>> =
                        std::rc::Rc::new(std::cell::RefCell::new(None));
                    let dyn_title_cb = dyn_title.clone();
                    let terminal = cx.new(|cx| {
                        TerminalView::new(
                            Box::new(pty_writer),
                            Box::new(pty_reader),
                            terminal_config,
                            cx,
                        )
                        .with_resize_callback(resize_callback)
                        .with_title_callback(move |_window, cx, title| {
                            *dyn_title_cb.borrow_mut() = Some(SharedString::from(title.to_string()));
                            cx.notify();
                        })
                    });

                    if let Some(session) = panel.sessions.get(&ws_id) {
                        session.update(cx, |s, cx| {
                            s.add_tab(TerminalTab {
                                tab_id,
                                label: SharedString::from(format!("{} guest shell", parent_label)),
                                dynamic_title: dyn_title,
                                terminal: Some(terminal.clone()),
                                setup_steps: None,
                                setup_error: None,
                                agent_color: None,
                                icon_path: Some(SharedString::from("icons/terminal.svg")),
                                kind: TabKind::Shell {
                                    parent_agent_tab_id,
                                    sandbox: sandbox_for_kind,
                                },
                                agent_status: AgentStatus::Unknown,
                                event_service: None,
                                checkpointing: false,
                                checkpoint_name: None,
                                tab_db_id: None,
                            }, cx);
                            s.tab_scroll.scroll_to_item(s.tabs.len() - 1);
                        });
                    }
                    if let Ok(mut map) = panel.pty_map.write() {
                        map.insert((ws_id, tab_id), pty_bus);
                    }
                    // Auto-focus the new shell terminal
                    let fh = terminal.read(cx).focus_handle().clone();
                    cx.defer(move |cx| {
                        if let Some(window) = cx.active_window() {
                            window.update(cx, |_, window, _cx| {
                                fh.focus(window);
                            }).ok();
                        }
                    });
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        Some(tab_id)
    }

    /// Open a host shell tab (local PTY, not sandboxed).
    pub fn open_host_shell_tab(&mut self, cx: &mut Context<Self>) -> Option<u64> {
        let ws_id = match self.active_workspace_id {
            Some(id) => id,
            None => return None,
        };

        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        // Create local PTY
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let pty_system = portable_pty::native_pty_system();
        let pair = match pty_system.openpty(portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("Failed to open PTY: {e}");
                return None;
            }
        };

        let mut cmd = portable_pty::CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env(
            "COLORFGBG",
            if crate::ui::theme::is_dark() { "15;0" } else { "0;15" },
        );

        // Start in the workspace's mount path if available
        let mount_path = self.sessions.get(&ws_id)
            .and_then(|s| s.read(cx).mount_path.clone());
        if let Some(ref path) = mount_path {
            cmd.cwd(path);
        }

        if let Err(e) = pair.slave.spawn_command(cmd) {
            eprintln!("Failed to spawn shell: {e}");
            return None;
        }

        let raw_writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => { eprintln!("Failed to get PTY writer: {e}"); return None; }
        };
        let raw_reader = match pair.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => { eprintln!("Failed to get PTY reader: {e}"); return None; }
        };

        let pty_master = std::sync::Arc::new(parking_lot::Mutex::new(pair.master));
        drop(pair.slave);

        // Remote-control plumbing: the reader is wrapped so every byte
        // destined for the local xterm also lands in a broadcast
        // channel + scrollback ring. Remote subscribers read from those.
        // The writer is shared so remote keystrokes end up on the same
        // PTY as local ones.
        let (tap_tx, _tap_rx) = tokio::sync::broadcast::channel::<bytes::Bytes>(1024);
        let scrollback = std::sync::Arc::new(std::sync::Mutex::new(
            crate::sandbox::pty_adapter::ScrollbackRing::new(256 * 1024),
        ));
        let shared_writer = std::sync::Arc::new(
            parking_lot::Mutex::new(raw_writer),
        );
        let reader: Box<dyn std::io::Read + Send> = Box::new(HostShellTappedReader {
            inner: raw_reader,
            tap: tap_tx.clone(),
            scrollback: scrollback.clone(),
        });
        let writer: Box<dyn std::io::Write + Send> = Box::new(HostShellSharedWriter(
            shared_writer.clone(),
        ));

        let terminal_config = Self::make_terminal_config();

        // Register the PtyBus so remote clients can attach. Host shell
        // uses a custom `PtyInput` impl since it isn't backed by a
        // shuru-sdk `ShellWriter`. Route local resize through the bus
        // too so `current_dimensions()` stays accurate.
        let host_pty_bus = crate::ui::remote::PtyBus::new(
            HostShellPtyInput {
                writer: shared_writer.clone(),
                master: pty_master.clone(),
            },
            tap_tx.clone(),
            scrollback.clone(),
        );
        let bus_for_resize = host_pty_bus.clone();
        let resize_callback = move |cols: usize, rows: usize| {
            bus_for_resize.resize(cols as u16, rows as u16);
        };
        if let Ok(mut map) = self.pty_map.write() {
            map.insert((ws_id, tab_id), host_pty_bus);
        }

        if let Some(session) = self.sessions.get(&ws_id) {
            let dyn_title: std::rc::Rc<std::cell::RefCell<Option<SharedString>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let dyn_title_cb = dyn_title.clone();
            let terminal = cx.new(|cx| {
                TerminalView::new(writer, reader, terminal_config, cx)
                    .with_resize_callback(resize_callback)
                    .with_title_callback(move |_window, cx, title| {
                        *dyn_title_cb.borrow_mut() = Some(SharedString::from(title.to_string()));
                        cx.notify();
                    })
            });

            let tab = TerminalTab {
                tab_id,
                label: SharedString::from("Host shell"),
                dynamic_title: dyn_title,
                terminal: Some(terminal),
                setup_steps: None,
                setup_error: None,
                agent_color: Some(crate::ui::theme::text_muted()),
                icon_path: Some(SharedString::from("icons/terminal.svg")),
                kind: TabKind::HostShell { pty_master },
                agent_status: AgentStatus::Unknown,
                event_service: None,
                checkpointing: false,
                checkpoint_name: None,
                tab_db_id: None,
            };

            session.update(cx, |s, cx| {
                s.add_tab(tab, cx);
                s.tab_scroll.scroll_to_item(s.tabs.len() - 1);
            });

            self.notify_side_panel(ws_id, cx);
            cx.notify();
        }
        Some(tab_id)
    }

}

/// Reader wrapper for host-shell PTYs. Every successful read is mirrored
/// into a broadcast channel + scrollback ring so remote clients can
/// attach and receive the same bytes the local xterm is drawing. The
/// scrollback push + broadcast send happen under the same lock so an
/// attach handler can snapshot + subscribe atomically — matching the
/// agent PTY bridge in `sandbox::pty_adapter`.
struct HostShellTappedReader {
    inner: Box<dyn std::io::Read + Send>,
    tap: tokio::sync::broadcast::Sender<bytes::Bytes>,
    scrollback:
        std::sync::Arc<std::sync::Mutex<crate::sandbox::pty_adapter::ScrollbackRing>>,
}

impl std::io::Read for HostShellTappedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            let bytes = bytes::Bytes::copy_from_slice(&buf[..n]);
            if let Ok(mut sb) = self.scrollback.lock() {
                sb.push(&buf[..n]);
                let _ = self.tap.send(bytes);
            }
        }
        Ok(n)
    }
}

/// Writer wrapper that lets the local `TerminalView` and a remote
/// input stream share the same underlying portable-pty writer. A
/// `parking_lot::Mutex` serialises the two sources of writes.
struct HostShellSharedWriter(std::sync::Arc<parking_lot::Mutex<Box<dyn std::io::Write + Send>>>);

impl std::io::Write for HostShellSharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().flush()
    }
}

/// `PtyInput` impl backed by a host-shell portable-pty. Input goes
/// through the shared writer (same as the local xterm uses); resize
/// drives the master directly.
struct HostShellPtyInput {
    writer: std::sync::Arc<parking_lot::Mutex<Box<dyn std::io::Write + Send>>>,
    master: std::sync::Arc<parking_lot::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
}

impl crate::ui::remote::pty_bus::PtyInput for HostShellPtyInput {
    fn send_input(&self, data: &[u8]) -> Result<(), String> {
        use std::io::Write;
        let mut w = self.writer.lock();
        w.write_all(data).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(())
    }
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.master
            .lock()
            .resize(portable_pty::PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())
    }
}
