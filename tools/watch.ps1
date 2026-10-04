# SPDX-License-Identifier: GPL-3.0-only
#
# Windows twin for tools/watch.sh: rebuilds the Rust GDExtension whenever the
# Rust workspace under client/rust/ changes. Uses System.IO.FileSystemWatcher.
# Build output (client/rust/target, client/bin) is ignored so the loop never
# retriggers itself.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$WatchDir = Join-Path $RepoRoot "client\rust"
$BuildScript = Join-Path $RepoRoot "tools\build.ps1"

$action = {
  $path = $Event.SourceEventArgs.FullPath
  if ($path -match '\\target\\' -or $path -match '\\bin\\') { return }
  if ($path -notmatch '\.(rs|toml)$') { return }
  Write-Host "[watch] change detected: $path"
  & $Event.MessageData.Build
}

$messageData = @{ Build = $BuildScript }

$watcher = New-Object System.IO.FileSystemWatcher
$watcher.Path = $WatchDir
$watcher.IncludeSubdirectories = $true
$watcher.Filter = "*.*"
$watcher.NotifyFilter = [System.IO.NotifyFilters]::LastWrite -bor `
  [System.IO.NotifyFilters]::FileName -bor `
  [System.IO.NotifyFilters]::CreationTime

foreach ($eventName in "Changed", "Created", "Deleted", "Renamed") {
  Register-ObjectEvent -InputObject $watcher -EventName $eventName `
    -Action $action -MessageData $messageData | Out-Null
}

$watcher.EnableRaisingEvents = $true
Write-Host "[watch] watching $WatchDir (Ctrl+C to stop)"

try {
  while ($true) { Start-Sleep -Seconds 2 }
} finally {
  $watcher.EnableRaisingEvents = $false
  $watcher.Dispose()
}
