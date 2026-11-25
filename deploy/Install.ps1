<#
  Password Alert - install on a server for every user.
  Run as administrator:  powershell -ExecutionPolicy Bypass -File Install.ps1
  Options:
    -InstallDir <folder>   where to put it (default: C:\Program Files\PasswordAlert)
    -NoAutoStart           don't start it at every user's sign-in
    -NoShortcut            don't add a Start menu shortcut
#>
param(
    [string]$InstallDir = (Join-Path $env:ProgramFiles 'PasswordAlert'),
    [switch]$NoAutoStart,
    [switch]$NoShortcut
)
$ErrorActionPreference = 'Stop'

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw 'Please run Install.ps1 as administrator.' }

$source = Join-Path $PSScriptRoot 'PasswordAlert.exe'
if (-not (Test-Path $source)) { throw "PasswordAlert.exe not found next to Install.ps1 ($PSScriptRoot)." }
$exe = Join-Path $InstallDir 'PasswordAlert.exe'

# Upgrading: close copies already running from the install folder (all sessions).
Get-Process -Name PasswordAlert -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ($_.Path -ieq $exe) } |
    Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $source $exe -Force
Copy-Item (Join-Path $PSScriptRoot 'README.txt') $InstallDir -Force -ErrorAction SilentlyContinue
Copy-Item (Join-Path $PSScriptRoot 'Uninstall.ps1') $InstallDir -Force -ErrorAction SilentlyContinue
Write-Host "Copied to $InstallDir"

$run = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run'
if ($NoAutoStart) {
    Remove-ItemProperty -Path $run -Name PasswordAlert -ErrorAction SilentlyContinue
} else {
    Set-ItemProperty -Path $run -Name PasswordAlert -Value "`"$exe`" --tray"
    Write-Host 'Starts in the tray at every user sign-in.'
}

if (-not $NoShortcut) {
    $lnk = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Password Alert.lnk'
    $sh = New-Object -ComObject WScript.Shell
    $s = $sh.CreateShortcut($lnk)
    $s.TargetPath = $exe
    $s.Arguments = '--show'
    $s.WorkingDirectory = $InstallDir
    $s.Description = 'Password Alert'
    $s.Save()
    Write-Host 'Added a Start menu shortcut.'
}

Write-Host 'Password Alert is installed. Users get it at their next sign-in (or start it from the Start menu).'
