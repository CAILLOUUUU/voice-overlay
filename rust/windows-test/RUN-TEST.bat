@echo off
title Voice Overlay (Rust) - Windows test
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0test-windows.ps1"
pause
