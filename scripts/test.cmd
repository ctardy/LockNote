@echo off
cd /d "%~dp0.."
echo Running LockNote tests (Rust)...
echo.

set PATH=C:\dev\tools\mingw\mingw64\bin;%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;%PATH%

cargo test

set RESULT=%ERRORLEVEL%
exit /b %RESULT%
