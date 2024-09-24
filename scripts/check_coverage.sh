#!/usr/bin/env bash
set -euo pipefail

THRESHOLD="${COVERAGE_THRESHOLD:-85}"

cargo tarpaulin --exclude-files 'src/main.rs' --out Stdout --fail-under "$THRESHOLD"
