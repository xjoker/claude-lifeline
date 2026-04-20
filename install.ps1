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
$Config = "$env:USERPROFILE\.claude\claude-lifeline\config.toml"
$Target = "x86_64-pc-windows-msvc"
$Action = if ($env:ACTION) { $env:ACTION } else { "install" }

# ── Layout config helpers ──
function Set-Layout {
    param([string]$Layout)
    $configDir = Split-Path $Config
    if (-not (Test-Path $configDir)) {
        New-Item -ItemType Directory -Force -Path $configDir | Out-Null
    }
    if (-not (Test-Path $Config)) {
        Set-Content -Path $Config -Value "[display]`nlayout = `"$Layout`"" -Encoding UTF8
        Write-Host "Created $Config with layout = `"$Layout`""
        return
    }
    Copy-Item $Config "$Config.bak"
    # 段内替换/插入：只在 [display] 段内改 layout，避免误伤其他段同名字段
    $lines = Get-Content $Config
    $out = New-Object System.Collections.Generic.List[string]
    $inDisplay = $false
    $replaced = $false
    foreach ($line in $lines) {
        if ($line -match '^\[[^\]]+\]\s*$') {
            if ($inDisplay -and -not $replaced) {
                $out.Add("layout = `"$Layout`"")
                $replaced = $true
            }
            $inDisplay = ($line -match '^\[display\]\s*$')
            $out.Add($line)
            continue
        }
        if ($inDisplay -and $line -match '^\s*layout\s*=') {
            $out.Add("layout = `"$Layout`"")
            $replaced = $true
            continue
        }
        $out.Add($line)
    }
    if ($inDisplay -and -not $replaced) {
        $out.Add("layout = `"$Layout`"")
        $replaced = $true
    }
    if (-not $replaced) {
        $out.Add("")
        $out.Add("[display]")
        $out.Add("layout = `"$Layout`"")
    }
    Set-Content -Path $Config -Value $out -Encoding UTF8
    Write-Host "Set layout = `"$Layout`" in $Config (backup: config.toml.bak)"
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

    # 配 settings.json
    if (Test-Path $Settings) {
        $json = Get-Content $Settings -Raw | ConvertFrom-Json
        $current = ""
        if ($json.statusLine -and $json.statusLine.command) {
            $current = $json.statusLine.command
        }
        if ($current -eq "~/.claude/bin/claude-lifeline") {
            Write-Host "settings.json already configured"
        } else {
            Copy-Item $Settings "$Settings.bak"
            $json | Add-Member -Force -MemberType NoteProperty -Name "statusLine" -Value @{
                type = "command"
                command = "~/.claude/bin/claude-lifeline"
            }
            $json | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
            Write-Host "Updated settings.json (backup: settings.json.bak)"
        }
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path $Settings) | Out-Null
        @{statusLine = @{type = "command"; command = "~/.claude/bin/claude-lifeline"}} | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
        Write-Host "Created $Settings"
    }
}

if ($Action -eq "mini") {
    Invoke-DoInstall
    Set-Layout "mini"
    Write-Host ""
    Write-Host "Done! Restart Claude Code to apply mini layout."
    exit 0
}
if ($Action -eq "standard") {
    Invoke-DoInstall
    Set-Layout "auto"
    Write-Host ""
    Write-Host "Done! Restart Claude Code to apply standard layout."
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

    if (Test-Path $Settings) {
        $json = Get-Content $Settings -Raw | ConvertFrom-Json
        $current = ""
        if ($json.statusLine -and $json.statusLine.command) {
            $current = $json.statusLine.command
        }
        if ($current -eq "~/.claude/bin/claude-lifeline") {
            Write-Host "settings.json already configured"
        } else {
            Copy-Item $Settings "$Settings.bak"
            $json | Add-Member -Force -MemberType NoteProperty -Name "statusLine" -Value @{
                type = "command"
                command = "~/.claude/bin/claude-lifeline"
            }
            $json | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
            Write-Host "Updated settings.json (backup: settings.json.bak)"
        }
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path $Settings) | Out-Null
        @{statusLine = @{type = "command"; command = "~/.claude/bin/claude-lifeline"}} | ConvertTo-Json -Depth 10 | Set-Content $Settings -Encoding UTF8
        Write-Host "Created $Settings"
    }

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
