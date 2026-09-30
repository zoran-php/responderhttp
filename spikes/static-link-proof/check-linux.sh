#!/usr/bin/env bash
# spikes/static-link-proof/check-linux.sh
#
# The Linux twin of check-windows.ps1 (PLAN-LINUX.md 17d): lists every shared
# library the binary names in its dynamic section (NEEDED), and fails if any
# is not on the allowlist below.
#
# What "one file, no external dependencies" means on Linux (PLAN-LINUX.md F1):
# the window is drawn by WebKitGTK and GTK, which no Linux desktop app ships
# statically. They come from the system (RPM) or the runtime (Flatpak), the
# way WebView2 comes from Windows. What must never appear here is what
# CLAUDE.md section 11 rule 2 is about: libcurl, OpenSSL and SQLite are
# compiled into the binary, and a NEEDED entry for any of them means they no
# longer are. Those fail by name, whatever the allowlist says.
#
# Only add a library after confirming a clean Fedora install gets it from
# the packages the RPM requires (webkit2gtk4.1, gtk3, libappindicator-gtk3)
# or from the base system, and write the reason beside it. The tray library
# (libappindicator) is loaded at run time, so it never shows up here.
#
# Usage:
#   spikes/static-link-proof/check-linux.sh src-tauri/target/release/responderhttp
set -euo pipefail

BINARY=${1:?usage: check-linux.sh <path to the release binary>}
[ -f "$BINARY" ] || { echo "Binary not found: $BINARY" >&2; exit 2; }

# Never allowed: these are compiled in, and a NEEDED entry means that broke.
FORBIDDEN='^lib(curl|ssl|crypto|sqlite3|nghttp2|rustls|ssh2?|brotli|zstd)[.-]'

ALLOWED=(
    # The C runtime and the dynamic loader: part of every Linux system.
    '^libc\.so\.6$'
    '^libm\.so\.6$'
    '^libgcc_s\.so\.1$'
    '^ld-linux-x86-64\.so\.2$'

    # WebKitGTK, the Linux webview (the counterpart of WebView2), and its
    # JavaScript engine. Required by the RPM; in the GNOME Flatpak runtime.
    '^libwebkit2gtk-4\.1\.so\.0$'
    '^libjavascriptcoregtk-4\.1\.so\.0$'
    '^libsoup-3\.0\.so\.0$'

    # GTK 3 and the libraries it is built on. Required by the RPM, and pulled
    # in by WebKitGTK anyway.
    '^libgtk-3\.so\.0$'
    '^libgdk-3\.so\.0$'
    '^libgdk_pixbuf-2\.0\.so\.0$'
    '^libpango-1\.0\.so\.0$'
    '^libcairo\.so\.2$'
    '^libcairo-gobject\.so\.2$'
    '^libgio-2\.0\.so\.0$'
    '^libgobject-2\.0\.so\.0$'
    '^libglib-2\.0\.so\.0$'

    # D-Bus, used by the single-instance check. In every Fedora install
    # (systemd depends on it) and in the Flatpak runtime.
    '^libdbus-1\.so\.3$'

    # zlib. libz-sys links the system copy on Linux rather than building its
    # own (the risk PLAN.md Phase 0 wrote down, confirmed in PLAN-LINUX.md
    # 17a). It is in every Fedora install and in the Flatpak runtime.
    '^libz\.so\.1$'
)

mapfile -t NEEDED < <(readelf -d "$BINARY" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | sort -u)

echo "Shared libraries named by $BINARY (${#NEEDED[@]}):"
printf '  %s\n' "${NEEDED[@]}"
echo

forbidden=()
unknown=()
for library in "${NEEDED[@]}"; do
    if [[ $library =~ $FORBIDDEN ]]; then
        forbidden+=("$library")
        continue
    fi
    allowed=false
    for pattern in "${ALLOWED[@]}"; do
        if [[ $library =~ $pattern ]]; then
            allowed=true
            break
        fi
    done
    $allowed || unknown+=("$library")
done

if [ ${#forbidden[@]} -gt 0 ]; then
    echo "FAIL: linked dynamically, but must be compiled in (CLAUDE.md section 11 rule 2):"
    printf '  %s\n' "${forbidden[@]}"
fi
if [ ${#unknown[@]} -gt 0 ]; then
    echo "FAIL: not on the allowlist; a user's system may not provide it:"
    printf '  %s\n' "${unknown[@]}"
fi
if [ ${#forbidden[@]} -gt 0 ] || [ ${#unknown[@]} -gt 0 ]; then
    exit 1
fi
echo "PASS: only the desktop's own libraries are linked; libcurl, TLS and SQLite are compiled in."
