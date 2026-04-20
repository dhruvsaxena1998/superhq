import init, { ClientHandle, DeviceCredential } from "../pkg/superhq_remote_client.js";
import {
    describeCredential,
    loadCredential,
    saveCredential,
    clearCredential,
} from "./storage.js?v=4";

const logEl = document.getElementById("log");
const myIdEl = document.getElementById("myId");
const peerEl = document.getElementById("peer");
const connectBtn = document.getElementById("connectBtn");
const pairBtn = document.getElementById("pairBtn");
const forgetBtn = document.getElementById("forgetBtn");
const credBanner = document.getElementById("credBanner");
const helloBtn = document.getElementById("helloBtn");
const workspacesBtn = document.getElementById("workspacesBtn");
const tabsBtn = document.getElementById("tabsBtn");
const closeBtn = document.getElementById("closeBtn");
const tabsPanel = document.getElementById("tabsPanel");
const termShell = document.getElementById("termShell");
const termLabel = document.getElementById("termLabel");
const termCloseBtn = document.getElementById("termCloseBtn");

function log(msg, cls) {
    const t = new Date().toISOString().split("T")[1].replace("Z", "");
    const line = document.createElement("div");
    line.textContent = `[${t}] ${msg}`;
    if (cls) line.className = cls;
    logEl.appendChild(line);
    logEl.scrollTop = logEl.scrollHeight;
}

let client = null;
let term = null;
let fitAddon = null;
let ptyHandle = null;
let ptyReadStopFlag = false;

async function boot() {
    log("main.js v8 loaded — WebAuthn-PRF credential storage (cached per tab)", "info");
    log("loading wasm...", "info");
    const t0 = performance.now();
    await init();
    log(`wasm loaded in ${(performance.now() - t0).toFixed(0)}ms`, "info");
    myIdEl.textContent = "(not connected — click Connect)";
    connectBtn.disabled = false;
    await refreshCredBanner();
}

function setConnected(isConnected) {
    connectBtn.disabled = isConnected;
    pairBtn.disabled = !isConnected;
    helloBtn.disabled = !isConnected;
    workspacesBtn.disabled = !isConnected;
    tabsBtn.disabled = !isConnected;
    closeBtn.disabled = !isConnected;
    peerEl.disabled = isConnected;
    refreshCredBanner().catch(() => {});
    if (!isConnected) {
        renderTabs([]);
        closeTerminal();
    }
}

async function refreshCredBanner() {
    const peer = peerEl.value.trim();
    if (!peer) {
        credBanner.textContent = "";
        forgetBtn.disabled = true;
        return;
    }
    const info = await describeCredential(peer);
    if (info) {
        credBanner.textContent = `✓ paired as ${info.device_id.slice(0, 16)}… — 🔒 WebAuthn-protected`;
        credBanner.style.color = "#7f7";
        forgetBtn.disabled = false;
    } else {
        credBanner.textContent = "not paired — click \"Pair device\" after connecting";
        credBanner.style.color = "#888";
        forgetBtn.disabled = true;
    }
}

peerEl.addEventListener("input", () => {
    refreshCredBanner().catch(() => {});
});

let lastWorkspaces = [];

function renderWorkspacesAndTabs(workspaces, tabs) {
    lastWorkspaces = workspaces;
    tabsPanel.innerHTML = "";

    // Group tabs by workspace.
    const tabsByWs = new Map();
    for (const tab of tabs) {
        const arr = tabsByWs.get(tab.workspace_id) || [];
        arr.push(tab);
        tabsByWs.set(tab.workspace_id, arr);
    }

    for (const ws of workspaces) {
        const header = document.createElement("div");
        header.className = "tab-card";
        header.style.background = ws.is_active ? "#1f2b20" : "#222";
        const h = document.createElement("div");
        h.className = "tab-title";
        h.textContent = ws.label;
        const m = document.createElement("div");
        m.className = "tab-meta";
        m.textContent = `ws=${ws.workspace_id} · ${ws.is_active ? "active" : "stopped"}`;
        header.appendChild(h);
        header.appendChild(m);
        tabsPanel.appendChild(header);

        const wsTabs = tabsByWs.get(ws.workspace_id) || [];
        for (const tab of wsTabs) {
            const card = document.createElement("div");
            card.className = "tab-card";
            card.style.marginLeft = "24px";
            const title = document.createElement("div");
            title.className = "tab-title";
            title.textContent = tab.label;
            const meta = document.createElement("div");
            meta.className = "tab-meta";
            const state = tab.agent_state.state;
            meta.textContent = `tab=${tab.tab_id} · ${tab.kind} · ${state}`;
            const btn = document.createElement("button");
            btn.className = "secondary";
            btn.textContent = "Open terminal";
            btn.addEventListener("click", () => openTerminal(tab));
            card.appendChild(title);
            card.appendChild(meta);
            card.appendChild(btn);
            tabsPanel.appendChild(card);
        }
    }
}

function renderTabs(tabs) {
    // Keep existing workspaces; just refresh tabs.
    renderWorkspacesAndTabs(lastWorkspaces, tabs);
}

function closeTerminal() {
    ptyReadStopFlag = true;
    if (ptyHandle) {
        try { ptyHandle.free?.(); } catch (_) {}
        ptyHandle = null;
    }
    if (term) {
        try { term.dispose(); } catch (_) {}
        term = null;
        fitAddon = null;
    }
    termShell.classList.remove("active");
    termLabel.textContent = "— no terminal —";
}

async function openTerminal(tab) {
    closeTerminal();
    ptyReadStopFlag = false;

    term = new window.Terminal({
        fontFamily: "ui-monospace, Menlo, monospace",
        fontSize: 13,
        theme: { background: "#000" },
        convertEol: true,
    });
    fitAddon = new window.FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    term.open(document.getElementById("term"));
    fitAddon.fit();
    const { cols, rows } = term;

    termShell.classList.add("active");
    termLabel.textContent = `${tab.label} — ws=${tab.workspace_id} tab=${tab.tab_id} — ${cols}×${rows}`;

    try {
        log(`opening pty stream: ${tab.label} (${cols}×${rows})...`, "info");
        const t0 = performance.now();
        ptyHandle = await client.open_pty(
            BigInt(tab.workspace_id),
            BigInt(tab.tab_id),
            cols,
            rows,
        );
        log(`pty attached in ${(performance.now() - t0).toFixed(0)}ms`, "ok");
    } catch (err) {
        log(`open_pty ERROR: ${err.message || err}`, "err");
        closeTerminal();
        return;
    }

    // Input: keystrokes → PTY write
    term.onData((data) => {
        if (!ptyHandle) return;
        const bytes = new TextEncoder().encode(data);
        ptyHandle.write(bytes).catch((err) => {
            log(`pty write ERROR: ${err.message || err}`, "err");
        });
    });

    // Resize: window/term resize → host-side pty.resize
    term.onResize(({ cols, rows }) => {
        termLabel.textContent = `${tab.label} — ws=${tab.workspace_id} tab=${tab.tab_id} — ${cols}×${rows}`;
        if (!ptyHandle) return;
        ptyHandle.resize(cols, rows).catch((err) => {
            log(`pty resize ERROR: ${err.message || err}`, "err");
        });
    });

    // Output: read loop, each chunk written into xterm
    (async () => {
        const handle = ptyHandle;
        while (!ptyReadStopFlag && handle === ptyHandle) {
            try {
                const chunk = await handle.read_chunk();
                if (!chunk.length) {
                    log("pty stream closed by host", "info");
                    break;
                }
                term.write(chunk);
            } catch (err) {
                log(`pty read ERROR: ${err.message || err}`, "err");
                break;
            }
        }
    })();

    // Fit on window resize too
    window.addEventListener("resize", handleWindowResize);
}

function handleWindowResize() {
    if (fitAddon) fitAddon.fit();
}

termCloseBtn.addEventListener("click", () => {
    window.removeEventListener("resize", handleWindowResize);
    closeTerminal();
    log("terminal closed", "info");
});

async function establishSession(peer) {
    log("establishSession: looking up stored credential...", "info");
    let cred;
    try {
        cred = await loadCredential(peer);
    } catch (err) {
        log(`credential unlock ERROR: ${err.message || err}`, "err");
        return false;
    }
    const label = `browser-${navigator.platform}`;
    if (!cred) {
        log("no stored credential — use \"Pair device\" to authenticate", "info");
        return false;
    }
    log(`establishSession: found ${cred.device_id.slice(0,16)}..., calling session.hello`, "info");
    try {
        const credHandle = new DeviceCredential(cred.device_id, cred.device_key);
        const t0 = performance.now();
        const json = await client.session_hello_auth(label, credHandle);
        const rtt = performance.now() - t0;
        const parsed = JSON.parse(json);
        const activeCount = parsed.workspaces.filter(w => w.is_active).length;
        log(`session established (${rtt.toFixed(0)}ms): ${parsed.workspaces.length} workspace(s), ${activeCount} active, ${parsed.tabs.length} tab(s)`, "ok");
        renderWorkspacesAndTabs(parsed.workspaces, parsed.tabs);
        return true;
    } catch (err) {
        const msg = err.message || String(err);
        log(`session.hello ERROR: ${msg}`, "err");
        if (msg.includes("1005") || msg.includes("AUTH_INVALID") || msg.includes("auth_invalid")) {
            // Stored credential no longer valid on the host — prompt re-pair.
            log("stored credential rejected by host — try \"Pair device\" again", "info");
        }
        return false;
    }
}

connectBtn.addEventListener("click", async () => {
    const peer = peerEl.value.trim();
    if (!peer) {
        log("ERROR: paste a peer endpoint id first", "err");
        return;
    }
    connectBtn.disabled = true;
    try {
        log(`connecting to ${peer.slice(0, 16)}...`, "info");
        const t0 = performance.now();
        client = await ClientHandle.connect(peer);
        log(`connected in ${(performance.now() - t0).toFixed(0)}ms`, "ok");
        myIdEl.textContent = client.endpoint_id();
        setConnected(true);
        // Auto-authenticate if we already have a credential for this peer.
        await establishSession(peer);
    } catch (err) {
        log(`ERROR: ${err.message || err}`, "err");
        connectBtn.disabled = false;
    }
});

helloBtn.addEventListener("click", async () => {
    try {
        const peer = peerEl.value.trim();
        let cred = null;
        try {
            cred = await loadCredential(peer);
        } catch (err) {
            log(`credential unlock ERROR: ${err.message || err}`, "err");
            return;
        }
        const t0 = performance.now();
        let json;
        if (cred) {
            const credHandle = new DeviceCredential(cred.device_id, cred.device_key);
            json = await client.session_hello_auth(
                `browser-${navigator.platform}`,
                credHandle,
            );
        } else {
            json = await client.session_hello(`browser-${navigator.platform}`);
        }
        const rtt = performance.now() - t0;
        const parsed = JSON.parse(json);
        const activeCount = parsed.workspaces.filter(w => w.is_active).length;
        const authNote = cred ? " (authed)" : " (no auth)";
        log(`session.hello${authNote} (${rtt.toFixed(0)}ms): session_id=${parsed.session_id.slice(0,8)}..., workspaces=${parsed.workspaces.length} (${activeCount} active), tabs=${parsed.tabs.length}`, "ok");
        renderWorkspacesAndTabs(parsed.workspaces, parsed.tabs);
    } catch (err) {
        log(`session.hello ERROR: ${err.message || err}`, "err");
    }
});

pairBtn.addEventListener("click", async () => {
    if (!client) return;
    const peer = peerEl.value.trim();
    try {
        const label = `browser-${navigator.platform}`;
        log(`pairing.request (${label})...`, "info");
        const t0 = performance.now();
        const cred = await client.pairing_request(label);
        const rtt = performance.now() - t0;
        log(`paired in ${rtt.toFixed(0)}ms: ${cred.device_id.slice(0,16)}... — protecting with WebAuthn…`, "ok");
        try {
            await saveCredential(peer, {
                device_id: cred.device_id,
                device_key: cred.device_key,
            });
        } catch (err) {
            log(`credential protect ERROR: ${err.message || err}`, "err");
            log("pairing succeeded on host but the browser could not wrap the key — revoke the device on the host and try again", "err");
            return;
        }
        log("credential wrapped 🔒", "ok");
        await refreshCredBanner();
        // Immediately establish the session with the new credential so
        // subsequent calls don't hit AUTH_REQUIRED.
        await establishSession(peer);
    } catch (err) {
        log(`pairing.request ERROR: ${err.message || err}`, "err");
    }
});

forgetBtn.addEventListener("click", async () => {
    const peer = peerEl.value.trim();
    if (!peer) return;
    await clearCredential(peer);
    await refreshCredBanner();
    log("credential forgotten (not revoked on host)", "info");
});

async function refreshAll() {
    try {
        const t0 = performance.now();
        const [wsJson, tabsJson] = await Promise.all([
            client.workspaces_list(),
            client.tabs_list(),
        ]);
        const rtt = performance.now() - t0;
        const workspaces = JSON.parse(wsJson);
        const tabs = JSON.parse(tabsJson);
        const activeCount = workspaces.filter(w => w.is_active).length;
        log(`refresh (${rtt.toFixed(0)}ms): ${workspaces.length} workspace(s), ${activeCount} active, ${tabs.length} tab(s)`, "ok");
        renderWorkspacesAndTabs(workspaces, tabs);
    } catch (err) {
        log(`refresh ERROR: ${err.message || err}`, "err");
    }
}

workspacesBtn.addEventListener("click", refreshAll);
tabsBtn.addEventListener("click", refreshAll);

closeBtn.addEventListener("click", () => {
    try {
        client.close();
        log("connection closed", "info");
    } catch (err) {
        log(`close ERROR: ${err.message || err}`, "err");
    } finally {
        client = null;
        setConnected(false);
        myIdEl.textContent = "(not connected)";
    }
});

boot().catch((err) => log(`FATAL: ${err.message || err}`, "err"));
