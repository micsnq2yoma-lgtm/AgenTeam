
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:GITHUB_REPOSITORY -ne 'micsnq2yoma-lgtm/AgenTeam' -or $env:RUNNER_OS -ne 'Windows') {throw 'Requires the disposable release runner'}
$root = $env:QA_ROOT
$oldFile = Join-Path $root 'original/AgenTeam_0.6.0_x64-setup.exe'
if ((Get-FileHash -LiteralPath $oldFile).Hash -ne 'D89876323C392CAF338FA40B067EC9F47863A84642E9F9821AE723B3EB4A34C3') { throw 'Original candidate digest mismatch' }
$installer = Start-Process -FilePath $oldFile -ArgumentList '/S' -PassThru -Wait -WindowStyle Hidden
if ($installer.ExitCode -ne 0) { throw "0.6.0 installer failed: $($installer.ExitCode)" }
Write-Host 'Original 0.6.0 installer finished'
$desktop = Get-Item -LiteralPath (Join-Path $env:LOCALAPPDATA 'AgenTeam/AgenTeam.exe')
if ($desktop.VersionInfo.ProductVersion -notmatch '^0\.6\.0(?:\.0)?$') {throw "Unexpected original desktop version: $($desktop.VersionInfo.ProductVersion)"}
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=19222 --remote-debugging-address=127.0.0.1'
$policy = 'HKCU:\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
New-Item -Path $policy -Force | Out-Null
New-ItemProperty -Path $policy -Name 'AgenTeam.exe' -Value $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -PropertyType String -Force | Out-Null
& node './qa/upgrade-public.cjs'
$driverExit = $LASTEXITCODE
if ($driverExit -ne 0) {exit $driverExit}
$actual = (Get-Item -LiteralPath $desktop.FullName).VersionInfo.ProductVersion
if ($actual -notmatch '^0\.6\.1(?:\.0)?$') {throw "Unexpected upgraded PE version $actual"}
Write-Host "Verified upgraded PE version $actual"
