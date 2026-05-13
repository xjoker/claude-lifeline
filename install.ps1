# claude-lifeline installer for Windows (PowerShell)
# Usage:
#   Install/Upgrade: irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex
#   Uninstall:       & { $env:ACTION='uninstall'; irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }
#   Dev (from repo):  $env:ACTION='dev'; .\install.ps1

$ErrorActionPreference = "Stop"

$Repo = "xjoker/claude-lifeline"
$InstallDir = "$env:USERPROFILE\.claude\bin"
$BinName = "claude-lifeline.exe"
$Settings = "$env:USERPROFILE\.claude\settings.json"
$Target = "x86_64-pc-windows-msvc"
$Action = if ($env:ACTION) { $env:ACTION } else { "install" }
# refreshInterval=15 让 cache TTL 倒计时和 quota ETA 接近实时（CC 默认事件驱动，
# idle 时不刷新）。15s 在视觉流畅度和 CPU 开销之间取平衡
$DefaultRefreshInterval = 15
$StatusLineCmd = "~/.claude/bin/claude-lifeline"

function Set-StatusLineConfig {
    # 在 settings.json 中写入 statusLine。保留用户已有的 refreshInterval（如果手动调过）
    $needCreate = -not (Test-Path $Settings)
    if ($needCreate) {
        New-Item -ItemType Directory -Force -Path (Split-Path $Settings) | Out-Null
        @{statusLine = @{type = "command"; command = $StatusLineCmd; refreshInterval = $DefaultRefreshInterval}} `
            | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
        Write-Host "Created $Settings"
        return
    }

    $json = Get-Content $Settings -Raw | ConvertFrom-Json
    $currentCmd = if ($json.statusLine -and $json.statusLine.command) { $json.statusLine.command } else { "" }
    $hasInterval = $json.statusLine -and $json.statusLine.PSObject.Properties.Name -contains "refreshInterval" `
                   -and $json.statusLine.refreshInterval -gt 0
    if ($currentCmd -eq $StatusLineCmd -and $hasInterval) {
        Write-Host "settings.json already configured"
        return
    }

    Copy-Item $Settings "$Settings.bak"
    $existingInterval = if ($hasInterval) { $json.statusLine.refreshInterval } else { $DefaultRefreshInterval }
    $json | Add-Member -Force -MemberType NoteProperty -Name "statusLine" -Value @{
        type = "command"
        command = $StatusLineCmd
        refreshInterval = $existingInterval
    }
    $json | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
    Write-Host "Updated settings.json (backup: settings.json.bak)"
}

# ── Install 流程：下载最新二进制 + 配 settings.json（幂等，等同 upgrade） ──

function Invoke-DoInstall {
    Write-Host "Platform: Windows/x86_64 -> $Target"

    $Latest = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name
    if (-not $Latest) {
        Write-Error "Failed to fetch latest release"
        exit 1
    }
    $LatestVer = $Latest.TrimStart("v")

    $needDownload = $true
    if (Test-Path "$InstallDir\$BinName") {
        try {
            $Current = & "$InstallDir\$BinName" --version 2>$null
            Write-Host "Current: $Current, Latest: $Latest"
            if ($Current -eq "claude-lifeline $LatestVer") {
                Write-Host "Binary already up to date."
                $needDownload = $false
            }
        } catch {}
    }

    if ($needDownload) {
        $Url = "https://github.com/$Repo/releases/download/$Latest/claude-lifeline-$Target.exe"
        Write-Host "Downloading $Latest..."
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        Invoke-WebRequest -Uri $Url -OutFile "$InstallDir\$BinName"
        Write-Host "Installed to $InstallDir\$BinName"
    }

    Set-StatusLineConfig
}

if ($Action -eq "mini" -or $Action -eq "standard") {
    Write-Host "Note: the 'mini'/'standard' subcommands are removed in this version."
    Write-Host "      claude-lifeline now ships with the mini layout only — running plain install."
    Invoke-DoInstall
    exit 0
}

# ── Uninstall ──

if ($Action -eq "uninstall") {
    Write-Host "Uninstalling claude-lifeline..."
    if (Test-Path "$InstallDir\$BinName") {
        Remove-Item "$InstallDir\$BinName" -Force
        Write-Host "Removed $InstallDir\$BinName"
    }
    if (Test-Path $Settings) {
        $json = Get-Content $Settings -Raw | ConvertFrom-Json
        if ($json.statusLine) {
            Copy-Item $Settings "$Settings.bak"
            $json.PSObject.Properties.Remove("statusLine")
            $json | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
            Write-Host "Removed statusLine from settings.json (backup: settings.json.bak)"
        }
    }
    Write-Host "Done! Restart Claude Code to apply."
    exit 0
}

# ── Dev: local source build ──

if ($Action -eq "dev") {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    if (-not (Test-Path "$ScriptDir\Cargo.toml")) {
        Write-Error "dev mode must be run from the repo root (Cargo.toml not found)"
        exit 1
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "cargo not found in PATH"
        exit 1
    }

    Write-Host "Building release binary from source..."
    Push-Location $ScriptDir
    try { cargo build --release } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $Built = "$ScriptDir\target\release\$BinName"
    if (-not (Test-Path $Built)) {
        Write-Error "build output missing: $Built"
        exit 1
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item $Built "$InstallDir\$BinName" -Force

    $Version = & "$InstallDir\$BinName" --version 2>$null
    Write-Host "Installed dev build to $InstallDir\$BinName ($Version)"

    Set-StatusLineConfig

    Write-Host ""
    Write-Host "Done! Restart Claude Code to see the dev build."
    exit 0
}

# ── Default: install or upgrade (treated identically) ──

if ($Action -ne "install" -and $Action -ne "upgrade") {
    Write-Error "Unknown action: $Action. Use install | upgrade | uninstall | dev | mini | standard"
    exit 1
}

Invoke-DoInstall
Write-Host ""
Write-Host "Done! Restart Claude Code to see the new status line."
