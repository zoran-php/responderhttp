#!/usr/bin/env bash
# tools/test-linux-session.sh
#
# Runs the Linux tests that need a desktop session's D-Bus services, and that
# `cargo test` therefore ignores (PLAN-LINUX.md 17b, 17c):
#
# - tests/linux_keyring.rs: the data key against a real GNOME Keyring;
# - desktop::tray: tray-host detection, by claiming the StatusNotifier name.
#
# Neither may touch the session of the person running it, so everything lives
# in a private D-Bus session and a temporary XDG_DATA_HOME, both gone when the
# script ends. The keyring is created there with a throwaway password.
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
    cargo test --lib desktop::tray -- --ignored
'
