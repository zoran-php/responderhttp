#!/usr/bin/env bash
# tools/test-linux-keyring.sh
#
# Runs the ignored test in src-tauri/tests/linux_keyring.rs against a real
# GNOME Keyring without touching yours (PLAN-LINUX.md 17c). Everything lives
# in a private D-Bus session and a temporary XDG_DATA_HOME, both gone when
# the script ends: the keyring is created there with a throwaway password.
#
# Needs gnome-keyring and dbus-run-session (Fedora: dnf install gnome-keyring
# dbus-daemon; WSL images may lack the second).
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"

command -v gnome-keyring-daemon >/dev/null || {
    echo "gnome-keyring-daemon not found: dnf install gnome-keyring" >&2
    exit 1
}

dbus-run-session -- bash -euo pipefail -c '
    XDG_DATA_HOME=$(mktemp -d)
    export XDG_DATA_HOME
    trap "rm -rf \"$XDG_DATA_HOME\"" EXIT
    # --unlock reads the password from stdin and creates the login keyring
    # with it, which becomes the default collection oo7 writes to.
    printf "throwaway" | gnome-keyring-daemon --unlock --components=secrets --daemonize >/dev/null
    cargo test --test linux_keyring -- --ignored
'
