#!/usr/bin/env bash
# Build and launch SFX LAB from the host project.
set -euo pipefail
cd "$(dirname "$0")"
exec cargo run --release -- "$@"
