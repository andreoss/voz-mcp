#!/usr/bin/env bash
set -euo pipefail

THRESHOLD="${COVERAGE_THRESHOLD:-85}"

cargo tarpaulin --lib --exclude-files 'src/main.rs' --out Stdout --fail-under "$THRESHOLD"
rm -f tarpaulin-report.*
