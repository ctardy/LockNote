@echo off
cd /d "%~dp0.."
echo Building LockNote (Rust)...

set PATH=C:\dev\tools\mingw\mingw64\bin;%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;%PATH%

cargo build --release 2> build-errors.log

if %ERRORLEVEL% EQU 0 (
    if not exist build mkdir build
    copy /y target\release\locknote.exe build\LockNote.exe >nul
    del build-errors.log 2>nul
    echo Build succeeded: build\LockNote.exe
) else (
    echo Build failed. See build-errors.log for details.
    exit /b 1
)
