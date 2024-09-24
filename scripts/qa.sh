#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
	GCC="$(find /nix/store -maxdepth 1 -type d -name '*gcc-wrapper*' ! -name '*.drv' | head -n1)"
	RUST="$(find /nix/store -maxdepth 1 -type d -name '*rustup*' ! -name '*.drv' | head -n1)"
	export PATH="$GCC/bin:$RUST/bin:$PATH"
fi

cd "$(dirname "$0")/.."

export TMPDIR="${TMPDIR:-$PWD/.tmp-test}"
mkdir -p "$TMPDIR"

cargo build

cargo test

cargo clippy --all-targets -- -D warnings

bash scripts/check_coverage.sh

if [ "${SKIP_SMOKE:-0}" = "1" ]; then
  exit 0
fi

BIN="$PWD/target/debug/voz-mcp"
SMOKE_DIR="$(mktemp -d "$TMPDIR/voz-qa-smoke-XXXXXX")"
OUT_FILE="$(mktemp "$TMPDIR/voz-qa-smoke-out-XXXXXX")"

cleanup() {
  rm -rf "$SMOKE_DIR" "$OUT_FILE"
}
trap cleanup EXIT

{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"qa","version":"0.0.0"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"speak","arguments":{"text":"smoke","lang":"en"}}}'
} | VOZ_OUT_DIR="$SMOKE_DIR" timeout 15 "$BIN" >"$OUT_FILE" 2>/dev/null

INIT_LINE="$(sed -n '1p' "$OUT_FILE")"
CALL_LINE="$(sed -n '2p' "$OUT_FILE")"

echo "$INIT_LINE" | grep -q '"serverInfo":{"name":"voz-mcp"'

echo "$CALL_LINE" | grep -q '"isError":false'

SPEECH_PATH="$(echo "$CALL_LINE" | grep -o '"path":"[^"]*"' | head -n1 | cut -d'"' -f4)"

[ -n "$SPEECH_PATH" ]
[ -f "$SPEECH_PATH" ]

HEADER="$(head -c 4 "$SPEECH_PATH")"
[ "$HEADER" = "RIFF" ]

echo "smoke ok: $SPEECH_PATH"
