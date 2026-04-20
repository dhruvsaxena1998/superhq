# Remote-Access Demo Page

Validates the browser WASM build of `superhq-remote-client` against a real
`superhq-remote-host` instance running `StubHandler`.

## Requirements

- Rust toolchain with `wasm32-unknown-unknown` target
- `wasm-pack` (`cargo install wasm-pack`)
- On macOS: `brew install llvm` (needed for ring's C code at wasm32 target)
- Python 3 (for the static dev server) or any other static file server

## Run

```bash
# 1) Build the WASM bundle:
./build.sh

# 2) In one terminal, start the host:
cd ../../superhq-remote-host
cargo run --example demo-server

#    Copy the "EndpointId:" line it prints.

# 3) In another terminal, serve the demo page:
cd ../superhq-remote-client
python3 -m http.server 8000 --directory demo

# 4) Open http://localhost:8000/ in a browser.
#    Paste the server's EndpointId, click Connect, then exercise the RPC
#    buttons (session.hello, tabs.list, pty echo).
```

## What it exercises

- Browser → native iroh connection over relay (this is the headline)
- `session.hello` — protocol version negotiation + initial snapshot
- `tabs.list` — simple RPC request/response (empty list against StubHandler)
- PTY echo — attach via control stream, open a new bidirectional data
  stream, send bytes, receive them echoed back. Full stream lifecycle.

## What's deferred

Real PTY integration (pointing at an actual terminal) is host-app work —
this demo uses `StubHandler` which just echoes bytes. When the main SuperHQ
app implements a `RemoteHandler` that wires `pty_stream` to a real
`TerminalView`, the web UI built on top of this client will stream real
terminal output.
