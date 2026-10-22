@echo off
rem FM-1 SMK-37 Desk: both synths in one window, with the firmware picker (F1).
rem Works from the zip's top folder (bin\fm1-desk.exe) or from windows\ in a source checkout.
set "D=%~dp0bin\fm1-desk.exe"
if not exist "%D%" set "D=%~dp0..\crates\fm1-desk\target\release\fm1-desk.exe"
if not exist "%D%" (
  echo fm1-desk.exe not found: use the zip from GitHub Actions, or build it: windows\build.ps1
  pause
  exit /b 1
)
start "" "%D%"
