@echo off
rem Visual Engine: start the app. Double-click this file (run setup.bat once first).
rem It rebuilds first if the code changed (for example after downloading an update).
cd /d "%~dp0"
set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust isn't installed yet. Double-click setup.bat first.
    pause
    exit /b 1
)
cargo run --release -- %*
if errorlevel 1 (
    echo.
    echo Visual Engine stopped with an error. The messages above say why.
    pause
)
