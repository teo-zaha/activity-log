; Removing the user's data when they ask the uninstaller to ("Also delete my settings and cached
; activity"). Only known folders, and only what is inside them: NSIS's `RMDir /r` follows a
; junction or a directory link into whatever it points at, so this walks the folders itself and
; removes a link as a link, never what it leads to.

!include LogicLib.nsh

!define AL_FILE_ATTRIBUTE_DIRECTORY 0x10
!define AL_FILE_ATTRIBUTE_REPARSE_POINT 0x400
!define AL_INVALID_FILE_ATTRIBUTES -1
!define /ifndef AL_FILE_ATTRIBUTE_NORMAL 0x80

!define AL_REMOVE_TRIES 3 ; a file in use (WebView2's, just closed) is often free a moment later
!define AL_REMOVE_PAUSE 500 ; ms between tries

Var ActivityLog.ReportLeft ; 1 on the last try: what stays is logged

; Removes FOLDER and all in it. A folder that is a link (junction or symbolic link) is removed as
; a link; one that is a file, or missing, is left alone. Best effort, tried again a few times:
; what still cannot be removed stays, is listed in the log, and sets $ActivityLog.Leftovers.
!macro ActivityLog.RemoveFolder FOLDER
  Push "${FOLDER}"
  Call un.ActivityLog.RemoveFolder
!macroend

Function un.ActivityLog.RemoveFolder
  Exch $0
  Push $1
  Push $2
  ; A path without a profile folder in front (an empty $APPDATA) would be relative to the
  ; current drive's root: only a full path is removed
  StrCpy $1 $0 2 1
  ${IfThen} $1 != ":\" ${|} StrCpy $1 $0 2 ${|}
  ${If} $1 == ":\"
  ${OrIf} $1 == "\\"
    System::Call 'kernel32::GetFileAttributesW(w r0) i .r1'
    ${If} $1 <> ${AL_INVALID_FILE_ATTRIBUTES}
      IntOp $1 $1 & ${AL_FILE_ATTRIBUTE_DIRECTORY}
      ${If} $1 <> 0
        ${ForEach} $1 1 ${AL_REMOVE_TRIES} + 1
          ${If} $1 = ${AL_REMOVE_TRIES}
            StrCpy $ActivityLog.ReportLeft 1
          ${Else}
            StrCpy $ActivityLog.ReportLeft 0
          ${EndIf}
          Push $0
          Call un.ActivityLog.RemoveEntry
          System::Call 'kernel32::GetFileAttributesW(w r0) i .r2'
          ${IfThen} $2 = ${AL_INVALID_FILE_ATTRIBUTES} ${|} ${ExitFor} ${|}
          ${If} $1 < ${AL_REMOVE_TRIES}
            Sleep ${AL_REMOVE_PAUSE}
          ${Else}
            StrCpy $ActivityLog.Leftovers 1
          ${EndIf}
        ${Next}
      ${EndIf}
    ${EndIf}
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; Removes one entry: a link as a link, a folder with all in it, a file.
Function un.ActivityLog.RemoveEntry
  Exch $0 ; path
  Push $1 ; attributes
  Push $2 ; search handle
  Push $3 ; entry name
  System::Call 'kernel32::GetFileAttributesW(w r0) i .r1'
  ${If} $1 = ${AL_INVALID_FILE_ATTRIBUTES}
    Goto done
  ${EndIf}
  IntOp $2 $1 & ${AL_FILE_ATTRIBUTE_REPARSE_POINT}
  IntOp $3 $1 & ${AL_FILE_ATTRIBUTE_DIRECTORY}
  ${If} $2 <> 0
    ; A junction or link: RemoveDirectory and DeleteFile remove the link, not its target
    ${If} $3 <> 0
      System::Call 'kernel32::RemoveDirectoryW(w r0) i .r1'
    ${Else}
      System::Call 'kernel32::DeleteFileW(w r0) i .r1'
    ${EndIf}
    ${If} $1 = 0
    ${AndIf} $ActivityLog.ReportLeft = 1
      DetailPrint "$(alNotRemoved)$0"
    ${EndIf}
  ${ElseIf} $3 <> 0
    FindFirst $2 $3 "$0\*"
    ${DoWhile} $3 != ""
      ${If} $3 != "."
      ${AndIf} $3 != ".."
        Push "$0\$3"
        Call un.ActivityLog.RemoveEntry
      ${EndIf}
      FindNext $2 $3
    ${Loop}
    FindClose $2
    System::Call 'kernel32::SetFileAttributesW(w r0, i ${AL_FILE_ATTRIBUTE_NORMAL})'
    System::Call 'kernel32::RemoveDirectoryW(w r0)'
    ; A folder that stays because of what is in it was reported with that; only one that is
    ; empty and still cannot go is reported itself
    System::Call 'kernel32::GetFileAttributesW(w r0) i .r1'
    ${If} $1 <> ${AL_INVALID_FILE_ATTRIBUTES}
    ${AndIf} $ActivityLog.ReportLeft = 1
      FindFirst $2 $3 "$0\*"
      ${While} $3 != ""
        ${If} $3 != "."
        ${AndIf} $3 != ".."
          ${ExitWhile}
        ${EndIf}
        FindNext $2 $3
      ${EndWhile}
      FindClose $2
      ${IfThen} $3 == "" ${|} DetailPrint "$(alNotRemoved)$0" ${|}
    ${EndIf}
  ${Else}
    System::Call 'kernel32::SetFileAttributesW(w r0, i ${AL_FILE_ATTRIBUTE_NORMAL})'
    System::Call 'kernel32::DeleteFileW(w r0) i .r1'
    ${If} $1 = 0
    ${AndIf} $ActivityLog.ReportLeft = 1
      DetailPrint "$(alNotRemoved)$0"
    ${EndIf}
  ${EndIf}
  done:
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; Activity Log's own folders, as `core/src/platform/dirs.rs` finds them on Windows without the
; `ACTIVITY_LOG_*` overrides (which belong to whoever set them): the settings folder
; `%APPDATA%\activity-log` (`config.toml`, its lock, private staging folders), and the cache
; `%LOCALAPPDATA%\activity-log\cache`, then that parent if it is left empty. When the parent is a
; link, the cache is somewhere else: both are left alone.
!macro ActivityLog.RemoveUserData
  SetShellVarContext current
  !insertmacro ActivityLog.RemoveFolder "$APPDATA\activity-log"
  ${If} $LOCALAPPDATA != ""
    System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\activity-log") i .r0'
    ${If} $0 <> ${AL_INVALID_FILE_ATTRIBUTES}
      IntOp $1 $0 & ${AL_FILE_ATTRIBUTE_REPARSE_POINT}
      IntOp $0 $0 & ${AL_FILE_ATTRIBUTE_DIRECTORY}
      ${If} $0 <> 0
      ${AndIf} $1 = 0
        !insertmacro ActivityLog.RemoveFolder "$LOCALAPPDATA\activity-log\cache"
        System::Call 'kernel32::RemoveDirectoryW(w "$LOCALAPPDATA\activity-log")' ; only if empty
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
