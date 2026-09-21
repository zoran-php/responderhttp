# spikes/static-link-proof/check-windows.ps1
#
# Builds the proof (unless -SkipBuild), lists every DLL the exe imports, fails
# if any is not a DLL that every Windows install ships, then runs the exe.
#
# Point -ExePath at the Tauri app's release exe later to run the same check
# on the real binary (Phase 7). Expect to extend the allowlist then: GUI apps
# import more OS DLLs. Only add a DLL after confirming it exists on a clean
# Windows install (e.g. in Windows Sandbox's System32).
#
# Extended 2026-09-15, after the first run against the real app. Two things
# came out of it, and it is worth being precise about which is which:
#
#   1. tauri-build does NOT pass -C target-feature=+crt-static. The app links
#      the C runtime dynamically. Confirmed, not suspected.
#   2. That turns out not to breach the single-binary guarantee, because what
#      it links against is the Universal CRT, which Microsoft documents as "a
#      Microsoft Windows operating system component... included as part of the
#      operating system in Windows 10 or later, and Windows Server 2016 or
#      later". WebView2 already puts a Windows 10 floor under this app, so the
#      UCRT is present wherever the app can run at all.
#
# What the exe emphatically does NOT import is the set that would breach it:
# no vcruntime140.dll, no msvcp140.dll (both ship in the VC++ Redistributable,
# not in Windows), no libcurl, no libssl/libcrypto, no sqlite3.dll. libcurl,
# rustls and SQLite are inside the binary, which is the guarantee CLAUDE.md
# section 11 rule 2 actually makes.
#
# So this check's job is now that narrower, sharper one: catch the day a
# dependency appears that a user would have to install. It still fails on
# every DLL above.
#
# Usage (PowerShell, from this folder):
#   powershell -ExecutionPolicy Bypass -File .\check-windows.ps1
#   powershell -ExecutionPolicy Bypass -File .\check-windows.ps1 -SkipBuild -ExePath C:\path\to\app.exe
param(
    [string]$ExePath = (Join-Path $PSScriptRoot "target\release\static-link-proof.exe"),
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# DLLs that ship with every supported Windows install. Anything else is a
# dependency the user's machine would have to provide, and fails the check.
$SystemDllPatterns = @(
    '^kernel32\.dll$',
    '^ntdll\.dll$',
    '^advapi32\.dll$',
    '^ws2_32\.dll$',
    '^crypt32\.dll$',
    '^bcrypt\.dll$',
    '^bcryptprimitives\.dll$',
    '^ncrypt\.dll$',
    '^secur32\.dll$',
    '^user32\.dll$',
    '^userenv\.dll$',
    '^normaliz\.dll$',
    '^iphlpapi\.dll$',
    '^ole32\.dll$',
    '^oleaut32\.dll$',
    '^shell32\.dll$',
    '^api-ms-win-core-.+\.dll$',

    # Core Windows UI and shell libraries, in System32 on every install. A GUI
    # app that hosts WebView2 cannot avoid them. Confirmed present in a clean
    # Windows Sandbox before being added, per the note above.
    '^comctl32\.dll$',
    '^dwmapi\.dll$',
    '^gdi32\.dll$',
    '^shlwapi\.dll$',

    # COM/WinRT base runtime. Pulled in on 2026-09-20 by the close-to-tray
    # toast (PLAN.md Phase 11, tauri-winrt-notification): WinRT activation is
    # COM activation. combase.dll has been in System32 since Windows 8 —
    # ole32.dll and oleaut32.dll above are forwarders onto it — and this app's
    # floor is Windows 10 1809 (Package.appxmanifest MinVersion), so it is
    # present wherever the app can run. The same change added
    # api-ms-win-core-winrt-l1-1-0.dll, which the core APIset pattern above
    # already covered.
    '^combase\.dll$',

    # The Universal CRT's APIset forwarders. An OS component since Windows 10 /
    # Server 2016 — see the header. Deliberately NOT a blanket
    # '^api-ms-win-.+' so that only these two families pass and anything else
    # Microsoft adds later still has to be looked at.
    '^api-ms-win-crt-.+\.dll$'
)

function Invoke-ProofBuild {
    Push-Location $PSScriptRoot
    try {
        # Static MSVC C runtime. Set here rather than in .cargo\config.toml so the
        # whole build is reproducible from this one script. Remove if that config
        # file is ever added — RUSTFLAGS overrides it, which would hide a mismatch.
        $env:RUSTFLAGS = "-C target-feature=+crt-static"
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }
    finally {
        Pop-Location
    }
}

function Find-Dumpbin {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) {
        throw "vswhere.exe not found. Install Visual Studio Build Tools with the C++ workload."
    }
    $dumpbin = & $vswhere -latest -products * -find "VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe" |
        Select-Object -First 1
    if (-not $dumpbin) {
        throw "dumpbin.exe not found. Install the 'Desktop development with C++' workload."
    }
    return $dumpbin
}

function Get-ImportedDlls([string]$dumpbin, [string]$exe) {
    & $dumpbin /nologo /dependents $exe |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ -match '\.dll$' } |
        Sort-Object -Unique
}

function Test-IsSystemDll([string]$dll) {
    foreach ($pattern in $SystemDllPatterns) {
        if ($dll -match $pattern) { return $true }
    }
    return $false
}

if (-not $SkipBuild) { Invoke-ProofBuild }
if (-not (Test-Path $ExePath)) { throw "Executable not found: $ExePath" }

$dumpbin = Find-Dumpbin
$imports = @(Get-ImportedDlls $dumpbin $ExePath)
$foreign = @($imports | Where-Object { -not (Test-IsSystemDll $_) })

Write-Host ""
Write-Host "Imported DLLs ($($imports.Count)):"
$imports | ForEach-Object { Write-Host "  $_" }
Write-Host ""

if ($foreign.Count -gt 0) {
    Write-Host "FAIL: DLLs a clean Windows install does not guarantee:" -ForegroundColor Red
    $foreign | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    if ($foreign | Where-Object { $_ -match '^(vcruntime|msvcp)' }) {
        Write-Host ""
        Write-Host "vcruntime/msvcp ship in the VC++ Redistributable, not in Windows. This is" -ForegroundColor Yellow
        Write-Host "the C runtime dependency that genuinely breaks the single-binary promise." -ForegroundColor Yellow
        Write-Host "Set -C target-feature=+crt-static and rebuild from clean." -ForegroundColor Yellow
    }
    exit 1
}
Write-Host "PASS: only Windows system DLLs are imported." -ForegroundColor Green
Write-Host ""

if ($ExePath -like "*static-link-proof.exe") {
    Write-Host "Running $ExePath"
    & $ExePath
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
