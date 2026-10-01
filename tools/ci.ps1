# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for tools/ci.sh. The canonical checks run in WSL2 Ubuntu (the
# project's primary dev host), so this script locates the repo inside WSL and
# executes the bash CI there. M7 extends tools/ci.sh, not this wrapper.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$WslRoot = (wsl -d Ubuntu -e wslpath -u $RepoRoot).Trim()
if ($LASTEXITCODE -ne 0 -or -not $WslRoot) {
  Write-Error "tools/ci.ps1: could not resolve $RepoRoot inside WSL Ubuntu"
  exit 1
}

wsl -d Ubuntu -e bash -lc "cd '$WslRoot' && bash tools/ci.sh"
exit $LASTEXITCODE
