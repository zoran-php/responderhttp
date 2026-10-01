#!/usr/bin/env bash
# tools/test-rpm-install.sh
#
# Installs the release RPM into a clean Fedora container, the way a user's
# `dnf install` would, and checks that what it declares is enough
# (PLAN-LINUX.md 17d): dnf resolves every dependency, and the dynamic loader
# then finds every library the binary names. Nothing from the build machine
# leaks in: the container starts from Fedora's own image.
#
# With --gui it also starts the installed app from inside the container on
# this desktop's display (WSLg or any X11 session), for a hand check that it
# draws and can send a request with only what the RPM brought.
#
# Needs podman. Build the RPM first (release.sh or `pnpm tauri build`).
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=${FEDORA_IMAGE:-registry.fedoraproject.org/fedora:44}
BUNDLE_DIR=src-tauri/target/release/bundle/rpm
shopt -s nullglob
rpms=("$BUNDLE_DIR"/*.rpm)
[ ${#rpms[@]} -eq 1 ] || { echo "expected exactly one RPM in $BUNDLE_DIR, found ${#rpms[@]}" >&2; exit 2; }
RPM_FILE=$(basename "${rpms[0]}")

GUI_ARGS=()
if [ "${1:-}" = "--gui" ]; then
    GUI_ARGS=(-e DISPLAY -v /tmp/.X11-unix:/tmp/.X11-unix:ro --security-opt label=disable)
fi

podman run --rm "${GUI_ARGS[@]}" -v "$PWD/$BUNDLE_DIR:/pkg:ro,Z" -e RPM_FILE="$RPM_FILE" \
    -e GUI="${1:-}" "$IMAGE" bash -euo pipefail -c '
    before=$(rpm -qa | wc -l)
    dnf install -y -q "/pkg/$RPM_FILE" >/dev/null
    after=$(rpm -qa | wc -l)
    echo "installed: $(rpm -q responder-http), plus $((after - before - 1)) dependencies from Fedora"

    missing=$(ldd /usr/bin/responderhttp | grep "not found" || true)
    if [ -n "$missing" ]; then
        echo "FAIL: libraries the binary needs that the RPM did not bring:"
        echo "$missing"
        exit 1
    fi
    echo "PASS: every library the binary needs is installed"

    for path in /usr/share/applications/ResponderHTTP.desktop /usr/share/licenses/responder-http/LICENSE; do
        [ -f "$path" ] || { echo "FAIL: $path is missing"; exit 1; }
    done
    echo "PASS: the desktop file and the licence are in place"

    if [ "$GUI" = "--gui" ]; then
        echo "starting ResponderHTTP on $DISPLAY; close its window to end the test"
        GDK_BACKEND=x11 responderhttp
    fi
'
