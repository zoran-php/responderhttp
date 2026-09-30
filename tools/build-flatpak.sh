#!/usr/bin/env bash
# tools/build-flatpak.sh
#
# Builds the Flatpak from this working tree the way Flathub does, offline,
# installs it for the current user, and runs Flathub's linter on the manifest
# and on the built repository (PLAN-LINUX.md 17e). Run it with:
#
#   flatpak run io.github.zoran_php.responderhttp
#
# Needs flatpak, and from Flathub: org.flatpak.Builder (the builder and the
# linter) and the GNOME 51 SDK with the rust-stable and node26 extensions for
# 26.08. Build state lives in src-tauri/target/flatpak/.
set -euo pipefail
cd "$(dirname "$0")/.."

APP_ID=io.github.zoran_php.responderhttp
MANIFEST="flatpak/$APP_ID.yml"
# Flathub's shared modules (the tray library), pinned. On Flathub they are a
# git submodule of the app's repository instead.
SHARED_MODULES_COMMIT=cb9ec602a1ece1c76d5a4f8aa1d87c4a6bf99c3e
WORK=src-tauri/target/flatpak

if [ ! -d flatpak/shared-modules/.git ]; then
    git clone -q https://github.com/flathub/shared-modules.git flatpak/shared-modules
fi
git -C flatpak/shared-modules fetch -q origin "$SHARED_MODULES_COMMIT"
git -C flatpak/shared-modules checkout -q "$SHARED_MODULES_COMMIT"

mkdir -p "$WORK"
# Flathub's own builder, with the flags of its `flathub-build` wrapper (the
# command docs.flathub.org's submission guide gives), so the result is what
# Flathub's build service produces. Only the folders differ: the wrapper
# writes repo/ and builddir/ into the current directory, and --sandbox is
# left out: it refuses a source outside the manifest's folder, which the
# local manifest's `path: ..` is. The Flathub copy uses a git source, which
# --sandbox allows, and no other option it forbids. Screenshots and
# icons are mirrored into the repository, so the screenshot URLs must already
# be reachable, which for docs/screenshots/ means published by GitHub Pages.
flatpak run --filesystem="$PWD" org.flatpak.Builder \
    --force-clean --override-source-date-epoch 1321009871 \
    --user --install --install-deps-from=flathub --ccache --disable-rofiles-fuse \
    --mirror-screenshots-url=https://dl.flathub.org/media --compose-url-policy=full \
    --state-dir="$WORK/state" --repo="$WORK/repo" \
    "$WORK/build" "$MANIFEST"

echo "=== flatpak-builder-lint: manifest ==="
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest "$MANIFEST" || lint_failed=1
echo "=== flatpak-builder-lint: repo ==="
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo "$WORK/repo" || lint_failed=1
[ -z "${lint_failed:-}" ] || { echo "the linter reported problems (above)" >&2; exit 1; }
echo "built, installed and linted: flatpak run $APP_ID"
