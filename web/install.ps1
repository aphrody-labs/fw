# Zero-manual installer, updater & uninstaller for fw (Windows PowerShell).
# Install / Update: irm https://fw-edge.aphrody-labs.workers.dev/install.ps1 | iex
# Uninstall:        fw self-uninstall
param(
    [string]$Version = "",
    [switch]$Uninstall
)
$ErrorActionPreference = "Stop"
$Repo = "aphrody-labs/fw"
$BinDir = if ($env:FW_INSTALL_DIR) { $env:FW_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
$Target = Join-Path $BinDir "fw.exe"

if ($Uninstall) {
    Remove-Item -Force -ErrorAction SilentlyContinue $Target, "$Target.prev", "$Target.new"
    Write-Output "fw uninstalled from $BinDir"
    exit 0
}

$Asset = "fw-x86_64-windows.exe"
$Url = if ($Version) {
    "https://github.com/$Repo/releases/download/$Version/$Asset"
} else {
    "https://github.com/$Repo/releases/latest/download/$Asset"
}

New-Item -ItemType Directory -Force $BinDir | Out-Null
$Tmp = "$Target.new"
Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Tmp
if (Test-Path $Target) {
    Move-Item -Force $Target "$Target.prev"
}
Move-Item -Force $Tmp $Target

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not ($UserPath -split ';' | Where-Object { $_ -eq $BinDir })) {
    [Environment]::SetEnvironmentVariable("Path", "$BinDir;$UserPath", "User")
}
& $Target --version
