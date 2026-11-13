#!/usr/bin/env bash
# Fails unless docs/reviews/ contains an approved record matching the current
# source tree hash. This is what makes the "second review" mandatory.
set -euo pipefail
cd "$(dirname "$0")/.."

hash="$("$PWD/scripts/source_hash.sh")"
echo "source_hash: $hash"

shopt -s nullglob
records=(docs/reviews/*.md)
shopt -u nullglob

if [ "${#records[@]}" -eq 0 ]; then
    echo "no review records in docs/reviews/" >&2
    echo "run ./scripts/review_gate.sh after writing docs/reviews/NNNN-*.md" >&2
    exit 1
fi

for record in "${records[@]}"; do
    status="$(sed -n 's/^-[[:space:]]*status:[[:space:]]*//p' "$record" | head -1 | tr -d '[:space:]')"
    source="$(sed -n 's/^-[[:space:]]*source_hash:[[:space:]]*//p' "$record" | head -1 | tr -d '[:space:]')"
    reviewer1="$(sed -n 's/^-[[:space:]]*reviewer_1:[[:space:]]*//p' "$record" | head -1)"
    reviewer2="$(sed -n 's/^-[[:space:]]*reviewer_2:[[:space:]]*//p' "$record" | head -1)"
    if [ "$status" = "approved" ] && [ "$source" = "$hash" ] &&
        [ -n "$reviewer1" ] && [ -n "$reviewer2" ]; then
        echo "approved: $record"
        exit 0
    fi
done

cat >&2 <<EOF
review gate failed: no approved record matches the current source hash.
  expected source_hash: $hash
  required fields: status: approved, reviewer_1, reviewer_2, source_hash
  write/update: docs/reviews/NNNN-<slug>.md (see docs/DEV_WORKFLOW.md)
EOF
exit 1
