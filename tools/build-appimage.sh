#!/usr/bin/env bash
# tools/build-appimage.sh
#
# Builds the AppImage inside an Ubuntu 22.04 container (tools/appimage/
# Containerfile says why that base), from a copy of this working tree, and
# puts it in src-tauri/target/appimage/. The Fedora build in
# src-tauri/target/release is not touched: its binary needs a newer glibc and
# must never end up inside the AppImage.
#
# The Cargo registry, the Cargo target folder and the pnpm store are kept in
# src-tauri/target/appimage-cache/ between runs, so only the first build is
# slow. Needs podman.
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=responderhttp-appimage-builder:ubuntu22.04
OUT=src-tauri/target/appimage
CACHE=src-tauri/target/appimage-cache

podman build -t "$IMAGE" tools/appimage
mkdir -p "$OUT" "$CACHE/cargo-registry" "$CACHE/target" "$CACHE/pnpm-store"
rm -f "$OUT"/*.AppImage

podman run --rm --security-opt label=disable \
    -v "$PWD:/src:ro" \
    -v "$PWD/$OUT:/out" \
    -v "$PWD/$CACHE/cargo-registry:/root/.cargo/registry" \
    -v "$PWD/$CACHE/target:/cache/target" \
    -v "$PWD/$CACHE/pnpm-store:/cache/pnpm-store" \
    -e CARGO_TARGET_DIR=/cache/target \
    -e npm_config_store_dir=/cache/pnpm-store \
    "$IMAGE" bash -euo pipefail -c '
    mkdir /build
    # The working tree, not HEAD, so an uncommitted change can be tried.
    # node_modules and build output from the host stay out.
    tar -C /src --exclude=./node_modules --exclude=./dist --exclude=./.git \
        --exclude=./src-tauri/target --exclude="./spikes/*/target" -cf - . | tar -C /build -xf -
    cd /build
    pnpm install --frozen-lockfile
    pnpm tauri build --bundles appimage
    cp /cache/target/release/bundle/appimage/*.AppImage /out/
'
ls -l "$OUT"/*.AppImage
