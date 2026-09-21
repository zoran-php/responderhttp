@echo off
REM verify.bat
REM
REM Full gate per CLAUDE.md section 10: frontend build (tsc --noEmit + vite),
REM eslint, vitest, cargo fmt, cargo clippy, cargo test. Output goes to
REM verify-log.txt so the result can be read from outside this machine.
REM
REM The frontend build runs first: tauri::generate_context! embeds ../dist at
REM compile time, so the Rust side will not check until dist exists.
REM
REM setlocal is load-bearing, not hygiene: release.bat calls this script and
REM keeps its own LOG variable. Without it, the `set LOG=` below overwrites the
REM caller's, and every line release.bat writes afterwards lands in THIS file
REM while release-log.txt stays one line long. It also keeps the `cd src-tauri`
REM further down from leaking into the caller's working directory.
setlocal
cd /d "%~dp0"
set LOG=%~dp0verify-log.txt

echo === pnpm install === > "%LOG%"
call pnpm install >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo === pnpm run build === >> "%LOG%"
call pnpm run build >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo === pnpm run lint === >> "%LOG%"
call pnpm run lint >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo === vitest run === >> "%LOG%"
call pnpm exec vitest run >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

cd src-tauri

echo === cargo fmt --check === >> "%LOG%"
cargo fmt --check >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo === cargo clippy --all-targets -D warnings === >> "%LOG%"
cargo clippy --all-targets -- -D warnings >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo === cargo test === >> "%LOG%"
cargo test >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo DONE >> "%LOG%"
