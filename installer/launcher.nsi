; S&F Commander - launcher ("SF Commander.exe")
;
; Starts the newest SF-Commander-<version>.exe in its own folder. The app's
; updater downloads the next release under a new name next to the running
; one, starts it and deletes the old file, so a shortcut pointing straight
; at the exe would point at nothing after the first update. The shortcuts
; point here instead.
;
;   makensis /DOUT=<file> /DICON=<icon.ico> launcher.nsi

; 64-bit when this NSIS has the amd64 stubs (Linux packages do), otherwise
; the standard 32-bit build of the official Windows NSIS - both run the same
!if /FileExists "${NSISDIR}/Stubs/lzma_solid-amd64-unicode"
  Target amd64-unicode
!else
  Unicode true
!endif
!include "WordFunc.nsh"

!ifndef OUT
  !define OUT "SF Commander.exe"
!endif

Name "S&F Commander"
OutFile "${OUT}"
Icon "${ICON}"
SilentInstall silent
RequestExecutionLevel user
SetCompressor /SOLID lzma

VIProductVersion "1.0.0.0"
VIAddVersionKey /LANG=1033 "ProductName" "S&F Commander"
VIAddVersionKey /LANG=1033 "FileDescription" "S&F Commander"
VIAddVersionKey /LANG=1033 "FileVersion" "1.0.0.0"
VIAddVersionKey /LANG=1033 "LegalCopyright" "MIT"

Section
  StrCpy $0 ""    ; newest file name
  StrCpy $1 "0"   ; its version
  FindFirst $2 $3 "$EXEDIR\SF-Commander-*.exe"
  next_file:
    StrCmp $3 "" scanned
    StrCpy $4 $3 "" 13   ; drop "SF-Commander-"
    StrCpy $4 $4 -4      ; drop ".exe"
    ${VersionCompare} $4 $1 $5
    StrCmp $5 "1" 0 +3
      StrCpy $1 $4
      StrCpy $0 $3
    FindNext $2 $3
    Goto next_file
  scanned:
  FindClose $2

  StrCmp $0 "" missing
  SetOutPath "$EXEDIR"
  Exec '"$EXEDIR\$0"'
  Quit

  missing:
  MessageBox MB_ICONSTOP "SF-Commander-*.exe not found in:$\n$EXEDIR$\n$\nReinstall S&F Commander."
SectionEnd
