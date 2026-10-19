# AGENTS.md — mind-core (Godot-free simulation core)

`mind-core` is the Godot-free, tokio-free simulation core of Mindustry-Godot: every game rule —
simulation, content, world/terrain, IO, campaign, units/combat/AI and logic (mlog) — plus the
device-independent halves of assets, audio, FX, UI and input. It links only plain libraries and the
`mind-derive`/`mind-macros` proc-macro crates, stays runnable under a bare `cargo test -p mind-core`,
and is consumed by the rest of the workspace. Read the root [`AGENTS.md`](../../../AGENTS.md) first.
The workspace map is [`client/rust/AGENTS.md`](../AGENTS.md).

## Layout

| Module | Responsibility |
|---|---|
| `ai` | Unit AI and pathfinding: 18 `AiKind` controllers (including `Player`), `Astar`/`ControlPathfinder`, RTS squads, `WaveSpawner`, `UnitHarness`. |
| `assets` | Godot-free asset runtime: `FileTree` virtual FS, atlas index, bundles, icons and sound/music registries. |
| `async_work` | `AsyncCore` deterministic physics/avoidance workers (`std::thread`, shard-indexed, joined in order). |
| `audio` | `SoundControl`/priority/music semantics and the `AudioSink` event boundary; playback lives in `mind-gdext`. |
| `combat` | Bullets, damage, fires, lightning, puddles, targeting and the view-only `FxSink` seam. |
| `command` | The `Command` intent enum and tick-stamped `CommandRecord` log helpers. |
| `config` | Minimal host config: `TILESIZE`, `MAX_BLOCK_SIZE`, `MindConfig`, `default_data_dir`. |
| `constants` | Simulation constants: tick rate, fixed Hz, `CHECKSUM_VERSION`, world bounds. |
| `content` | Content framework: `ContentType`, dense IDs, `ContentRegistry` and the vanilla registries/tech tree. |
| `crash` | Crash-report builder (`CrashHandler` port) over caller-injected host facts. |
| `determinism` | `SimRng` streams, `CommandLog`/`SimCommand`, and the canonical FNV-1a-64 `Checksum`. |
| `ecs` | `MindWorld` bevy wrapper, `BuildingComp`, `EntitySeq`/`EntitySequencer`, `TeamId`. |
| `editor` | Map-editor model: `EditorGrid`, tile ops, tools, palette, preview and playtest. |
| `entities` | Entity framework: `SimComponent` metadata, groups, lifecycle, pools and mapping. |
| `event` | Deterministic, registration-ordered event bus (`define_events!`). |
| `fixtures` | Reusable deterministic simulation fixtures (`fixtures::power`). |
| `fx` | Effect registry, pools, trails, decals, shake and draw-part program resolution (view-only). |
| `game` | `State`/`GameState` runtime model plus campaign, universe, rules, waves and schematics. |
| `input` | Pure input state machine, placement algorithms, plan mirror and RTS selection. |
| `io` | Persistence: `MGRS` saves, `TypeIO`, entity revision IO, `JsonIO`, settings, map headers/previews; the map registry and generation live in `maps`. |
| `log` | Logging facade (`arc.util.Log` port) with console + `last_log.txt` output. |
| `logic` | The mlog system: parser, assembler, `Executor` VM, statements, values and globals. |
| `maps` | Map registry (`Maps`), generators, previews, shuffle and planet/sector map glue. |
| `math` | Noise, interpolation, `WindowedMean` and `ArcRand` helpers. |
| `mods` | Mod manager: discovery, states, dependency ordering, import/remove, patching and assets. |
| `platform` | `Platform` host seam, `HeadlessPlatform`, launch args and client hooks. |
| `random` | Exact `java.util.Random` (`JavaRandom`) plus `Rand`. |
| `render` | Godot-free render data/scan: layers, caches, `RenderQueue`, bands and visible-set scans. |
| `scenario` | Scenario file schema and command-log JSONL. |
| `schedule` | Reserved `SimSet` slot order and `build_p0_schedule`. |
| `service` | Achievement/stat `GameService` seam and the no-op `NullService`. |
| `sim` | The spine: `Sim`, `SimBuilder`, `FixedStepRunner`, `SimClock`, `IoQueue`, the opt-in plan-07 live block runtime (`runtime`) and state dump. |
| `time` | Arc `Time` parity: delta provider and delayed-run queue. |
| `ui` | Godot-free UI data/format/model logic. |
| `util` | Scratch/pooling utilities: `VecPool`, `TempVec`, `IdSet`, string helpers. |
| `version` | Crate/format version constants and the `BuildInfo` build report. |
| `weapons` | Weapon behavior: shoot patterns, `WeaponMount`, per-weapon-kind engines. |
| `world` | World/terrain: `WorldGrid`, tiles, tile ops, building IO, generation and block families. |

## Responsibilities

- `mind-core` owns every game rule. Hosts hold none: `mind-gdext` (the Godot `cdylib`) owns the
  fixed-step pump, view drivers, input translation, render/audio/FX execution and net/platform glue.
- `mind-headless` is the Godot-free oracle and dedicated server: scenario registry, goldens and
  checksums, benches, `parity` verification and the `server`/`serve` modes.
- `mind-stdb` is the SpacetimeDB client facade (connector, subscription waves, typed table binders,
  ordered command relay); `mind-core` never depends on it.
- **View-only vs sim state.** `render`, `fx`, `audio`, `combat::view` and the `ui`/`input` data
  layers compute view data or emit events, never feed simulation, and are excluded from saves, sync
  and the checksum.

## ECS, schedule and determinism

- `bevy_ecs` is used strictly as a library (pinned `=0.19.1`; no `bevy_app`/render stack): `MindWorld`
  wraps `bevy_ecs::World`, components derive `Component` and sim state is inserted as `Resource`s
  (e.g. `audio::AudioSinkRes`).
- `schedule::build_p0_schedule` builds a single-threaded `Schedule` chaining the reserved `SimSet`
  slots; `sim::schedule` defines the `TickSet`/`EntitySet` order, systems register into named
  `SystemSet`s and are never re-ordered. Determinism comes from stable iteration (`EntitySeq`,
  `Tiles::each` x-outer vs `Tiles::iter` row-major), not archetype order.
- Fixed 60 Hz: `constants::TICKS_PER_SECOND`/`FIXED_HZ`, `sim::FixedStepRunner` (`SIM_STEP`, catch-up
  cap) and `sim::SimClock`. All randomness flows through `determinism::{SimRng, RngStream}` or
  `random::JavaRandom`; no wall clock or OS access in sim.
- The canonical checksum is the versioned `determinism::Checksummer` (FNV-1a-64, `CHECKSUM_VERSION`);
  `sim::ChecksumScope` masks possessed/view fields without changing field walk order.

## Key types

- Simulation: `Sim`, `SimBuilder`, `TickReport`, `SimConfig`, `FixedStepRunner`, `SimClock`,
  `IoQueue`, `ChecksumScope` (`sim`).
- ECS: `MindWorld`, `BuildingComp`, `EntitySeq`, `EntitySequencer`, `TeamId` (`ecs`); `Groups`,
  `EntityPools`, `SimEntity` (`entities`).
- Determinism/command: `Checksum`, `Checksummer`, `CommandLog`, `SimCommand` (`determinism`);
  `Command`, `CommandRecord` (`command`).
- Content/world: `ContentRegistry`, `create_base_content`, `ContentType`, `BlockId`, `MemoryBundle`
  (`content`); `WorldGrid`, `Tile`, `WorldCtx`, `BehaviorRegistry`, `BlockInstance` (`world`).
- IO/game: `SaveIo`, `TypeIO`, `EntityCodec`, `StateDump` (`sim::dump`); `GameState`/`State`, `Scenario`.

## Invariants

- **Godot-free and tokio-free.** The dependency tree must not contain `godot` or `tokio`, and no
  source under `client/rust/mind-core` may `use godot`/`use tokio`; CI enforces both with
  `cargo tree -p mind-core` and `grep -RnE --include='*.rs' 'use (godot|tokio)' client/rust/mind-core`.
- **View never feeds sim.** `render`, `fx`, `audio` and `combat::view` output is excluded from
  saves/sync/checksum (`ChecksumScope::include_view` is always `false`).
- **Content IDs are append-only.** Construction order defines dense per-type IDs; `ContentType`
  ordinals, names, bundle keys and sprite region names are frozen parity ABI. Entity field order is
  serialization order: append `#[entity(since = N)]` fields, never reorder.
- **Ordered containers on serialized paths.** `io::StringMap` is an `IndexMap` and `serde_json` uses
  `preserve_order`. No `unwrap`/`expect` on runtime data (lint policy:
  [`client/rust/AGENTS.md`](../AGENTS.md)).

## Rules

- Workspace dependency, lint and toolchain policy lives in [`client/rust/AGENTS.md`](../AGENTS.md).
  Do not relax the boundary to land a feature — move shared logic into `mind-core` instead; hosts
  call in, `mind-core` never calls back out.
- Keep every feature runnable with a plain `cargo test -p mind-core`, with no Godot or network.
- Header/license/upstream-citation rules follow the root [`AGENTS.md`](../../../AGENTS.md).

## Conventions

- Optional behavior is gated by default-off features in `Cargo.toml`: `msav-import` (upstream `MSAV`
  save import), `alloc-audit` (allocation counting) and `profile-build` (per-tick cost attribution).
- `extern crate self as mind_core;` lets `mind_derive` output reference `::mind_core` inside the
  crate; the root re-exports `version::MIND_VERSION` and `sim::dump`.

## Deeper subsystem notes

Child guides own only local layout, invariants and focused commands; crate-wide rules stay here.

- Simulation and data: [`sim`](src/sim/AGENTS.md), [`ai`](src/ai/AGENTS.md),
  [`combat`](src/combat/AGENTS.md), [`content`](src/content/AGENTS.md),
  [`maps`](src/maps/AGENTS.md), [`mods`](src/mods/AGENTS.md).
- World, game and persistence: [`world`](src/world/AGENTS.md),
  [`world/blocks`](src/world/blocks/AGENTS.md), [`game`](src/game/AGENTS.md),
  [`io`](src/io/AGENTS.md).
- View, control and scripting: [`fx`](src/fx/AGENTS.md), [`render`](src/render/AGENTS.md),
  [`ui`](src/ui/AGENTS.md), [`input`](src/input/AGENTS.md), [`logic`](src/logic/AGENTS.md).

## Verification

Run from the repo root (Linux).

```bash
cargo test --manifest-path client/rust/Cargo.toml -p mind-core

# boundary gate (must print nothing / find nothing)
cargo tree --manifest-path client/rust/Cargo.toml -p mind-core --prefix none | grep -Eq '^(godot|tokio)( |$)' && echo FAIL
grep -RnE --include='*.rs' 'use (godot|tokio)' client/rust/mind-core && echo FAIL

# headless oracle golden: spine_place_break -> 3e48c583a94fbea4
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- run spine_place_break --json
```
