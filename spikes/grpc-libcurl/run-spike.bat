@echo off
REM spikes/grpc-libcurl/run-spike.bat
REM
REM Double-click target for PLAN-GRPC.md 16a: builds the spike with the app's
REM exact libcurl, runs every gate, then checks the exe's DLL imports.
REM Everything is captured in spike-log.txt so the result can be read from
REM outside this machine. The gate table alone is in spike-report.txt.
REM
REM Optional: set SPIKE_PROXY=http://host:port before running for gate 11.
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
echo === gates === >> "%LOG%"
"%~dp0target\release\grpc-libcurl.exe" >> "%LOG%" 2>&1
set "RUN_RC=%errorlevel%"

echo. >> "%LOG%"
echo === static-link check === >> "%LOG%"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\static-link-proof\check-windows.ps1" -SkipBuild -ExePath "%~dp0target\release\grpc-libcurl.exe" >> "%LOG%" 2>&1
set "CHECK_RC=%errorlevel%"

echo. >> "%LOG%"
echo gates exit=%RUN_RC% static-link exit=%CHECK_RC% >> "%LOG%"
echo Done: gates exit=%RUN_RC%, static-link exit=%CHECK_RC%. See spike-log.txt
exit /b 0
