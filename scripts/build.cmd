@echo off
cd /d "%~dp0.."
echo Building LockNote (Rust)...

set PATH=%PATH%;%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;C:\msys64\mingw64\bin

cargo build --release

if %ERRORLEVEL% EQU 0 (
    if not exist build mkdir build
    copy /y target\release\locknote.exe build\LockNote.exe >nul
    echo Build succeeded: build\LockNote.exe
) else (
    echo Build failed.
    exit /b 1
)
