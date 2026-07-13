#!/usr/bin/env bash
# Builds the uhura-wasm bundle for both targets (design §12.3):
#   crates/uhura-wasm/pkg/web/   — ES module for the play shell
#   crates/uhura-wasm/pkg/node/  — CommonJS for scripts/parity.mjs
#
# wasm-bindgen-cli MUST match the workspace's wasm-bindgen pin exactly
# (Cargo.lock) — the CLI and the crate write two halves of one ABI.
set -euo pipefail
cd "$(dirname "$0")/.."

WBG_VERSION="$(grep -A1 'name = "wasm-bindgen"' Cargo.lock | grep version | head -1 | cut -d'"' -f2)"

WBG="${WASM_BINDGEN:-}"
if [ -z "$WBG" ]; then
  if [ -x "target/tools/bin/wasm-bindgen" ]; then
    WBG="target/tools/bin/wasm-bindgen"
  elif command -v wasm-bindgen >/dev/null 2>&1; then
    WBG="$(command -v wasm-bindgen)"
  else
    echo "wasm-bindgen-cli $WBG_VERSION is required. Install it project-local:" >&2
    echo "  cargo install wasm-bindgen-cli --version $WBG_VERSION --locked --root target/tools" >&2
    exit 2
  fi
fi

ACTUAL="$("$WBG" --version | awk '{print $2}')"
if [ "$ACTUAL" != "$WBG_VERSION" ]; then
  echo "wasm-bindgen-cli is $ACTUAL but the workspace pins $WBG_VERSION (Cargo.lock)" >&2
  echo "  cargo install wasm-bindgen-cli --version $WBG_VERSION --locked --root target/tools" >&2
  exit 2
fi

cargo build --locked -p uhura-wasm --target wasm32-unknown-unknown --release

WASM="target/wasm32-unknown-unknown/release/uhura_wasm.wasm"
rm -rf crates/uhura-wasm/pkg
"$WBG" --target web    --out-dir crates/uhura-wasm/pkg/web  --no-typescript "$WASM"
"$WBG" --target nodejs --out-dir crates/uhura-wasm/pkg/node --no-typescript "$WASM"

# The repo root's package.json says `"type": "module"`; pin each bundle's
# own module kind so node never misparses the CommonJS output.
printf '{ "type": "commonjs" }\n' > crates/uhura-wasm/pkg/node/package.json
printf '{ "type": "module" }\n'   > crates/uhura-wasm/pkg/web/package.json

echo "built pkg/web + pkg/node (wasm-bindgen $WBG_VERSION)"
