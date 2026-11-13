#!/usr/bin/env bash
# SHA-256 of every source file that requires an approved second review.
# docs/reviews/ is intentionally excluded so a review record can describe the
# exact code it just reviewed.
set -euo pipefail
cd "$(dirname "$0")/.."

if command -v sha256sum >/dev/null 2>&1; then
    hash_file() { sha256sum "$1"; }
else
    hash_file() { shasum -a 256 "$1"; }
fi

list="$(mktemp)"
trap 'rm -f "$list"' EXIT

find src scripts .githooks -type f \( -name '*.rs' -o -name '*.sh' \) -print >>"$list"
find .githooks -type f -print >>"$list" 2>/dev/null || true
printf '%s\n' Cargo.toml Cargo.lock >>"$list"
find plugins -type f -name '*.gd' -print >>"$list" 2>/dev/null || true

LC_ALL=C sort -u "$list" |
    while IFS= read -r file; do
        [ -f "$file" ] && hash_file "$file"
    done |
    hash_file /dev/stdin |
    awk '{print $1}'
