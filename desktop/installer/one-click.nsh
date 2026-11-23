; The one-click installer's window and pages, for `installer.nsi` (our copy of Tauri's template).
;
; One small window, about 420 x 238 client pixels at 100% (420 x 270 with Windows' title bar): the
; logo, a title, a status line and a flat brand-coloured bar. Screens show only when needed:
; a newer version installed (refused), Activity Log open (close it first), a failure (with "Show
; details"), and in the uninstaller one question. Text is real Segoe UI at the window's DPI and
; the logo is a frame of the multi-size `installer.ico` (the exe's own icon) loaded at the size the
; DPI asks for, so nothing is a bitmap Windows scales. Light and dark follow Windows' app setting;
; with high contrast on, the system's colours stay.
;
; How it is built on MUI2 without its look: `.onGUIInit` hides every control MUI's dialog has,
; sizes the window, and makes the page area (control 1018) the whole client area. Each page
; then fills it, under the outer Next and Cancel buttons, which every page moves into its own
; layout: Enter, Escape and NSIS's own enabling of them keep working. The pages are nsDialogs
; pages, but for MUI's InstFiles page, which is restyled where it shows and, when it ends,
; turned into "Starting Activity Log…" or the failure screen.
;
; `installer.nsi` inserts ACTIVITY_LOG_INSTALLER_UI and ACTIVITY_LOG_UNINSTALLER_UI where its pages
; are declared: the functions use Tauri's defines (`MAINBINARYNAME`, `VERSION`), which exist only
; from there on.

!define ACTIVITY_LOG_ONE_CLICK

!include LogicLib.nsh
!include nsDialogs.nsh
!include WinMessages.nsh
!include "Win\RestartManager.nsh"

; Win32 values WinMessages.nsh does not define.
!define AL_IDI_ERROR 32513
!define AL_SWP_NOZORDER_NOACTIVATE 0x14
!define AL_HWND_BOTTOM 1
!define AL_PBM_SETBKCOLOR 0x2001
!define AL_PB_RANGE 30000 ; NSIS's progress range
!define AL_LVM_GETITEMCOUNT 0x1004
!define AL_LVM_GETITEMTEXTW 0x1073
!define AL_LVM_SETBKCOLOR 0x1001
!define AL_LVM_SETTEXTCOLOR 0x1024
!define AL_LVM_SETTEXTBKCOLOR 0x1026
!define AL_BCM_GETIDEALSIZE 0x1601
!define AL_DM_SETDEFID 0x0401
!define AL_DT_MEASURE 0xC11 ; DT_CALCRECT | DT_NOPREFIX | DT_WORDBREAK | DT_CENTER
!define AL_DT_MEASURE_LINE 0xC20 ; DT_CALCRECT | DT_NOPREFIX | DT_SINGLELINE
!define AL_FILE_ATTRIBUTE_NORMAL 0x80

; The layout, in pixels at 96 DPI (100%), as `option-b.html` draws it.
!define AL_WIDTH 420
!define AL_HEIGHT 238
!define AL_SIDE 32 ; text and bar margin
!define AL_LOGO 56
!define AL_LOGO_TOP 26
!define AL_ERROR_ICON 40
!define AL_ERROR_TOP 28
!define AL_TITLE_GAP 14 ; logo to title
!define AL_TITLE_HEIGHT 20
!define AL_LINE 16 ; one line of body text
!define AL_TEXT_GAP 2 ; title to text
!define AL_ROW_GAP 18 ; text to bar or buttons
!define AL_BAR_HEIGHT 4
!define AL_FOOTER_HEIGHT 14
!define AL_FOOTER_BOTTOM 12
!define AL_BUTTON_HEIGHT 24
!define AL_BUTTON_MIN 75
!define AL_BUTTON_PAD 16 ; added to the button's ideal width
!define AL_BUTTON_GAP 8
!define AL_CHECK_GAP 10 ; title to the uninstaller's checkbox
!define AL_LABEL_GAP 4 ; dark checkbox to its label

; Colours: text and background as SetCtlColors takes them, the bar's as COLORREF (0xBBGGRR).
!define AL_LIGHT_BG FFFFFF
!define AL_LIGHT_TEXT 1A1A1A
!define AL_LIGHT_MUTED 5D5D5D
!define AL_LIGHT_BAR 0xD84E1D ; #1d4ed8, the brand blue
!define AL_LIGHT_TRACK 0xEDE6E2 ; #e2e6ed
!define AL_DARK_BG 202020
!define AL_DARK_TEXT FFFFFF
!define AL_DARK_MUTED C5C5C5
!define AL_DARK_BAR 0xFAA560 ; #60a5fa
!define AL_DARK_TRACK 0x3D3D3D
!define AL_DARK_LIST_BG 0x202020
!define AL_DARK_LIST_TEXT 0xFFFFFF

Var ActivityLog.Dpi
Var ActivityLog.Theme ; light, dark or contrast
Var ActivityLog.FontBody
Var ActivityLog.FontTitle
Var ActivityLog.FontSmall
Var ActivityLog.Width ; the client area, in pixels
Var ActivityLog.Height
Var ActivityLog.Logo ; the InstFiles page's controls of ours
Var ActivityLog.Title
Var ActivityLog.Footer
Var ActivityLog.Text ; the App open page's text, which a failed close replaces
Var ActivityLog.NewerVersion ; the installed version when it is newer than this one (.onInit)
Var ActivityLog.LogoIcon ; the icons, loaded once at their size here (GUIInit), freed at the end
Var ActivityLog.ErrorIcon
Var ActivityLog.SmallIcon
Var ActivityLog.CheckFrame ; the dark checkbox's focus frame (uninstaller)

; OUT = VALUE pixels at 96 DPI, at the window's DPI.
!macro AL_PX OUT VALUE
  IntOp ${OUT} ${VALUE} * $ActivityLog.Dpi
  IntOp ${OUT} ${OUT} + 48
  IntOp ${OUT} ${OUT} / 96
!macroend
!define AL_PX "!insertmacro AL_PX"

; ROLE is TEXT or MUTED. High contrast keeps the system's colours.
!macro AL_COLORS HWND ROLE
  ${If} $ActivityLog.Theme == dark
    SetCtlColors ${HWND} ${AL_DARK_${ROLE}} ${AL_DARK_BG}
  ${ElseIf} $ActivityLog.Theme == light
    SetCtlColors ${HWND} ${AL_LIGHT_${ROLE}} ${AL_LIGHT_BG}
  ${EndIf}
!macroend

; OUT = the icon RESOURCE of MODULE (0: the system's) loaded at SIZE pixels, or 0. Uses $9.
!macro AL_LOAD_ICON OUT MODULE RESOURCE SIZE
  StrCpy $9 0
  System::Call 'comctl32::LoadIconWithScaleDown(p ${MODULE}, p ${RESOURCE}, i ${SIZE}, i ${SIZE}, *p .r9)'
  StrCpy ${OUT} $9
!macroend

; A static control of ours on PARENT, empty and unplaced.
!macro AL_STATIC OUT PARENT STYLE FONT ROLE
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", p 0, i ${WS_CHILD}|${WS_VISIBLE}|${WS_CLIPSIBLINGS}|${SS_NOPREFIX}|${STYLE}, i 0, i 0, i 0, i 0, p ${PARENT}, p 0, p 0, p 0) p .s'
  Pop ${OUT}
  SendMessage ${OUT} ${WM_SETFONT} ${FONT} 0
  !insertmacro AL_COLORS ${OUT} ${ROLE}
!macroend

!macro AL_PLACE HWND X Y W H
  System::Call 'user32::SetWindowPos(p ${HWND}, p 0, i ${X}, i ${Y}, i ${W}, i ${H}, i ${AL_SWP_NOZORDER_NOACTIVATE})'
!macroend

; The text block between the side margins at TOP (pixels), as tall as its wrapped text; OUT is
; its bottom.
!macro AL_PLACE_TEXT HWND TOP OUT
  Push ${HWND}
  Push ${TOP}
  Call ${AL_UN}ActivityLog.PlaceText
  Pop ${OUT}
!macroend

; The page's buttons, centred in a row at TOP (pixels); RIGHT may be 0 for one button.
!macro AL_BUTTONS TOP LEFT RIGHT
  Push ${TOP}
  Push ${LEFT}
  Push ${RIGHT}
  Call ${AL_UN}ActivityLog.Buttons
!macroend

; Functions both the installer and the uninstaller have, as `ActivityLog.*` and
; `un.ActivityLog.*` (NSIS keeps the two apart). VERB is what closing the open app is for.
; AL_UN is UN, for the macros above, while the UI macros below are inserted.
!macro ACTIVITY_LOG_SHARED UN VERB

  Function ${UN}ActivityLog.GUIInit
    System::Store "S"

    ; The window's DPI (PerMonitorV2). GetDpiForWindow is Windows 10 1607's; before, the system's.
    System::Call 'user32::GetDpiForWindow(p $HWNDPARENT) i .r0'
    ${If} $0 < 96
      System::Call 'user32::GetDC(p 0) p .r1'
      System::Call 'gdi32::GetDeviceCaps(p r1, i 90) i .r0' ; LOGPIXELSY
      System::Call 'user32::ReleaseDC(p 0, p r1)'
      ${IfThen} $0 < 96 ${|} StrCpy $0 96 ${|}
    ${EndIf}
    StrCpy $ActivityLog.Dpi $0

    ; High contrast first (HIGHCONTRASTW: cbSize, dwFlags, lpszDefaultScheme), then the apps' theme
    StrCpy $ActivityLog.Theme light
    System::Call '*(i 12, i 0, p 0) p .r1'
    System::Call 'user32::SystemParametersInfoW(i 0x42, i 12, p r1, i 0) i .r2' ; SPI_GETHIGHCONTRAST
    System::Call '*$1(i, i .r3)'
    System::Free $1
    IntOp $3 $3 & 1 ; HCF_HIGHCONTRASTON
    ${If} $2 <> 0
    ${AndIf} $3 <> 0
      StrCpy $ActivityLog.Theme contrast
    ${Else}
      ClearErrors
      ReadRegDWORD $3 HKCU "Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" "AppsUseLightTheme"
      ${IfNot} ${Errors}
      ${AndIf} $3 = 0
        StrCpy $ActivityLog.Theme dark
      ${EndIf}
    ${EndIf}

    ; Segoe UI 9 pt, Semibold 11 pt (15 px) and 11 px, at the window's DPI
    ${AL_PX} $0 12
    IntOp $0 0 - $0
    System::Call 'gdi32::CreateFontW(i r0, i 0, i 0, i 0, i 400, i 0, i 0, i 0, i 1, i 0, i 0, i 5, i 0, w "Segoe UI") p .s'
    Pop $ActivityLog.FontBody
    ${AL_PX} $0 15
    IntOp $0 0 - $0
    System::Call 'gdi32::CreateFontW(i r0, i 0, i 0, i 0, i 600, i 0, i 0, i 0, i 1, i 0, i 0, i 5, i 0, w "Segoe UI") p .s'
    Pop $ActivityLog.FontTitle
    ${AL_PX} $0 11
    IntOp $0 0 - $0
    System::Call 'gdi32::CreateFontW(i r0, i 0, i 0, i 0, i 400, i 0, i 0, i 0, i 1, i 0, i 0, i 5, i 0, w "Segoe UI") p .s'
    Pop $ActivityLog.FontSmall

    ; Hide all MUI put in the window: header, lines, branding, buttons. Pages show the buttons
    ; they use; Back is also moved out of sight, since NSIS shows it again on every page.
    StrCpy $1 0
    ${Do}
      System::Call 'user32::FindWindowExW(p $HWNDPARENT, p r1, p 0, p 0) p .r1'
      ${IfThen} $1 P= 0 ${|} ${ExitDo} ${|}
      ShowWindow $1 ${SW_HIDE}
    ${Loop}
    GetDlgItem $1 $HWNDPARENT 3
    System::Call 'user32::SetWindowPos(p r1, p 0, i -32000, i -32000, i 0, i 0, i ${AL_SWP_NOZORDER_NOACTIVATE})'

    ; The client area, and the page area (1018), where nsDialogs and NSIS put each page, filling it
    ${AL_PX} $ActivityLog.Width ${AL_WIDTH}
    ${AL_PX} $ActivityLog.Height ${AL_HEIGHT}
    GetDlgItem $1 $HWNDPARENT 1018
    !insertmacro AL_PLACE $1 0 0 $ActivityLog.Width $ActivityLog.Height

    ; The window, sized to that client area around the centre NSIS chose; it can be minimized
    ; (a slow WebView2 download), never maximized
    System::Call 'user32::GetWindowLongW(p $HWNDPARENT, i ${GWL_STYLE}) i .r1'
    IntOp $1 $1 & 0xFFFEFFFF ; without WS_MAXIMIZEBOX
    System::Call 'user32::SetWindowLongW(p $HWNDPARENT, i ${GWL_STYLE}, i r1)'
    System::Call '*(i, i, i, i) p .r9'
    System::Call 'user32::GetWindowRect(p $HWNDPARENT, p r9)'
    System::Call '*$9(i .r2, i .r3, i .r4, i .r5)'
    System::Call 'user32::GetClientRect(p $HWNDPARENT, p r9)'
    System::Call '*$9(i, i, i .r6, i .r7)'
    System::Free $9
    IntOp $4 $4 - $2 ; window width
    IntOp $5 $5 - $3 ; window height
    IntOp $6 $4 - $6 ; frame width
    IntOp $7 $5 - $7 ; frame height
    IntOp $0 $4 / 2
    IntOp $2 $2 + $0 ; centre x
    IntOp $0 $5 / 2
    IntOp $3 $3 + $0 ; centre y
    IntOp $4 $ActivityLog.Width + $6
    IntOp $5 $ActivityLog.Height + $7
    IntOp $0 $4 / 2
    IntOp $2 $2 - $0
    IntOp $0 $5 / 2
    IntOp $3 $3 - $0
    System::Call 'user32::SetWindowPos(p $HWNDPARENT, p 0, i r2, i r3, i r4, i r5, i 0x34)' ; + SWP_FRAMECHANGED

    ; Dark title bar: DWMWA_USE_IMMERSIVE_DARK_MODE, 20 from Windows 10 20H1, 19 before
    ${If} $ActivityLog.Theme == dark
      System::Call 'dwmapi::DwmSetWindowAttribute(p $HWNDPARENT, i 20, *i 1, i 4) i .r1'
      ${If} $1 <> 0
        System::Call 'dwmapi::DwmSetWindowAttribute(p $HWNDPARENT, i 19, *i 1, i 4)'
      ${EndIf}
    ${EndIf}

    ; The title bar's icon: the exe's icon (103) at the small icon size of this DPI
    System::Call 'user32::GetSystemMetricsForDpi(i 49, i $ActivityLog.Dpi) i .r1' ; SM_CXSMICON
    ${If} $1 < 16
      ${AL_PX} $1 16
    ${EndIf}
    System::Call 'kernel32::GetModuleHandleW(p 0) p .r2'
    !insertmacro AL_LOAD_ICON $ActivityLog.SmallIcon r2 103 $1
    ${If} $ActivityLog.SmallIcon P<> 0
      SendMessage $HWNDPARENT ${WM_SETICON} 0 $ActivityLog.SmallIcon ; ICON_SMALL
    ${EndIf}
    ; The logo and the system's error icon, each loaded at its size here (Windows scales down
    ; from a larger frame only when the ico has no exact one)
    ${AL_PX} $1 ${AL_LOGO}
    !insertmacro AL_LOAD_ICON $ActivityLog.LogoIcon r2 103 $1
    ${AL_PX} $1 ${AL_ERROR_ICON}
    !insertmacro AL_LOAD_ICON $ActivityLog.ErrorIcon 0 ${AL_IDI_ERROR} $1

    !insertmacro AL_COLORS $HWNDPARENT TEXT

    ; Next and Cancel, which the pages place: our font, and in dark mode Windows' dark buttons
    ${ForEach} $0 1 2 + 1
      GetDlgItem $1 $HWNDPARENT $0
      SendMessage $1 ${WM_SETFONT} $ActivityLog.FontBody 0
      !insertmacro AL_COLORS $1 TEXT
      ${If} $ActivityLog.Theme == dark
        System::Call 'uxtheme::SetWindowTheme(p r1, w "DarkMode_Explorer", p 0)'
      ${EndIf}
    ${Next}

    System::Store "L"
  FunctionEnd

  ; The window is gone: free the fonts and icons GUIInit made
  !if "${UN}" == ""
    Function .onGUIEnd
  !else
    Function un.onGUIEnd
  !endif
    System::Call 'gdi32::DeleteObject(p $ActivityLog.FontBody)'
    System::Call 'gdi32::DeleteObject(p $ActivityLog.FontTitle)'
    System::Call 'gdi32::DeleteObject(p $ActivityLog.FontSmall)'
    System::Call 'user32::DestroyIcon(p $ActivityLog.SmallIcon)'
    System::Call 'user32::DestroyIcon(p $ActivityLog.LogoIcon)'
    System::Call 'user32::DestroyIcon(p $ActivityLog.ErrorIcon)'
  FunctionEnd

  ; Stack: the page's dialog. It covers the client area under the outer buttons, which it hides
  ; until the page places the ones it uses.
  Function ${UN}ActivityLog.PageBegin
    Exch $0
    Push $1
    GetDlgItem $1 $HWNDPARENT 3
    ShowWindow $1 ${SW_HIDE}
    EnableWindow $1 0
    GetDlgItem $1 $HWNDPARENT 1
    ShowWindow $1 ${SW_HIDE}
    GetDlgItem $1 $HWNDPARENT 2
    ShowWindow $1 ${SW_HIDE}
    System::Call 'user32::GetWindowLongW(p r0, i ${GWL_STYLE}) i .r1'
    IntOp $1 $1 | ${WS_CLIPSIBLINGS}
    System::Call 'user32::SetWindowLongW(p r0, i ${GWL_STYLE}, i r1)'
    System::Call 'user32::SetWindowPos(p r0, p ${AL_HWND_BOTTOM}, i 0, i 0, i $ActivityLog.Width, i $ActivityLog.Height, i 0x30)' ; SWP_NOACTIVATE | SWP_FRAMECHANGED
    !insertmacro AL_COLORS $0 TEXT
    Pop $1
    Pop $0
  FunctionEnd

  ; Stack: parent, icon, size and top at 96 DPI. Puts the icon, loaded at its size here
  ; (GUIInit), centred at the top; returns the static.
  Function ${UN}ActivityLog.Icon
    System::Store "S"
    Pop $4 ; top
    Pop $3 ; size
    Pop $5 ; icon
    Pop $0 ; parent
    ${AL_PX} $3 $3
    ${AL_PX} $4 $4
    !insertmacro AL_STATIC $6 $0 ${SS_ICON} $ActivityLog.FontBody TEXT
    SendMessage $6 ${STM_SETICON} $5 0
    IntOp $7 $ActivityLog.Width - $3
    IntOp $7 $7 / 2
    !insertmacro AL_PLACE $6 $7 $4 $3 $3
    Push $6
    System::Store "L"
  FunctionEnd

  ; Stack: a static, top (pixels). Places it between the side margins, as tall as its wrapped
  ; text (one line at least); returns its bottom.
  Function ${UN}ActivityLog.PlaceText
    System::Store "S"
    Pop $1 ; top
    Pop $0 ; static
    ${AL_PX} $2 ${AL_SIDE}
    IntOp $3 $2 * 2
    IntOp $3 $ActivityLog.Width - $3 ; width
    System::Call '*(&w2048) p .r4'
    System::Call 'user32::GetWindowTextW(p r0, p r4, i 2048)'
    SendMessage $0 ${WM_GETFONT} 0 0 $5
    System::Call 'user32::GetDC(p r0) p .r6'
    System::Call 'gdi32::SelectObject(p r6, p r5) p .r7'
    System::Call '*(i 0, i 0, i r3, i 0) p .r8'
    System::Call 'user32::DrawTextW(p r6, p r4, i -1, p r8, i ${AL_DT_MEASURE})'
    System::Call '*$8(i, i, i, i .r9)'
    System::Call 'gdi32::SelectObject(p r6, p r7)'
    System::Call 'user32::ReleaseDC(p r0, p r6)'
    System::Free $8
    System::Free $4
    ${AL_PX} $8 ${AL_LINE}
    ${IfThen} $9 < $8 ${|} StrCpy $9 $8 ${|}
    !insertmacro AL_PLACE $0 $2 $1 $3 $9
    IntOp $9 $1 + $9
    Push $9
    System::Store "L"
  FunctionEnd

  ; Stack: top (pixels), left button, right button or 0. Sizes each to its text (75 px at
  ; least), centres them in a row and shows them.
  Function ${UN}ActivityLog.Buttons
    System::Store "S"
    Pop $2 ; right
    Pop $1 ; left
    Pop $0 ; top
    ${AL_PX} $3 ${AL_BUTTON_HEIGHT}
    ${AL_PX} $4 ${AL_BUTTON_MIN}
    ${AL_PX} $5 ${AL_BUTTON_PAD}
    ${AL_PX} $6 ${AL_BUTTON_GAP}
    System::Call '*(i 0, i 0) p .r9' ; SIZE
    ; $7, $8: the two widths
    ${ForEach} $R0 1 2 + 1
      ${If} $R0 = 1
        StrCpy $R1 $1
      ${Else}
        StrCpy $R1 $2
      ${EndIf}
      StrCpy $R2 0
      ${If} $R1 P<> 0
        System::Call '*$9(i 0, i 0)'
        SendMessage $R1 ${AL_BCM_GETIDEALSIZE} 0 $9
        System::Call '*$9(i .R2)'
        IntOp $R2 $R2 + $5
        ${IfThen} $R2 < $4 ${|} StrCpy $R2 $4 ${|}
      ${EndIf}
      ${If} $R0 = 1
        StrCpy $7 $R2
      ${Else}
        StrCpy $8 $R2
      ${EndIf}
    ${Next}
    System::Free $9
    IntOp $R3 $7 + $8
    ${If} $2 P<> 0
      IntOp $R3 $R3 + $6
    ${EndIf}
    IntOp $R4 $ActivityLog.Width - $R3
    IntOp $R4 $R4 / 2 ; left edge
    !insertmacro AL_PLACE $1 $R4 $0 $7 $3
    ShowWindow $1 ${SW_SHOW}
    ${If} $2 P<> 0
      IntOp $R4 $R4 + $7
      IntOp $R4 $R4 + $6
      !insertmacro AL_PLACE $2 $R4 $0 $8 $3
      ShowWindow $2 ${SW_SHOW}
    ${EndIf}
    System::Store "L"
  FunctionEnd

  ; Pushes 1 when Activity Log is running from the install folder, as Tauri's
  ; CheckIfAppIsRunning (utils.nsh) finds it: through the Restart Manager.
  Function ${UN}ActivityLog.AppIsRunning
    System::Store "S"
    StrCpy $9 0
    !insertmacro RestartManager_StartSession $8
    ${If} $8 != ""
      !insertmacro RestartManager_RegisterFile $8 "$INSTDIR\${MAINBINARYNAME}.exe"
      ${If} $0 = 0
        System::Call 'RSTRTMGR::RmGetList(i r8, *i .r1, *i .r2, p 0, *i .r3) i .r0'
        ${IfThen} $0 = 234 ${|} StrCpy $9 1 ${|} ; ERROR_MORE_DATA: some are running
      ${EndIf}
      !insertmacro RestartManager_EndSession $8
    ${EndIf}
    Push $9
    System::Store "L"
  FunctionEnd

  ; Pushes 1 when Activity Log is closed (or was not running), 0 when it could not be: Tauri's
  ; RmShutdown, forced, as CheckIfAppIsRunning does after its question.
  Function ${UN}ActivityLog.CloseApp
    System::Store "S"
    StrCpy $9 1
    !insertmacro RestartManager_StartSession $8
    ${If} $8 != ""
      !insertmacro RestartManager_RegisterFile $8 "$INSTDIR\${MAINBINARYNAME}.exe"
      ${If} $0 = 0
        System::Call 'RSTRTMGR::RmShutdown(i r8, i ${RmForceShutdown}, p 0) i .r0'
        ${IfThen} $0 <> 0 ${|} StrCpy $9 0 ${|}
      ${EndIf}
      !insertmacro RestartManager_EndSession $8
    ${EndIf}
    Push $9
    System::Store "L"
  FunctionEnd

  ; Shown only while Activity Log is open, before anything is changed
  Function ${UN}ActivityLog.AppOpenPage
    Call ${UN}SkipIfPassive
    Call ${UN}ActivityLog.AppIsRunning
    Pop $0
    ${IfThen} $0 <> 1 ${|} Abort ${|}
    nsDialogs::Create 1018
    Pop $0
    ${IfThen} $0 == error ${|} Abort ${|}
    Push $0
    Call ${UN}ActivityLog.PageBegin
    Push $0
    Push $ActivityLog.LogoIcon
    Push ${AL_LOGO}
    Push ${AL_LOGO_TOP}
    Call ${UN}ActivityLog.Icon
    Pop $1
    !insertmacro AL_STATIC $1 $0 ${SS_CENTER} $ActivityLog.FontTitle TEXT
    SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alAppOpen)"
    ${AL_PX} $2 ${AL_LOGO_TOP}
    ${AL_PX} $3 ${AL_LOGO}
    IntOp $2 $2 + $3
    ${AL_PX} $3 ${AL_TITLE_GAP}
    IntOp $2 $2 + $3
    !insertmacro AL_PLACE_TEXT $1 $2 $2
    !insertmacro AL_STATIC $ActivityLog.Text $0 ${SS_CENTER} $ActivityLog.FontBody MUTED
    SendMessage $ActivityLog.Text ${WM_SETTEXT} 0 "STR:$(alAppOpen${VERB})"
    GetDlgItem $1 $HWNDPARENT 1
    SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alClose${VERB})"
    GetDlgItem $1 $HWNDPARENT 2
    SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alCancel)"
    Call ${UN}ActivityLog.AppOpenLayout
    nsDialogs::Show
  FunctionEnd

  ; The text below the title and the buttons below it, again after the text changed
  Function ${UN}ActivityLog.AppOpenLayout
    System::Store "S"
    ${AL_PX} $0 ${AL_LOGO_TOP}
    ${AL_PX} $1 ${AL_LOGO}
    IntOp $0 $0 + $1
    ${AL_PX} $1 ${AL_TITLE_GAP}
    IntOp $0 $0 + $1
    ${AL_PX} $1 ${AL_TITLE_HEIGHT}
    IntOp $0 $0 + $1
    ${AL_PX} $1 ${AL_TEXT_GAP}
    IntOp $0 $0 + $1
    !insertmacro AL_PLACE_TEXT $ActivityLog.Text $0 $0
    ${AL_PX} $1 ${AL_ROW_GAP}
    IntOp $0 $0 + $1
    GetDlgItem $1 $HWNDPARENT 1
    GetDlgItem $2 $HWNDPARENT 2
    !insertmacro AL_BUTTONS $0 $1 $2
    System::Store "L"
  FunctionEnd

  Function ${UN}ActivityLog.AppOpenLeave
    Call ${UN}ActivityLog.CloseApp
    Pop $0
    ${If} $0 <> 1
      SendMessage $ActivityLog.Text ${WM_SETTEXT} 0 "STR:$(alCloseFailed)"
      Call ${UN}ActivityLog.AppOpenLayout
      Abort
    ${EndIf}
  FunctionEnd

  ; MUI's InstFiles page, restyled: the logo, the title, NSIS's status line and progress bar, and
  ; the footer. NSIS's details button and log stay hidden until the install fails.
  Function ${UN}ActivityLog.InstFilesShow
    System::Store "S"
    FindWindow $0 "#32770" "" $HWNDPARENT ; the page (MUI declares its variable after this)
    Push $0
    Call ${UN}ActivityLog.PageBegin
    GetDlgItem $1 $0 1027 ; Show details
    ShowWindow $1 ${SW_HIDE}
    SendMessage $1 ${WM_SETFONT} $ActivityLog.FontBody 0
    !insertmacro AL_COLORS $1 TEXT
    ${If} $ActivityLog.Theme == dark
      System::Call 'uxtheme::SetWindowTheme(p r1, w "DarkMode_Explorer", p 0)'
    ${EndIf}
    GetDlgItem $1 $0 1016 ; the log
    SendMessage $1 ${WM_SETFONT} $ActivityLog.FontBody 0
    ${If} $ActivityLog.Theme == dark
      System::Call 'uxtheme::SetWindowTheme(p r1, w "DarkMode_Explorer", p 0)'
      SendMessage $1 ${AL_LVM_SETBKCOLOR} 0 ${AL_DARK_LIST_BG}
      SendMessage $1 ${AL_LVM_SETTEXTBKCOLOR} 0 ${AL_DARK_LIST_BG}
      SendMessage $1 ${AL_LVM_SETTEXTCOLOR} 0 ${AL_DARK_LIST_TEXT}
    ${EndIf}
    GetDlgItem $1 $0 1031 ; the icon of NSIS's own dialog, when there is one
    ${IfThen} $1 P<> 0 ${|} ShowWindow $1 ${SW_HIDE} ${|}

    Push $0
    Push $ActivityLog.LogoIcon
    Push ${AL_LOGO}
    Push ${AL_LOGO_TOP}
    Call ${UN}ActivityLog.Icon
    Pop $ActivityLog.Logo

    ${AL_PX} $2 ${AL_SIDE}
    IntOp $3 $2 * 2
    IntOp $3 $ActivityLog.Width - $3 ; text width
    ${AL_PX} $4 ${AL_LOGO_TOP}
    ${AL_PX} $5 ${AL_LOGO}
    IntOp $4 $4 + $5
    ${AL_PX} $5 ${AL_TITLE_GAP}
    IntOp $4 $4 + $5 ; title top
    ${AL_PX} $5 ${AL_TITLE_HEIGHT}
    !insertmacro AL_STATIC $ActivityLog.Title $0 ${SS_CENTER} $ActivityLog.FontTitle TEXT
    !insertmacro AL_PLACE $ActivityLog.Title $2 $4 $3 $5
    IntOp $4 $4 + $5
    ${AL_PX} $5 ${AL_TEXT_GAP}
    IntOp $4 $4 + $5 ; status top

    ; The status line: one centred line, cut with an ellipsis
    GetDlgItem $1 $0 1006
    System::Call 'user32::GetWindowLongW(p r1, i ${GWL_STYLE}) i .r6'
    IntOp $6 $6 & 0xFFFFFFE0 ; no SS_TYPEMASK type (was SS_LEFTNOWORDWRAP)
    IntOp $6 $6 | ${SS_CENTER}
    IntOp $6 $6 | ${SS_ENDELLIPSIS}
    IntOp $6 $6 | ${SS_NOPREFIX}
    IntOp $6 $6 | ${WS_CLIPSIBLINGS}
    System::Call 'user32::SetWindowLongW(p r1, i ${GWL_STYLE}, i r6)'
    SendMessage $1 ${WM_SETFONT} $ActivityLog.FontBody 0
    !insertmacro AL_COLORS $1 MUTED
    ${AL_PX} $5 ${AL_LINE}
    !insertmacro AL_PLACE $1 $2 $4 $3 $5
    IntOp $4 $4 + $5
    ${AL_PX} $5 ${AL_ROW_GAP}
    IntOp $4 $4 + $5 ; bar top

    ; The bar: Windows' theme off, so it is flat, square and in our colours (but in high contrast)
    GetDlgItem $1 $0 1004
    ${If} $ActivityLog.Theme != contrast
      System::Call 'uxtheme::SetWindowTheme(p r1, w "", w "")'
      System::Call 'user32::GetWindowLongW(p r1, i ${GWL_STYLE}) i .r6'
      IntOp $6 $6 & 0xFF7FFFFF ; no WS_BORDER
      System::Call 'user32::SetWindowLongW(p r1, i ${GWL_STYLE}, i r6)'
      System::Call 'user32::GetWindowLongW(p r1, i ${GWL_EXSTYLE}) i .r6'
      IntOp $6 $6 & 0xFFFDFDFF ; no WS_EX_STATICEDGE, WS_EX_CLIENTEDGE
      System::Call 'user32::SetWindowLongW(p r1, i ${GWL_EXSTYLE}, i r6)'
      ${If} $ActivityLog.Theme == dark
        SendMessage $1 ${PBM_SETBARCOLOR} 0 ${AL_DARK_BAR}
        SendMessage $1 ${AL_PBM_SETBKCOLOR} 0 ${AL_DARK_TRACK}
      ${Else}
        SendMessage $1 ${PBM_SETBARCOLOR} 0 ${AL_LIGHT_BAR}
        SendMessage $1 ${AL_PBM_SETBKCOLOR} 0 ${AL_LIGHT_TRACK}
      ${EndIf}
    ${EndIf}
    ${AL_PX} $5 ${AL_BAR_HEIGHT}
    System::Call 'user32::SetWindowPos(p r1, p 0, i r2, i r4, i r3, i r5, i 0x34)' ; + SWP_FRAMECHANGED

    ${AL_PX} $5 ${AL_FOOTER_HEIGHT}
    ${AL_PX} $6 ${AL_FOOTER_BOTTOM}
    IntOp $4 $ActivityLog.Height - $5
    IntOp $4 $4 - $6
    !insertmacro AL_STATIC $ActivityLog.Footer $0 ${SS_CENTER} $ActivityLog.FontSmall MUTED
    SendMessage $ActivityLog.Footer ${WM_SETTEXT} 0 "STR:$(alFooter)"
    !insertmacro AL_PLACE $ActivityLog.Footer $2 $4 $3 $5

    ; Close when done: the screen after it is shown by the leave function, and nothing follows
    SetAutoClose true
    !if "${UN}" == ""
      SendMessage $ActivityLog.Title ${WM_SETTEXT} 0 "STR:$(alInstalling)"
      GetDlgItem $1 $0 1006
      SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alCopying)"
    !else
      SendMessage $ActivityLog.Title ${WM_SETTEXT} 0 "STR:$(alUninstalling)"
      GetDlgItem $1 $0 1006
      SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alRemoving)"
      ; Tauri's sections report each file: into the log, not the status line
      SetDetailsPrint listonly
    !endif
    System::Store "L"
  FunctionEnd

  ; When the sections ended (NSIS calls the leave function then, failed or not)
  Function ${UN}ActivityLog.InstFilesLeave
    System::Store "S"
    FindWindow $0 "#32770" "" $HWNDPARENT ; the page (MUI declares its variable after this)
    IfAbort failed
    ${If} $PassiveMode <> 1
    !if "${UN}" == "un."
    ${AndIf} $UpdateMode <> 1
    !endif
      GetDlgItem $1 $0 1004
      SendMessage $1 ${PBM_SETPOS} ${AL_PB_RANGE} 0
      GetDlgItem $1 $0 1006
      !if "${UN}" == ""
        SendMessage $ActivityLog.Title ${WM_SETTEXT} 0 "STR:$(alStarting)"
        ${If} $NoShortcutMode = 1
          SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alInstalledNoShortcut)"
        ${Else}
          SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alInstalled)"
        ${EndIf}
      !else
        SendMessage $ActivityLog.Title ${WM_SETTEXT} 0 "STR:$(alUninstalled)"
        SendMessage $1 ${WM_SETTEXT} 0 "STR:"
      !endif
      System::Call 'user32::RedrawWindow(p $HWNDPARENT, p 0, p 0, i 0x185)' ; now, children too
      !if "${UN}" == ""
        ; Not elevated (nsis_tauri_utils' RunAsUser), as Tauri's Finish page does
        Call RunMainBinary
        Sleep 1500 ; while the app's window comes, then this one closes
      !else
        Sleep 1200
      !endif
    ${EndIf}
    Goto done

    failed:
    ; The last line of the log is why: Tauri's Abort message, or its last detail
    GetDlgItem $1 $0 1016
    SendMessage $1 ${AL_LVM_GETITEMCOUNT} 0 0 $2
    StrCpy $3 ""
    ${If} $2 > 0
      IntOp $2 $2 - 1
      System::Call '*(&w1024) p .r4'
      ; LVITEMW: mask, iItem, iSubItem, state, stateMask, pszText, cchTextMax, ...
      System::Call '*(i 0, i r2, i 0, i 0, i 0, p r4, i 1024, i 0, p 0, i 0, i 0, i 0, p 0, i 0, i 0) p .r5'
      SendMessage $1 ${AL_LVM_GETITEMTEXTW} $2 $5
      System::Call '*$4(&w1024 .r3)'
      System::Free $5
      System::Free $4
    ${EndIf}
    ShowWindow $ActivityLog.Logo ${SW_HIDE}
    ShowWindow $ActivityLog.Footer ${SW_HIDE}
    GetDlgItem $1 $0 1004
    ShowWindow $1 ${SW_HIDE}
    GetDlgItem $1 $0 1006
    ShowWindow $1 ${SW_HIDE}
    Push $0
    Push $ActivityLog.Title
    !if "${UN}" == ""
      Push "$(alInstallFailed)"
    !else
      Push "$(alUninstallFailed)"
    !endif
    Push $3
    Call ${UN}ActivityLog.ErrorLayout
    Pop $2 ; buttons' row
    Push $0
    Push $2
    GetDlgItem $1 $HWNDPARENT 2 ; Close: Cancel, `^CancelBtn`, all NSIS enables after a failure
    Push $1
    Call ${UN}ActivityLog.DetailsRow
    done:
    System::Store "L"
  FunctionEnd

  ; Stack: the InstFiles page, the buttons' row top, the button that closes. Shows NSIS's Show
  ; details and that button in the row; the log, once shown, covers all above them.
  Function ${UN}ActivityLog.DetailsRow
    System::Store "S"
    Pop $3 ; the closing button
    Pop $2 ; row top
    Pop $0 ; page
    GetDlgItem $1 $0 1016
    ${AL_PX} $4 16
    ${AL_PX} $5 12
    IntOp $6 $ActivityLog.Width - $4
    IntOp $6 $6 - $4
    IntOp $7 $2 - $5
    IntOp $7 $7 - $4
    System::Call 'user32::SetWindowPos(p r1, p 0, i r4, i r4, i r6, i r7, i 0x10)' ; on top, SWP_NOACTIVATE
    GetDlgItem $1 $0 1027
    !insertmacro AL_BUTTONS $2 $1 $3
    System::Store "L"
  FunctionEnd

  ; Stack: parent, title static (0 for a new one), title, message. The failure screens' error
  ; icon, title and message; pushes the top of the buttons' row below them.
  Function ${UN}ActivityLog.ErrorLayout
    System::Store "S"
    Pop $1 ; message
    Pop $5 ; title
    Pop $2 ; title static
    Pop $0 ; parent
    Push $0
    Push $ActivityLog.ErrorIcon
    Push ${AL_ERROR_ICON}
    Push ${AL_ERROR_TOP}
    Call ${UN}ActivityLog.Icon
    Pop $6
    ${AL_PX} $3 ${AL_ERROR_TOP}
    ${AL_PX} $4 ${AL_ERROR_ICON}
    IntOp $3 $3 + $4
    ${AL_PX} $4 ${AL_TITLE_GAP}
    IntOp $3 $3 + $4 ; title top
    ${If} $2 P= 0
      !insertmacro AL_STATIC $2 $0 ${SS_CENTER} $ActivityLog.FontTitle TEXT
    ${EndIf}
    SendMessage $2 ${WM_SETTEXT} 0 "STR:$5"
    !insertmacro AL_PLACE_TEXT $2 $3 $3
    ${AL_PX} $4 ${AL_TEXT_GAP}
    IntOp $3 $3 + $4
    !insertmacro AL_STATIC $2 $0 ${SS_CENTER} $ActivityLog.FontBody MUTED
    SendMessage $2 ${WM_SETTEXT} 0 "STR:$1"
    StrCpy $7 $3 ; message top
    !insertmacro AL_PLACE_TEXT $2 $3 $3
    ; A message too long for the window is cut at its last line that fits, with an ellipsis
    ; (Show details has it whole), so the buttons stay in the window
    ${AL_PX} $4 ${AL_ROW_GAP}
    ${AL_PX} $5 ${AL_BUTTON_HEIGHT}
    ${AL_PX} $6 ${AL_FOOTER_BOTTOM}
    IntOp $8 $ActivityLog.Height - $6
    IntOp $8 $8 - $5
    IntOp $8 $8 - $4 ; the lowest the message may end
    ${If} $3 > $8
      ${AL_PX} $6 ${AL_LINE}
      IntOp $5 $8 - $7
      IntOp $5 $5 / $6
      IntOp $5 $5 * $6 ; whole lines
      System::Call 'user32::GetWindowLongW(p r2, i ${GWL_STYLE}) i .r6'
      IntOp $6 $6 | 0x2000 ; SS_EDITCONTROL: the ellipsis on the last line shown
      IntOp $6 $6 | ${SS_ENDELLIPSIS}
      System::Call 'user32::SetWindowLongW(p r2, i ${GWL_STYLE}, i r6)'
      ${AL_PX} $6 ${AL_SIDE}
      IntOp $9 $6 * 2
      IntOp $9 $ActivityLog.Width - $9
      !insertmacro AL_PLACE $2 $6 $7 $9 $5
      IntOp $3 $7 + $5
    ${EndIf}
    IntOp $3 $3 + $4
    Push $3
    System::Store "L"
  FunctionEnd
!macroend

; Inserted by installer.nsi in place of the Welcome page.
!macro ACTIVITY_LOG_INSTALLER_UI
  ; English only, as the app. `^CancelBtn` is only ever shown on the failure screen, where
  ; Cancel closes; the pages with a Cancel set their own text.
  LangString ^CancelBtn 1033 "Close"
  LangString alInstalling 1033 "Installing $(^Name)"
  LangString alCopying 1033 "Copying files…"
  LangString alStarting 1033 "Starting $(^Name)…"
  LangString alInstalled 1033 "Installed. You'll find it in the Start menu."
  LangString alInstalledNoShortcut 1033 "Installed."
  LangString alFooter 1033 "Version ${VERSION} · for you only"
  LangString alInstallFailed 1033 "$(^Name) couldn't be installed"
  LangString alAppOpen 1033 "$(^Name) is open"
  LangString alAppOpenUpdate 1033 "Setup closes it to update it. Nothing you collected is lost."
  LangString alCloseUpdate 1033 "Close it and update"
  LangString alAppOpenUninstall 1033 "Setup closes it to uninstall it."
  LangString alCloseUninstall 1033 "Close it and uninstall"
  LangString alCancel 1033 "Cancel"
  LangString alCloseFailed 1033 "Setup couldn't close $(^Name). Close it, then try again."
  LangString alNewerTitle 1033 "A newer version is installed"
  LangString alNewerText 1033 "$(^Name) $ActivityLog.NewerVersion is installed, which is newer than this setup (${VERSION}). To install ${VERSION}, uninstall $(^Name) first."
  LangString alClose 1033 "Close"
  LangString alUninstallQuestion 1033 "Uninstall $(^Name)?"
  LangString alDeleteData 1033 "Also delete my settings and cached activity"
  LangString alUninstall 1033 "Uninstall"
  LangString alUninstalling 1033 "Uninstalling $(^Name)"
  LangString alRemoving 1033 "Removing files…"
  LangString alUninstalled 1033 "$(^Name) was uninstalled"
  LangString alUninstallFailed 1033 "$(^Name) couldn't be uninstalled"

  !define AL_UN ""
  !define MUI_CUSTOMFUNCTION_GUIINIT ActivityLog.GUIInit
  !insertmacro ACTIVITY_LOG_SHARED "" Update

  ; A newer version installed: refused, with only Close (.onInit found it)
  Function ActivityLog.DowngradePage
    ${IfThen} $ActivityLog.NewerVersion == "" ${|} Abort ${|}
    nsDialogs::Create 1018
    Pop $0
    ${IfThen} $0 == error ${|} Abort ${|}
    Push $0
    Call ActivityLog.PageBegin
    Push $0
    Push 0
    Push "$(alNewerTitle)"
    Push "$(alNewerText)"
    Call ActivityLog.ErrorLayout
    Pop $1
    GetDlgItem $2 $HWNDPARENT 1
    EnableWindow $2 0 ; Enter must not install
    GetDlgItem $2 $HWNDPARENT 2
    SendMessage $2 ${WM_SETTEXT} 0 "STR:$(alClose)"
    !insertmacro AL_BUTTONS $1 $2 0
    SendMessage $HWNDPARENT ${AL_DM_SETDEFID} 2 0
    SendMessage $2 ${BM_SETSTYLE} ${BS_DEFPUSHBUTTON} 1
    System::Call 'user32::SetFocus(p r2)'
    nsDialogs::Show
  FunctionEnd

  Function ActivityLog.DowngradeLeave
    Abort
  FunctionEnd

  Page custom ActivityLog.DowngradePage ActivityLog.DowngradeLeave
  !undef AL_UN
!macroend

; Inserted by installer.nsi in place of MUI's uninstall confirm page.
!macro ACTIVITY_LOG_UNINSTALLER_UI
  !define AL_UN "un."
  !define MUI_CUSTOMFUNCTION_UNGUIINIT un.ActivityLog.GUIInit
  !insertmacro ACTIVITY_LOG_SHARED "un." Uninstall

  ; The one question. Tauri's `$DeleteAppDataCheckbox` is the checkbox; the leave function sets
  ; `$DeleteAppDataCheckboxState`, which the Uninstall section reads.
  Function un.ActivityLog.ConfirmPage
    Call un.SkipIfPassive
    System::Store "S"
    nsDialogs::Create 1018
    Pop $0
    ${If} $0 == error
      System::Store "L"
      Abort
    ${EndIf}
    Push $0
    Call un.ActivityLog.PageBegin
    Push $0
    Push $ActivityLog.LogoIcon
    Push ${AL_LOGO}
    Push ${AL_LOGO_TOP}
    Call un.ActivityLog.Icon
    Pop $1
    ${AL_PX} $2 ${AL_LOGO_TOP}
    ${AL_PX} $3 ${AL_LOGO}
    IntOp $2 $2 + $3
    ${AL_PX} $3 ${AL_TITLE_GAP}
    IntOp $2 $2 + $3
    !insertmacro AL_STATIC $1 $0 ${SS_CENTER} $ActivityLog.FontTitle TEXT
    SendMessage $1 ${WM_SETTEXT} 0 "STR:$(alUninstallQuestion)"
    !insertmacro AL_PLACE_TEXT $1 $2 $2
    ${AL_PX} $3 ${AL_CHECK_GAP}
    IntOp $2 $2 + $3 ; checkbox top

    System::Call '*(i 0, i 0) p .r9' ; SIZE
    ${If} $ActivityLog.Theme == dark
      ; A themed checkbox draws its text black whatever the colours. So in dark mode the box
      ; keeps its text, which screen readers read as its name, but is only as wide as the box,
      ; which cuts the text off; a label beside it draws the text and toggles the box. The box
      ; cannot show its own focus rectangle then, so a frame around the label shows while the
      ; box has keyboard focus (`un.ActivityLog.CheckFocus`).
      ${NSD_CreateCheckbox} 0 0 0 0 ""
      Pop $DeleteAppDataCheckbox
      SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $ActivityLog.FontBody 0
      System::Call 'uxtheme::SetWindowTheme(p $DeleteAppDataCheckbox, w "DarkMode_Explorer", p 0)'
      !insertmacro AL_COLORS $DeleteAppDataCheckbox TEXT
      SendMessage $DeleteAppDataCheckbox ${AL_BCM_GETIDEALSIZE} 0 $9
      System::Call '*$9(i .r4, i .r5)' ; the box alone
      SendMessage $DeleteAppDataCheckbox ${WM_SETTEXT} 0 "STR:$(alDeleteData)"
      ${NSD_CreateLabel} 0 0 0 0 "$(alDeleteData)"
      Pop $3
      ${NSD_AddStyle} $3 ${SS_NOTIFY}
      SendMessage $3 ${WM_SETFONT} $ActivityLog.FontBody 0
      !insertmacro AL_COLORS $3 TEXT
      ${NSD_OnClick} $3 un.ActivityLog.ToggleDeleteData
      ; the frame, under the label; SS_WHITEFRAME draws in the window colour, light on dark
      System::Call 'user32::CreateWindowExW(i 0, w "STATIC", p 0, i ${WS_CHILD}|${WS_CLIPSIBLINGS}|0x9, i 0, i 0, i 0, i 0, p r0, p 0, p 0, p 0) p .s'
      Pop $ActivityLog.CheckFrame
      ; the label's width, one line
      System::Call 'user32::GetDC(p r3) p .r6'
      System::Call 'gdi32::SelectObject(p r6, p $ActivityLog.FontBody) p .r7'
      System::Call '*(i 0, i 0, i 0, i 0) p .r8'
      System::Call 'user32::DrawTextW(p r6, w "$(alDeleteData)", i -1, p r8, i ${AL_DT_MEASURE_LINE})'
      System::Call '*$8(i, i, i .r1, i .R0)'
      System::Free $8
      System::Call 'gdi32::SelectObject(p r6, p r7)'
      System::Call 'user32::ReleaseDC(p r3, p r6)'
      ${AL_PX} $6 ${AL_LABEL_GAP}
      IntOp $7 $4 + $6
      IntOp $7 $7 + $1 ; the pair's width
      IntOp $7 $ActivityLog.Width - $7
      IntOp $7 $7 / 2
      ${IfThen} $5 < $R0 ${|} StrCpy $5 $R0 ${|} ; the row's height
      !insertmacro AL_PLACE $DeleteAppDataCheckbox $7 $2 $4 $5
      IntOp $7 $7 + $4
      IntOp $7 $7 + $6
      !insertmacro AL_PLACE $3 $7 $2 $1 $5
      ${AL_PX} $8 2
      IntOp $R1 $7 - $8
      IntOp $R2 $2 - $8
      IntOp $R3 $1 + $8
      IntOp $R3 $R3 + $8
      IntOp $R4 $5 + $8
      IntOp $R4 $R4 + $8
      !insertmacro AL_PLACE $ActivityLog.CheckFrame $R1 $R2 $R3 $R4
      ${NSD_CreateTimer} un.ActivityLog.CheckFocus 100
    ${Else}
      ${NSD_CreateCheckbox} 0 0 0 0 "$(alDeleteData)"
      Pop $DeleteAppDataCheckbox
      SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $ActivityLog.FontBody 0
      !insertmacro AL_COLORS $DeleteAppDataCheckbox TEXT
      SendMessage $DeleteAppDataCheckbox ${AL_BCM_GETIDEALSIZE} 0 $9
      System::Call '*$9(i .r4, i .r5)'
      IntOp $7 $ActivityLog.Width - $4
      IntOp $7 $7 / 2
      !insertmacro AL_PLACE $DeleteAppDataCheckbox $7 $2 $4 $5
    ${EndIf}
    System::Free $9
    ${If} $DeleteAppDataCheckboxState = ${BST_CHECKED}
      ${NSD_Check} $DeleteAppDataCheckbox
    ${EndIf}

    IntOp $2 $2 + $5
    ${AL_PX} $3 ${AL_ROW_GAP}
    IntOp $2 $2 + $3
    GetDlgItem $3 $HWNDPARENT 1
    SendMessage $3 ${WM_SETTEXT} 0 "STR:$(alUninstall)"
    GetDlgItem $4 $HWNDPARENT 2
    SendMessage $4 ${WM_SETTEXT} 0 "STR:$(alCancel)"
    !insertmacro AL_BUTTONS $2 $3 $4
    System::Store "L"
    nsDialogs::Show
  FunctionEnd

  Function un.ActivityLog.ToggleDeleteData
    Pop $0 ; the label
    ${NSD_GetState} $DeleteAppDataCheckbox $0
    ${If} $0 = ${BST_CHECKED}
      ${NSD_Uncheck} $DeleteAppDataCheckbox
    ${Else}
      ${NSD_Check} $DeleteAppDataCheckbox
    ${EndIf}
    ${NSD_SetFocus} $DeleteAppDataCheckbox
  FunctionEnd

  ; Dark mode: the frame around the label while the box has the focus and Windows shows focus
  ; (keyboard use: WM_QUERYUISTATE without UISF_HIDEFOCUS)
  Function un.ActivityLog.CheckFocus
    System::Store "S"
    System::Call 'user32::GetFocus() p .r0'
    SendMessage $DeleteAppDataCheckbox 0x0129 0 0 $1 ; WM_QUERYUISTATE
    IntOp $1 $1 & 1 ; UISF_HIDEFOCUS
    ${If} $0 P= $DeleteAppDataCheckbox
    ${AndIf} $1 = 0
      ShowWindow $ActivityLog.CheckFrame ${SW_SHOW}
    ${Else}
      ShowWindow $ActivityLog.CheckFrame ${SW_HIDE}
    ${EndIf}
    System::Store "L"
  FunctionEnd

  Function un.ActivityLog.ConfirmLeave
    ${NSD_KillTimer} un.ActivityLog.CheckFocus
    ${NSD_GetState} $DeleteAppDataCheckbox $DeleteAppDataCheckboxState
  FunctionEnd
  !undef AL_UN
!macroend
