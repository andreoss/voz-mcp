#!/usr/bin/env bash
set -euo pipefail

THRESHOLD="${COVERAGE_THRESHOLD:-85}"

TARGET_DIR="${TARPAULIN_TARGET_DIR:-$PWD/.tarpaulin-target}"

cargo tarpaulin \
	--lib \
	--target-dir "$TARGET_DIR" \
	--exclude-files 'src/main.rs' \
	--out Stdout \
	--fail-under "$THRESHOLD"
rm -f tarpaulin-report.*
