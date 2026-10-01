# SPDX-License-Identifier: GPL-3.0-only
#
# Windows parity for tools/mcp-smoke.sh. The canonical smoke runs from WSL2
# Ubuntu (the dev host); this wrapper drives the same stdlib Python driver with
# the Windows python so it also works from a plain Windows terminal. The MCP
# server binary is resolved by tools/mcp_smoke.py (OPEN_GODOT_MCP_BIN overrides).
#
# The editor must already be running -- see the repo playtest skill:
#   wsl -d Ubuntu -e bash -lc "nohup godot4 --editor \
#     --path /mnt/c/Users/Clinton/g/code_examples/mindustry-godot/client \
#     >/tmp/mind-editor.log 2>&1 &"

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Driver = Join-Path $RepoRoot "tools\mcp_smoke.py"

$Python = Get-Command python -ErrorAction SilentlyContinue
if (-not $Python) {
  Write-Error "mcp-smoke.ps1: python not found on PATH"
  exit 1
}

& $Python.Source $Driver @args
exit $LASTEXITCODE
