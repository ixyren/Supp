!macro NSIS_HOOK_PREINSTALL
  ; Cerrar instancias en ejecucion de supp.exe antes de actualizar/instalar archivos
  nsExec::ExecToStack 'taskkill /F /IM supp.exe'
  Pop $0
  Pop $1
  Sleep 300
!macroend
