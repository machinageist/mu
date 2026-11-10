#!/bin/sh
# Author: machinageist
# Date: 2026-11-09
# Description: Fail when CHANGELOG.md changed since BASE but nothing under docs/wiki/ did
# Notes: Usage: scripts/check-docs.sh [BASE]. BASE defaults to the latest v* tag. Counts commits
# from BASE to HEAD plus uncommitted and untracked files, so it works before committing too.
# Exit 0 when the docs kept up, 1 when they did not, 2 when BASE cannot be resolved

set -eu

CHANGELOG=CHANGELOG.md
WIKI_DIR=docs/wiki/

cd "$(git rev-parse --show-toplevel)"

# Latest release tag unless a base is given
if [ $# -gt 0 ]; then
    base=$1
else
    base=$(git describe --tags --abbrev=0 --match 'v*') || {
        echo "check-docs: no v* tag found; pass a BASE" >&2
        exit 2
    }
fi
if ! git rev-parse --verify --quiet "$base^{commit}" >/dev/null; then
    echo "check-docs: unknown BASE: $base" >&2
    exit 2
fi

# Files changed in commits since BASE, in the index or working tree, or new and untracked
changed=$(
    git diff --name-only "$base" HEAD
    git diff --name-only HEAD
    git ls-files --others --exclude-standard
)

if ! printf '%s\n' "$changed" | grep -qx "$CHANGELOG"; then
    echo "check-docs: $CHANGELOG unchanged since $base, nothing to check"
    exit 0
fi
if printf '%s\n' "$changed" | grep -q "^$WIKI_DIR"; then
    echo "check-docs: $CHANGELOG and $WIKI_DIR both changed since $base"
    exit 0
fi

echo "check-docs: $CHANGELOG changed since $base but nothing under $WIKI_DIR did." >&2
echo "Update the wiki pages for the user-visible change before releasing." >&2
exit 1
