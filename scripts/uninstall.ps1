# Removes anyzoom from this PC: the program folder, the virtual camera
# registration, the Start Menu shortcut, the "Start with Windows" entry and the
# shared frame file. Your settings (%APPDATA%\anyzoom) are kept unless you pass
# -RemoveSettings.
param([switch]$RemoveSettings)
$ErrorActionPreference = 'Stop'

# Per-user items first, while we are still the normal user.
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$entry = (Get-ItemProperty -Path $run -ErrorAction SilentlyContinue).anyzoom
if ($entry -and $entry -like '*anyzoom.exe*') { Remove-ItemProperty -Path $run -Name anyzoom -ErrorAction SilentlyContinue }
if ($RemoveSettings) { Remove-Item (Join-Path $env:APPDATA 'anyzoom') -Recurse -Force -ErrorAction SilentlyContinue }

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    # the script may be running from the folder it is about to delete: use a copy
    $copy = Join-Path $env:TEMP 'anyzoom-uninstall.ps1'
    Copy-Item $PSCommandPath $copy -Force
    Start-Process powershell -Verb RunAs -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$copy`"")
    exit
}

try {
    Get-Process anyzoom -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Remove-Item 'HKLM:\SOFTWARE\Classes\CLSID\{3E3E3E3E-A11C-4A00-9D00-616E797A6F01}' -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\anyzoom.lnk') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $env:ProgramData 'anyzoom') -Recurse -Force -ErrorAction SilentlyContinue

    $target = Join-Path $env:ProgramFiles 'anyzoom'
    Remove-Item $target -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $target) {
        Write-Host "Windows still has the camera component loaded, so $target could not be fully removed." -ForegroundColor Yellow
        Write-Host 'It is no longer registered. Delete the folder after your next restart.' -ForegroundColor Yellow
    }
    Write-Host 'anyzoom is uninstalled.' -ForegroundColor Green
}
catch {
    Write-Host "Uninstall failed: $_" -ForegroundColor Red
}
Read-Host 'Press Enter to close'
