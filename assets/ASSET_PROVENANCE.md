# Asset provenance (plan 03 M0)

This tree is vendored from the upstream Mindustry checkout by
`mind-tools migrate` (NUD-11; see `03_ASSETS_IMPLEMENTATION_PLAN.md`).
Mindustry is GPL-3.0 (<https://github.com/Anuken/Mindustry>); the port
keeps region names, bundle keys and file paths byte-identical.

- Source: `../Mindustry` (`core/assets` -> `assets/`, `core/assets-raw` -> `assets-raw/`)
- Upstream commit: `2cd7aeecf1378b3db456be9bfde8691b3cdc1bcc` (2026-10-01)
- Migrated file count: 2977
- Input tree sha256: `cc949a8cae58eac80aa2f9b611c00cbb0fd825f329fd21a403c6154d9648c265`
- Manifest: `build/assets/migration_manifest.json` (regenerate with
`mind-tools migrate --from $MIND_UPSTREAM --to .`; verify with
`mind-headless assets migrate-check`)

Trimmed (never vendored; see `mind-atlas/src/migrate.rs`): `logicids.dat`,
`version.properties`, `locales`, `basepartnames`, `sprites.aatls`, generated
`sprites/sprites*.png` pages, `sprites/fallback/`, `sprites/block_colors.png`,
`bundles/output/`, `cache/`, `sprites_out/`, upstream `AGENTS.md` files.
Generated atlas pages/manifests under `assets/sprites/` are produced by
`mind-tools pack` and are gitignored.
