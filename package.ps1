# Builds the release program and makes dist\PasswordAlert-<version>.zip for deploying to servers.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed' }

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$stage = Join-Path $PSScriptRoot 'dist\PasswordAlert'
$zip = Join-Path $PSScriptRoot "dist\PasswordAlert-$version.zip"

if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
if (Test-Path $zip) { Remove-Item $zip -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item 'target\release\password-alert.exe' (Join-Path $stage 'PasswordAlert.exe')
Copy-Item 'deploy\*' $stage

Compress-Archive -Path $stage -DestinationPath $zip
Get-Item $zip | Select-Object FullName, Length
