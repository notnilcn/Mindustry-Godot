# SPDX-License-Identifier: GPL-3.0-only
#
# Runs the offline asset pipeline (plan 03). PowerShell twin of pack.sh.
# All arguments are forwarded to `mind-tools`.

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot

cargo run `
  --manifest-path "$RepoRoot/client/rust/Cargo.toml" `
  --release -p mind-tools -- @args
