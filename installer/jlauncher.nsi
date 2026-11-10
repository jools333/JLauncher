; JLauncher NSIS Modern Installer Script
Unicode True

!define PRODUCT_NAME "JLauncher"
!ifndef VERSION
  !define VERSION "0.2.8"
!endif
!define PRODUCT_PUBLISHER "JLauncher Contributors"
!define PRODUCT_WEB_SITE "https://github.com/jools333/JLauncher"
!define PRODUCT_EXE "jlauncher.exe"
!define UNINST_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"

; MultiUser: Per-User install by default (no admin required), matches modern app standards
!define MULTIUSER_EXECUTIONLEVEL Highest
!define MULTIUSER_MUI
!define MULTIUSER_INSTALLMODE_DEFAULT_CURRENTUSER
!define MULTIUSER_INSTALLMODE_COMMANDLINE
!define MULTIUSER_INSTALLMODE_INSTDIR "Programs\${PRODUCT_NAME}"
!define MULTIUSER_INSTALLMODE_INSTDIR_REGISTRY_KEY "${UNINST_REG_KEY}"
!define MULTIUSER_INSTALLMODE_INSTDIR_REGISTRY_VALUENAME "InstallLocation"

!include "MultiUser.nsh"
!include "MUI2.nsh"
!include "FileFunc.nsh"

Name "${PRODUCT_NAME} ${VERSION}"
OutFile "..\target\installer\JLauncher_${VERSION}_x64-setup.exe"
SetCompressor /SOLID lzma

; Interface Configuration
!define MUI_ABORTWARNING
!define MUI_ICON "..\assets\icon.ico"
!define MUI_UNICON "..\assets\icon.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP_STRETCH AspectFit

; Installer Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MULTIUSER_PAGE_INSTALLMODE
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish page with launch option
!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${PRODUCT_NAME} now"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Language
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Russian"

Function .onInit
  !insertmacro MULTIUSER_INIT
  ; Close running instance before updating
  ExecWait 'taskkill /F /IM "${PRODUCT_EXE}"' $0
FunctionEnd

Function un.onInit
  !insertmacro MULTIUSER_UNINIT
  ; Close running instance before uninstalling
  ExecWait 'taskkill /F /IM "${PRODUCT_EXE}"' $0
FunctionEnd

Section "MainSection" SEC01
  SetOutPath "$INSTDIR"
  SetOverwrite on

  ; Ensure any active process is closed before file replacement
  ExecWait 'taskkill /F /IM "${PRODUCT_EXE}"' $0

  ; Copy application files
  File "..\jlauncher.exe"
  File "..\assets\icon.ico"
  File "..\README.md"

  ; Create Shortcuts
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\icon.ico" 0
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\icon.ico" 0
  CreateShortCut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\icon.ico" 0

  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Registry configuration for Windows "Installed Apps" / "Add or Remove Programs"
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "DisplayIcon" "$INSTDIR\${PRODUCT_EXE},0"
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr SHCTX "${UNINST_REG_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD SHCTX "${UNINST_REG_KEY}" "NoModify" 1
  WriteRegDWORD SHCTX "${UNINST_REG_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD SHCTX "${UNINST_REG_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  ; Terminate running instance if active
  ExecWait 'taskkill /F /IM "${PRODUCT_EXE}"' $0

  ; Remove shortcuts
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"

  ; Remove application files
  Delete "$INSTDIR\${PRODUCT_EXE}"
  Delete "$INSTDIR\icon.ico"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"

  ; Remove installation directory if empty
  RMDir "$INSTDIR"

  ; Clean registry
  DeleteRegKey SHCTX "${UNINST_REG_KEY}"
SectionEnd
