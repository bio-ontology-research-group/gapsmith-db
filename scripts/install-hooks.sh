#!/usr/bin/env bash
# Install repo-local git hooks.
set -euo pipefail
repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
ln -sf ../../scripts/pre-commit .git/hooks/pre-commit
chmod +x scripts/pre-commit
echo "installed .git/hooks/pre-commit -> ../../scripts/pre-commit"
