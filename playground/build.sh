#!/usr/bin/env bash
# Build the Aether playground — compiles aether-wasm to WebAssembly via wasm-pack,
# outputs the JS/wasm bundle into playground/pkg/.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

export PATH="$HOME/.cargo/bin:$PATH"

# Install wasm-pack if not present.
if ! command -v wasm-pack &>/dev/null; then
    echo "[playground] wasm-pack not found — installing (this may take a minute)..."
    cargo install wasm-pack --locked
fi

echo "[playground] Building aether-wasm → wasm32-unknown-unknown..."
wasm-pack build \
    --target web \
    --out-dir "$SCRIPT_DIR/pkg" \
    "$REPO_ROOT/crates/aether-wasm"

echo ""
echo "[playground] Done!"
echo "  Open playground/index.html in a browser (needs a static server)."
echo "  Quick start:"
echo "    python3 -m http.server 8080 -d \"$SCRIPT_DIR\""
echo "  Then visit:  http://localhost:8080"
