param([switch]$Rehearse)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:GITHUB_REPOSITORY -ne 'micsnq2yoma-lgtm/AgenTeam' -or $env:RUNNER_OS -ne 'Windows') {throw 'Requires the disposable release runner'}
$root = $env:QA_ROOT
$oldFile = Join-Path $root 'original/AgenTeam_0.6.0_x64-setup.exe'
if ((Get-FileHash -LiteralPath $oldFile).Hash -ne 'D89876323C392CAF338FA40B067EC9F47863A84642E9F9821AE723B3EB4A34C3') { throw 'Original candidate digest mismatch' }
$installer = Start-Process -FilePath $oldFile -ArgumentList '/S' -PassThru -Wait -WindowStyle Hidden
if ($installer.ExitCode -ne 0) { throw "0.6.0 installer failed: $($installer.ExitCode)" }
$desktop = Get-Item -LiteralPath (Join-Path $env:LOCALAPPDATA 'AgenTeam/AgenTeam.exe')
if ($desktop.VersionInfo.ProductVersion -notmatch '^0\.6\.0(?:\.0)?$') {throw "Unexpected original desktop version: $($desktop.VersionInfo.ProductVersion)"}
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=19222 --remote-debugging-address=127.0.0.1'
$policy = 'HKCU:\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
New-Item -Path $policy -Force | Out-Null
New-ItemProperty -Path $policy -Name 'AgenTeam.exe' -Value $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -PropertyType String -Force | Out-Null
if ($Rehearse) { & node './qa/upgrade.cjs' --rehearse; exit $LASTEXITCODE }
$shells = @(Get-ChildItem -LiteralPath (Join-Path $root 'shell') -Filter '*.exe' -Recurse | Where-Object { $_.Name -in @('myagents.exe','AgenTeam.exe') })
if (!$shells.Count) { throw 'Missing separately built 0.6.0 shell' }
$env:QA_BASELINE_SHELL = ($shells | Sort-Object @{Expression={$_.Name -eq 'AgenTeam.exe'};Descending=$true} | Select-Object -First 1).FullName
Write-Host "Testing shell $([IO.Path]::GetFileName($env:QA_BASELINE_SHELL))"
$cert = Join-Path $root 'localhost.crt'
$key = Join-Path $root 'localhost.key'
& openssl req -x509 -newkey rsa:2048 -noenc -days 1 -keyout $key -out $cert -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1' -addext 'basicConstraints=critical,CA:FALSE' -addext 'extendedKeyUsage=serverAuth'
if ($LASTEXITCODE -ne 0) { throw 'Localhost test certificate generation failed' }
certutil -user -addstore Root $cert | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not trust this localhost leaf certificate on this one-off runner' }
& node './qa/upgrade.cjs'
$driverExit = $LASTEXITCODE
if ($driverExit -eq 0) {
    $actual = (Get-Item -LiteralPath $desktop.FullName).VersionInfo.ProductVersion
    if ($actual -notmatch '^0\.6\.1(?:\.0)?$') {throw "Unexpected updated version resource $actual"}
    Write-Host "Verified installed PE version $actual"
}
exit $driverExit
