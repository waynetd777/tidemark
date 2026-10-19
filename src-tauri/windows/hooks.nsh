; Copyright (c) 2026 Wayne Davies
; SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

; The app's data is in %LOCALAPPDATA%\<identifier> (cache.rs dir), which the uninstaller's "Delete
; the application data" box already clears. What it doesn't know about: the Run key's values for
; Open at Login and the background helper (login_win.rs, recorder_win.rs), which would otherwise
; start a program that is gone at every sign-in, and the update installers the updater leaves in
; Temp. Not for an update, which runs the uninstaller too and keeps both settings.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Tidemark"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Tidemark Helper"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Tidemark"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Tidemark Helper"
    SetShellVarContext current
    FindFirst $0 $1 "$TEMP\${PRODUCTNAME}-*-updater-*"
    ${DoWhile} $1 != ""
      RmDir /r "$TEMP\$1"
      FindNext $0 $1
    ${Loop}
    FindClose $0
  ${EndIf}
!macroend
