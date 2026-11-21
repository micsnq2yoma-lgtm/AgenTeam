# Exercise NSIS and its registry writes only inside the disposable release runner.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or $env:GITHUB_REPOSITORY -ne 'micsnq2yoma-lgtm/AgenTeam') {
    throw 'This install test requires the disposable AgenTeam release runner.'
}

$version = (Get-Content -Raw package.json | ConvertFrom-Json).version
$release = Invoke-RestMethod 'https://api.github.com/repos/micsnq2yoma-lgtm/AgenTeam/releases/latest'
if ($release.tag_name -notmatch '^v(\d+\.\d+\.\d+)$') { throw 'Invalid public version' }
$baseline = $Matches[1]
if ([version]$baseline -eq [version]$version) {
    Write-Host "Latest already points to $version. Clean-install coverage applies."
    return
}
if ([version]$baseline -gt [version]$version) { throw "Cannot upgrade $baseline to $version" }

$baselineName = "AgenTeam_${baseline}_x64-setup.exe"
$asset = @($release.assets | Where-Object name -eq $baselineName)
if ($asset.Count -ne 1 -or $asset[0].digest -notmatch '^sha256:([a-fA-F0-9]{64})$') {
    throw 'Missing official Windows release asset or SHA-256'
}
$expectedHash = $Matches[1]
$oldPackage = Join-Path $env:RUNNER_TEMP $baselineName
$url = "https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/download/v$baseline/$baselineName"
Invoke-WebRequest -Uri $url -OutFile $oldPackage
if ((Get-FileHash -LiteralPath $oldPackage).Hash -ne $expectedHash) { throw 'Baseline installer SHA-256 mismatch' }

$target = Join-Path $env:RUNNER_TEMP 'AgenTeam-Upgrade'
function Install-AndCheckVersion($package, $expected) {
    $process = Start-Process -FilePath $package -ArgumentList '/S',"/D=$target" -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { throw "NSIS $expected exited with $($process.ExitCode)" }
    $exe = Get-Item -LiteralPath (Join-Path $target 'AgenTeam.exe')
    if ($exe.VersionInfo.ProductVersion -notin @($expected, "$expected.0")) {
        throw "Expected $expected, found $($exe.VersionInfo.ProductVersion)"
    }
    Write-Host "Installed $($exe.VersionInfo.ProductVersion)"
}

Install-AndCheckVersion $oldPackage $baseline
$skills = Join-Path $env:USERPROFILE '.agenteam/skills/upgrade-fixture'
New-Item -ItemType Directory $skills -Force | Out-Null
$fixture = Join-Path $skills 'SKILL.md'
Set-Content -LiteralPath $fixture -Value "AgenTeam upgrade $env:GITHUB_RUN_ID" -Encoding utf8
$fixtureHash = (Get-FileHash -LiteralPath $fixture).Hash
$candidate = Get-Item -LiteralPath "candidate/AgenTeam_${version}_x64-setup.exe"
Install-AndCheckVersion $candidate.FullName $version
if ((Get-FileHash -LiteralPath $fixture).Hash -ne $fixtureHash) { throw 'Upgrade changed the saved skill' }
$claude = Join-Path $target 'claude-agent-sdk/claude.exe'
if ((Get-FileHash -LiteralPath $claude).Hash -ne $env:EXPECTED_SDK_SHA256) { throw 'Installed SDK differs from build staging' }
& $claude --version
if ($LASTEXITCODE -ne 0) { throw 'Installed SDK failed to start' }
& (Join-Path $target 'nodejs/node.exe') (Join-Path $target 'cli/myagents.cjs') --help
if ($LASTEXITCODE -ne 0) { throw 'Installed AgenTeam CLI failed to start' }
Write-Host "Verified $baseline to $version, saved skill and packaged executables."
