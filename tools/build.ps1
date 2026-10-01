# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for tools/build.sh: builds mind-gdext + mind-headless into
# client/bin/rust, then syncs scenarios if the sync script exists.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot

cargo build `
  --manifest-path (Join-Path $RepoRoot "client\rust\Cargo.toml") `
  -p mind-gdext -p mind-headless `
  --target-dir (Join-Path $RepoRoot "client\bin\rust") `
  @args
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$sync = Join-Path $RepoRoot "tools\sync_scenarios.ps1"
if (Test-Path -LiteralPath $sync) { & $sync }
