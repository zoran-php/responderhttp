@echo off
REM spikes/static-link-proof/run-proof.bat
REM
REM Double-click target: builds the proof, checks its DLL imports and runs it,
REM with everything captured in build-log.txt so the result can be read from
REM outside this machine.
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0check-windows.ps1" > "%~dp0build-log.txt" 2>&1
set RC=%errorlevel%
echo exit=%RC% >> "%~dp0build-log.txt"
