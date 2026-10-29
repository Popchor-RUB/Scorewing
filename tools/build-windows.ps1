$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Run this script on Windows in an x64 Visual Studio Developer PowerShell.'
}

# The Linux devcontainer cannot compile native Windows WebView applications.
$repoDir = Split-Path -Parent $PSScriptRoot
Push-Location $repoDir
try {
    $missing = [System.Collections.Generic.List[string]]::new()
    foreach ($tool in @('node', 'npm.cmd', 'rustup', 'cargo', 'cl.exe', 'link.exe', 'rc.exe')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { $missing.Add($tool) }
    }
    if (Get-Command node -ErrorAction SilentlyContinue) {
        & node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22 || (a===22 && b>=22) ? 0 : 1)'
        if ($LASTEXITCODE -ne 0) { $missing.Add('Node.js >=22.22') }
    }
    if (Get-Command rustup -ErrorAction SilentlyContinue) {
        # Windows PowerShell 5.1 treats redirected native stderr as error records.
        $ErrorActionPreference = 'Continue'
        $rustVersion = & rustup run 1.90.0 rustc -vV 2>&1
        $rustExitCode = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if ($rustExitCode -ne 0 -or "$rustVersion" -notmatch 'host: x86_64-pc-windows-msvc') {
            $missing.Add('Rust 1.90.0 x86_64-pc-windows-msvc (prepare with rustup toolchain install 1.90.0-x86_64-pc-windows-msvc)')
        }
    }
    if (Get-Command npm.cmd -ErrorAction SilentlyContinue) {
        $ErrorActionPreference = 'Continue'
        & npm.cmd --prefix apps/desktop ls --depth=0 --offline *> $null
        $npmExitCode = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if ($npmExitCode -ne 0) { $missing.Add('frontend dependencies (prepare with npm --prefix apps/desktop ci)') }
    }
    if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = Join-Path $env:ProgramFiles 'LLVM/bin' }
    if (-not (Test-Path "$env:LIBCLANG_PATH/libclang.dll")) { $missing.Add('LLVM libclang.dll (set LIBCLANG_PATH to its directory)') }
    if (-not (Test-Path "$env:ProgramFiles/LLVM/bin/llvm-readobj.exe")) { $missing.Add('LLVM llvm-readobj.exe in Program Files/LLVM/bin') }
    if (-not $env:VCPKG_ROOT) { $env:VCPKG_ROOT = $env:VCPKG_INSTALLATION_ROOT }
    if (-not $env:VCPKG_ROOT -or -not (Test-Path "$env:VCPKG_ROOT/.vcpkg-root")) {
        $missing.Add('vcpkg checkout (set VCPKG_ROOT or VCPKG_INSTALLATION_ROOT)')
    }
    if (-not $env:VCPKG_INSTALLED_DIR -and $env:VCPKG_ROOT) { $env:VCPKG_INSTALLED_DIR = Join-Path $env:VCPKG_ROOT 'installed' }
    $env:VCPKGRS_TRIPLET = 'x64-windows-static'
    $xmlDir = "$env:VCPKG_INSTALLED_DIR/x64-windows-static"
    if (-not (Test-Path "$xmlDir/lib/*xml2*.lib") -or
        (-not (Test-Path "$xmlDir/include/libxml/parser.h") -and -not (Test-Path "$xmlDir/include/libxml2/libxml/parser.h"))) {
        $missing.Add('vcpkg libxml2[core]:x64-windows-static (prepare in VCPKG_ROOT/installed or set VCPKG_INSTALLED_DIR)')
    }
    if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64') { $missing.Add('x64 Visual Studio Developer PowerShell with C++ Build Tools and Windows SDK') }
    if ($missing.Count) { throw "Missing prerequisites (nothing was installed):`n  - $($missing -join "`n  - ")" }

    $env:RUSTUP_TOOLCHAIN = '1.90.0'
    $env:CARGO_NET_OFFLINE = 'true'
    $env:LIBXML2_STATIC = '1'
    $env:CARGO_TARGET_DIR = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $repoDir 'target' }
    & cargo metadata --locked --offline --format-version 1 > $null
    if ($LASTEXITCODE -ne 0) { throw 'Prepare the Cargo cache separately with cargo fetch --locked, then retry.' }
    & cargo build -p scorewing-cli --release --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'CLI build failed.' }
    # Avoid installer bundlers, which may download WiX, NSIS, or WebView2.
    & npm.cmd --offline --prefix apps/desktop run tauri -- build --no-bundle -- --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repoDir 'target' }
    & node tools/check-linkage.mjs "$targetDir/release/scorewing.exe"
    if ($LASTEXITCODE -ne 0) { throw 'CLI linkage check failed.' }
    & node tools/check-linkage.mjs --desktop "$targetDir/release/scorewing-desktop.exe"
    if ($LASTEXITCODE -ne 0) { throw 'Desktop linkage check failed.' }
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item "$targetDir/release/scorewing.exe", "$targetDir/release/scorewing-desktop.exe", 'docs/THIRD_PARTY_NOTICES.txt' dist/
    Write-Host "Built CLI and desktop executables in $repoDir/dist/ (desktop requires the WebView2 runtime)."
} finally {
    Pop-Location
}
