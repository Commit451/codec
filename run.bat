@echo off
REM Launch Compose VST standalone (Rust audio + Compose UI)
REM Usage:
REM   run.bat                              default 440Hz sine
REM   run.bat --tone noise                 white noise
REM   run.bat --tone sweep                 frequency sweep
REM   run.bat --wav sample.wav             loop a WAV file
REM Close this window to shut everything down.

setlocal
set SCRIPT_DIR=%~dp0

echo ▶ Starting Rust audio engine...
cd /d "%SCRIPT_DIR%plugin"
start "ComposeVST-Audio" /B cargo run --features standalone --bin compose-vst-standalone -- %*

REM Give the Rust IPC server a moment to bind
timeout /t 2 /nobreak >nul

echo ▶ Starting Compose UI...
cd /d "%SCRIPT_DIR%ui"
if not exist gradlew.bat (
    echo   (generating Gradle wrapper...)
    call gradle wrapper
)
start "ComposeVST-UI" /B gradlew.bat run

echo.
echo ✅ Both running. Close this window to stop.
echo.

pause >nul
