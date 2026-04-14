use super::{AgentConfig, InstallStep};

pub fn config() -> AgentConfig {
    AgentConfig {
        name: "cursor",
        display_name: "Cursor",
        command: "/root/.local/bin/cursor-agent",
        icon: Some("icons/agents/cursor.svg"),
        // Cursor's brand mark is monochrome; leave color unset so it picks up
        // the theme's text color (white on dark, ink on light).
        color: None,
        tab_order: 2,
        install_steps: vec![
            // Cursor ships a versioned tarball URL that rotates with each release,
            // so pinning a specific URL in our config goes stale fast. Run the
            // upstream installer inside the sandbox instead — it fetches the
            // current build, extracts it into ~/.local/share/cursor-agent, and
            // symlinks ~/.local/bin/cursor-agent.
            InstallStep::Cmd {
                label: "Installing Cursor",
                command: "curl -fsSL https://cursor.com/install | bash",
                skip_if: Some("test -x /root/.local/bin/cursor-agent"),
            },
            InstallStep::Cmd {
                label: "Verifying installation",
                command: "/root/.local/bin/cursor-agent --version",
                skip_if: None,
            },
        ],
        secrets: vec![],
        auth_gateway: None,
    }
}
