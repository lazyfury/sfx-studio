#!/usr/bin/env bash
# Mandatory quality gate. Must pass before every commit/push.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

step "cargo fmt --check"
cargo fmt --check

step "cargo clippy --all-targets -- -D warnings"
cargo clippy --all-targets -- -D warnings

step "cargo test"
cargo test

if [ "${SKIP_SELFCHECK:-0}" != "1" ]; then
    step "headless selfcheck"
    cargo run --quiet -- --selfcheck
fi

printf '\n\033[32mpreflight passed\033[0m\n'
