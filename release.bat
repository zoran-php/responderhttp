@echo off
REM release.bat
REM
REM The Phase 7 release gate, in one command. Runs verify.bat first, then a
REM production bundle, then points the Phase 0 static-link check at the real
REM app binary rather than at the console spike it was written for.
REM
REM That last step is the one that matters: the spike proved libcurl, the TLS
REM stack and the MSVC C runtime are inside a binary built with
REM -C target-feature=+crt-static set by hand. Nothing yet proves tauri-build
REM passes that flag. If it does not, the exe imports VCRUNTIME140.dll, which
REM is not on a clean Windows install, and the single-binary guarantee in
REM CLAUDE.md is false. This is where that gets answered.
REM
REM Output goes to release-log.txt so the result can be read from off-machine.
REM
REM setlocal here and in verify.bat: the two scripts both use a LOG variable,
REM and the callee's used to clobber the caller's.
setlocal
cd /d "%~dp0"
set LOG=%~dp0release-log.txt
set EXE=%~dp0src-tauri\target\release\responderhttp.exe

echo === verify.bat === > "%LOG%"
call "%~dp0verify.bat"
if not exist "%~dp0verify-log.txt" goto :fail
type "%~dp0verify-log.txt" >> "%LOG%"

REM verify.bat records a per-step "exit=N" line and always returns 0 itself,
REM so the log is what says whether it passed. Any non-zero step stops the
REM release here rather than bundling something that does not compile.
findstr /R /C:"exit=[1-9]" "%~dp0verify-log.txt" >nul
if not errorlevel 1 (
    echo. >> "%LOG%"
    echo verify.bat reported a failing step - fix it before bundling. >> "%LOG%"
    goto :fail
)

echo. >> "%LOG%"
echo === pnpm tauri build === >> "%LOG%"
REM verify.bat ends inside src-tauri; this is belt and braces now that it runs
REM under setlocal, and it costs nothing.
cd /d "%~dp0"
call pnpm tauri build >> "%LOG%" 2>&1
REM Not inside an if-block: %errorlevel% in a parenthesised block expands when
REM the block is parsed, so the failure branch would have reported a stale one.
set BUILD_EXIT=%errorlevel%
echo exit=%BUILD_EXIT% >> "%LOG%"
if not "%BUILD_EXIT%"=="0" (
    echo BUILD FAILED >> "%LOG%"
    goto :fail
)

echo. >> "%LOG%"
echo === static-link check against the real binary === >> "%LOG%"
if not exist "%EXE%" (
    echo Executable not found: %EXE% >> "%LOG%"
    echo Check the bundle output path in the build log above. >> "%LOG%"
    goto :fail
)
powershell -ExecutionPolicy Bypass -File "%~dp0spikes\static-link-proof\check-windows.ps1" -SkipBuild -ExePath "%EXE%" >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo. >> "%LOG%"
echo DONE >> "%LOG%"
goto :eof

:fail
echo. >> "%LOG%"
echo STOPPED EARLY >> "%LOG%"
