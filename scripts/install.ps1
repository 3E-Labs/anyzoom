# Installs anyzoom for this PC.
#   - copies anyzoom.exe and anyzoom_vcam.dll to C:\Program Files\anyzoom
#   - registers the virtual camera component (this is why it asks for admin)
#   - adds a Start Menu shortcut
# Run it from the unzipped folder: right-click > "Run with PowerShell", or
# double-click install.cmd. Undo everything with uninstall.ps1.
$ErrorActionPreference = 'Stop'

$build = [Environment]::OSVersion.Version.Build
if ($build -lt 22000) {
    Write-Host "anyzoom needs Windows 11. This PC runs build $build." -ForegroundColor Red
    Read-Host 'Press Enter to close'; exit 1
}

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Start-Process powershell -Verb RunAs -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"")
    exit
}

try {
    $source = Split-Path -Parent $PSCommandPath
    $target = Join-Path $env:ProgramFiles 'anyzoom'
    $clsid  = '{3E3E3E3E-A11C-4A00-9D00-616E797A6F01}'
    foreach ($file in 'anyzoom.exe', 'anyzoom_vcam.dll') {
        if (-not (Test-Path (Join-Path $source $file))) { throw "$file is missing next to install.ps1" }
    }

    Get-Process anyzoom -ErrorAction SilentlyContinue | Stop-Process -Force
    New-Item -ItemType Directory -Path $target -Force | Out-Null

    # Windows may still have the old camera component loaded; a loaded file can
    # be renamed but not overwritten, so move it aside first.
    $dll = Join-Path $target 'anyzoom_vcam.dll'
    Get-ChildItem $target -Filter 'anyzoom_vcam.old-*.dll' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    if (Test-Path $dll) {
        try { Remove-Item $dll -Force } catch { Rename-Item $dll ("anyzoom_vcam.old-{0}.dll" -f (Get-Date -Format 'yyyyMMddHHmmss')) }
    }
    Copy-Item (Join-Path $source 'anyzoom_vcam.dll') $dll -Force
    Copy-Item (Join-Path $source 'anyzoom.exe') $target -Force
    foreach ($extra in 'uninstall.ps1', 'uninstall.cmd', 'README.md', 'LICENSE') {
        if (Test-Path (Join-Path $source $extra)) { Copy-Item (Join-Path $source $extra) $target -Force }
    }

    $key = "HKLM:\SOFTWARE\Classes\CLSID\$clsid"
    New-Item -Path "$key\InprocServer32" -Force | Out-Null
    Set-ItemProperty -Path $key -Name '(default)' -Value 'anyzoom virtual camera'
    Set-ItemProperty -Path "$key\InprocServer32" -Name '(default)' -Value $dll
    Set-ItemProperty -Path "$key\InprocServer32" -Name 'ThreadingModel' -Value 'Both'

    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut((Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\anyzoom.lnk'))
    $link.TargetPath = Join-Path $target 'anyzoom.exe'
    $link.WorkingDirectory = $target
    $link.Description = 'Zoom and pan for any webcam'
    $link.Save()

    Write-Host "anyzoom is installed. Open it from the Start Menu, then pick 'anyzoom Camera' in your call app." -ForegroundColor Green
}
catch {
    Write-Host "Install failed: $_" -ForegroundColor Red
}
Read-Host 'Press Enter to close'
