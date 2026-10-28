; S&F Commander - Windows installer
;
;   makensis /DVERSION=6.2.10 /DAPP_EXE=<SF-Commander-6.2.10.exe>
;            /DLAUNCHER=<SF Commander.exe> /DICON=<icon.ico>
;            /DOUT=<SF-Commander-Setup-6.2.10.exe> sf-commander.nsi
;
; Installs per user into %LOCALAPPDATA%\Programs\SF Commander - no admin
; rights. The app keeps helper.toml and its other data next to the exe and
; its updater writes the next release into the same folder, so it has to
; live somewhere the user can write; Program Files would break both.

; 64-bit when this NSIS has the amd64 stubs (Linux packages do), otherwise
; the standard 32-bit build of the official Windows NSIS - both run the same
!if /FileExists "${NSISDIR}/Stubs/lzma_solid-amd64-unicode"
  Target amd64-unicode
!else
  Unicode true
!endif
!include "MUI2.nsh"
!include "WordFunc.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !error "Pass /DVERSION=x.y.z"
!endif
!ifndef VIVERSION
  !define VIVERSION "${VERSION}.0"
!endif

!define REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SFCommander"
!define LINK "S&F Commander.lnk"
!define LAUNCH "SF Commander.exe"

Name "S&F Commander" "S&&F Commander"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\SF Commander"
InstallDirRegKey HKCU "${REGKEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "SF Commander ${VERSION}"
ShowInstDetails nevershow
ShowUninstDetails nevershow

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_NODESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\${LAUNCH}"
!define MUI_WELCOMEFINISHPAGE_BITMAP "welcome.bmp"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP "header.bmp"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; The first language is the fallback for every system that is neither
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Czech"

LangString SEC_APP ${LANG_ENGLISH} "S&F Commander"
LangString SEC_APP ${LANG_CZECH} "S&F Commander"
LangString SEC_DESKTOP ${LANG_ENGLISH} "Desktop shortcut"
LangString SEC_DESKTOP ${LANG_CZECH} "Zástupce na ploše"
LangString UN_DATA ${LANG_ENGLISH} "Also delete settings and data (saved login, logs, guild history)?$\n$\nChoose No to keep them for a later reinstall."
LangString UN_DATA ${LANG_CZECH} "Smazat i nastavení a data (uložené přihlášení, logy, historii cechu)?$\n$\nZvol Ne, pokud je chceš nechat pro pozdější instalaci."

VIProductVersion "${VIVERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductName" "S&F Commander"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileDescription" "S&F Commander ${VERSION} Setup"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "LegalCopyright" "MIT"

Section "!$(SEC_APP)" SecApp
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "/oname=SF-Commander-${VERSION}.exe" "${APP_EXE}"
  File "/oname=${LAUNCH}" "${LAUNCHER}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  Call RemoveOlderReleases

  CreateShortcut "$SMPROGRAMS\${LINK}" "$INSTDIR\${LAUNCH}" "" "$INSTDIR\${LAUNCH}" 0

  WriteRegStr HKCU "${REGKEY}" "DisplayName" "S&F Commander"
  WriteRegStr HKCU "${REGKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${REGKEY}" "DisplayIcon" "$INSTDIR\${LAUNCH}"
  WriteRegStr HKCU "${REGKEY}" "Publisher" "sfcommander-app"
  WriteRegStr HKCU "${REGKEY}" "URLInfoAbout" "https://github.com/sfcommander-app/sf-commander"
  WriteRegStr HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${REGKEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${REGKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${REGKEY}" "EstimatedSize" $0
SectionEnd

Section "$(SEC_DESKTOP)" SecDesktop
  CreateShortcut "$DESKTOP\${LINK}" "$INSTDIR\${LAUNCH}" "" "$INSTDIR\${LAUNCH}" 0
SectionEnd

; Installing over an older copy: drop the releases older than this one.
; Newer ones stay - the updater may already have fetched one, and the
; launcher picks the newest anyway. A running exe cannot be deleted and is
; simply left for the app's own clean-up after its next update
Function RemoveOlderReleases
  FindFirst $0 $1 "$INSTDIR\SF-Commander-*.exe"
  next_file:
    StrCmp $1 "" done
    StrCpy $2 $1 "" 13
    StrCpy $2 $2 -4
    ${VersionCompare} $2 "${VERSION}" $3
    StrCmp $3 "2" 0 +2
      Delete "$INSTDIR\$1"
    FindNext $0 $1
    Goto next_file
  done:
  FindClose $0
FunctionEnd

Section "Uninstall"
  Delete "$INSTDIR\SF-Commander-*.exe"
  Delete "$INSTDIR\${LAUNCH}"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\${LINK}"
  Delete "$DESKTOP\${LINK}"
  DeleteRegKey HKCU "${REGKEY}"

  ; Wipe the data only from a folder that is ours by name - never from
  ; wherever $INSTDIR might have been typed to point at
  ${GetFileName} "$INSTDIR" $0
  StrCmp $0 "SF Commander" 0 keep_data
  MessageBox MB_YESNO|MB_ICONQUESTION "$(UN_DATA)" /SD IDNO IDNO keep_data
    RMDir /r "$INSTDIR"
    Goto done
  keep_data:
    RMDir "$INSTDIR"
  done:
SectionEnd
