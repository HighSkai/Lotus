<#
.SYNOPSIS
  Builds the LOTUS Windows installer.

  1. fetches + verifies the pinned engine and rule-set
  2. runs the core test suite (including `sing-box check` on generated configs)
  3. builds the privileged service (release) and stages it as a Tauri sidecar
  4. builds the desktop app and the NSIS installer
  5. with the update signing key: signs the installer and writes latest.json

  Output (target\release\bundle\nsis\):
    LOTUS_<version>_x64-setup.exe          the installer
    LOTUS_<version>_x64-setup.exe.sha256   checksum for the release notes
    LOTUS_<version>_x64-setup.exe.sig      update signature   (signed builds)
    latest.json                            update manifest    (signed builds)

  Upload all of them to the GitHub release: installed copies of LOTUS read
  latest.json from the latest release to find updates.

  The signing key is read from $env:LOTUS_UPDATER_KEY or
  %USERPROFILE%\.tauri\lotus-updater.key (create one with
  `npx tauri signer generate -w <path>`; its public key goes in
  tauri.conf.json > plugins.updater.pubkey). Without a key the build still
  works but produces no update files.
#>
param([switch]$SkipTests, [string]$Notes = '')
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

$keyPath = if ($env:LOTUS_UPDATER_KEY) { $env:LOTUS_UPDATER_KEY } else { Join-Path $env:USERPROFILE '.tauri\lotus-updater.key' }
$signed = [bool]$env:TAURI_SIGNING_PRIVATE_KEY
if (-not $signed -and (Test-Path $keyPath)) {
  # The CLI wants the key itself, not its path. A key made without a password needs no password variable.
  $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $keyPath -Raw).Trim()
  $signed = $true
}
if ($signed) {
  npx tauri build
} else {
  Write-Host 'No update signing key found: building without update files.' -ForegroundColor Yellow
  $noUpdater = Join-Path $env:TEMP 'lotus-no-updater.json'
  [IO.File]::WriteAllText($noUpdater, '{"bundle":{"createUpdaterArtifacts":false}}')
  npx tauri build --config $noUpdater
}
if ($LASTEXITCODE -ne 0) { throw 'tauri build failed' }

$version = (Get-Content (Join-Path $Root 'app\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$bundle = Join-Path $Root 'target\release\bundle\nsis'
$setup = Get-Item (Join-Path $bundle "LOTUS_${version}_x64-setup.exe")
Step "Installer: $($setup.FullName) ($([math]::Round($setup.Length / 1MB, 1)) MB)"
(Get-FileHash $setup.FullName -Algorithm SHA256).Hash.ToLower() + "  " + $setup.Name | Set-Content ($setup.FullName + '.sha256')

if ($signed) {
  $sig = Get-Item ($setup.FullName + '.sig')
  if (-not $Notes) { $Notes = "See what's new: https://github.com/HighSkai/Lotus/releases/tag/v$version" }
  $manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
      'windows-x86_64' = [ordered]@{
        signature = (Get-Content $sig.FullName -Raw).Trim()
        url       = "https://github.com/HighSkai/Lotus/releases/download/v$version/$($setup.Name)"
      }
    }
  }
  # No byte-order mark: the updater's JSON parser rejects one.
  $latest = Join-Path $bundle 'latest.json'
  [IO.File]::WriteAllText($latest, ($manifest | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
  Step "Update manifest: $latest"
}
