@echo off
REM pack-store.bat
REM
REM Builds the release exe and wraps it in an MSIX package for the Microsoft
REM Store (PLAN.md Phase 10). Output goes to pack-store-log.txt so the result
REM can be read from off-machine, the same way verify.bat and release.bat work.
REM
REM   pack-store.bat          unsigned package, for submitting to the Store.
REM                           The Store re-signs it; a signature of ours would
REM                           be discarded.
REM   pack-store.bat test     signed with a local development certificate, so
REM                           the package can be installed on this PC and
REM                           smoke-tested before submitting.
REM
REM Requires the winapp CLI (winget install microsoft.winappcli) on Windows 11.
REM
REM Why a staging folder and not dist\: winapp pack packages everything in the
REM folder it is given, and dist\ is the Vite frontend build (tauri.conf.json
REM frontendDist). Packing dist\ would ship the whole web build alongside the
REM exe that already has it embedded. msix-stage\ holds exactly two things:
REM the exe and the Assets folder.
REM
REM Why labels and not if/else blocks: cmd parses a parenthesised block in one
REM go, so an unescaped ")" inside an echo — "=== pack (signed) ===" — closes
REM the block early. The 2026-09-19 run did exactly that: the signed and the
REM unsigned branch both ran, four redirections collided on the log file, and
REM the package that came out was not the one the script said it was. Every
REM branch below is a goto, and no echo here contains a bracket.
setlocal
cd /d "%~dp0"
set LOG=%~dp0pack-store-log.txt
set STAGE=%~dp0msix-stage
set EXE=%~dp0src-tauri\target\release\responderhttp.exe
set MANIFEST=%~dp0Package.appxmanifest
set CERT=%~dp0devcert.pfx

set SIGN=
if /I "%~1"=="test" set SIGN=1

echo === pack-store.bat === > "%LOG%"
echo. >> "%LOG%"

where winapp >nul 2>&1
if errorlevel 1 goto :nowinapp

REM The Store rejects a package whose version does not match what the listing
REM expects, and the manifest carries its own copy of the version. Catching the
REM drift here costs a second; catching it at submission costs a round trip.
echo === version check === >> "%LOG%"
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$t = (Get-Content 'src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json).version;" ^
  "$m = ([xml](Get-Content 'Package.appxmanifest' -Raw)).Package.Identity.Version;" ^
  "Write-Host ('tauri.conf.json {0} -> expected manifest {0}.0, manifest has {1}' -f $t, $m);" ^
  "if ($m -ne ($t + '.0')) { exit 1 }" >> "%LOG%" 2>&1
if errorlevel 1 goto :versiondrift

REM Assets are derived from app-icon.png, the same 1024x1024 source the Tauri
REM icons come from. winapp draws every size and scale variant the package
REM needs, which is a long list to maintain by hand. Delete the Assets folder
REM and re-run if app-icon.png ever changes.
if exist "%~dp0Assets\Square150x150Logo.png" goto :haveassets
echo. >> "%LOG%"
echo === winapp manifest update-assets === >> "%LOG%"
call winapp manifest update-assets "%~dp0app-icon.png" --manifest "%MANIFEST%" >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"
if not exist "%~dp0Assets\Square150x150Logo.png" goto :noassets
:haveassets

echo. >> "%LOG%"
echo === pnpm tauri build --no-bundle === >> "%LOG%"
REM --no-bundle: the NSIS and MSI installers are not wanted here and cost
REM minutes. The exe is the only input the package needs.
call pnpm tauri build --no-bundle >> "%LOG%" 2>&1
REM Captured immediately: %errorlevel% inside a parenthesised block expands
REM when the block is parsed, which would report a stale one.
set BUILD_EXIT=%errorlevel%
echo exit=%BUILD_EXIT% >> "%LOG%"
if not "%BUILD_EXIT%"=="0" goto :buildfailed
if not exist "%EXE%" goto :noexe

echo. >> "%LOG%"
echo === stage === >> "%LOG%"
if exist "%STAGE%" rmdir /s /q "%STAGE%"
mkdir "%STAGE%"
REM Renamed on the way in, so Task Manager and the install folder show the
REM product name rather than the Cargo crate name. Package.appxmanifest's
REM Executable attribute matches this name.
copy /Y "%EXE%" "%STAGE%\ResponderHTTP.exe" >> "%LOG%" 2>&1
xcopy /E /I /Y "%~dp0Assets" "%STAGE%\Assets" >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"

echo. >> "%LOG%"
if defined SIGN goto :signed

echo === winapp pack, unsigned, for the Store === >> "%LOG%"
call winapp pack "%STAGE%" --manifest "%MANIFEST%" >> "%LOG%" 2>&1
set PACK_EXIT=%errorlevel%
goto :packed

:signed
echo === winapp cert generate === >> "%LOG%"
REM The certificate's publisher must equal the manifest's Publisher or Windows
REM refuses to install the package, so the manifest is what it reads.
call winapp cert generate --manifest "%MANIFEST%" --output "%CERT%" --if-exists skip >> "%LOG%" 2>&1
echo exit=%errorlevel% >> "%LOG%"
if not exist "%CERT%" goto :nocert
echo. >> "%LOG%"
echo === winapp pack, signed, for local testing === >> "%LOG%"
call winapp pack "%STAGE%" --manifest "%MANIFEST%" --cert "%CERT%" >> "%LOG%" 2>&1
set PACK_EXIT=%errorlevel%

:packed
echo exit=%PACK_EXIT% >> "%LOG%"
if not "%PACK_EXIT%"=="0" goto :packfailed

REM winapp names the file after the manifest's Identity/Name, not after the
REM product, so the name is read back rather than assumed. Newest first, in
REM case an older package is still lying around.
set PKG=
for /f "delims=" %%F in ('dir /b /o-d "%~dp0*.msix" 2^>nul') do if not defined PKG set PKG=%%F

echo. >> "%LOG%"
echo === package === >> "%LOG%"
echo %PKG% >> "%LOG%"

echo. >> "%LOG%"
if not defined SIGN goto :doneunsigned
echo DONE - signed package. To install it on this PC: >> "%LOG%"
echo   winapp cert install .\devcert.pfx        admin, once per certificate >> "%LOG%"
echo   Add-AppxPackage .\%PKG% >> "%LOG%"
goto :eof

:doneunsigned
echo DONE - unsigned package, ready to upload to Partner Center: >> "%LOG%"
echo   %PKG% >> "%LOG%"
goto :eof

:nowinapp
echo winapp CLI not found on PATH. >> "%LOG%"
echo Install it with: winget install microsoft.winappcli >> "%LOG%"
goto :fail

:versiondrift
echo VERSION DRIFT - update Package.appxmanifest to match tauri.conf.json. >> "%LOG%"
goto :fail

:noassets
echo Asset generation did not produce Assets\Square150x150Logo.png. >> "%LOG%"
goto :fail

:buildfailed
echo BUILD FAILED >> "%LOG%"
goto :fail

:noexe
echo Executable not found: %EXE% >> "%LOG%"
goto :fail

:nocert
echo Certificate not created: %CERT% >> "%LOG%"
echo Nothing was packed - a signed run without a certificate would produce >> "%LOG%"
echo an unsigned package that cannot be installed locally. >> "%LOG%"
goto :fail

:packfailed
echo PACK FAILED >> "%LOG%"
goto :fail

:fail
echo. >> "%LOG%"
echo STOPPED EARLY >> "%LOG%"
