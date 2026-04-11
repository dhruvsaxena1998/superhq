//! Agent auth setup orchestrator.
//!
//! Reads proxy placeholder env vars from the sandbox, merges with
//! gateway env vars, then dispatches to the agent's Rust module
//! for config file writes.

use crate::agents;
use crate::db::Agent;
use shuru_sdk::AsyncSandbox;
use std::collections::HashMap;
use std::sync::Arc;

pub async fn run_auth_setup(
    sandbox: &Arc<AsyncSandbox>,
    agent: &Agent,
    gateway_env: &HashMap<String, String>,
) {
    // Read proxy placeholder env vars from the sandbox
    let sandbox_env = match sandbox.exec_in("sh", "printenv").await {
        Ok(r) => r
            .stdout
            .lines()
            .filter_map(|line| line.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
            .collect::<HashMap<_, _>>(),
        Err(e) => {
            eprintln!("[auth_setup] {} failed to read env: {e}", agent.name);
            HashMap::new()
        }
    };

    let mut vars = sandbox_env;
    vars.extend(gateway_env.iter().map(|(k, v)| (k.clone(), v.clone())));

    // Dispatch to agent-specific auth setup
    agents::run_auth_setup(&agent.name, sandbox, &vars).await;

    // Build profile.d for interactive shell env vars
    let mut profile_lines = String::new();

    // Proxy CA cert (only if MITM is active)
    if let Ok(r) = sandbox.exec_in("sh", "test -f /usr/local/share/ca-certificates/shuru-proxy.crt && echo yes").await {
        if r.stdout.trim() == "yes" {
            write_export(&mut profile_lines, "NODE_EXTRA_CA_CERTS", "/usr/local/share/ca-certificates/shuru-proxy.crt");
            write_export(&mut profile_lines, "SSL_CERT_FILE", "/etc/ssl/certs/ca-certificates.crt");
        }
    }

    // Gateway env vars (skip internal _GATEWAY_* keys used only by auth_setup)
    for (k, v) in gateway_env {
        if !k.starts_with("_GATEWAY_") {
            write_export(&mut profile_lines, k, v);
        }
    }

    if let Err(e) = sandbox
        .write_file("/etc/profile.d/shuru-env.sh", profile_lines.as_bytes())
        .await
    {
        eprintln!("[auth_setup] {} failed to write profile.d: {e}", agent.name);
    }
}

fn write_export(lines: &mut String, k: &str, v: &str) {
    lines.push_str(&format!("export {}='{}'\n", k, v.replace('\'', "'\\''")));
}
