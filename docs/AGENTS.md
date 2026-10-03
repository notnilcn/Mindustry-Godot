# AGENTS.md — docs/ (platform & packaging notes)

`docs/` holds operator-facing notes that sit alongside the code they describe. It contains only
`docs/platform/`: prose documents covering how the port integrates with external platforms and how
release artifacts are packaged. These are reference and runbook material for release owners and
platform work, not API docs. Read the root [`AGENTS.md`](../AGENTS.md) first.

## Layout

| Path | Responsibility |
|---|---|
| `docs/platform/discord.md` | Discord Rich Presence: app id, opt-out env var, per-state `state`/`details`/image keys, and the compile-gated `discord` feature in `mind-gdext`. |
| `docs/platform/mobile.md` | Android/iOS platform policy: `PlatformKind`/`PerformanceTier`/`PlatformCaps` in `mind_core::platform::caps`, mobile UI/effects defaults, and the Godot pause/orientation/safe-area glue. |
| `docs/platform/steam-parity.md` | The Steam surface (`SVars`, `SNet`, `SUser`, `SStats`, `SWorkshop`), what the port exposes (`GameService`/`NullService`, `workshop_*` stubs), and the full-parity checklist. |
| `docs/platform/store-packaging.md` | Store/export runbook: Godot export presets, Android package fields and signing, iOS status, desktop signing, and `tools/version.sh` metadata. |
| `docs/platform/web.md` | Why HTML5/Web is out of scope: no native file dialogs, unproven STDB/WASM transport and determinism, open decisions. |

## Responsibilities

- `docs/platform/` is the operator's entry point for external integrations and shipping; each file
  maps a platform concern to its implementation seam.
- Platform integrations meet the `mind-gdext::platform` module
  (`client/rust/mind-gdext/src/platform/`), which hosts the `MindPlatform` node and the `args`,
  `crash`, `desktop`, `dialogs`, `discord`, `service`, `update`, `uri` and `workshop` glue;
  `discord.md` and `steam-parity.md` document its Discord and Steam branches.
- Packaging notes reference the release tooling: `tools/pack.sh` (`tools/pack.ps1`) drives the
  offline `mind-tools` pipeline, and `tools/version.sh` writes the version metadata the export
  presets consume.
- Godot-free platform policy lives in `mind_core::platform::caps` and `mind_core::service`; the docs
  describe the contract, the Rust code owns it.

## Conventions

- Prose-only: these files contain no code and no generated output; they explain behavior owned
  elsewhere.
- Keep each document in sync with the paths and behavior it names; when `MindPlatform` or the
  tooling changes, update the matching note.
- Backtick every path, type and identifier, and cite the ported Mindustry source where behavior is
  ported (GPL-3.0).
- LF/UTF-8, one `#` heading per file with `##` sections.
