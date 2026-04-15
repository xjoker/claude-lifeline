# claude-lifeline installer for Windows (PowerShell)
# Usage:
#   Install/Upgrade: irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex
#   Uninstall:       & { $env:ACTION='uninstall'; irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex }

$ErrorActionPreference = "Stop"

$Repo = "xjoker/claude-lifeline"
$InstallDir = "$env:USERPROFILE\.claude\bin"
$BinName = "claude-lifeline.exe"
$Settings = "$env:USERPROFILE\.claude\settings.json"
$Target = "x86_64-pc-windows-msvc"
$Action = if ($env:ACTION) { $env:ACTION } else { "install" }

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

# ── Platform ──

Write-Host "Platform: Windows/x86_64 -> $Target"

# ── Version check ──

$Latest = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name
if (-not $Latest) {
    Write-Error "Failed to fetch latest release"
    exit 1
}

if (Test-Path "$InstallDir\$BinName") {
    try {
        $Current = & "$InstallDir\$BinName" --version 2>$null
        Write-Host "Current: $Current, Latest: $Latest"
        if ($Current -match $Latest) {
            Write-Host "Already up to date."
            exit 0
        }
    } catch {}
}

# ── Download ──

$Url = "https://github.com/$Repo/releases/download/$Latest/claude-lifeline-$Target.exe"
Write-Host "Downloading $Latest..."

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Invoke-WebRequest -Uri $Url -OutFile "$InstallDir\$BinName"

Write-Host "Installed to $InstallDir\$BinName"

# ── Configure settings.json ──

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
    Write-Host "Warning: $Settings not found. Create it or add statusLine config manually."
}

Write-Host ""
Write-Host "Done! Restart Claude Code to see the new status line."
