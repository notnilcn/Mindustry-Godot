# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for tools/sync_scenarios.sh: mirrors repo-root scenarios/ into
# client/scenarios/ (gitignored) for the in-engine scenario loader.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Src = Join-Path $RepoRoot "scenarios"
$Dst = Join-Path $RepoRoot "client\scenarios"

if (-not (Test-Path -LiteralPath $Src)) {
  Write-Error "sync_scenarios: missing $Src"
  exit 1
}

New-Item -ItemType Directory -Force -Path $Dst | Out-Null
Get-ChildItem -LiteralPath $Dst -Filter *.json -File -ErrorAction SilentlyContinue | Remove-Item -Force
$files = Get-ChildItem -LiteralPath $Src -Filter *.json -File
foreach ($file in $files) {
  Copy-Item -LiteralPath $file.FullName -Destination $Dst -Force
}
Write-Output "sync_scenarios: synced $($files.Count) file(s) -> client/scenarios/"
