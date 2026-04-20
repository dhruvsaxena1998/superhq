#!/usr/bin/env bash
# Build the WASM bundle for the demo page.
set -euo pipefail
cd "$(dirname "$0")/.."

# Point wasm32 builds at brew's LLVM (macOS default clang lacks wasm32 target).
if [ -x "/opt/homebrew/opt/llvm/bin/clang" ]; then
    export CC_wasm32_unknown_unknown="/opt/homebrew/opt/llvm/bin/clang"
    export AR_wasm32_unknown_unknown="/opt/homebrew/opt/llvm/bin/llvm-ar"
fi

echo "=> Building WASM..."
wasm-pack build --target web --out-dir pkg --release --no-default-features

echo ""
echo "WASM bundle sizes:"
ls -lah pkg/*.wasm | awk '{print "  " $5 "  " $9}'

echo ""
echo "=> Done."
echo "Next:"
echo "  1) Run the server:"
echo "     cd ../superhq-remote-host && cargo run --example demo-server"
echo ""
echo "  2) Serve the demo page:"
echo "     python3 -m http.server 8000 --directory demo"
echo ""
echo "  3) Open http://localhost:8000/"
