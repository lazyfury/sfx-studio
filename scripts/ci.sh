#!/usr/bin/env bash
# Local/shared CI entry point. Requires the sibling igui checkout at ../igui.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f ../igui/Cargo.toml ]; then
    echo "error: missing sibling checkout ../igui" >&2
    echo "clone the igui repository next to sfx-studio (or set up the symlink) and retry" >&2
    exit 1
fi

./scripts/preflight.sh
./scripts/review_gate.sh
