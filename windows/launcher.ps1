# FM-1 / SMK-37 emulator launcher for Windows 10/11 (Windows PowerShell 5.1 or PowerShell 7).
# Pick a board, pick a firmware, play. Started by Start.cmd.
#
# Finds fm1-live.exe in bin\ (the release zip) or target\release\ (a build from source).
# Firmwares live in firmwares\fm1\ and firmwares\smk37\ (windows\fetch-firmwares.mjs, needs Node.js).
# Settings (last board and firmware, clock, MIDI) are kept in launcher-settings.json.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "web\firmwares.json"))) { $Root = $PSScriptRoot }

$Boards = [ordered]@{
    "fm1"   = @{ Name = "M-VAVE FM-1";  Stock = "FM-1.fwsc" }
    "smk37" = @{ Name = "M-VAVE SMK-37"; Stock = "SMK-37 Pro_016.fwsc" }
}
$SettingsPath = Join-Path $Root "launcher-settings.json"

function Find-Exe {
    foreach ($p in @("bin\fm1-live.exe", "target\release\fm1-live.exe", "fm1-live.exe")) {
        $f = Join-Path $Root $p
        if (Test-Path $f) { return $f }
    }
    return $null
}

function Load-Settings {
    $s = @{ board = "fm1"; last = @{ fm1 = ""; smk37 = "" }; mhz = 240; midi = "all" }
    if (Test-Path $SettingsPath) {
        try {
            $j = Get-Content $SettingsPath -Raw | ConvertFrom-Json
            if ($j.board -and $Boards.Contains([string]$j.board)) { $s.board = [string]$j.board }
            if ($j.mhz) { $s.mhz = [int]$j.mhz }
            if ($j.midi) { $s.midi = [string]$j.midi }
            if ($j.last) { $s.last.fm1 = [string]$j.last.fm1; $s.last.smk37 = [string]$j.last.smk37 }
        } catch { Write-Host "(launcher-settings.json unreadable, using defaults)" -ForegroundColor DarkYellow }
    }
    return $s
}

function Save-Settings($s) {
    $s | ConvertTo-Json -Depth 4 | Set-Content -Path $SettingsPath -Encoding UTF8
}

# The downloaded firmwares of a board, in catalog order where the index has it: list of objects.
function Get-Firmwares($board) {
    $dir = Join-Path $Root "firmwares\$board"
    $index = Join-Path $dir "index.json"
    $list = @()
    if (Test-Path $index) {
        $j = Get-Content $index -Raw | ConvertFrom-Json
        foreach ($p in $j.firmwares.PSObject.Properties) {
            $f = Join-Path $dir $p.Value.file
            if (Test-Path $f) {
                $list += [pscustomobject]@{ Id = $p.Name; Name = $p.Value.name; Version = $p.Value.version
                                            About = $p.Value.about; Path = $f }
            }
        }
    }
    return ,$list
}

function Invoke-Fetch($fetchArgs) {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        Write-Host ""
        Write-Host "Downloading firmwares needs Node.js (it runs the same download code as the web page)." -ForegroundColor Yellow
        Write-Host "Install it once:  winget install OpenJS.NodeJS.LTS   (or nodejs.org), then reopen this window."
        Read-Host "Enter to go back" | Out-Null
        return
    }
    & node (Join-Path $Root "windows\fetch-firmwares.mjs") @fetchArgs
    Read-Host "Enter to go back" | Out-Null
}

function Open-OwnFwsc($board) {
    Add-Type -AssemblyName System.Windows.Forms
    $dlg = New-Object System.Windows.Forms.OpenFileDialog
    $dlg.Title = "Open a firmware package (.fwsc), e.g. " + $Boards[$board].Stock
    $dlg.Filter = "Firmware packages (*.fwsc)|*.fwsc|All files (*.*)|*.*"
    if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { return $dlg.FileName }
    return $null
}

function Start-Emu($exe, $fwsc, $s) {
    $emuArgs = @("--fwsc", $fwsc, "--mhz", [string]$s.mhz, "--midi", $s.midi)
    Write-Host ""
    Write-Host "> fm1-live $($emuArgs -join ' ')" -ForegroundColor DarkGray
    Write-Host "  Keys: A..' play, W E T Y U O P sharps, Z/X octave, Space play, R rec, 1-8 pads (SMK-37)," -ForegroundColor DarkGray
    Write-Host "        arrows and , . turn the encoders, Esc closes the window." -ForegroundColor DarkGray
    & $exe @emuArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "fm1-live ended with code $LASTEXITCODE" -ForegroundColor Yellow
        Read-Host "Enter to go back" | Out-Null
    }
}

$exe = Find-Exe
$s = Load-Settings
while ($true) {
    Clear-Host
    $board = $s.board
    $fws = Get-Firmwares $board
    Write-Host "FM-1 / SMK-37 emulator" -ForegroundColor Cyan
    Write-Host ("Board: {0}    clock {1} MHz    MIDI in: {2}" -f $Boards[$board].Name, $s.mhz, $s.midi)
    if (-not $exe) { Write-Host "fm1-live.exe not found (bin\ or target\release\): build it with windows\build.ps1" -ForegroundColor Red }
    Write-Host ""
    if ($fws.Count -eq 0) {
        Write-Host "  No firmwares downloaded for this board yet: press D." -ForegroundColor Yellow
    }
    for ($i = 0; $i -lt $fws.Count; $i++) {
        $mark = if ($fws[$i].Id -eq $s.last[$board]) { "*" } else { " " }
        Write-Host ("  {0}{1,3}  {2} {3}" -f $mark, ($i + 1), $fws[$i].Name, $fws[$i].Version)
    }
    Write-Host ""
    Write-Host "  number = play   Enter = play the * one   B = switch board   O = open own .fwsc"
    Write-Host "  D = download missing   U = update all (both boards)   C = clock   M = MIDI input   Q = quit"
    $c = (Read-Host ">").Trim()
    switch -Regex ($c) {
        "^[qQ]$" { Save-Settings $s; return }
        "^[bB]$" { $keys = @($Boards.Keys); $s.board = $keys[($keys.IndexOf($board) + 1) % $keys.Count]; Save-Settings $s }
        "^[dD]$" { Invoke-Fetch @("--board", $board, "--missing") }
        "^[uU]$" { Invoke-Fetch @() }
        "^[cC]$" {
            $v = Read-Host "Clock in MHz (240 = the real chip; lower it if the sound crackles)"
            if ($v -match "^\d+$" -and [int]$v -ge 10 -and [int]$v -le 480) { $s.mhz = [int]$v; Save-Settings $s }
        }
        "^[mM]$" {
            $v = Read-Host "MIDI input: all, none, or part of a device's name"
            if ($v.Trim()) { $s.midi = $v.Trim(); Save-Settings $s }
        }
        "^[oO]$" {
            if (-not $exe) { break }
            $f = Open-OwnFwsc $board
            if ($f) { Start-Emu $exe $f $s }
        }
        "^\d+$" {
            $n = [int]$c - 1
            if ($exe -and $n -ge 0 -and $n -lt $fws.Count) {
                $s.last[$board] = $fws[$n].Id; Save-Settings $s
                Start-Emu $exe $fws[$n].Path $s
            }
        }
        "^$" {
            $f = $fws | Where-Object { $_.Id -eq $s.last[$board] } | Select-Object -First 1
            if ($exe -and $f) { Start-Emu $exe $f.Path $s }
        }
    }
}
