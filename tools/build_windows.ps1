# Сборка Windows NSIS-инсталлятора Bubanya.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
npm install
npm run tauri build -- --bundles nsis
Write-Host ""
Write-Host "Готово: src-tauri\target\release\bundle\nsis\"
