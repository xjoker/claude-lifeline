# claude-lifeline installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/xjoker/claude-lifeline/master/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "xjoker/claude-lifeline"
$InstallDir = "$env:USERPROFILE\.claude\bin"
$BinName = "claude-lifeline.exe"
$Settings = "$env:USERPROFILE\.claude\settings.json"
$Target = "x86_64-pc-windows-msvc"

Write-Host "Platform: Windows/x86_64 -> $Target"

# Download latest release
$Latest = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name
if (-not $Latest) {
    Write-Error "Failed to fetch latest release"
    exit 1
}

$Url = "https://github.com/$Repo/releases/download/$Latest/claude-lifeline-$Target.exe"
Write-Host "Downloading $Latest..."

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Invoke-WebRequest -Uri $Url -OutFile "$InstallDir\$BinName"

Write-Host "Installed to $InstallDir\$BinName"

# Update settings.json
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
