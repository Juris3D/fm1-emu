@echo off
rem FM-1 / SMK-37 emulator launcher. Works from windows\ (a source checkout) or the zip's root.
set "L=%~dp0launcher.ps1"
if not exist "%L%" set "L=%~dp0windows\launcher.ps1"
title FM-1 / SMK-37 emulator
powershell -NoProfile -ExecutionPolicy Bypass -File "%L%"
