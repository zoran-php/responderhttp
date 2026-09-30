#!/usr/bin/env bash
# tools/test-appimage.sh
#
# Starts the AppImage in clean containers of several distributions
# (PLAN-LINUX.md 17d). Each container gets only what any desktop already
# has — GTK 3 and Mesa — plus a virtual X server, and none of the -devel
# packages the build needed. The app must still be running after
# STARTUP_SECONDS and its log must show the webview started, which means
# every library loaded, glibc was new enough and WebKitGTK drew the page.
#
# With --gui it instead starts the AppImage in the first distribution given,
# on this desktop's display (WSLg or any X11 session), for a hand check such
# as sending an HTTPS request with that distribution's CA store.
#
# Usage:
#   tools/test-appimage.sh [--gui] [image ...]
# Default images: Ubuntu 22.04, Debian 12, Ubuntu 24.04, Fedora 44, Arch.
set -euo pipefail
cd "$(dirname "$0")/.."

GUI=false
if [ "${1:-}" = "--gui" ]; then
    GUI=true
    shift
fi
IMAGES=("$@")
if [ ${#IMAGES[@]} -eq 0 ]; then
    IMAGES=(
        docker.io/library/ubuntu:22.04
        docker.io/library/debian:12
        docker.io/library/ubuntu:24.04
        registry.fedoraproject.org/fedora:44
        docker.io/library/archlinux:latest
    )
fi

APPIMAGE_DIR=src-tauri/target/appimage
shopt -s nullglob
appimages=("$APPIMAGE_DIR"/*.AppImage)
[ ${#appimages[@]} -eq 1 ] || { echo "expected exactly one AppImage in $APPIMAGE_DIR, found ${#appimages[@]}; run tools/build-appimage.sh" >&2; exit 2; }
APPIMAGE_FILE=$(basename "${appimages[0]}")

# What a desktop has before any app is installed, per package manager.
read -r -d '' INSTALL_DESKTOP_BASE <<'EOF' || true
if command -v apt-get >/dev/null; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    gtk=libgtk-3-0t64; apt-cache show "$gtk" >/dev/null 2>&1 || gtk=libgtk-3-0
    apt-get install -y -qq "$gtk" libgbm1 libegl1 libgl1 libgles2 xvfb ca-certificates >/dev/null
elif command -v dnf >/dev/null; then
    dnf install -y -q gtk3 mesa-libgbm mesa-libEGL mesa-libGL xorg-x11-server-Xvfb >/dev/null 2>&1
elif command -v pacman >/dev/null; then
    pacman -Sy --noconfirm --needed gtk3 mesa xorg-server-xvfb >/dev/null
fi
EOF

STARTUP_SECONDS=20
failures=0
for image in "${IMAGES[@]}"; do
    echo "=== $image"
    GUI_ARGS=()
    if $GUI; then
        GUI_ARGS=(-e DISPLAY -v /tmp/.X11-unix:/tmp/.X11-unix:ro)
    fi
    if podman run --rm --security-opt label=disable "${GUI_ARGS[@]}" \
        -v "$PWD/$APPIMAGE_DIR:/app:ro" -e APPIMAGE_FILE="$APPIMAGE_FILE" \
        -e INSTALL_DESKTOP_BASE="$INSTALL_DESKTOP_BASE" -e GUI="$GUI" \
        -e STARTUP_SECONDS="$STARTUP_SECONDS" -e APPIMAGE_EXTRACT_AND_RUN=1 \
        "$image" bash -euo pipefail -c '
        eval "$INSTALL_DESKTOP_BASE"
        echo "glibc $(ldd --version | head -1 | grep -oE "[0-9]+\.[0-9]+$")"
        cp "/app/$APPIMAGE_FILE" /tmp/app.AppImage
        export HOME=/tmp/home
        mkdir -p "$HOME"
        if [ "$GUI" = true ]; then
            echo "starting ResponderHTTP on $DISPLAY; close its window to end the test"
            GDK_BACKEND=x11 /tmp/app.AppImage
            exit 0
        fi
        Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 &
        export DISPLAY=:99
        /tmp/app.AppImage >/tmp/app-output.txt 2>&1 &
        app=$!
        sleep "$STARTUP_SECONDS"
        log=$(cat "$HOME"/.local/share/io.github.zoran-php.responderhttp/logs/*.log 2>/dev/null || true)
        if ! kill -0 "$app" 2>/dev/null; then
            echo "FAIL: the app exited within $STARTUP_SECONDS s:"
            head -20 /tmp/app-output.txt
            exit 1
        fi
        if ! grep -q "\"webview: " <<<"$log"; then
            echo "FAIL: running, but the log never recorded the webview starting:"
            head -20 /tmp/app-output.txt
            exit 1
        fi
        echo "PASS: running after $STARTUP_SECONDS s, $(grep -o "\"webview: [^\"]*\"" <<<"$log" | head -1)"
        kill "$app"
    '; then :; else
        failures=$((failures + 1))
    fi
done

[ "$failures" -eq 0 ] || { echo "$failures distribution(s) failed" >&2; exit 1; }
