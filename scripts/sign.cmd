@echo off
cd /d "%~dp0.."

if "%~1"=="" (
    echo Usage: scripts\sign.cmd ^<file.exe^>
    echo Example: scripts\sign.cmd build\LockNote.exe
    exit /b 1
)

if not exist "%~1" (
    echo File not found: %~1
    exit /b 1
)

echo Signing %~1 with Azure Trusted Signing...

signtool sign /v /fd SHA256 ^
    /tr http://timestamp.acs.microsoft.com /td SHA256 ^
    /dlib "%USERPROFILE%\.dotnet\tools\.store\azure.codesigning.dlib\*\azure.codesigning.dlib\*\tools\net8.0\any\Azure.CodeSigning.Dlib.dll" ^
    /dmdf scripts\sign-metadata.json ^
    "%~1"

if %ERRORLEVEL% EQU 0 (
    echo Signed successfully: %~1
    signtool verify /pa "%~1"
) else (
    echo Signing failed.
    exit /b 1
)
