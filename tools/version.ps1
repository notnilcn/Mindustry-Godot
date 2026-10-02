# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# PowerShell twin of tools/version.sh (plan 22 M0 §3.2). See that file for the
# upstream key set and precedence.

[CmdletBinding()]
param(
    [string]$BuildNumber = "-1",
    [string]$Revision = "0",
    [string]$Type = "unknown",
    [string]$Modifier = "unknown",
    [string]$CommitHash = "",
    [string]$BuildDate = "",
    [string]$Out = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrEmpty($Out)) {
    $Out = Join-Path $RepoRoot "client/assets/version.properties"
}
if (-not $CommitHash) {
    $CommitHash = (git -C $RepoRoot rev-parse --short HEAD 2>$null)
    if (-not $CommitHash) { $CommitHash = "unknown" }
}
if (-not $BuildDate) {
    $BuildDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd")
}

$build = $BuildNumber
if ($BuildNumber -match '^-?\d+$' -and [int]$BuildNumber -lt 0) {
    $build = "-1"
} elseif ($Revision -match '^\d+$' -and [int]$Revision -gt 0) {
    $build = "$BuildNumber.$Revision"
}

$dir = Split-Path -Parent $Out
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$lines = @(
    "type=$Type",
    "number=4",
    "modifier=$Modifier",
    "commitHash=$CommitHash",
    "buildDate=$BuildDate",
    "build=$build"
)
[System.IO.File]::WriteAllText($Out, ($lines -join "`n") + "`n")
Write-Host "version.ps1: wrote $Out"
Get-Content $Out
