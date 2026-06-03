@echo off
REM Launch Codec standalone (Rust audio + Compose UI)
REM Usage:
REM   run.bat                              loop the bundled loop.wav (default)
REM   run.bat --tone sine                  440Hz sine test tone
REM   run.bat --tone sine --freq 880       880Hz sine test tone
REM   run.bat --tone noise                 white noise
REM   run.bat --tone sweep                 frequency sweep
REM   run.bat --wav sample.wav             loop a different WAV file
REM Close this window to shut everything down.

setlocal
set "SCRIPT_DIR=%~dp0"
set "RUST_ARGS=%*"

REM Default source: loop the bundled loop.wav through the effect.
REM Passing --tone / --freq / --wav overrides this and skips the loop.
set "SOURCE_SPECIFIED="
echo %*| findstr /C:"--tone" /C:"--freq" /C:"--wav" >nul && set "SOURCE_SPECIFIED=1"

if not defined SOURCE_SPECIFIED (
    if exist "%SCRIPT_DIR%loop.wav" (
        echo Looping loop.wav ^(pass --tone sine for a test tone^)
        set "RUST_ARGS=--wav "%SCRIPT_DIR%loop.wav" %*"
    ) else (
        echo loop.wav not found at %SCRIPT_DIR%loop.wav - using default test tone
    )
)

echo ▶ Starting Rust audio engine...
cd /d "%SCRIPT_DIR%plugin"
start "Codec-Audio" /B cargo run --features standalone --bin codec-standalone -- %RUST_ARGS%

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
