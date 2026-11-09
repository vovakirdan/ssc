Unicode true
!include "MUI2.nsh"
!include "WordFunc.nsh"
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
; Microsoft Learn: detect WebView2 Runtime, not the Edge browser channel.
; SetRegView 32 selects HKLM\SOFTWARE\WOW6432Node on 64-bit Windows.
Function DetectWebView2
  StrCpy $0 0
  SetRegView 32
  ReadRegStr $1 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${VersionCompare} "$1" "0.0.0.0" $2
  StrCmp $2 1 runtime_present
  ReadRegStr $1 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${VersionCompare} "$1" "0.0.0.0" $2
  StrCmp $2 1 runtime_present
  SetRegView 64
  ReadRegStr $1 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${VersionCompare} "$1" "0.0.0.0" $2
  StrCmp $2 1 runtime_present
  ReadRegStr $1 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${VersionCompare} "$1" "0.0.0.0" $2
  StrCmp $2 1 runtime_present runtime_checked
runtime_present:
  StrCpy $0 1
runtime_checked:
  SetRegView 32
  ClearErrors
FunctionEnd
Section "Install SSC"
  SetShellVarContext current
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "MicrosoftEdgeWebview2Setup.exe"
  File "protect-profile.ps1"
  Call DetectWebView2
  StrCmp $0 1 webview_ok
  nsExec::ExecToStack /TIMEOUT=120000 '"$PLUGINSDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install'
  Pop $0
  Pop $1
  Call DetectWebView2
  StrCmp $0 1 webview_ok
  MessageBox MB_OK|MB_ICONSTOP "WebView2 installation failed. Please check your connection and try again." /SD IDOK
  SetErrorLevel 10
  Abort
webview_ok:
  SetOutPath "$INSTDIR"
  File "ssc.exe"
  StrCpy $SSCProfile "$APPDATA\com.vladimirkirdan.ssc"
  CreateDirectory "$SSCProfile"
  ; An intermediate NSIS process otherwise forwards pwsh 7's module search path
  ; to Windows PowerShell 5, which can break its module autoloading at startup.
  System::Call 'kernel32::SetEnvironmentVariableW(w "PSModulePath", p 0) i .r0'
  nsExec::ExecToStack /TIMEOUT=30000 '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\protect-profile.ps1" -Path "$SSCProfile"'
  Pop $0
  Pop $1
  StrCmp $0 0 acl_ok
  MessageBox MB_OK|MB_ICONSTOP "Cannot protect the activation profile. Installation stopped." /SD IDOK
  SetErrorLevel 11
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
