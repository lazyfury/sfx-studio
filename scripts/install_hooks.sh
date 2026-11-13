#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

chmod +x scripts/*.sh .githooks/*
git config core.hooksPath .githooks
echo "installed git hooks: core.hooksPath=.githooks"
echo "  pre-commit: ./scripts/preflight.sh"
echo "  pre-push:   preflight + ./scripts/review_gate.sh"
echo
echo "a review record is required for push: docs/reviews/NNNN-*.md"
echo "see docs/DEV_WORKFLOW.md"
