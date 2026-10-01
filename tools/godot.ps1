# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for tools/godot.sh: resolves $env:GODOT_BIN -> godot4 ->
# godot4-mono and appends --path <repo>\client unless one was passed.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ClientDir = Join-Path $RepoRoot "client"

function Resolve-Godot {
    if ($env:GODOT_BIN) {
        if (-not (Test-Path -LiteralPath $env:GODOT_BIN)) {
            throw "GODOT_BIN is set but not found: $env:GODOT_BIN"
        }
        return (Get-Item -LiteralPath $env:GODOT_BIN).FullName
    }
    foreach ($name in @("godot4", "godot4-mono")) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if ($cmd) { return $cmd.Source }
    }
    return $null
}

$godot = Resolve-Godot
if (-not $godot) {
    Write-Error "godot.ps1: no Godot binary found. Set GODOT_BIN, or install Godot 4.7.2 so 'godot4'/'godot4-mono' is on PATH."
    exit 1
}

if ($env:GODOT_VERBOSE -eq "1") { Write-Host "godot.ps1: using '$godot'" }

$hasPath = $false
foreach ($a in $args) {
    if ($a -eq "--path" -or $a -like "--path=*") { $hasPath = $true; break }
}

if ($hasPath) {
    & $godot @args
} else {
    & $godot --path $ClientDir @args
}
exit $LASTEXITCODE
