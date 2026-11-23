; Sharp installer images on scaled displays (`bundle.windows.nsis.installerHooks`).
;
; MUI2 sizes its image controls in dialog units, so their pixel size follows the dialog
; font at the window's DPI, and by default it resizes one bitmap to them with LoadImage's
; coarse resampling. Here every image is drawn 1:1 (`NoStretchNoCrop`), and once the window
; exists the bitmaps drawn for its scale replace the 100% ones Tauri configures
; (`generate-images.py` draws them all at their controls' sizes).
;
; Tauri's template (tauri-bundler 2.10.4) includes this file after MUI2.nsh and before its
; first page, and defines none of these itself; a template that did would fail to compile
; here rather than lose either definition.

!define MUI_HEADERIMAGE_BITMAP_STRETCH NoStretchNoCrop
!define MUI_WELCOMEFINISHPAGE_BITMAP_STRETCH NoStretchNoCrop
!define MUI_CUSTOMFUNCTION_GUIINIT ActivityLog.ScaledImages
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.ActivityLog.ScaledImages

!define ACTIVITY_LOG_IMAGES "${__FILEDIR__}"

; Relies on MUI 2.1 (NSIS 3.12 and 3.13): see the sidebar below.
!if "${MUI_SYSVERSION}" != "2.1"
  !warning "hooks.nsh was written for MUI 2.1, this is ${MUI_SYSVERSION}: check that the installer's images still follow the display scale"
!endif

!macro ACTIVITY_LOG_SCALED_PAIR UN SCALE
  File "/oname=$PLUGINSDIR\activity-log-header.bmp" "${ACTIVITY_LOG_IMAGES}/nsis-header-${SCALE}.bmp"
  !if "${UN}" == ""
    ; MUI 2.1's Welcome and Finish pages (Contrib\Modern UI 2\Pages\Welcome.nsh, Finish.nsh)
    ; extract the sidebar to $PLUGINSDIR\modern-wizard.bmp in their GUIInit, which runs before
    ; this function, and load it from there when the page is created. Replace it only where
    ; MUI put it, so a MUI that keeps it elsewhere still shows its own image.
    ${If} ${FileExists} "$PLUGINSDIR\modern-wizard.bmp"
      File "/oname=$PLUGINSDIR\modern-wizard.bmp" "${ACTIVITY_LOG_IMAGES}/nsis-sidebar-${SCALE}.bmp"
    ${EndIf}
  !endif
!macroend

!macro ACTIVITY_LOG_SCALED_IMAGES UN
  Function ${UN}ActivityLog.ScaledImages
    System::Store "S"
    StrCpy $0 0
    System::Call "user32::GetDpiForWindow(p $HWNDPARENT) i .r0"
    ${If} $0 = 0 ; Before Windows 10 1607: the system DPI
      System::Call "user32::GetDC(p 0) p .r1"
      System::Call "gdi32::GetDeviceCaps(p r1, i 88) i .r0" ; LOGPIXELSX
      System::Call "user32::ReleaseDC(p 0, p r1)"
    ${EndIf}
    ; The largest scale drawn at or below the window's.
    ${If} $0 >= 288
      !insertmacro ACTIVITY_LOG_SCALED_PAIR "${UN}" 300
    ${ElseIf} $0 >= 240
      !insertmacro ACTIVITY_LOG_SCALED_PAIR "${UN}" 250
    ${ElseIf} $0 >= 192
      !insertmacro ACTIVITY_LOG_SCALED_PAIR "${UN}" 200
    ${ElseIf} $0 >= 168
      !insertmacro ACTIVITY_LOG_SCALED_PAIR "${UN}" 175
    ${ElseIf} $0 >= 144
      !insertmacro ACTIVITY_LOG_SCALED_PAIR "${UN}" 150
    ${ElseIf} $0 >= 120
      !insertmacro ACTIVITY_LOG_SCALED_PAIR "${UN}" 125
    ${Else}
      ; 100%: MUI already shows the configured images.
      System::Store "L"
      Return
    ${EndIf}
    ; The header control (1046 in MUI's modern_headerbmp.exe) takes the bitmap's size.
    GetDlgItem $1 $HWNDPARENT 1046
    !insertmacro MUI_LOADANDXALIGNIMAGE $1 "$PLUGINSDIR\activity-log-header.bmp" Auto Leak
    System::Store "L"
  FunctionEnd
!macroend

!insertmacro ACTIVITY_LOG_SCALED_IMAGES ""
!insertmacro ACTIVITY_LOG_SCALED_IMAGES "un."
