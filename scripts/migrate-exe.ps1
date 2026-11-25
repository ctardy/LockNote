<#
.SYNOPSIS
  Move the encrypted data of an existing LockNote exe into a new program build.

.DESCRIPTION
  LockNote keeps your notes inside its own exe:
    [program][16-byte marker "LN_DATA_" DE AD BE EF CA FE F0 0D][encrypted payload]
  Replacing the exe with the one from a release zip therefore loses the data.
  This script writes  new program + marker + the SAME payload  and checks it
  byte for byte. Nothing is decrypted, no password is needed.

  Always run it twice:
    1. -Target on a COPY in another folder, open it, check your data.
       (A copy has its own staging file: the production exe is not touched.)
    2. -Target equal to -OldExe: production. A backup is made first.

.PARAMETER OldExe
  The exe that holds your data (the one you use today).

.PARAMETER NewExe
  LockNote.exe taken from the release zip (a clean program, no data).

.PARAMETER Target
  Where to write the result. If it is the same path as OldExe, OldExe is
  backed up (<OldExe>.bak-<date>) and then replaced atomically.

.EXAMPLE
  .\migrate-exe.ps1 -OldExe C:\tools\LockNote.exe -NewExe C:\dl\LockNote.exe -Target C:\test\LockNote.exe
  .\migrate-exe.ps1 -OldExe C:\tools\LockNote.exe -NewExe C:\dl\LockNote.exe -Target C:\tools\LockNote.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OldExe,
    [Parameter(Mandatory = $true)][string]$NewExe,
    [Parameter(Mandatory = $true)][string]$Target
)

$ErrorActionPreference = 'Stop'

# Same constants as src/storage/mod.rs (get_marker) and src/crypto/mod.rs.
$Marker = [byte[]](0x4C, 0x4E, 0x5F, 0x44, 0x41, 0x54, 0x41, 0x5F, 0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xF0, 0x0D)
$MarkerLen = 16
$MinPayload = 92

function Fail([string]$msg) {
    Write-Host "ERREUR: $msg" -ForegroundColor Red
    exit 1
}

function Find-LastMarker([byte[]]$data) {
    # Backward scan from the end, like storage::find_marker.
    $i = $data.Length - $MarkerLen
    while ($i -ge 0) {
        $i = [Array]::LastIndexOf($data, [byte]0x4C, $i)
        if ($i -lt 0) { return -1 }
        $ok = $true
        for ($k = 1; $k -lt $MarkerLen; $k++) {
            if ($data[$i + $k] -ne $Marker[$k]) { $ok = $false; break }
        }
        if ($ok) { return $i }
        $i--
    }
    return -1
}

function Get-Sha256Hex([byte[]]$bytes) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-StagingPath([string]$exePath) {
    # src/storage/mod.rs get_tmp_path: first 8 bytes of SHA256(UPPERCASE path).
    $base = $env:LOCALAPPDATA
    if (-not $base) { $base = [System.IO.Path]::GetTempPath() }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $h = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($exePath.ToUpperInvariant()))
    } finally { $sha.Dispose() }
    $name = -join ($h[0..7] | ForEach-Object { $_.ToString('x2') })
    return (Join-Path (Join-Path $base 'LockNote') "$name.tmp")
}

# ---------------------------------------------------------------- inputs ----
foreach ($p in @($OldExe, $NewExe)) {
    if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { Fail "fichier introuvable: $p" }
}
$OldExe = [System.IO.Path]::GetFullPath($OldExe)
$NewExe = [System.IO.Path]::GetFullPath($NewExe)
$Target = [System.IO.Path]::GetFullPath($Target)
$production = $Target.Equals($OldExe, [StringComparison]::OrdinalIgnoreCase)

if ($NewExe.Equals($OldExe, [StringComparison]::OrdinalIgnoreCase)) { Fail "-NewExe et -OldExe sont le meme fichier." }
if ($NewExe.Equals($Target, [StringComparison]::OrdinalIgnoreCase)) { Fail "-Target ne doit pas etre -NewExe." }
if ((-not $production) -and (Test-Path -LiteralPath $Target)) { Fail "-Target existe deja (rien n'est ecrase): $Target" }

# 1. The exe being read or replaced must not be running. A LockNote started
#    from another path is left alone; when Windows hides a process path, it is
#    treated as a match (safer to stop than to risk the data).
$running = @(Get-Process -Name 'LockNote*' -ErrorAction SilentlyContinue | Where-Object {
    $procPath = $null
    try { $procPath = $_.Path } catch { }
    (-not $procPath) -or
    $procPath.Equals($OldExe, [StringComparison]::OrdinalIgnoreCase) -or
    $procPath.Equals($Target, [StringComparison]::OrdinalIgnoreCase)
})
if ($running.Count -gt 0) { Fail "LockNote tourne encore depuis ce fichier (PID $($running.Id -join ', ')). Fermez-le completement, puis relancez." }

# 2. A pending staging file holds the freshest data: let the app apply it first.
$staging = Get-StagingPath $OldExe
if (Test-Path -LiteralPath $staging) {
    Fail ("un fichier de sauvegarde en attente existe: $staging`n" +
          "Il contient vos donnees les plus recentes. Lancez l'ancien exe, ouvrez-le, fermez-le normalement, puis relancez ce script.")
}

# ------------------------------------------------------------- read + check -
$oldBytes = [System.IO.File]::ReadAllBytes($OldExe)
$pos = Find-LastMarker $oldBytes
if ($pos -lt 0) { Fail "aucune donnee LockNote dans $OldExe (marqueur absent). Rien n'a ete modifie." }
$payloadStart = $pos + $MarkerLen
$payloadLen = $oldBytes.Length - $payloadStart
if ($payloadLen -lt $MinPayload) { Fail "donnees trop courtes ($payloadLen octets) : fichier suspect. Rien n'a ete modifie." }
$payload = New-Object byte[] $payloadLen
[Array]::Copy($oldBytes, $payloadStart, $payload, 0, $payloadLen)
$payloadHash = Get-Sha256Hex $payload
Write-Host ("Donnees trouvees : {0} octets, SHA-256 {1}" -f $payloadLen, $payloadHash)

$newBytes = [System.IO.File]::ReadAllBytes($NewExe)
if ((Find-LastMarker $newBytes) -ge 0) { Fail "$NewExe contient deja des donnees LockNote : utilisez le programme propre du zip de la release." }
if ($newBytes.Length -lt 1MB) { Fail "$NewExe est trop petit pour etre LockNote ($($newBytes.Length) octets)." }

# ----------------------------------------------------------------- backup ----
if ($production) {
    $backup = "$OldExe.bak-" + (Get-Date -Format 'yyyyMMdd-HHmmss')
    [System.IO.File]::WriteAllBytes($backup, $oldBytes)
    if ((Get-Sha256Hex ([System.IO.File]::ReadAllBytes($backup))) -ne (Get-Sha256Hex $oldBytes)) {
        Remove-Item -LiteralPath $backup -Force
        Fail "la sauvegarde ne correspond pas a l'original. Rien n'a ete modifie."
    }
    Write-Host "Sauvegarde verifiee : $backup  (copiez-la AUSSI hors de ce PC)"
}

# ------------------------------------------------------------ build + verify -
$out = New-Object byte[] ($newBytes.Length + $MarkerLen + $payloadLen)
[Array]::Copy($newBytes, 0, $out, 0, $newBytes.Length)
[Array]::Copy($Marker, 0, $out, $newBytes.Length, $MarkerLen)
[Array]::Copy($payload, 0, $out, $newBytes.Length + $MarkerLen, $payloadLen)

if (-not $production) {
    $targetDir = [System.IO.Path]::GetDirectoryName($Target)
    if (-not (Test-Path -LiteralPath $targetDir)) { New-Item -ItemType Directory -Path $targetDir | Out-Null }
}
$staged = "$Target.new"
if (Test-Path -LiteralPath $staged) { Fail "$staged existe deja (reste d'un essai precedent ?). Supprimez-le, puis relancez." }
[System.IO.File]::WriteAllBytes($staged, $out)

$check = [System.IO.File]::ReadAllBytes($staged)
$cpos = Find-LastMarker $check
$okPrefix = ($cpos -eq $newBytes.Length)
$cpayload = $null
if ($cpos -ge 0) {
    $cpayload = New-Object byte[] ($check.Length - $cpos - $MarkerLen)
    [Array]::Copy($check, $cpos + $MarkerLen, $cpayload, 0, $cpayload.Length)
}
if (-not $okPrefix -or $null -eq $cpayload -or (Get-Sha256Hex $cpayload) -ne $payloadHash -or
    (Get-Sha256Hex $check[0..($newBytes.Length - 1)]) -ne (Get-Sha256Hex $newBytes)) {
    Remove-Item -LiteralPath $staged -Force
    Fail "la verification du fichier produit a echoue. Rien n'a ete modifie."
}

# ----------------------------------------------------------------- install ---
try {
    if ($production) {
        # [NullString]::Value: PowerShell turns a plain $null into "" here.
        [System.IO.File]::Replace($staged, $Target, [NullString]::Value)
    } else {
        [System.IO.File]::Move($staged, $Target)
    }
} catch {
    if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
    Fail "remplacement impossible ($($_.Exception.Message)). Le fichier d'origine n'a PAS ete modifie."
}

$final = [System.IO.File]::ReadAllBytes($Target)
$fpos = Find-LastMarker $final
$fpayload = New-Object byte[] ($final.Length - $fpos - $MarkerLen)
[Array]::Copy($final, $fpos + $MarkerLen, $fpayload, 0, $fpayload.Length)
if ((Get-Sha256Hex $fpayload) -ne $payloadHash) { Fail "ATTENTION: les donnees du fichier final different de l'original. Restaurez la sauvegarde." }

Write-Host ""
Write-Host "OK : $Target" -ForegroundColor Green
Write-Host ("  programme : $NewExe ({0} octets)" -f $newBytes.Length)
Write-Host ("  donnees   : identiques a l'octet pres ({0} octets, SHA-256 {1})" -f $payloadLen, $payloadHash)
if ($production) {
    Write-Host "Ouvrez LockNote, deverrouillez, verifiez onglets, coffre et categories."
    Write-Host "Retour arriere : fermez LockNote et recopiez la sauvegarde sur l'exe."
} else {
    Write-Host "C'est une COPIE : ouvrez-la, verifiez vos donnees, puis supprimez-la (elle contient vos donnees chiffrees)."
}
