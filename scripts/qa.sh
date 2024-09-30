#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=C

if ! command -v cargo >/dev/null 2>&1; then
	GCC="$(find /nix/store -maxdepth 1 -type d -name '*gcc-wrapper*' ! -name '*.drv' | head -n1)"
	RUST="$(find /nix/store -maxdepth 1 -type d -name '*rustup*' ! -name '*.drv' | head -n1)"
	export PATH="$GCC/bin:$RUST/bin:$PATH"
fi

cd "$(dirname "$0")/.."

export TMPDIR="${TMPDIR:-$PWD/.tmp-test}"
mkdir -p "$TMPDIR"

cargo build

cargo test --lib

cargo test --test e2e -- --test-threads=1

cargo clippy --all-targets -- -D warnings

bash scripts/check_coverage.sh

if [ "${SKIP_SMOKE:-0}" = "1" ]; then
  exit 0
fi

cargo build --examples

BIN="$PWD/target/debug/voz"
SMOKE_DIR="$(mktemp -d "$TMPDIR/voz-qa-smoke-XXXXXX")"

cleanup() {
  kill "${SRV_PID_STABLE:-}" >/dev/null 2>&1 || true
  rm -rf "$SMOKE_DIR"
}
trap cleanup EXIT

coproc SRV { VOZ_OUT_DIR="$SMOKE_DIR" timeout 1200 "$BIN" mcp 2>/dev/null; }
SRV_PID_STABLE="$SRV_PID"

send() {
  printf '%s\n' "$1" >&"${SRV[1]}"
}

recv() {
  local deadline=$(( $(date +%s) + 300 ))
  while true; do
    IFS= read -r -t 1 -u "${SRV[0]}" REPLY || true
    if [ -n "${REPLY:-}" ]; then
      printf '%s' "$REPLY"
      return 0
    fi
    if ! kill -0 "$SRV_PID_STABLE" >/dev/null 2>&1; then
      echo "qa: server died; last line: ${REPLY:-<none>}" >&2
      return 1
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "qa: timed out waiting for server response" >&2
      return 1
    fi
  done
}

send '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"qa","version":"0.0.0"}}}'
INIT_LINE="$(recv)"

send '{"jsonrpc":"2.0","method":"notifications/initialized"}'

send '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
LIST_LINE="$(recv)"

send '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"speak","arguments":{"text":"smoke","lang":"en"}}}'
CALL_LINE="$(recv)"

send '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"speak","arguments":{"text":"smoke rate","lang":"en","rate":150}}}'
NEURAL_RATE_LINE="$(recv)"

send '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"speak","arguments":{"text":"smoke pitch","lang":"en","pitch":60}}}'
NEURAL_PITCH_LINE="$(recv)"

send '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"speak","arguments":{"text":"smoke lang","lang":"zz"}}}'
BAD_LANG_LINE="$(recv)"

send '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"readback","arguments":{}}}'
READBACK_LINE="$(recv)"

echo "$INIT_LINE" | grep -q '"serverInfo":{"name":"voz-mcp"'

echo "$CALL_LINE" | grep -q '"isError":false'

SPEECH_PATH="$(echo "$CALL_LINE" | grep -o '"path":"[^"]*"' | head -n1 | cut -d'"' -f4)"

[ -n "$SPEECH_PATH" ]
[ -f "$SPEECH_PATH" ]

HEADER="$(head -c 4 "$SPEECH_PATH")"
[ "$HEADER" = "RIFF" ]

echo "smoke ok: $SPEECH_PATH"

echo "$LIST_LINE" | grep -q '"name":"speak"'
echo "$LIST_LINE" | grep -q '"name":"readback"'

echo "tools/list ok: speak, readback"

echo "$NEURAL_RATE_LINE" | grep -F -q "rate is not supported by the neural backend"
echo "$NEURAL_PITCH_LINE" | grep -F -q "pitch is not supported by the neural backend"

echo "neural rate and pitch refusal ok"

SPEECH_SIZE="$(wc -c < "$SPEECH_PATH")"
[ "$SPEECH_SIZE" -gt 1000 ]

"$PWD/target/debug/examples/wav_check" "$SPEECH_PATH" >/dev/null

echo "wav depth ok: $SPEECH_PATH ($SPEECH_SIZE bytes)"

echo "$BAD_LANG_LINE" | grep -q '"code":-32602'
echo "$BAD_LANG_LINE" | grep -F -q 'expected ru|en|es|de|fr|it|pt|zh|ja|ko'

echo "-32602 ok: unsupported language rejected"

echo "$READBACK_LINE" | grep -q '"isError":false'
echo "$READBACK_LINE" | grep -F -q "$SPEECH_PATH"

echo "readback ok: $SPEECH_PATH listed"

CLI_OUT="$SMOKE_DIR/cli-smoke.wav"

timeout 600 "$PWD/target/debug/voz" "cli smoke" --lang en --out "$CLI_OUT"

[ -f "$CLI_OUT" ]

"$PWD/target/debug/examples/wav_check" "$CLI_OUT" >/dev/null

CLI_SIZE="$(wc -c < "$CLI_OUT")"
[ "$CLI_SIZE" -gt 1000 ]

echo "cli smoke ok: $CLI_OUT ($CLI_SIZE bytes)"

VOZ_ROOT_DEFAULT="${XDG_DATA_HOME:-$HOME/.local/share}/voz"
FALLBACK_BIN="${VOZ_PIPER_BIN:-${VOZ_NEURAL_ROOT:-$VOZ_ROOT_DEFAULT}/piper/bin/piper}"

if [ -x "$FALLBACK_BIN" ]; then
  FB_OUT="$SMOKE_DIR/fallback-en.wav"

  VOZ_BACKEND=fallback VOZ_PIPER_BIN="$FALLBACK_BIN" VOZ_OUT_DIR="$SMOKE_DIR" \
    timeout 600 "$BIN" "fallback smoke" --lang en --rate 200 --out "$FB_OUT"

  [ -f "$FB_OUT" ]

  "$PWD/target/debug/examples/wav_check" "$FB_OUT" >/dev/null

  FB_SIZE="$(wc -c < "$FB_OUT")"
  [ "$FB_SIZE" -gt 1000 ]

  echo "fallback smoke ok: $FB_OUT ($FB_SIZE bytes)"

  set +e
  FB_JA_ERR="$(VOZ_BACKEND=fallback VOZ_PIPER_BIN="$FALLBACK_BIN" VOZ_OUT_DIR="$SMOKE_DIR" \
    timeout 600 "$BIN" "fallback ja" --lang ja --out "$SMOKE_DIR/fallback-ja.wav" 2>&1 >/dev/null)"
  FB_JA_CODE=$?
  set -e

  [ "$FB_JA_CODE" -ne 0 ]
  echo "$FB_JA_ERR" | grep -F -q "voice for language ja"

  echo "fallback ja rejected ok"
else
  echo "fallback smoke skipped: no fallback runtime at $FALLBACK_BIN"
fi

set +e
BAD_BACKEND_ERR="$(VOZ_BACKEND=espeak VOZ_OUT_DIR="$SMOKE_DIR" "$BIN" "backend probe" 2>&1 >/dev/null)"
BAD_BACKEND_CODE=$?
set -e

[ "$BAD_BACKEND_CODE" -eq 2 ]
echo "$BAD_BACKEND_ERR" | grep -F -q 'auto|neural|fallback|null'

echo "backend override validation ok"

TIMEOUT_START="$(date +%s)"

set +e
TIMEOUT_ERR="$(VOZ_TIMEOUT_SECS=1 VOZ_OUT_DIR="$SMOKE_DIR" \
  timeout 300 "$BIN" "timeout probe" --lang en --out "$SMOKE_DIR/timeout.wav" 2>&1 >/dev/null)"
TIMEOUT_CODE=$?
set -e

TIMEOUT_ELAPSED=$(( $(date +%s) - TIMEOUT_START ))

[ "$TIMEOUT_CODE" -ne 0 ]
echo "$TIMEOUT_ERR" | grep -F -q "exceeded"
[ "$TIMEOUT_ELAPSED" -lt 60 ]

echo "bounded synthesis ok: engine killed after 1 s budget (${TIMEOUT_ELAPSED}s)"

set +e
BAD_TIMEOUT_ERR="$(VOZ_TIMEOUT_SECS=soon VOZ_OUT_DIR="$SMOKE_DIR" "$BIN" "timeout probe" 2>&1 >/dev/null)"
BAD_TIMEOUT_CODE=$?
set -e

[ "$BAD_TIMEOUT_CODE" -eq 2 ]
echo "$BAD_TIMEOUT_ERR" | grep -F -q "1..86400"

echo "timeout override validation ok"

if [ -x "$FALLBACK_BIN" ]; then
  PITCH_TEXT="the quick brown fox jumps over the lazy dog"
  for P in 10 90; do
    VOZ_BACKEND=fallback VOZ_PIPER_BIN="$FALLBACK_BIN" VOZ_OUT_DIR="$SMOKE_DIR" \
      timeout 600 "$BIN" "$PITCH_TEXT" --lang en --pitch "$P" --out "$SMOKE_DIR/pitch-$P.wav"
    "$PWD/target/debug/examples/wav_check" "$SMOKE_DIR/pitch-$P.wav" >/dev/null
  done

  ZCR_LOW="$("$PWD/target/debug/examples/wav_check" "$SMOKE_DIR/pitch-10.wav" | awk '{print $8}')"
  ZCR_HIGH="$("$PWD/target/debug/examples/wav_check" "$SMOKE_DIR/pitch-90.wav" | awk '{print $8}')"

  [ "$ZCR_HIGH" -gt "$ZCR_LOW" ]

  echo "fallback pitch ok: zcr $ZCR_LOW -> $ZCR_HIGH"
fi

if [ -x "$FALLBACK_BIN" ]; then
  RATE_TEXT="the quick brown fox jumps over the lazy dog"
  for R in 85 340; do
    VOZ_BACKEND=fallback VOZ_PIPER_BIN="$FALLBACK_BIN" VOZ_OUT_DIR="$SMOKE_DIR" \
      timeout 600 "$BIN" "$RATE_TEXT" --lang en --rate "$R" --out "$SMOKE_DIR/rate-$R.wav"
    "$PWD/target/debug/examples/wav_check" "$SMOKE_DIR/rate-$R.wav" >/dev/null
  done

  DUR_SLOW="$("$PWD/target/debug/examples/wav_check" "$SMOKE_DIR/rate-85.wav" | awk '{print $2}' | tr -d 's')"
  DUR_FAST="$("$PWD/target/debug/examples/wav_check" "$SMOKE_DIR/rate-340.wav" | awk '{print $2}' | tr -d 's')"

  awk -v s="$DUR_SLOW" -v f="$DUR_FAST" 'BEGIN { exit !(s > f * 1.5) }'

  echo "fallback rate ok: ${DUR_SLOW}s at 85 vs ${DUR_FAST}s at 340"
fi
