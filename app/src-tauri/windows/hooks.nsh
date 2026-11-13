; LOTUS installer hooks (Tauri NSIS). The installer runs elevated (per-machine).

!macro NSIS_HOOK_PREINSTALL
  ; Upgrade: stop the running service so its files can be replaced.
  ; It restores DNS and removes its firewall rules when it stops.
  nsExec::Exec 'sc.exe stop LotusService'
  Sleep 2000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Register (or reconfigure) and start the privileged service from $INSTDIR.
  nsExec::ExecToLog '"$INSTDIR\lotus-service.exe" install'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "The LOTUS service could not be installed (code $0). Open LOTUS and choose Repair service."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop and remove the service. This restores original DNS settings and
  ; removes every LOTUS firewall rule before files are deleted.
  nsExec::ExecToLog '"$INSTDIR\lotus-service.exe" uninstall'
!macroend
