#!/usr/bin/env bash
# release.sh
#
# The Linux twin of release.bat (PLAN-LINUX.md 17d): verify.sh, then the
# release build and its bundles, then the link check against the real
# binary. Output goes to release-log.txt in the same "=== step ===" /
# "exit=N" shape, so a run can be read off-machine.
#
# The link check is the step that matters (check-linux.sh): libcurl, TLS and
# SQLite must be inside the binary, and only the desktop's own libraries
# (WebKitGTK, GTK, glib, D-Bus, zlib, libc) may be linked.
set -u
cd "$(dirname "$0")"
LOG="$PWD/release-log.txt"
BINARY="$PWD/src-tauri/target/release/responderhttp"

stop() {
    echo >> "$LOG"
    echo "$1" >> "$LOG"
    echo "STOPPED EARLY" >> "$LOG"
    echo "release.sh stopped early; see $LOG" >&2
    exit 1
}

echo "=== verify.sh ===" > "$LOG"
./verify.sh
cat verify-log.txt >> "$LOG"
# verify.sh records "exit=N" per step and always returns 0 itself, so the
# log is what says whether it passed.
if grep -qE '^exit=[1-9]' verify-log.txt; then
    stop "verify.sh reported a failing step - fix it before bundling."
fi

echo >> "$LOG"
echo "=== pnpm tauri build ===" >> "$LOG"
pnpm tauri build >> "$LOG" 2>&1
build_exit=$?
echo "exit=$build_exit" >> "$LOG"
[ "$build_exit" -eq 0 ] || stop "BUILD FAILED"

echo >> "$LOG"
echo "=== link check against the real binary ===" >> "$LOG"
spikes/static-link-proof/check-linux.sh "$BINARY" >> "$LOG" 2>&1
echo "exit=$?" >> "$LOG"

echo >> "$LOG"
echo "=== bundles ===" >> "$LOG"
find src-tauri/target/release/bundle -maxdepth 2 -type f \( -name '*.rpm' -o -name '*.AppImage' \) \
    -exec ls -l {} \; >> "$LOG" 2>&1
for rpm in src-tauri/target/release/bundle/rpm/*.rpm; do
    [ -e "$rpm" ] || continue
    echo "--- $rpm requires:" >> "$LOG"
    rpm -qpR "$rpm" >> "$LOG" 2>&1
done

echo >> "$LOG"
echo "DONE" >> "$LOG"
if grep -qE '^exit=[1-9]' "$LOG"; then
    echo "release.sh finished with a failing step; see $LOG" >&2
    exit 1
fi
echo "release.sh green; see $LOG"
