@echo off
cd /d "%~dp0.."
echo Building LockNote (Rust)...

rem ── Stage 1: build the companion swap helper ──
rem   locknote-swap.exe performs the .tmp -> .exe atomic replacement via
rem   MoveFileExW after the main process exits. It is embedded into the main
rem   binary through include_bytes!("assets/locknote-swap-embedded.exe").
echo Building swap helper...
cargo build --release --bin locknote-swap 2> build-errors.log
if not %ERRORLEVEL% EQU 0 (
    echo Swap helper build failed. See build-errors.log for details.
    exit /b 1
)

if not exist assets mkdir assets
copy /y target\release\locknote-swap.exe assets\locknote-swap-embedded.exe >nul
if not %ERRORLEVEL% EQU 0 (
    echo Failed to stage embedded swap helper.
    exit /b 1
)

rem ── Stage 2: build the main binary ──
rem   cargo clean removes the previous artefact so the updated embedded
rem   helper is re-ingested by include_bytes!.
cargo clean -p locknote --release >nul 2>&1
cargo build --release --bin locknote 2> build-errors.log

if %ERRORLEVEL% EQU 0 (
    if not exist build mkdir build
    copy /y target\release\locknote.exe build\LockNote.exe >nul
    del build-errors.log 2>nul
    echo Build succeeded: build\LockNote.exe
) else (
    echo Build failed. See build-errors.log for details.
    exit /b 1
)
