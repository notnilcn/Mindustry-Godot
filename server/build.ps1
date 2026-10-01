# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for server/build.sh: resolves the repo inside WSL2 Ubuntu (where the
# spacetime CLI and cargo toolchain live) and runs the canonical bash script there, so
# there is a single implementation of the publish/generate/drift logic.
#
# Usage:
#   server/build.ps1 [-Db NAME] [-Server HOST] [-Check]
#
# Default db is `mindustry`; the module/crate stays `mindustry_godot`.

param(
    [string]$Db = "mindustry",
    [string]$Server = "local",
    [switch]$Check
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$WslRoot = (wsl -d Ubuntu -e wslpath -u $RepoRoot).Trim()
if ($LASTEXITCODE -ne 0 -or -not $WslRoot) {
    Write-Error "server/build.ps1: could not resolve $RepoRoot inside WSL Ubuntu"
    exit 1
}

$bashArgs = "--db '$Db' --server '$Server'"
if ($Check) { $bashArgs += " --check" }

# PS 5.1 surfaces native stderr (cargo/spacetime progress) as terminating errors when
# ErrorActionPreference is Stop; relax it, fold stderr into plain host output, and rely
# on the child's exit code.
$ErrorActionPreference = "Continue"
wsl -d Ubuntu -e bash -lc "cd '$WslRoot' && bash server/build.sh $bashArgs" 2>&1 |
    ForEach-Object { Write-Host $_ }
$code = $LASTEXITCODE
exit $code
