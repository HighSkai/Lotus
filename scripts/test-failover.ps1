<#
.SYNOPSIS
  Elevated failure-injection test for SHADOW (requires an active SHADOW session
  with the kill switch on). Kills the engine, samples direct connectivity
  while the service recovers, then kills the service itself.
  Writes a timestamped report to -Log.
#>
param([Parameter(Mandatory)][string]$Log)
$ErrorActionPreference = 'Continue'
$Ctl = Join-Path (Split-Path -Parent $PSScriptRoot) 'target\debug\lotus-service.exe'
$t0 = Get-Date
function Out([string]$m) { $line = '{0,7:N2}s  {1}' -f ((Get-Date) - $t0).TotalSeconds, $m; Add-Content -LiteralPath $Log $line }
function State { try { (& $Ctl ctl status | ConvertFrom-Json).state } catch { 'unreachable' } }
function Probe {
  $o = curl.exe -s --max-time 2 https://1.1.1.1/cdn-cgi/trace 2>$null
  if ($LASTEXITCODE -eq 0) { 'TRAFFIC FLOWS' } else { "blocked (curl exit $LASTEXITCODE)" }
}
Set-Content -LiteralPath $Log "Failover test $(Get-Date -Format o)"

function Engine-Pid {
  $svc = (Get-CimInstance Win32_Service -Filter "Name='LotusService'").ProcessId
  (Get-CimInstance Win32_Process -Filter "Name='sing-box.exe'" | Where-Object { $_.ParentProcessId -eq $svc } | Select-Object -First 1).ProcessId
}

Out "state before: $(State); probe: $(Probe)"

# --- 1. engine crash ------------------------------------------------------
$e = Engine-Pid
Out "killing engine pid $e"
Stop-Process -Id $e -Force
$sawDrop = $false
for ($i = 0; $i -lt 60; $i++) {
  $st = State
  $p = Probe
  Out "state=$st  direct traffic: $p"
  if ($st -ne 'connected') { $sawDrop = $true }
  if ($st -eq 'connected' -and $sawDrop) { break }
  Start-Sleep -Milliseconds 300
}
Out "engine crash noticed by service: $sawDrop"
Out "after engine crash: state=$(State) probe=$(Probe)"

# --- 2. service crash -----------------------------------------------------
Start-Sleep 2
$svcPid = (Get-CimInstance Win32_Service -Filter "Name='LotusService'").ProcessId
Out "killing service pid $svcPid (simulated crash)"
Stop-Process -Id $svcPid -Force
for ($i = 0; $i -lt 60; $i++) {
  $st = State
  # Distinguish leaks from tunnelled traffic: only probe while the TUN is absent.
  $tun = Get-NetAdapter -Name 'LOTUS Shadow' -ErrorAction SilentlyContinue
  $p = Probe
  Out ("state={0,-12} tun={1,-5} traffic: {2}" -f $st, [bool]$tun, $p)
  if ($st -eq 'connected') { break }
  Start-Sleep -Milliseconds 300
}
Out "after service crash: state=$(State) probe=$(Probe)"
Out "done"
