. (Join-Path $PSScriptRoot 'download-build-file.ps1')

function Get-GitInstaller {
    param([Parameter(Mandatory)][string]$ProjectDir)
    # Git for Windows version - update this when upgrading
    # Also update version comment in: src-tauri/nsis/installer.nsi (search "Current version: Git for Windows")
    # Download page: https://git-scm.com/downloads/win
    $GitVersion = "2.52.0"
    $GitUrl = "https://github.com/git-for-windows/git/releases/download/v$GitVersion.windows.1/Git-$GitVersion-64-bit.exe"

    $NsisDir = Join-Path $ProjectDir "src-tauri\nsis"
    if (-not (Test-Path $NsisDir)) {
        New-Item -ItemType Directory -Path $NsisDir -Force | Out-Null
    }

    $GitFile = Join-Path $NsisDir "Git-Installer.exe"

    Write-Host "下载 Git for Windows (v$GitVersion)..." -ForegroundColor Blue

    # Existence alone accepted partial downloads from older setup runs.
    $ValidInstaller = { param($Path) (Get-AuthenticodeSignature -LiteralPath $Path).Status -eq 'Valid' }
    if (-not (Test-Path -LiteralPath $GitFile -PathType Leaf) -or -not (& $ValidInstaller $GitFile)) {
        Write-Host "  下载 Git 安装包..." -ForegroundColor Cyan
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Get-BuildDownload -Uri $GitUrl -OutFile $GitFile -Validate $ValidInstaller
            Write-Host "  OK - Git installer downloaded" -ForegroundColor Green
        } catch {
            Write-Host "  下载失败: $_" -ForegroundColor Red
            Write-Host "  请手动下载: $GitUrl" -ForegroundColor Yellow
            Write-Host "  并保存到: $GitFile" -ForegroundColor Yellow
            throw "Git installer download failed"
        }
    } else {
        Write-Host "  OK - Git installer (already exists)" -ForegroundColor Green
    }
    Write-Host "OK - Git installer ready" -ForegroundColor Green
}
