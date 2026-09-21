@echo off
REM run-app.bat
REM
REM Runs the app the way `pnpm tauri dev` does: a debug build points at the
REM Vite dev server (devUrl in tauri.conf.json), so the dev server has to be
REM running or the window shows "can't reach this page".
REM
REM Output goes to run-app-log.txt. The redirect uses a relative path on
REM purpose: cmd has no escape for quotes inside an already-quoted /c string.
cd /d "%~dp0"
start "tauri dev" cmd /c "pnpm tauri dev > run-app-log.txt 2>&1"
