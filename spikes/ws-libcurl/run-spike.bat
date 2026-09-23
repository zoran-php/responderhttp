@echo off
REM spikes/ws-libcurl/run-spike.bat
REM
REM Double-click target for PLAN.md Phase 13a: builds the spike with the app's
REM exact libcurl, runs every scenario, then checks the exe's DLL imports.
REM Everything is captured in spike-log.txt so the result can be read from
REM outside this machine. The scenario table alone is in spike-report.txt.
REM
REM No +crt-static here: the app does not set it either (PLAN.md Phase 7), and
REM the spike should link the way the app does.
setlocal
cd /d "%~dp0"
set "LOG=%~dp0spike-log.txt"

echo === cargo build --release === > "%LOG%"
cargo build --release >> "%LOG%" 2>&1
if errorlevel 1 (
  echo BUILD FAILED >> "%LOG%"
  echo Build failed - see spike-log.txt
  exit /b 1
)

echo. >> "%LOG%"
echo === scenarios === >> "%LOG%"
"%~dp0target\release\ws-libcurl.exe" >> "%LOG%" 2>&1
set "RUN_RC=%errorlevel%"

echo. >> "%LOG%"
echo === static-link check === >> "%LOG%"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\static-link-proof\check-windows.ps1" -SkipBuild -ExePath "%~dp0target\release\ws-libcurl.exe" >> "%LOG%" 2>&1
set "CHECK_RC=%errorlevel%"

echo. >> "%LOG%"
echo scenarios exit=%RUN_RC% static-link exit=%CHECK_RC% >> "%LOG%"
echo Done: scenarios exit=%RUN_RC%, static-link exit=%CHECK_RC%. See spike-log.txt
exit /b 0
