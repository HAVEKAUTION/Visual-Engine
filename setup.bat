@echo off
rem Visual Engine: one-time setup. Double-click this file.
rem It runs setup.ps1 (next to it), which installs what's needed and builds the app.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup.ps1" %*
echo.
pause
