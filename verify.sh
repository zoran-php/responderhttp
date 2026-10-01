#!/usr/bin/env bash
# verify.sh
#
# The Linux twin of verify.bat (PLAN-LINUX.md 17a): the CLAUDE.md section 10
# gate — frontend build (tsc --noEmit + vite), eslint, vitest, cargo fmt,
# cargo clippy, cargo test. Output goes to verify-log.txt in the same
# "=== step ===" / "exit=N" shape, so release.sh and a reader off-machine can
# check it the same way on both platforms.
#
# The frontend build runs first: tauri::generate_context! embeds ../dist at
# compile time, so the Rust side will not check until dist exists.
#
# Like verify.bat it always exits 0 and leaves the verdict to the log; it
# does not stop at the first failing step, so one run reports every failure.
set -u
cd "$(dirname "$0")"
LOG="$PWD/verify-log.txt"

step() {
    local name=$1
    shift
    echo "=== $name ===" >> "$LOG"
    "$@" >> "$LOG" 2>&1
    echo "exit=$?" >> "$LOG"
}

: > "$LOG"
step "pnpm install" pnpm install
step "pnpm run build" pnpm run build
step "pnpm run lint" pnpm run lint
step "vitest run" pnpm exec vitest run

cd src-tauri
step "cargo fmt --check" cargo fmt --check
step "cargo clippy --all-targets -D warnings" cargo clippy --all-targets -- -D warnings
step "cargo test" cargo test

echo DONE >> "$LOG"
