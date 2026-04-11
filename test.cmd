@echo off
echo Running LockNote tests...
echo.

C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /target:exe /platform:x64 /optimize+ /out:LockNote.Tests.exe src\Crypto.cs src\Storage.cs src\Settings.cs tests\TestFramework.cs tests\CryptoTests.cs tests\SettingsTests.cs tests\StorageTests.cs tests\TestMain.cs

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo Test compilation failed.
    exit /b 1
)

echo.
LockNote.Tests.exe
set RESULT=%ERRORLEVEL%

del LockNote.Tests.exe 2>nul

exit /b %RESULT%
