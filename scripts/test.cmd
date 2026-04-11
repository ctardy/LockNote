@echo off
cd /d "%~dp0.."
echo Running LockNote tests (Rust)...
echo.

set PATH=%PATH%;%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;C:\msys64\mingw64\bin

cargo test

set RESULT=%ERRORLEVEL%
exit /b %RESULT%
