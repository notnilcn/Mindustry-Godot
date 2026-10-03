# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for tools/parity.sh. The harness runs in WSL2 Ubuntu (the
# project's primary dev host), so this wrapper locates the repo inside WSL and
# forwards every argument to the bash driver.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$WslRoot = (wsl -d Ubuntu -e wslpath -u $RepoRoot).Trim()
if ($LASTEXITCODE -ne 0 -or -not $WslRoot) {
  Write-Error "tools/parity.ps1: could not resolve $RepoRoot inside WSL Ubuntu"
  exit 1
}

$Forward = ($args | ForEach-Object { "'" + ($_ -replace "'", "'\''") + "'" }) -join " "
wsl -d Ubuntu -e bash -lc "cd '$WslRoot' && bash tools/parity.sh $Forward"
exit $LASTEXITCODE
