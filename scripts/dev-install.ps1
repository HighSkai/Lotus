<#
.SYNOPSIS
  Development only: stage the engine next to the debug service build and
  register the service from target\debug. Must run elevated.

  -AllowUserControl lets interactive users start/stop the service, so the
  service can be rebuilt and restarted without further elevation. Never use
  it on a production install.
#>
param(
  [ValidateSet('debug', 'release')][string]$Profile = 'debug',
  [switch]$AllowUserControl,
  [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root "target\$Profile"
$Svc = Join-Path $Out 'lotus-service.exe'

if ($Uninstall) { & $Svc uninstall; exit $LASTEXITCODE }

Copy-Item (Join-Path $Root 'app\src-tauri\binaries\sing-box-x86_64-pc-windows-msvc.exe') (Join-Path $Out 'sing-box.exe') -Force
Copy-Item (Join-Path $Root 'app\src-tauri\resources\libcronet.dll') $Out -Force
New-Item -ItemType Directory -Force (Join-Path $Out 'rules') | Out-Null
Copy-Item (Join-Path $Root 'app\src-tauri\resources\block-trackers.srs') (Join-Path $Out 'rules') -Force

& $Svc install
if ($LASTEXITCODE -ne 0) { throw "install failed" }
if ($AllowUserControl) {
  # Default service DACL plus start/stop (RP/WP) for interactive users.
  sc.exe sdset LotusService "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWRPWPDTLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)" | Out-Null
  Write-Host 'Interactive users may now start/stop LotusService (development only).'
}
Get-Service LotusService | Format-Table -AutoSize
