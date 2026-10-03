# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# PowerShell twin of tools/export.sh (plan 22 M1/M7). Native Windows entrypoint:
# checks the Godot export templates under %APPDATA%\Godot\export_templates,
# runs a headless export, and optionally verifies the artifact. Outputs default
# under the gitignored client/bin/export/; override with -Out.

[CmdletBinding()]
param(
    [ValidateSet("windows", "linux", "macos", "android", "ios")]
    [string]$Platform = "windows",
    [ValidateSet("release", "debug")]
    [string]$Target = "release",
    [string]$Preset = "",
    [string]$Out = "",
    [switch]$Clean,
    [switch]$Verify
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$GodotBin = if ($env:GODOT_BIN) { $env:GODOT_BIN } else { (Get-Command godot4 -ErrorAction SilentlyContinue).Source }
if (-not $GodotBin) {
    Write-Error "export.ps1: godot4 not found (set GODOT_BIN)"
    exit 2
}

$ExportDir = Join-Path $RepoRoot "client/bin/export"
switch ($Platform) {
    "windows" { if (-not $Preset) { $Preset = "Windows Desktop" }; if (-not $Out) { $Out = Join-Path $ExportDir "Mindustry-Godot.exe" } }
    "linux"   { if (-not $Preset) { $Preset = "Linux/X11" };       if (-not $Out) { $Out = Join-Path $ExportDir "Mindustry-Godot.x86_64" } }
    "macos"   { if (-not $Preset) { $Preset = "macOS" };           if (-not $Out) { $Out = Join-Path $ExportDir "Mindustry-Godot.zip" } }
    "android" { if (-not $Preset) { $Preset = "Android" };         if (-not $Out) { $Out = Join-Path $ExportDir "Mindustry-Godot.apk" } }
    "ios"     { if (-not $Preset) { $Preset = "iOS" };             if (-not $Out) { $Out = Join-Path $ExportDir "Mindustry-Godot.ipa" } }
}

$VersionRaw = (& $GodotBin --version 2>&1 | Out-String).Trim()
$Version = ($VersionRaw -split '\.')[0..2] -join '.'
$TemplatesDir = if ($env:GODOT_TEMPLATES_DIR) { $env:GODOT_TEMPLATES_DIR } else { Join-Path $env:APPDATA "Godot/export_templates/$Version.stable" }
if (-not (Test-Path (Join-Path $TemplatesDir "version.txt"))) {
    Write-Error "export.ps1: Godot $Version export templates missing at $TemplatesDir`n  install Godot_v$Version-stable_export_templates.tpz under $TemplatesDir"
    exit 2
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
if ($Clean -and (Test-Path $Out)) { Remove-Item -LiteralPath $Out -Force }

Write-Host "export.ps1: preset='$Preset' target=$Target -> $Out"
& $GodotBin --headless --path (Join-Path $RepoRoot "client") "--export-$Target" $Preset $Out
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($Verify) {
    $deadline = (Get-Date).AddSeconds(15)
    while ((-not (Test-Path $Out)) -and ((Get-Date) -lt $deadline)) { Start-Sleep -Milliseconds 250 }
    if (-not (Test-Path $Out) -or (Get-Item -LiteralPath $Out).Length -eq 0) {
        Write-Error "export.ps1: export artifact missing or empty: $Out"
        exit 1
    }
    Write-Host "export.ps1: verified $Out ($((Get-Item -LiteralPath $Out).Length) bytes)"
}
