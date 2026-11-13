<#
.SYNOPSIS
  Builds the LOTUS Windows installer.

  1. fetches + verifies the pinned engine and rule-set
  2. runs the core test suite (including `sing-box check` on generated configs)
  3. builds the privileged service (release) and stages it as a Tauri sidecar
  4. builds the desktop app and the NSIS installer

  Output: target\release\bundle\nsis\LOTUS_<version>_x64-setup.exe
#>
param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Set-Location $Root

function Step($m) { Write-Host "`n==> $m" -ForegroundColor Cyan }

Step 'Fetching networking engine'
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'fetch-engine.ps1')
if ($LASTEXITCODE -ne 0) { throw 'engine fetch failed' }

if (-not $SkipTests) {
  Step 'Running tests'
  cargo test -p lotus-core --release
  if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
}

Step 'Building service'
cargo build -p lotus-service --release
if ($LASTEXITCODE -ne 0) { throw 'service build failed' }
Copy-Item target\release\lotus-service.exe app\src-tauri\binaries\lotus-service-x86_64-pc-windows-msvc.exe -Force

Step 'Building desktop app and installer'
Set-Location (Join-Path $Root 'app')
if (-not (Test-Path node_modules)) { npm ci --no-audit --no-fund }
npx tauri build
if ($LASTEXITCODE -ne 0) { throw 'tauri build failed' }

$setup = Get-ChildItem (Join-Path $Root 'target\release\bundle\nsis') -Filter '*-setup.exe' | Sort-Object LastWriteTime | Select-Object -Last 1
Step "Installer: $($setup.FullName) ($([math]::Round($setup.Length / 1MB, 1)) MB)"
(Get-FileHash $setup.FullName -Algorithm SHA256).Hash.ToLower() + "  " + $setup.Name | Set-Content ($setup.FullName + '.sha256')
