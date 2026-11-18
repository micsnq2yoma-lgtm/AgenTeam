[CmdletBinding()]
param([Parameter(Mandatory)][string]$Directory)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$directoryPath = (Resolve-Path -LiteralPath $Directory).Path
$version = (Get-Content -Raw -LiteralPath (Join-Path $root 'package.json') | ConvertFrom-Json).version
$name = "AgenTeam_${version}_x64-setup.exe"
$installer = Get-Item -LiteralPath (Join-Path $directoryPath $name)
$record = "$((Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $name"
if ((Get-Content -LiteralPath (Join-Path $directoryPath 'SHA256SUMS')) -cnotcontains $record) {
    throw 'Installer differs from the candidate build checksum.'
}

$previousPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
$previousPath = $env:TAURI_SIGNING_PRIVATE_KEY_PATH
try {
    if (-not ($env:TAURI_SIGNING_PRIVATE_KEY -or $previousPath)) {
        $keyDir = Join-Path $env:LOCALAPPDATA 'AgenTeam-Release'
        $env:TAURI_SIGNING_PRIVATE_KEY_PATH = (Get-Item -LiteralPath (Join-Path $keyDir 'updater.key')).FullName
        $protected = ConvertTo-SecureString (Get-Content -Raw -LiteralPath (Join-Path $keyDir 'updater.password.dpapi'))
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Net.NetworkCredential]::new('', $protected).Password
    }
    & node (Join-Path $root 'node_modules/@tauri-apps/cli/tauri.js') signer sign $installer.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Tauri signing failed' }
} finally {
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $previousPassword
    $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $previousPath
}
& node (Join-Path $root 'scripts/agenteam-release.mjs') $directoryPath
if ($LASTEXITCODE -ne 0) { throw 'Release verification failed' }
Write-Host "Verified candidate and release metadata in $directoryPath"
