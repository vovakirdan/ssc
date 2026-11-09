Unicode true
!include "MUI2.nsh"
Name "SSC"
OutFile "@@OUTPUT@@"
InstallDir "$LOCALAPPDATA\Programs\SSC"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails nevershow
ShowUninstDetails nevershow
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\ssc.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Var SSCProfile
Section "Install SSC"
  SetShellVarContext current
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "MicrosoftEdgeWebview2Setup.exe"
  File "protect-profile.ps1"
  ExecWait '"$PLUGINSDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install' $0
  StrCmp $0 0 webview_ok
  StrCmp $0 3010 webview_ok
  MessageBox MB_OK|MB_ICONSTOP "WebView2 installation failed. Please check your connection and try again."
  SetErrorLevel 1
  Abort
webview_ok:
  SetOutPath "$INSTDIR"
  File "ssc.exe"
  StrCpy $SSCProfile "$APPDATA\com.vladimirkirdan.ssc"
  CreateDirectory "$SSCProfile"
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\protect-profile.ps1" -Path "$SSCProfile"'
  Pop $0
  Pop $1
  StrCmp $0 0 acl_ok
  MessageBox MB_OK|MB_ICONSTOP "Cannot protect the activation profile. Installation stopped."
  SetErrorLevel 1
  Abort
acl_ok:
  IfFileExists "$SSCProfile\access.active" profile_done
  SetOutPath "$SSCProfile"
  File /oname=bootstrap.json "bootstrap.json"
profile_done:
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\SSC"
  CreateShortcut "$SMPROGRAMS\SSC\SSC.lnk" "$INSTDIR\ssc.exe"
  CreateShortcut "$SMPROGRAMS\SSC\Uninstall SSC.lnk" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSC" "DisplayName" "SSC"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSC" "DisplayVersion" "@@VERSION@@"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSC" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSC" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSC" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\ssc.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\SSC\SSC.lnk"
  Delete "$SMPROGRAMS\SSC\Uninstall SSC.lnk"
  RMDir "$SMPROGRAMS\SSC"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSC"
SectionEnd
