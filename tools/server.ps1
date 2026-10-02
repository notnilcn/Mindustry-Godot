# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# PowerShell twin of tools/server.sh (plan 22 M2). Runs the dedicated server or
# drives a running server's console socket.

[CmdletBinding()]
param(
    [string]$Socket = "",
    [string]$Host_ = "127.0.0.1",
    [int]$Port = 6859,
    [string]$ConfigDir = "config",
    [switch]$Serve,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ServerArgs
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Bin = if ($env:MIND_HEADLESS_BIN) { $env:MIND_HEADLESS_BIN } else { Join-Path $RepoRoot "client/rust/target/debug/mind-headless.exe" }
if (-not (Test-Path $Bin)) {
    $Bin = Join-Path $RepoRoot "client/rust/target/debug/mind-headless"
}
if (-not (Test-Path $Bin)) {
    Write-Host "server.ps1: building mind-headless..." -ForegroundColor Yellow
    cargo build --manifest-path (Join-Path $RepoRoot "client/rust/Cargo.toml") -p mind-headless
}

if (-not $Socket) {
    & $Bin server @ServerArgs
    exit $LASTEXITCODE
}

if ($Serve) {
    $proc = Start-Process -FilePath $Bin -ArgumentList @("server", "--config-dir", $ConfigDir, "--socket-port", $Port) -PassThru
}

try {
    $client = [System.Net.Sockets.TcpClient]::new($Host_, $Port)
    $stream = $client.GetStream()
    $writer = [System.IO.StreamWriter]::new($stream)
    $reader = [System.IO.StreamReader]::new($stream)
    foreach ($cmd in $Socket.Split(",")) {
        $writer.WriteLine($cmd)
    }
    $writer.Flush()
    while (-not $reader.EndOfStream) {
        $line = $reader.ReadLine()
        if ($null -ne $line) { Write-Output $line }
    }
    $client.Close()
} finally {
    if ($Serve -and $proc) { Stop-Process -Id $proc.Id -ErrorAction SilentlyContinue }
}
