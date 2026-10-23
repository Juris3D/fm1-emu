# Build fm1-desk.exe (the app), fm1-live.exe and fm1-emu.exe from source on Windows, and optionally the zip.
#   powershell -ExecutionPolicy Bypass -File windows\build.ps1            build
#   powershell -ExecutionPolicy Bypass -File windows\build.ps1 -Package   build + dist\fm1-emu-windows-x64.zip
#   ... -Bench                                                            build + speed check (FM-1 SID)
# Needs Rust (rustup.rs, the default MSVC toolchain) and the Visual Studio Build Tools
# ("Desktop development with C++"); rustup's installer offers them.
param([switch]$Package, [switch]$Bench)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found: install Rust from https://rustup.rs (it also offers the C++ build tools)"
}
cargo build --release --locked -p fm1-live -p fm1-emu
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
& target\release\fm1-live.exe --version
# the app: its own workspace (crates/fm1-desk), its own target folder
cargo build --release --locked --manifest-path crates\fm1-desk\Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "cargo build (fm1-desk) failed" }
if (-not (Test-Path crates\fm1-desk\target\release\fm1-desk.exe)) { throw "no fm1-desk.exe" }

if ($Bench) {
    # FM-1 SID's package is in the repository: no download needed. "x real time" over 1.0 = keeps up.
    & target\release\fm1-live.exe --fwsc fm1-sid\web\sidplayer.fwsc --bench 3000
}

if ($Package) {
    $v = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $stage = Join-Path $Root "dist\stage"
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Force -Path "$stage\bin", "$stage\web\smk37", "$stage\windows", "$stage\examples" | Out-Null
    Copy-Item target\release\fm1-live.exe, target\release\fm1-emu.exe, crates\fm1-desk\target\release\fm1-desk.exe "$stage\bin\"
    Copy-Item web\fwsc.js, web\firmwares.json "$stage\web\"
    Copy-Item web\smk37\firmwares.json "$stage\web\smk37\"
    Copy-Item windows\launcher.ps1, windows\fetch-firmwares.mjs, windows\README.md, "windows\Console launcher.cmd" "$stage\windows\"
    Copy-Item windows\Start.cmd "$stage\"
    Copy-Item fm1-sid\web\sidplayer.fwsc "$stage\examples\fm1-sid.fwsc"
    Copy-Item LICENSE "$stage\LICENSE.txt"
    Set-Content -Path "$stage\VERSION.txt" -Value "fm1-emu $v (upstream joakimeriksson/fm1-emu), Windows x64 build"
    $zip = Join-Path $Root "dist\fm1-emu-$v-windows-x64.zip"
    if (Test-Path $zip) { Remove-Item $zip }
    Compress-Archive -Path "$stage\*" -DestinationPath $zip
    Write-Host "packaged: $zip"

    # the shareable one: the app, its readme and the licence, in one folder
    $dv = (Select-String -Path crates\fm1-desk\Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $name = "FM-1 SMK-37 Desk"
    $share = Join-Path $Root "dist\share\$name"
    if (Test-Path (Join-Path $Root "dist\share")) { Remove-Item (Join-Path $Root "dist\share") -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $share | Out-Null
    Copy-Item crates\fm1-desk\target\release\fm1-desk.exe (Join-Path $share "$name.exe")
    Copy-Item windows\SHARE-README.txt (Join-Path $share "README.txt")
    Copy-Item LICENSE (Join-Path $share "LICENSE.txt")
    $szip = Join-Path $Root "dist\FM-1-SMK-37-Desk-$dv-windows-x64.zip"
    if (Test-Path $szip) { Remove-Item $szip }
    Compress-Archive -Path $share -DestinationPath $szip
    Write-Host "packaged: $szip"
}
