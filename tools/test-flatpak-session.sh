#!/usr/bin/env bash
# tools/test-flatpak-session.sh
#
# Checks the installed Flatpak against real desktop services (PLAN-LINUX.md
# 17e), in a private D-Bus session with a throwaway GNOME Keyring behind the
# Secret portal:
#
# 1. The first start finds no data key and creates one: oo7 keeps it in an
#    encrypted keyring file inside the sandbox, whose own secret the portal
#    takes from the host keyring.
# 2. The second start, a new process, loads that same key.
# 3. A second launch while the app runs hands over to it and exits
#    (single instance, through the D-Bus name under the app's ID).
#
# Nothing of the person running it is touched: the app runs with a temporary
# HOME, so its sandbox data lands there, and the keyring lives in a temporary
# folder. FLATPAK_USER_DIR keeps the per-user installation findable.
#
# Needs the Flatpak installed (tools/build-flatpak.sh), xdg-desktop-portal,
# xdg-desktop-portal-gtk, gnome-keyring and dbus-run-session, and a display.
set -euo pipefail

APP_ID=io.github.zoran_php.responderhttp
export FLATPAK_USER_DIR="${FLATPAK_USER_DIR:-$HOME/.local/share/flatpak}"
export APP_ID

# gnome-keyring's portal backend declares UseIn=gnome.
XDG_CURRENT_DESKTOP=GNOME dbus-run-session -- bash -euo pipefail -c '
    work=$(mktemp -d)
    trap "flatpak kill $APP_ID 2>/dev/null || true; rm -rf \"$work\"" EXIT
    mkdir -p "$work/home" "$work/keyring"
    printf "throwaway" | XDG_DATA_HOME="$work/keyring" \
        gnome-keyring-daemon --unlock --components=secrets --daemonize >/dev/null

    logs="$work/home/.var/app/$APP_ID/data/$APP_ID/logs"
    key_lines() { cat "$logs"/*.log 2>/dev/null | grep -cE "secrets: (no data key found|data key loaded|no usable data key)" || true; }
    start_and_wait() {
        HOME="$work/home" flatpak run "$APP_ID" >"$work/run-$1.txt" 2>&1 &
        for _ in $(seq 60); do
            [ "$(key_lines)" -ge "$1" ] && return 0
            sleep 1
        done
        echo "FAIL: start $1 logged nothing about the data key within 60 s"; tail -5 "$work/run-$1.txt"; exit 1
    }
    last_key_line() { cat "$logs"/*.log | grep -oE "secrets: (no data key found, created one|data key loaded|no usable data key[^\"]*)" | tail -1; }

    start_and_wait 1
    first=$(last_key_line)
    [ "$first" = "secrets: no data key found, created one" ] || { echo "FAIL: first start: $first"; exit 1; }
    echo "PASS: first start created the data key through the Secret portal"

    started=$(date +%s%N)
    HOME="$work/home" timeout 30 flatpak run "$APP_ID" >/dev/null 2>&1
    elapsed_ms=$(( ($(date +%s%N) - started) / 1000000 ))
    echo "PASS: a second launch handed over to the running app and exited in ${elapsed_ms} ms"

    flatpak kill "$APP_ID"; sleep 2
    start_and_wait 2
    second=$(last_key_line)
    [ "$second" = "secrets: data key loaded" ] || { echo "FAIL: second start: $second"; exit 1; }
    echo "PASS: the next start loaded the same key"
    ls "$work/home/.var/app/$APP_ID/data/keyrings/"
' 2>&1 | grep -E '^(PASS|FAIL)|keyring$|^  '
