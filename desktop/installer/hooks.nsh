; Tauri's installer hooks (`bundle.windows.nsis.installerHooks`), and the include point of our
; installer's own parts: `installer.nsi` (our copy of Tauri's template) includes this
; file before its first page, and its marked blocks insert what these files define.

!define ACTIVITY_LOG_INSTALLER "${__FILEDIR__}"
!include "${ACTIVITY_LOG_INSTALLER}/one-click.nsh"
!include "${ACTIVITY_LOG_INSTALLER}/user-data.nsh"

; The Install section's files go to the log, not the status line, which says what is happening
; (the WebView2 section before it prints its own steps there).
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "$(alCopying)"
  SetDetailsPrint listonly
!macroend

; "Also delete my settings and cached activity": Tauri's block before this has removed the app's
; own folders (WebView2's data, the window state); ours are Activity Log's settings and cache.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    !insertmacro ActivityLog.RemoveUserData
  ${EndIf}
!macroend
