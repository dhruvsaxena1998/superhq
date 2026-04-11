use super::{secret_entry, AgentConfig, InstallStep, NODE_INSTALL_STEP};

pub fn config() -> AgentConfig {
    AgentConfig {
        name: "claude",
        display_name: "Claude Code",
        command: "/usr/local/bin/claude",
        icon: Some("icons/agents/claude.svg"),
        color: Some("#D97757"),
        tab_order: 0,
        install_steps: vec![
            NODE_INSTALL_STEP,
            InstallStep::Cmd {
                label: "Installing Claude Code",
                command: "/usr/local/bin/npm install -g @anthropic-ai/claude-code",
                skip_if: None,
            },
            InstallStep::Cmd {
                label: "Verifying installation",
                command: "/usr/local/bin/claude --version",
                skip_if: None,
            },
        ],
        secrets: vec![secret_entry(
            "ANTHROPIC_API_KEY",
            "Anthropic API Key",
            &["api.anthropic.com"],
            &[],
            false,
        )],
        auth_gateway: None,
    }
}
