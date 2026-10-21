; Instalador do HyzeVM para Windows (por usuário, sem precisar de administrador).
!define NAME "HyzeVM"
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
Name "${NAME}"
OutFile "..\HyzeVM-windows-x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\${NAME}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "..\app\icons\icon.ico"
UninstallIcon "..\app\icons\icon.ico"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  File "..\dist-win\hyzevm-desktop.exe"
  File "..\dist-win\hyzevm.exe"
  File "..\app\icons\icon.ico"
  CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\hyzevm-desktop.exe" "" "$INSTDIR\icon.ico"
  CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\hyzevm-desktop.exe" "" "$INSTDIR\icon.ico"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "DisplayIcon" "$INSTDIR\icon.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "UninstallString" "$INSTDIR\Uninstall.exe"
  Exec '"$INSTDIR\hyzevm-desktop.exe"'
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"
  RMDir /r "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}"
SectionEnd
