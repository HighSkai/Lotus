<#
.SYNOPSIS
  Elevated uninstall/reinstall verification. Run while LOTUS mode is connected:
  uninstalls silently, records whether DNS, firewall rules, service and files
  were all cleaned up, then reinstalls from -Setup.
#>
param([Parameter(Mandatory)][string]$Log, [Parameter(Mandatory)][string]$Setup)
$ErrorActionPreference = 'Continue'
function Out([string]$m) { Add-Content -LiteralPath $Log $m }
Set-Content -LiteralPath $Log "Uninstall test $(Get-Date -Format o)"
$alias = (Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway } | Select-Object -First 1).InterfaceAlias
$guid = (Get-NetAdapter -Name $alias).InterfaceGuid
Out "BEFORE: $alias DNS v4=$((Get-DnsClientServerAddress -InterfaceAlias $alias -AddressFamily IPv4).ServerAddresses -join ',')"
Out "BEFORE: LOTUS WFP filters: $(@(netsh wfp show filters file=- | Select-String 'LOTUS').Count)"

Get-Process lotus -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Process "C:\Program Files\LOTUS\uninstall.exe" -ArgumentList '/S' -Wait
for ($i = 0; $i -lt 120; $i++) {
  if (-not (Get-Service LotusService -ErrorAction SilentlyContinue) -and -not (Test-Path 'C:\Program Files\LOTUS\lotus-service.exe')) { break }
  Start-Sleep -Milliseconds 500
}
Out "AFTER UNINSTALL: service present: $([bool](Get-Service LotusService -ErrorAction SilentlyContinue))"
Out "AFTER UNINSTALL: $alias DNS v4=$((Get-DnsClientServerAddress -InterfaceAlias $alias -AddressFamily IPv4).ServerAddresses -join ',') v6=$((Get-DnsClientServerAddress -InterfaceAlias $alias -AddressFamily IPv6).ServerAddresses -join ',')"
Out "AFTER UNINSTALL: static NameServer (registry, empty = DHCP): '$((Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$guid").NameServer)'"
Out "AFTER UNINSTALL: LOTUS WFP filters: $(@(netsh wfp show filters file=- | Select-String 'LOTUS').Count)"
Out "AFTER UNINSTALL: Program Files\LOTUS exists: $(Test-Path 'C:\Program Files\LOTUS\lotus.exe')  ProgramData\LOTUS exists: $(Test-Path "$env:ProgramData\LOTUS")"
$r = Resolve-DnsName example.com -DnsOnly -QuickTimeout -ErrorAction SilentlyContinue | Select-Object -First 1
Out "AFTER UNINSTALL: DNS works: $([bool]$r)"
Out "AFTER UNINSTALL: engine processes: $(@(Get-Process sing-box -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*Program Files\LOTUS*' }).Count)"

Start-Process $Setup -ArgumentList '/S' -Wait
for ($i = 0; $i -lt 60; $i++) { if ((Get-Service LotusService -ErrorAction SilentlyContinue).Status -eq 'Running') { break }; Start-Sleep -Milliseconds 500 }
Out "AFTER REINSTALL: service: $((Get-Service LotusService -ErrorAction SilentlyContinue).Status)"
