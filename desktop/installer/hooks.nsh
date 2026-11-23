; Tauri's installer hooks (`bundle.windows.nsis.installerHooks`), and the include point of our
; installer's own parts: `installer.nsi` (our copy of Tauri's template) includes this
; file before its first page, and its marked blocks insert what these files define.

!define ACTIVITY_LOG_INSTALLER "${__FILEDIR__}"
!include "${ACTIVITY_LOG_INSTALLER}/one-click.nsh"

; The Install section's files go to the log, not the status line, which says what is happening
; (the WebView2 section before it prints its own steps there).
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "$(alCopying)"
  SetDetailsPrint listonly
!macroend
