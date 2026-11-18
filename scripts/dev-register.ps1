# Development only: registers the virtual camera component from a build folder.
# Needs administrator rights (writes under HKLM). The installer does the same
# for real installs; see scripts/install.ps1.
param([string]$Dll = 'C:\ProgramData\anyzoom-dev\anyzoom_vcam.dll')

if (-not (Test-Path $Dll)) { throw "DLL not found: $Dll" }
$key = 'HKLM:\SOFTWARE\Classes\CLSID\{3E3E3E3E-A11C-4A00-9D00-616E797A6F01}'
New-Item -Path "$key\InprocServer32" -Force | Out-Null
Set-ItemProperty -Path $key -Name '(default)' -Value 'anyzoom virtual camera'
Set-ItemProperty -Path "$key\InprocServer32" -Name '(default)' -Value $Dll
Set-ItemProperty -Path "$key\InprocServer32" -Name 'ThreadingModel' -Value 'Both'
'registered' | Out-File (Join-Path (Split-Path $Dll) 'register.done')
