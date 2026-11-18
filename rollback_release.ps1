[CmdletBinding()]
param([Parameter(Mandatory)][ValidatePattern('^v\d+\.\d+\.\d+$')][string]$HealthyTag)
$ErrorActionPreference = 'Stop'
$repo = 'micsnq2yoma-lgtm/AgenTeam'
$release = gh release view $HealthyTag --repo $repo --json isDraft,isPrerelease,assets | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or !$release -or $release.isDraft -or $release.isPrerelease -or $release.assets.name -notcontains 'latest.json') {
    throw 'Select a published stable AgenTeam release with its update manifest.'
}
gh release edit $HealthyTag --repo $repo --latest
exit $LASTEXITCODE
