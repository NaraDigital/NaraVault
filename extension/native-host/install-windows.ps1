# Registers the NaraVault native-messaging host for a Chromium browser (current
# user only — no admin needed). Run from PowerShell:
#
#   .\install-windows.ps1 -ExtensionId <your-extension-id> [-Browser chrome|edge|brave]
#
# The ExtensionId is shown on chrome://extensions (Developer mode) after you load
# the unpacked `extension` folder.

param(
  [Parameter(Mandatory = $true)][string]$ExtensionId,
  [ValidateSet("chrome", "edge", "brave")][string]$Browser = "chrome",
  [string]$BinaryPath
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not $BinaryPath) {
  $candidates = @(
    (Join-Path $here "..\..\src-tauri\target\release\naravault-host.exe"),
    (Join-Path $here "..\..\src-tauri\target\debug\naravault-host.exe")
  )
  $BinaryPath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $BinaryPath -or -not (Test-Path $BinaryPath)) {
  throw "naravault-host.exe not found. Build it first (cargo build --release in src-tauri) or pass -BinaryPath."
}
$BinaryPath = (Resolve-Path $BinaryPath).Path

# Write the host manifest to a stable per-user location.
$manifest = [ordered]@{
  name            = "com.naravault.host"
  description     = "NaraVault native messaging host"
  path            = $BinaryPath
  type            = "stdio"
  allowed_origins = @("chrome-extension://$ExtensionId/")
}
$targetDir = Join-Path $env:APPDATA "NaraVault"
New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
$manifestPath = Join-Path $targetDir "com.naravault.host.json"
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path $manifestPath -Encoding UTF8

# Point the browser at the manifest via a per-user registry key.
$regBase = switch ($Browser) {
  "edge"  { "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts" }
  "brave" { "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts" }
  default { "HKCU:\Software\Google\Chrome\NativeMessagingHosts" }
}
$key = Join-Path $regBase "com.naravault.host"
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name "(Default)" -Value $manifestPath

Write-Host "Installed NaraVault host for $Browser." -ForegroundColor Green
Write-Host "  host binary : $BinaryPath"
Write-Host "  manifest    : $manifestPath"
Write-Host "  extension   : $ExtensionId"
Write-Host "Restart the browser, then reload the extension."
