# AGENTS.md — Mindustry-Godot

Godot 4.7 client over the Rust simulation in `client/rust/`, exported as the `mind-gdext` GDExtension: a 1:1 parity port of [Mindustry](https://github.com/Anuken/Mindustry). Rust owns behavior; GDScript owns scenes and presentation.

## Top-level map

| Path | Responsibility |
|---|---|
| `client/` | Godot project: `project.godot`, scenes, GDScript UI, shaders, vendored addons. |
| `client/rust/` | Cargo workspace: `mind-core`, `mind-gdext`, `mind-headless`, `mind-stdb`, `mind-tools`, `mind-atlas` and the proc-macro crates. |
| `assets/`, `assets-raw/` | Packed runtime assets vs authored art (fonts, icons, source sprites). |
| `scenarios/` | Canonical scenario JSON; mirrored to `client/scenarios/`. |
| `parity/` | Oracle registries, sha256 goldens, screenshots, audit ledgers and gate reports. |
| `server/` | SpacetimeDB `mindustry_godot` module and publish/bindings tooling. |
| `tools/` | Build, pack, verify, launch and parity scripts. |
| `docs/` | Implementation plans, round summaries and platform handoffs. |
| `bench/` | Recorded performance baselines behind `parity/bench_budgets.json`. |
| `.github/` | CI workflows: `ci.yml`, `parity-nightly.yml`. |

## Generated / transient — do not hand-edit

- `client/bin/`, `client/.godot/`, `client/rust/target/`, `server/spacetimedb/target/`, `build/` — build and import caches (`tools/build.sh`, `bash tools/pack.sh`); `client/scenarios/` — mirror of `scenarios/` (`bash tools/sync_scenarios.sh`).
- `assets/sprites/sprites*.png`, `sprites.atlas.json`, `fallback/`, `block_colors.png`, `assets/icons/icon_codes.json`, `assets/icons/icons.properties`, `assets/sounds.index.json`, `assets/locales/`, `assets/shaders/godot/`, `assets/shaders/shader.index.json`, `assets/parity/region_names.txt` — atlas/index outputs (`bash tools/pack.sh pack`); `assets/ASSET_PROVENANCE.md` — `mind-tools migrate` output; `client/rust/mind-stdb/src/module_bindings/` — `spacetime generate` output (`bash server/build.sh`).
- `runs/`, `parity-java-home/`, `screenshots_for_ai/`, `client/assets/version.properties`, `.opencode/` session state, `.claude/worktrees/` — transient outputs and session state; the worktrees are stale duplicates, ignore them.

## Global invariants

- Every source file carries `SPDX-License-Identifier: GPL-3.0-only` and cites the upstream Mindustry file it ports; UTF-8/LF only.
- Toolchain: Rust `1.98.1` (`rust-toolchain.toml`), Godot `4.7.x` (`4.7.2` recommended).
- Rust owns behavior; GDScript is UI-only and never mutates simulation state. `mind-core` stays Godot-free and tokio-free; hosts call in, never the reverse.
- ABIs are append-only: content IDs, entity field order, UI manifests, STDB schema and bindings. Never hand-edit generated outputs.

## Quickstart (`tools/AGENTS.md` covers every script and gate)

```bash
./everything_you_need.sh    # first checkout: Rust build + asset pack + scenario sync
tools/build.sh              # rebuild mind-gdext + mind-headless, sync scenarios
bash tools/pack.sh pack     # regenerate the runtime asset tree under assets/
bash tools/godot.sh         # launch the client (-e opens the editor)
bash tools/ci.sh            # full local gate
```

## Guide index

| Guide | Scope |
|---|---|
| [`client/AGENTS.md`](client/AGENTS.md) | Scene graph, GDScript UI, dialog/widget conventions, manifest ABI. |
| [`client/rust/AGENTS.md`](client/rust/AGENTS.md) | Workspace layout, crate dependency direction, lints, toolchain. |
| [`client/rust/mind-core/AGENTS.md`](client/rust/mind-core/AGENTS.md) | Simulation core: modules, ECS/schedule, determinism, content IDs. |
| `client/rust/mind-core/src/` — simulation/data | [`sim`](client/rust/mind-core/src/sim/AGENTS.md), [`ai`](client/rust/mind-core/src/ai/AGENTS.md), [`combat`](client/rust/mind-core/src/combat/AGENTS.md), [`content`](client/rust/mind-core/src/content/AGENTS.md), [`maps`](client/rust/mind-core/src/maps/AGENTS.md), [`mods`](client/rust/mind-core/src/mods/AGENTS.md). |
| `client/rust/mind-core/src/` — world/game/io | [`world`](client/rust/mind-core/src/world/AGENTS.md), [`world/blocks`](client/rust/mind-core/src/world/blocks/AGENTS.md), [`game`](client/rust/mind-core/src/game/AGENTS.md), [`io`](client/rust/mind-core/src/io/AGENTS.md). |
| `client/rust/mind-core/src/` — view/control | [`fx`](client/rust/mind-core/src/fx/AGENTS.md), [`render`](client/rust/mind-core/src/render/AGENTS.md), [`ui`](client/rust/mind-core/src/ui/AGENTS.md), [`input`](client/rust/mind-core/src/input/AGENTS.md), [`logic`](client/rust/mind-core/src/logic/AGENTS.md). |
| [`client/rust/mind-gdext/AGENTS.md`](client/rust/mind-gdext/AGENTS.md) | `Mind*` classes, frame pump, view/input/audio/net/platform glue. |
| [`client/rust/mind-headless/AGENTS.md`](client/rust/mind-headless/AGENTS.md) | Scenarios, goldens, benches, parity harness, dedicated server modes. |
| [`client/rust/mind-stdb/AGENTS.md`](client/rust/mind-stdb/AGENTS.md) | Connector lifecycle, subscription waves, table binders, command relay. |
| [`client/rust/mind-tools/AGENTS.md`](client/rust/mind-tools/AGENTS.md) | Offline asset pipeline: migrate, pack, icons, sounds, shaders, mods classmap. |
| [`client/addons/open_godot_mcp/AGENTS.md`](client/addons/open_godot_mcp/AGENTS.md) | Editor plugin and runtime autoload for MCP inspection and playtesting. |
| [`parity/AGENTS.md`](parity/AGENTS.md) | Registries, goldens, screenshots, ledgers and `parity` harness rules. |
| [`server/AGENTS.md`](server/AGENTS.md) | Module map, publish/bindings workflow, protocol sync. |
| [`server/spacetimedb/AGENTS.md`](server/spacetimedb/AGENTS.md) | Tables, reducers, views, relay/chat/admin/campaign schema ABI. |
| [`tools/AGENTS.md`](tools/AGENTS.md) | Script reference, environment overrides, CI gate contents, export layout. |

## Routing hints

- Gameplay rule, sim state or determinism: `client/rust/mind-core/AGENTS.md` and the module guide under `src/`.
- Godot scene, dialog, widget or theme: `client/AGENTS.md`.
- Networking: `server/spacetimedb/AGENTS.md` for schema/reducers, `client/rust/mind-stdb/AGENTS.md` for the client.
- Asset pipeline, atlas or icons: `client/rust/mind-tools/AGENTS.md`.
- Parity evidence: `parity/AGENTS.md` and `TWIN-VERIFICATION.json` (minimal items: `title`/`status`/`similarity`/`fork`/`action`/`images`, plus paired recipes and gaps); in-engine checks use the `playtest` skill, Java↔Godot walks use `parity-eval`.
