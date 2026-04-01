@echo off
REM Launch Compose VST standalone (Rust audio + Compose UI)
REM Usage: run.bat [--tone sine|noise|sweep] [--freq 440]
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

REM Wait for user to close — on window close, child processes terminate
pause >nul
