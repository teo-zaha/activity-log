; Sharp installer images on scaled displays (`bundle.windows.nsis.installerHooks`).
;
; MUI2 sizes its image controls in dialog units, so their pixel size follows the dialog font
; at the window's DPI, and it resizes the bitmap to them with LoadImage's coarse resampling
; (`FitControl`, MUI's default, kept). Once the window exists, this measures the controls and
; puts the largest bitmap drawn for one of them that fits in place of the 100% one Tauri
; configures (`nsis-scales.nsh`, written by `generate-images.py`): at Windows' scale presets
; with Tahoma that is the control's own size, which LoadImage copies 1:1; with another dialog
; font or a custom scale it is stretched a little, never past the control. A control smaller
; than the 100% bitmap keeps MUI's own image.
;
; The bitmaps are chosen once, as the window opens (PerMonitorV2): moving it to a monitor with
; another scale keeps them.
;
; Tauri's template (tauri-bundler 2.10.4) includes this file after MUI2.nsh and before its
; first page, and defines neither function itself; a template that did would fail to compile
; here rather than lose either definition. It defines MUI_HEADERIMAGE only after this file,
; so the header is guarded at run time, by its control.

!define ACTIVITY_LOG_IMAGES "${__FILEDIR__}"
!include "${ACTIVITY_LOG_IMAGES}/nsis-scales.nsh"

!define MUI_CUSTOMFUNCTION_GUIINIT ActivityLog.ScaledImages
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.ActivityLog.ScaledImages

; Relies on MUI 2.1 (NSIS 3.12 and 3.13): the header control's id and the sidebar's file below.
!if "${MUI_SYSVERSION}" != "2.1"
  !warning "hooks.nsh was written for MUI 2.1, this is ${MUI_SYSVERSION}: check that the installer's images still follow the display scale"
!endif

!macro ACTIVITY_LOG_SCALED_IMAGES UN
  Function ${UN}ActivityLog.ScaledImages
    System::Store "S"
    InitPluginsDir
    System::Call "*(i, i, i, i) p .r9"

    ; The header: control 1046 in MUI 2.1's modern_headerbmp.exe, there only with a header image.
    GetDlgItem $0 $HWNDPARENT 1046
    ${If} $0 P<> 0
      System::Call "user32::GetClientRect(p r0, p r9)"
      System::Call "*$9(i, i, i .r1, i .r2)"
      !insertmacro ACTIVITY_LOG_HEADER_FOR $1 $2 "$PLUGINSDIR\activity-log-header.bmp" $3
      ${If} $3 = 1
        SetBrandingImage /IMGID=1046 /RESIZETOFIT "$PLUGINSDIR\activity-log-header.bmp"
      ${EndIf}
    ${EndIf}

    !if "${UN}" == ""
      ; The sidebar: MUI 2.1's Welcome and Finish pages (Contrib\Modern UI 2\Pages\Welcome.nsh,
      ; Finish.nsh) extract it to $PLUGINSDIR\modern-wizard.bmp in their GUIInit, which runs
      ; before this function, and load it from there into a 109x193 DLU control when the page is
      ; created. That control does not exist yet, so its size is the dialog's mapping of those
      ; units, the one nsDialogs uses. Replace the file only where MUI put it, so a MUI that keeps
      ; it elsewhere still shows its own image.
      ${If} ${FileExists} "$PLUGINSDIR\modern-wizard.bmp"
        System::Call "*$9(i 0, i 0, i 109, i 193)"
        System::Call "user32::MapDialogRect(p $HWNDPARENT, p r9)"
        System::Call "*$9(i, i, i .r1, i .r2)"
        !insertmacro ACTIVITY_LOG_SIDEBAR_FOR $1 $2 "$PLUGINSDIR\modern-wizard.bmp" $3
      ${EndIf}
    !endif

    System::Free $9
    System::Store "L"
  FunctionEnd
!macroend

!insertmacro ACTIVITY_LOG_SCALED_IMAGES ""
!insertmacro ACTIVITY_LOG_SCALED_IMAGES "un."
