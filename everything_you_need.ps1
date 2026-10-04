# SPDX-License-Identifier: GPL-3.0-only
#
# everything_you_need.ps1 - one-shot bootstrap for a fresh Mindustry-Godot checkout.
#
# Builds the Rust GDExtension (mind-gdext) + headless oracle (mind-headless) into
# client/bin/rust so res://mind.gdextension resolves, packs the runtime asset tree
# (assets/sprites, shaders, icons, sounds, locales) with the offline mind-tools
# pipeline, then mirrors the canonical scenario JSON into client/scenarios/. Run
# this before opening client/project.godot in the Windows Godot editor.
#
# Why not automatic in Godot: GDExtension libraries are loaded during project
# initialization, before any editor plugin or autoload runs, so there is no hook
# to build them on first open. Run this first, or wire it into a git
# post-checkout/post-merge hook.
#
# Usage:
#   ./everything_you_need.ps1 [-Release] [-NoPack] [-Stdb] [-Import]
#
#   -Release   build optimized (release) instead of debug
#   -NoPack    skip the offline asset pipeline; a fresh checkout needs it
#   -Stdb      also publish the SpacetimeDB module + regenerate the client
#              bindings (needs the `spacetime` CLI and a running local server;
#              wipes local dev data). Checked-in bindings normally suffice.
#   -Import    also run a headless Godot import/parse pass (needs godot4 on PATH)

param(
    [switch]$Release,
    [switch]$NoPack,
    [switch]$Stdb,
    [switch]$Import
)

$ErrorActionPreference = "Stop"

# The script lives at the repo root; tolerate being moved under tools/.
$RepoRoot = $PSScriptRoot
if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot "client\project.godot"))) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot "client\project.godot"))) {
    Write-Error "everything_you_need.ps1: could not locate client/project.godot from $PSScriptRoot"
    exit 1
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "everything_you_need.ps1: cargo not found. Install Rust (https://rustup.rs) and retry."
    exit 1
}

$cargoArgs = @(
    "build",
    "--manifest-path", (Join-Path $RepoRoot "client\rust\Cargo.toml"),
    "-p", "mind-gdext", "-p", "mind-headless",
    "--target-dir", (Join-Path $RepoRoot "client\bin\rust")
)
if ($Release) { $cargoArgs += "--release" }

Write-Host "== [1/4] build mind-gdext + mind-headless =="
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (-not $NoPack) {
    Write-Host "== [2/4] pack runtime assets -> assets/ =="
    & (Join-Path $RepoRoot "tools\pack.ps1") pack
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    Write-Host "== [2/4] asset pack skipped (-NoPack) =="
}

Write-Host "== [3/4] sync scenarios -> client/scenarios =="
& (Join-Path $RepoRoot "tools\sync_scenarios.ps1")

if ($Stdb) {
    Write-Host "== [4/4] publish SpacetimeDB module + regenerate bindings =="
    & (Join-Path $RepoRoot "server\build.ps1")
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    Write-Host "== [4/4] SpacetimeDB bindings already checked in (skip; pass -Stdb to regenerate) =="
}

if ($Import) {
    Write-Host "== import / parse check (headless Godot) =="
    & (Join-Path $RepoRoot "tools\godot.ps1") --headless --editor --quit
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host ""
Write-Host "done: shared library is in client/bin/rust/<profile>/."
Write-Host "      open client/project.godot, or run: tools/godot.ps1"
