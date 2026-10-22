@echo off
rem The console launcher (the text menu): the app before FM-1 SMK-37 Desk. Next to launcher.ps1.
set "L=%~dp0launcher.ps1"
if not exist "%L%" set "L=%~dp0windows\launcher.ps1"
title FM-1 / SMK-37 emulator
powershell -NoProfile -ExecutionPolicy Bypass -File "%L%"
