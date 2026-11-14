#!/usr/bin/env bash
# Rust ↔ Godot golden parity for deterministic waveforms.
# Usage: ./scripts/parity.sh [out-dir]   (GODOT=/path/to/godot to override)
set -euo pipefail
cd "$(dirname "$0")/.."

out="${1:-target/parity}"
godot="${GODOT:-/Applications/Godot.app/Contents/MacOS/Godot}"
if [ ! -x "$godot" ]; then
    echo "Godot not found (set GODOT=/path/to/godot)" >&2
    exit 2
fi

rm -rf "$out"
mkdir -p "$out/project/addons" "$out/fixtures" "$out/rust" "$out/godot"
ln -s "$PWD/plugins/sfx_studio" "$out/project/addons/sfx_studio"
cp scripts/parity/render.gd "$out/project/render.gd"
cat > "$out/project/project.godot" <<'PROJECT'
config_version=5

[application]
config/name="sfx-parity"

[editor_plugins]
enabled=PackedStringArray("res://addons/sfx_studio/plugin.cfg")
PROJECT

echo "== fixtures =="
cargo run --quiet -- --parity-fixtures "$out"

echo "== godot import =="
# Godot needs one editor pass to register the plugin's global classes.
if ! "$godot" --headless --path "$out/project" --editor --quit >/dev/null 2>&1; then
    echo "godot import failed" >&2
    exit 1
fi

echo "== godot render =="
"$godot" --headless --path "$out/project" --script res://render.gd -- \
    "$PWD/$out/fixtures" "$PWD/$out/godot"

echo "== compare =="
cargo run --quiet -- --parity-check "$out"
