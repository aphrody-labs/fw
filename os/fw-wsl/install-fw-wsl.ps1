# Fw-WSL Installer (Fused NixOS-WSL 2605.7.2 + CachyOS-WSL 2026.11.05.5 + Custom .wslconfig/.wslgconfig)
param(
  [switch]$ApplyHostConfigs,
  [switch]$InstallNixOsWsl,
  [switch]$InstallCachyOsWsl
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

if ($ApplyHostConfigs) {
  Copy-Item -Force (Join-Path $Root "wslconfig.ini") "$HOME\.wslconfig"
  Copy-Item -Force (Join-Path $Root "wslg\wslgconfig.ini") "$HOME\.wslgconfig"
  Write-Host "[fw-wsl] Installed $HOME\.wslconfig and $HOME\.wslgconfig"
}

if ($InstallNixOsWsl) {
  $url = "https://github.com/nix-community/NixOS-WSL/releases/download/2605.7.2/nixos.wsl"
  $dst = "$env:TEMP\nixos-2605.7.2.wsl"
  curl.exe -sSfL $url -o $dst
  wsl --install --from-file $dst --name Fw-NixOS
}

if ($InstallCachyOsWsl) {
  $url = "https://github.com/CachyOS/cachyos-wsl/releases/download/2026.11.05.5/cachyos-2026.11.05.5.wsl"
  $dst = "$env:TEMP\cachyos-2026.11.05.5.wsl"
  curl.exe -sSfL $url -o $dst
  wsl --install --from-file $dst --name Fw-CachyOS
}

if (-not ($ApplyHostConfigs -or $InstallNixOsWsl -or $InstallCachyOsWsl)) {
  Write-Host "Usage: .\install-fw-wsl.ps1 [-ApplyHostConfigs] [-InstallNixOsWsl] [-InstallCachyOsWsl]"
}
