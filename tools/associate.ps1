# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# Plan 22 M5 §6.7: PowerShell twin of tools/associate.sh. Installs/removes the
# `.msav`/`.msch` file associations and the `mindustry://` / `mindustry-godot://`
# URL protocol under HKCU. Run with `-Action Print` to inspect the .reg body.

[CmdletBinding()]
param(
    [ValidateSet("Install", "Uninstall", "Print")]
    [string]$Action = "Print",
    [string]$Exe = "",
    [switch]$Windows
)

$ErrorActionPreference = "Stop"
$AppName = "Mindustry-Godot"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $Exe) { $Exe = Join-Path $RepoRoot "build/export/$AppName.exe" }
$Esc = $Exe.Replace('\', '\\')

$RegBody = @"
Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\Classes\.msav]
@="MindustryGodot.Save"

[HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Save\DefaultIcon]
@="\"$Esc\",0"

[HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Save\shell\open\command]
@="\"$Esc\" \"%1\""

[HKEY_CURRENT_USER\Software\Classes\.msch]
@="MindustryGodot.Schematic"

[HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Schematic\shell\open\command]
@="\"$Esc\" \"%1\""

[HKEY_CURRENT_USER\Software\Classes\mindustry]
@="URL:Mindustry-Godot Protocol"
"URL Protocol"=""

[HKEY_CURRENT_USER\Software\Classes\mindustry\shell\open\command]
@="\"$Esc\" \"%1\""

[HKEY_CURRENT_USER\Software\Classes\mindustry-godot]
@="URL:Mindustry-Godot Protocol"
"URL Protocol"=""

[HKEY_CURRENT_USER\Software\Classes\mindustry-godot\shell\open\command]
@="\"$Esc\" \"%1\""
"@

$UnregBody = @"
Windows Registry Editor Version 5.00

[-HKEY_CURRENT_USER\Software\Classes\.msav]
[-HKEY_CURRENT_USER\Software\Classes\.msch]
[-HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Save]
[-HKEY_CURRENT_USER\Software\Classes\MindustryGodot.Schematic]
[-HKEY_CURRENT_USER\Software\Classes\mindustry]
[-HKEY_CURRENT_USER\Software\Classes\mindustry-godot]
"@

switch ($Action) {
    "Print" { Write-Output $RegBody }
    "Install" {
        $tmp = New-TemporaryFile
        Set-Content -LiteralPath $tmp -Value $RegBody -Encoding ascii
        reg.exe import $tmp.FullName | Out-Null
        Remove-Item -LiteralPath $tmp -Force
        Write-Host "associate.ps1: registered HKCU .msav/.msch + mindustry(-godot)://"
    }
    "Uninstall" {
        $tmp = New-TemporaryFile
        Set-Content -LiteralPath $tmp -Value $UnregBody -Encoding ascii
        reg.exe import $tmp.FullName | Out-Null
        Remove-Item -LiteralPath $tmp -Force
        Write-Host "associate.ps1: removed HKCU associations"
    }
}
