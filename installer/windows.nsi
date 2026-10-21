Unicode true
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

  ; Coloca o `hyzevm` no PATH do usuário (vale em terminais novos).
  nsExec::ExecToLog 'powershell -NoProfile -Command "$$p=[Environment]::GetEnvironmentVariable(\"Path\",\"User\"); if(($$p -split \";\") -notcontains \"$INSTDIR\"){[Environment]::SetEnvironmentVariable(\"Path\",(($$p+\";$INSTDIR\").TrimStart(\";\")),\"User\")}"'
  Pop $0

  ; QEMU (o motor das máquinas): instala pelo winget se não existir. Pede permissão de administrador (UAC).
  IfFileExists "$PROGRAMFILES64\qemu\qemu-system-x86_64.exe" qemu_ok 0
    DetailPrint "Instalando o QEMU..."
    nsExec::ExecToLog 'winget install -e --id SoftwareFreedomConservancy.QEMU --silent --accept-source-agreements --accept-package-agreements'
    Pop $0
    IfFileExists "$PROGRAMFILES64\qemu\qemu-system-x86_64.exe" qemu_ok 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Não consegui instalar o QEMU sozinho. Instale em https://www.qemu.org/download/#windows e rode: hyzevm doctor" /SD IDOK
  qemu_ok:

  ; Aceleração do Windows (Plataforma do Hipervisor). Sem ela o QEMU não sobe.
  nsExec::ExecToStack 'powershell -NoProfile -Command "if((Get-CimInstance Win32_ComputerSystem).HypervisorPresent){exit 0}else{exit 1}"'
  Pop $0
  StrCmp $0 "0" hv_ok 0
    MessageBox MB_YESNO|MB_ICONQUESTION "Para as máquinas rodarem rápido o Windows precisa da 'Plataforma do Hipervisor'. Ativar agora? (pede permissão de administrador e depois é preciso reiniciar o computador)" /SD IDYES IDNO hv_ok
    nsExec::ExecToLog 'powershell -NoProfile -Command "Start-Process powershell -Verb RunAs -Wait -ArgumentList \"-NoProfile\",\"-Command\",\"Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All -NoRestart\""'
    Pop $0
    MessageBox MB_OK|MB_ICONINFORMATION "Pronto. REINICIE o computador para ativar a Plataforma do Hipervisor; depois abra o HyzeVM." /SD IDOK
  hv_ok:
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "DisplayIcon" "$INSTDIR\icon.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}" "UninstallString" "$INSTDIR\Uninstall.exe"
  Exec '"$INSTDIR\hyzevm-desktop.exe"'
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog 'powershell -NoProfile -Command "$$p=[Environment]::GetEnvironmentVariable(\"Path\",\"User\"); [Environment]::SetEnvironmentVariable(\"Path\",((($$p -split \";\") | Where-Object { $$_ -ne \"$INSTDIR\" }) -join \";\"),\"User\")"'
  Pop $0
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"
  RMDir /r "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}"
SectionEnd
