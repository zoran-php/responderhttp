#!/usr/bin/env bash
# spikes/static-link-proof/check-appimage.sh
#
# The AppImage's release check (PLAN-LINUX.md 17d). An AppImage carries its
# own GTK and WebKitGTK, so check-linux.sh's allowlist does not describe it
# as a whole. Three checks do:
#
# 1. The app binary inside passes check-linux.sh, exactly as the RPM's does:
#    libcurl, TLS and SQLite are compiled in. The libraries bundled beside it
#    are WebKitGTK's and GTK's own dependencies, not the engine's.
# 2. No ELF file inside needs a newer glibc than GLIBC_FLOOR.
# 3. No ELF file inside needs a newer C++ runtime (libstdc++) than
#    GLIBCXX_FLOOR.
#
# Neither glibc nor libstdc++ is bundled: the AppImage uses the system's.
# Both floors are what the Ubuntu 22.04 builder has (tools/appimage/
# Containerfile), and the newest version any file asks for decides which
# distributions the AppImage starts on. One library copied from a newer
# system would raise that without anything else failing, and docs/linux.html
# promises these two floors to users.
#
# Usage:
#   spikes/static-link-proof/check-appimage.sh src-tauri/target/appimage/ResponderHTTP_1.1.0_amd64.AppImage
#   spikes/static-link-proof/check-appimage.sh <an extracted squashfs-root>
set -euo pipefail

TARGET=${1:?usage: check-appimage.sh <AppImage, or an extracted AppDir>}
# Ubuntu 22.04: glibc 2.35, and libstdc++ from GCC 12 (GLIBCXX_3.4.30).
# Raise them only together with the builder's base image and docs/linux.html.
GLIBC_FLOOR=35
GLIBCXX_FLOOR=30
HERE=$(cd "$(dirname "$0")" && pwd)

if [ -d "$TARGET" ]; then
    root=$(cd "$TARGET" && pwd)
elif [ -f "$TARGET" ]; then
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    cp "$TARGET" "$work/app.AppImage"
    (cd "$work" && ./app.AppImage --appimage-extract >/dev/null)
    root="$work/squashfs-root"
else
    echo "not found: $TARGET" >&2
    exit 2
fi

echo "=== 1. the app binary ==="
"$HERE/check-linux.sh" "$root/usr/bin/responderhttp"
echo

# Every ELF file inside, found once for both floors.
elf_files=()
while IFS= read -r -d '' file; do
    file -b "$file" | grep -q '^ELF' && elf_files+=("$file")
done < <(find "$root" -type f -print0)

# The highest last number of a versioned symbol, such as 35 for GLIBC_2.35.
# A file with no such symbols (a font, a data blob that happens to be
# ELF-shaped) finds nothing, which is not a failure.
newest_version() {
    local file=$1 prefix=$2
    (objdump -T "$file" 2>/dev/null || true) | { grep -oE "${prefix//./\\.}[0-9]+" || true; } \
        | sed "s/^${prefix//./\\.}//" | sort -n | tail -1
}

# Fails when any file needs more than the floor, naming each such file.
check_floor() {
    local label=$1 prefix=$2 floor=$3
    local newest=0 newest_file="" version relative
    local too_new=()
    for file in "${elf_files[@]}"; do
        version=$(newest_version "$file" "$prefix")
        [ -n "$version" ] || continue
        relative=${file#"$root"/}
        if [ "$version" -gt "$newest" ]; then
            newest=$version
            newest_file=$relative
        fi
        if [ "$version" -gt "$floor" ]; then
            too_new+=("$relative ($prefix$version)")
        fi
    done
    echo "${#elf_files[@]} ELF files; the newest $label any of them needs is $prefix$newest ($newest_file)"
    if [ ${#too_new[@]} -gt 0 ]; then
        echo "FAIL: these need a newer $label than $prefix$floor, so the AppImage would not start on older distributions:"
        printf '  %s\n' "${too_new[@]}"
        return 1
    fi
    echo "PASS: every file runs with $prefix$floor (Ubuntu 22.04, Debian 12 and newer)."
}

failed=0
echo "=== 2. glibc floor (at most GLIBC_2.$GLIBC_FLOOR) ==="
check_floor "glibc" "GLIBC_2." "$GLIBC_FLOOR" || failed=1
echo
echo "=== 3. C++ runtime floor (at most GLIBCXX_3.4.$GLIBCXX_FLOOR) ==="
check_floor "C++ runtime" "GLIBCXX_3.4." "$GLIBCXX_FLOOR" || failed=1
exit "$failed"
