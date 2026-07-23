#!/usr/bin/env bash
# Assemble a relocatable Uhura directory package. Node and pnpm are build-time
# dependencies only; the packaged native host serves the copied application
# and wasm-bindgen web bundle directly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
CALLER_PWD="$(pwd -P)"
OUT_ARG="${1:-$ROOT/dist/uhura}"

for tool in cargo corepack; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "package: required build command not found: $tool" >&2
    exit 2
  fi
done

# Interpret an explicit relative destination from the caller's directory, not
# from the repository directory that the build commands use below.
case "$OUT_ARG" in
  /*) OUT_INPUT="$OUT_ARG" ;;
  *) OUT_INPUT="$CALLER_PWD/$OUT_ARG" ;;
esac
while [[ "$OUT_INPUT" != "/" && "$OUT_INPUT" == */ ]]; do
  OUT_INPUT="${OUT_INPUT%/}"
done
OUT_NAME="$(basename "$OUT_INPUT")"
OUT_PARENT_INPUT="$(dirname "$OUT_INPUT")"
if [[ -z "$OUT_NAME" || "$OUT_NAME" == "/" || "$OUT_NAME" == "." || "$OUT_NAME" == ".." ]]; then
  echo "package: refusing unsafe output directory: $OUT_ARG" >&2
  exit 2
fi
mkdir -p "$OUT_PARENT_INPUT"
OUT_PARENT="$(cd "$OUT_PARENT_INPUT" && pwd -P)"
OUT="$OUT_PARENT/$OUT_NAME"
case "$OUT" in
  /|"$ROOT")
    echo "package: refusing unsafe output directory: $OUT" >&2
    exit 2
    ;;
  "$ROOT"/*)
    case "$OUT" in
      "$ROOT/dist"|"$ROOT/dist/"*) ;;
      *)
        echo "package: refusing unsafe output directory: $OUT" >&2
        exit 2
        ;;
    esac
    ;;
esac

cd "$ROOT"
(cd web && corepack pnpm build)
scripts/build-wasm.sh
cargo build --locked --release -p uhura-cli

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$TARGET_DIR" in
  /*) ;;
  *) TARGET_DIR="$ROOT/$TARGET_DIR" ;;
esac

required_files=(
  "$TARGET_DIR/release/uhura"
  "$ROOT/web/dist/index.html"
  "$ROOT/web/dist/uhura-web-build.json"
  "$ROOT/web/dist-export/index.html"
  "$ROOT/web/dist-export/uhura-web-build.json"
  "$ROOT/crates/uhura-wasm/pkg/web/uhura_wasm.js"
  "$ROOT/crates/uhura-wasm/pkg/web/uhura_wasm_bg.wasm"
)
for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "package: expected build output is missing: $file" >&2
    exit 1
  fi
done

STAGING="$(mktemp -d "$OUT_PARENT/.${OUT_NAME}.tmp.XXXXXX")"
cleanup() {
  if [[ -n "${STAGING:-}" && -d "$STAGING" ]]; then
    rm -rf "$STAGING"
  fi
}
trap cleanup EXIT

mkdir -p \
  "$STAGING/bin" \
  "$STAGING/share/uhura/web" \
  "$STAGING/share/uhura/web-export" \
  "$STAGING/share/uhura/wasm"
install -m 755 "$TARGET_DIR/release/uhura" "$STAGING/bin/uhura"
cp -R "$ROOT/web/dist/." "$STAGING/share/uhura/web/"
cp -R "$ROOT/web/dist-export/." "$STAGING/share/uhura/web-export/"
cp -R "$ROOT/crates/uhura-wasm/pkg/web/." "$STAGING/share/uhura/wasm/"

# Replace only after the full package has been assembled. The destination was
# normalized and guarded above, so cleanup cannot target a source directory.
rm -rf "$OUT"
mv "$STAGING" "$OUT"
STAGING=""

echo "Uhura package: $OUT"
echo "Run: $OUT/bin/uhura editor <project>"
echo "Export: $OUT/bin/uhura export <project> --out <directory>"
