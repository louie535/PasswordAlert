<#
  Password Alert - remove from a server. Run as administrator:
    powershell -ExecutionPolicy Bypass -File Uninstall.ps1
  Users' own settings (%APPDATA%\PasswordAlert) and per-user start-up entries are left alone;
  the app removes a per-user entry itself when "Start with Windows" is unticked.
#>
param([string]$InstallDir = (Join-Path $env:ProgramFiles 'PasswordAlert'))
$ErrorActionPreference = 'Stop'

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw 'Please run Uninstall.ps1 as administrator.' }

$exe = Join-Path $InstallDir 'PasswordAlert.exe'
Get-Process -Name PasswordAlert -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ($_.Path -ieq $exe) } |
    Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

Remove-ItemProperty -Path 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run' -Name PasswordAlert -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Password Alert.lnk') -Force -ErrorAction SilentlyContinue

# This script may live inside the folder it removes; delete the folder from a copy-free command.
Set-Location $env:TEMP
Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'Password Alert was removed.'
