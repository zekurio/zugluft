Unicode True
ManifestDPIAware True
RequestExecutionLevel admin

!include LogicLib.nsh
!include MUI2.nsh
!include WinMessages.nsh
!include x64.nsh

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef PAYLOAD_DIR
  !error "PAYLOAD_DIR is required"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE is required"
!endif
!ifndef ICON_FILE
  !error "ICON_FILE is required"
!endif

!define PRODUCT_NAME "zugluft"
!define PRODUCT_PUBLISHER "zekurio"
!define PRODUCT_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\zugluft"
!define PAWNIO_URL "https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe"

Name "${PRODUCT_NAME}"
OutFile "${OUT_FILE}"
InstallDir "$PROGRAMFILES64\zugluft"
InstallDirRegKey HKLM "${PRODUCT_UNINSTALL_KEY}" "InstallLocation"
Icon "${ICON_FILE}"
UninstallIcon "${ICON_FILE}"

BrandingText "zugluft ${VERSION}"
ShowInstDetails show
ShowUninstDetails show

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\zugluft.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start zugluft"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "zugluft requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function un.onInit
  SetRegView 64
FunctionEnd

Function IsPawnIOInstalled
  nsExec::ExecToStack '"$SYSDIR\sc.exe" query pawnio'
  Pop $0
  Pop $1
  ${If} $0 == 0
    Push "1"
  ${Else}
    Push "0"
  ${EndIf}
FunctionEnd

Function StopAndRemoveZugluftService
  DetailPrint "Stopping existing zugluft service..."
  nsExec::ExecToLog '"$SYSDIR\sc.exe" stop zugluft'
  Sleep 3000

  DetailPrint "Removing existing zugluft service registration..."
  nsExec::ExecToLog '"$SYSDIR\sc.exe" delete zugluft'
  Sleep 1000
FunctionEnd

Function InstallPawnIO
  DetailPrint "Checking PawnIO driver..."
  Call IsPawnIOInstalled
  Pop $0
  ${If} $0 == "1"
    DetailPrint "PawnIO driver is already installed."
    Return
  ${EndIf}

  DetailPrint "Installing PawnIO driver with winget..."
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -Command "if (Get-Command winget.exe -ErrorAction SilentlyContinue) { winget install --exact --id namazso.PawnIO --accept-package-agreements --accept-source-agreements } else { exit 127 }"'

  Call IsPawnIOInstalled
  Pop $0
  ${If} $0 == "1"
    Return
  ${EndIf}

  DetailPrint "Downloading official PawnIO installer..."
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -Command "$$ProgressPreference = 'SilentlyContinue'; $$url = '${PAWNIO_URL}'; $$path = Join-Path $$env:TEMP 'PawnIO_setup.exe'; Invoke-WebRequest -Uri $$url -OutFile $$path; Start-Process -FilePath $$path -Wait"'

  Call IsPawnIOInstalled
  Pop $0
  ${If} $0 != "1"
    MessageBox MB_ICONEXCLAMATION "PawnIO was not detected after running its installer. A reboot may be required before motherboard sensors work."
  ${EndIf}
FunctionEnd

Function AddInstallDirToPath
  DetailPrint "Adding zugluft to the system PATH..."
  System::Call 'Kernel32::SetEnvironmentVariable(t "ZUGLUFT_INSTALL_DIR", t "$INSTDIR")i'
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -Command "$$dir = ($$env:ZUGLUFT_INSTALL_DIR).TrimEnd('\'); $$path = [Environment]::GetEnvironmentVariable('Path', 'Machine'); $$parts = @($$path -split ';' | Where-Object { $$_ }); if (($$parts | ForEach-Object { $$_.TrimEnd('\') }) -notcontains $$dir) { [Environment]::SetEnvironmentVariable('Path', (($$parts + $$env:ZUGLUFT_INSTALL_DIR) -join ';'), 'Machine') }"'
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
FunctionEnd

Function un.RemoveInstallDirFromPath
  DetailPrint "Removing zugluft from the system PATH..."
  System::Call 'Kernel32::SetEnvironmentVariable(t "ZUGLUFT_INSTALL_DIR", t "$INSTDIR")i'
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -Command "$$dir = ($$env:ZUGLUFT_INSTALL_DIR).TrimEnd('\'); $$path = [Environment]::GetEnvironmentVariable('Path', 'Machine'); $$parts = @($$path -split ';' | Where-Object { $$_ -and $$_.TrimEnd('\') -ine $$dir }); [Environment]::SetEnvironmentVariable('Path', ($$parts -join ';'), 'Machine')"'
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
FunctionEnd

Section "zugluft" SEC_MAIN
  SectionIn RO
  SetShellVarContext all
  SetRegView 64

  Call StopAndRemoveZugluftService

  SetOutPath "$INSTDIR"
  File /oname=zugluft.exe "${PAYLOAD_DIR}\zugluft.exe"
  File /oname=zugluftctl.exe "${PAYLOAD_DIR}\zugluftctl.exe"
  File /oname=zugluft-service.exe "${PAYLOAD_DIR}\zugluft-service.exe"
  File /oname=zugluft-lhm-bridge.dll "${PAYLOAD_DIR}\zugluft-lhm-bridge.dll"
  File /oname=README.md "${PAYLOAD_DIR}\README.md"
  File /oname=THIRD-PARTY-NOTICES.txt "${PAYLOAD_DIR}\THIRD-PARTY-NOTICES.txt"

  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\zugluft"
  CreateShortcut "$SMPROGRAMS\zugluft\zugluft.lnk" "$INSTDIR\zugluft.exe"
  CreateShortcut "$SMPROGRAMS\zugluft\zugluftctl.lnk" "$INSTDIR\zugluftctl.exe"
  CreateShortcut "$SMPROGRAMS\zugluft\Uninstall zugluft.lnk" "$INSTDIR\uninstall.exe"

  Call AddInstallDirToPath

  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayName" "zugluft ${VERSION}"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\zugluft.exe"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD HKLM "${PRODUCT_UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${PRODUCT_UNINSTALL_KEY}" "NoRepair" 1

  Call InstallPawnIO

  DetailPrint "Installing and starting zugluft service..."
  nsExec::ExecToStack '"$INSTDIR\zugluft-service.exe" install'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "zugluft service installation failed. Setup cannot continue.$\r$\n$\r$\n$1"
    Abort
  ${EndIf}
SectionEnd

Section "Uninstall"
  SetShellVarContext all
  SetRegView 64

  DetailPrint "Removing zugluft service..."
  ${If} ${FileExists} "$INSTDIR\zugluft-service.exe"
    nsExec::ExecToLog '"$INSTDIR\zugluft-service.exe" uninstall'
  ${Else}
    nsExec::ExecToLog '"$SYSDIR\sc.exe" stop zugluft'
    Sleep 3000
    nsExec::ExecToLog '"$SYSDIR\sc.exe" delete zugluft'
  ${EndIf}

  Delete "$SMPROGRAMS\zugluft\zugluft.lnk"
  Delete "$SMPROGRAMS\zugluft\zugluftctl.lnk"
  Delete "$SMPROGRAMS\zugluft\Uninstall zugluft.lnk"
  RMDir "$SMPROGRAMS\zugluft"

  Call un.RemoveInstallDirFromPath

  Delete "$INSTDIR\zugluft.exe"
  Delete "$INSTDIR\zugluftctl.exe"
  Delete "$INSTDIR\zugluft-service.exe"
  Delete "$INSTDIR\zugluft-lhm-bridge.dll"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${PRODUCT_UNINSTALL_KEY}"

  DetailPrint "PawnIO was left installed because it is shared by other hardware tools."
SectionEnd
