@echo off
rem Double-click (or "Run as administrator") to install Password Alert for every user of this server.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install.ps1" %*
pause
