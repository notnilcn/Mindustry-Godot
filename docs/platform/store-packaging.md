# Store packaging (plan 22 §3.8 — Android/iOS/desktop)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

**Status: preset-only this phase.** Export templates are absent on the WSL host
(R10), so no signed or unsigned store artifact is produced here. This document is
the runbook for when templates are installed.

## Godot export presets (`client/export_presets.cfg`)

Presets to commit: `Windows Desktop`, `Linux/X11`, `macOS`, `Android`, `iOS`.
Fields captured per preset: `name`, `platform`, `export_path`,
`application/{product_name, company_name, file_version, product_version}` (from
`BuildInfo`), `application/icon`, console wrapper, `application/modify_resources`,
`binary_format/embed_pck`, `codesign/*` (macOS), `keystore/*` (Android, empty),
`package/unique_name`, `screen/orientation`, permissions, `notarization/*`.

> M1 (Windows preset + `tools/export.sh`) is deferred because Godot **4.7.2 export
> templates are not installed** (`~/.local/share/godot/export_templates/` empty).
> `tools/export.sh` must fail with a download hint when they are missing.

## Android

- Package id default `com.mindustrygodot.game` (flag), min SDK 21 / target 36,
  `screen/immersive_mode=true`, `screen/support_small|normal|large|xlarge=true`,
  orientation `sensor_landscape`.
- Renderer inherits the project's `mobile` method (NUD-06=B); no per-preset override.
- Keystore fields stay **empty**; release signing is passed via `--keystore`
  args/env at build time and never committed.
- Data root is Godot `user://` (Android internal app files) with SAF import/export
  (P22-7). No `files_moved` migration.
- `tools/adb-smoke.sh` installs/launches/taps/screenshots when `adb` and a device
  are available (currently neither in WSL → documented NOT-RUN).

## iOS (cut — NUD-52=B)

Locked 2026-10-01: **iOS is cut from this phase entirely** — no preset, plugin
design, or store notes. Tracked as a platform deviation in `HIGH_LEVEL_PLAN.md`
§9. Revisit only if scope changes. (`share_file`/save-dialog would have required a
small objc plugin.)

## Desktop signing

- Windows: no installer in this phase; `File > Export` produces a portable
  `.exe` + `.pck`. Code signing is a release-owner step.
- macOS: preset carries empty `codesign/*` and `notarization/*`; building and
  signing require a macOS host. Local unsigned runs only.
- Linux: AppImage/flatpak notes are deferred to the release owner; the exported
  ELF + PCK is the artifact.

## Version metadata

`tools/version.sh` writes `client/assets/version.properties` (LF, gitignored) from
git/env. `application/file_version`/`product_version` are derived from
`BuildInfo::build_string()`; the same values drive the in-game
`[Mindustry] Version:` startup line.
