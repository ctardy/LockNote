@echo off
echo Building LockNote...
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /target:winexe /platform:x64 /optimize+ /out:LockNote.exe src\Program.cs src\Storage.cs src\Crypto.cs src\Settings.cs src\Theme.cs src\LineNumberTextBox.cs src\EditorForm.cs src\CreatePasswordDialog.cs src\UnlockDialog.cs src\SearchBar.cs src\SettingsDialog.cs src\CloseConfirmDialog.cs src\GoToLineDialog.cs
if %ERRORLEVEL% EQU 0 (
    echo Build succeeded: LockNote.exe
) else (
    echo Build failed.
    exit /b 1
)
