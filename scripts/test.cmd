@echo off
setlocal
cd /d "%~dp0.."
echo Running LockNote tests (Rust)...
echo.

rem ── The swap helper embedded by include_bytes! (see build.cmd, stage 1) ──
cargo build --release --bin locknote-swap
if not %ERRORLEVEL% EQU 0 (
    echo Swap helper build failed.
    exit /b 1
)
if not exist assets mkdir assets
copy /y target\release\locknote-swap.exe assets\locknote-swap-embedded.exe >nul
if not %ERRORLEVEL% EQU 0 (
    echo Failed to stage embedded swap helper.
    exit /b 1
)

rem ── Tests never touch the real %LOCALAPPDATA%\LockNote: the staging
rem   directory may hold the only copy of the latest save of a real note. ──
set "LN_TEST_DIR=%TEMP%\locknote-tests\%RANDOM%%RANDOM%"
mkdir "%LN_TEST_DIR%"
if not %ERRORLEVEL% EQU 0 (
    echo Cannot create the test directory %LN_TEST_DIR%.
    exit /b 1
)
set "LOCALAPPDATA=%LN_TEST_DIR%"
set "TEMP=%LN_TEST_DIR%"
set "TMP=%LN_TEST_DIR%"
echo Test directory: %LN_TEST_DIR%

cargo test

exit /b %ERRORLEVEL%
