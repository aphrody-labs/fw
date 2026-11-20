#!/usr/bin/env pwsh
#Requires -Version 7.0
param(
  [ValidateSet('status', 'tools', 'cache', 'check', 'help')]
  [string]$Command = 'help',
  [string]$Target = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = if ($env:WORKSPACE_ROOT) { $env:WORKSPACE_ROOT } else { Split-Path -Parent (Split-Path -Parent $PSScriptRoot) }

if ((Get-Command cargo -ErrorAction SilentlyContinue) -and (Test-Path (Join-Path $Root 'ay\xtask\Cargo.toml'))) {
  $cargoArgs = @('run', '--quiet', '--manifest-path', (Join-Path $Root 'ay\xtask\Cargo.toml'), '--', $Command)
  if ($Target) { $cargoArgs += $Target }
  & cargo @cargoArgs
  exit $LASTEXITCODE
}

switch ($Command) {
  'status' {
    foreach ($r in @('aphrody','ay','fw','nie','rosegriffon','shenron','wiki')) {
      $p = Join-Path $Root $r
      if (-not (Test-Path (Join-Path $p '.git'))) { "{0,-14} | missing" -f $r; continue }
      $b = (git -C $p branch --show-current 2>$null) -join ''
      $d = @(git -C $p status --short 2>$null).Count
      "{0,-14} | branch={1,-10} | dirty={2}" -f $r, $b, $d
    }
  }
  'tools' {
    foreach ($t in @('bun','rustc','cargo','sccache','pwsh','dotnet','go','python','clang','cmake','ninja','gh')) {
      $c = Get-Command $t -ErrorAction SilentlyContinue
      $v = if ($c) { (& $t --version 2>$null | Select-Object -First 1) -join '' } else { 'NOT FOUND' }
      "{0,-10} | {1}" -f $t, $v
    }
  }
  'cache' {
    foreach ($k in @('CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','SCCACHE_DIR','BUN_INSTALL','VCPKG_DEFAULT_BINARY_CACHE')) {
      "{0,-28} = {1}" -f $k, ([Environment]::GetEnvironmentVariable($k) ?? '(unset)')
    }
  }
  default {
    Write-Output 'Usage: ./scripts/xtask.ps1 <status|tools|cache|check> [repo]'
  }
}
