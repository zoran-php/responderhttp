#!/usr/bin/env bash
# tools/flatpak-sources.sh
#
# Regenerates flatpak/cargo-sources.json and flatpak/node-sources.json from
# src-tauri/Cargo.lock and pnpm-lock.yaml (PLAN-LINUX.md 17e). Flathub builds
# with no network access, so every crate and npm package has to be listed in
# the manifest in advance; run this whenever either lockfile changes, and
# commit the two files with it.
#
# The generators come from flatpak-builder-tools at a pinned commit and run in
# a virtualenv under src-tauri/target/flatpak/, so nothing is installed
# system-wide and a later upstream change cannot alter the output unnoticed.
set -euo pipefail
cd "$(dirname "$0")/.."

TOOLS_COMMIT=74697c75b630d7330e77250fc13cb5ea688d9479
WORK=src-tauri/target/flatpak/tools
TOOLS="$WORK/flatpak-builder-tools"
VENV="$WORK/venv"

if [ ! -d "$TOOLS/.git" ]; then
    git clone -q https://github.com/flatpak/flatpak-builder-tools.git "$TOOLS"
fi
git -C "$TOOLS" fetch -q origin "$TOOLS_COMMIT"
git -C "$TOOLS" checkout -q "$TOOLS_COMMIT"

if [ ! -x "$VENV/bin/flatpak-node-generator" ]; then
    python3 -m venv "$VENV"
    "$VENV/bin/pip" install -q "$TOOLS/node" aiohttp tomlkit
fi

"$VENV/bin/python3" "$TOOLS/cargo/flatpak-cargo-generator.py" \
    src-tauri/Cargo.lock -o flatpak/cargo-sources.json
# pnpm 12 keeps its store in format v11 (`pnpm store path` ends in /v11).
"$VENV/bin/flatpak-node-generator" pnpm pnpm-lock.yaml \
    --pnpm-store-version v11 -o flatpak/node-sources.json >/dev/null

echo "flatpak/cargo-sources.json: $(grep -c '"type": "archive"' flatpak/cargo-sources.json) crates"
echo "flatpak/node-sources.json: $(grep -c '"type": "file"' flatpak/node-sources.json) npm packages"
