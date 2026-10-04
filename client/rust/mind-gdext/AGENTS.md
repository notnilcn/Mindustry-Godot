# AGENTS.md — mind-gdext (Godot GDExtension)

`mind-gdext` is the `cdylib` that Godot 4.7 loads as the GDExtension, bridging `res://` scenes and
GDScript UI to `mind-core` and `mind-stdb`. It registers the `Mind*` classes below, drives the
fixed-step simulation pump, translates Godot input into core commands, runs the render, audio and FX
layers, and wires the SpacetimeDB and platform seams. It holds no game rules: every simulation
mutation crosses into `mind-core`. Read the root [`AGENTS.md`](../../../AGENTS.md) first; the
project- and client-level maps are `client/AGENTS.md` and `client/rust/AGENTS.md`. The `MindNet` API
surface is documented in [`NET_API.md`](NET_API.md).

## Layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Crate root: module declarations, public class re-exports, `MindExtension` `ExtensionLibrary`, `MIND_VERSION`. |
| `src/sim_host.rs` | `MindSimHost` — owns `mind_core::Sim`, the fixed 60 Hz pump and the MCP/test API. |
| `src/camera.rs` | `MindCamera2D` — pan/zoom/edge-pan/shake; screen↔tile transforms over `mind_core::input::CameraState`. |
| `src/tile_grid.rs` | `MindTileGrid` — debug grid lines plus one quad per non-air block. |
| `src/render.rs` | `MindWorldRenderer` (frame pipeline, band children, `RenderStats`, `build_menu_texture` menu-world bake) and `MindRender` facade. |
| `src/fx.rs` | `MindFx` — effect/decal/trail/shake pools and the `FxBus` drain; geometry helpers in `fx/`. |
| `src/logic.rs` | `MindLogic` — mlog statement metadata plus parse/compile/run probes over `mind_core::logic`. |
| `src/campaign.rs` | `MindCampaign` — sector/play/rules/tech/schematic facade over `mind_core::game`. |
| `src/mods.rs` | `MindMods` — scans `user://mods`, persists enable flags, exposes listing/import/errors. |
| `src/stdb.rs` | `StdbConnector` autoload and `StdbBinder` node over `mind-stdb`. |
| `src/settings.rs` | Reads/writes `user://settings.json` (`selected_block`, `zoom`). |
| `src/log_bridge.rs` | `log` → Godot console + `user://last_log.txt` bridge and panic hook. |
| `src/hello.rs` | `MindHello` — smoke node proving the class registry and log bridge. |
| `src/assets/` | `MindAssets` plus atlas, bundle, font, audio and path loaders (`atlas.rs`, `bundle.rs`, `fonts.rs`, `audio.rs`, `loader.rs`). |
| `src/audio/` | `MindAudio` (buses, stream cache, voice pool) over `mind_core::audio` state machines. |
| `src/editor/` | `MindEditor` (state/command/palette/map-view facade) and `MindPreview` (map preview textures). |
| `src/fx/` | `draw_turret.rs`, `env.rs` — turret and environment FX geometry builders. |
| `src/input/` | `MindInput` plus event translation, bindings, gesture and mobile bridges. |
| `src/net/` | `MindNet` multiplayer autoload and the `relay.rs` runtime mapping. |
| `src/platform/` | `MindPlatform` plus args, crash, desktop, dialogs, discord, service, update, uri and workshop host glue. |
| `src/render/` | Render passes: `floor`, `blocks`, `building_cache`, `shadow`, `light`, `fog`, `overlays`, `debug`, `env`, `menu`, `g3d`, `load`, `minimap`, `pixelate`, `atlas_bind`, `shaders`. |
| `src/ui/` | `MindUi` (dialog registry, pause governor, prompt/toast signals) and the `MindHud` read-only surface. |

## Responsibilities

- **Thin Godot shell.** Each module owns the Godot-facing half of a subsystem; the rules, math and
  deterministic logic live in `mind-core`. Facades convert across the Godot boundary and never invent
  state.
- **Fixed-step frame pump.** `MindSimHost` owns a `mind_core::sim::FixedStepRunner` built from
  `sim.config().fixed_hz` and advances it in `_process`. `advance_steps` drains `pending_commands` at
  each tick start, ticks the sim (or `ScenarioPlayer`), ticks `MindFx::tick_view`, bumps the world
  revision and emits `state_changed(tick, checksum)` / `world_changed`. `step()` runs ticks
  synchronously; `set_paused` rebuilds the accumulator.
- **Render split.** `mind_core::render` owns the data/scan side (`Layer`, `BandPlan`, `RenderQueue`,
  `CameraView`, `Lod`, `RulesRenderView`, minimap math). `render.rs` owns the GPU/Godot side: one
  `Node2D` per band entry plus the listed passes. The renderer reads the sim; it never feeds it.
- **Input translation.** `MindInput` captures `InputEvent`s into `mind_core::input::RawEvent`s and
  owns `BindingState`, `InputLocks` and `FocusState`; `MindCamera2D` polls key state for pan/zoom and
  exposes `screen_to_tile`/`tile_to_screen`; `MindSimHost::input` turns left/right mouse into
  `Command::Place`/`Command::Break`.
- **Net and STDB wiring.** `MindNet` owns the `MatchSession`, `Connector`, `RelayRuntime` and
  `CommandSender`; foreign `SimCommand`s are queued into `MindSimHost.pending_commands` so they apply
  only at tick boundaries, and scoped checksums are published while in game. `StdbConnector` owns the
  single `mind-stdb` connector pump; `StdbBinder` mirrors one table's row signals.
- **Platform glue.** `MindPlatform` snapshots capabilities, applies launch/window args and exposes
  native-dialog, URI, crash, file-import and service hooks. `platform::discord` is compile-gated
  behind the default-off `discord` feature.

## Key types

- `MindExtension` (`lib.rs`): `ExtensionLibrary`. On `InitStage::Scene` it calls
  `log_bridge::install()` (level-prefixed console output, `user://last_log.txt` append, panic hook)
  then `platform::crash::install()` (chained panic hook writing a report under the data root).
- `MindSimHost`: `sim`, `runner`, `player`, `pending_commands: VecDeque<SimCommand>`, `world_dirty`,
  `revision`, `audio_log`.
- `MindWorldRenderer` / `MindRender`: `BandPlan`, `RenderQueue`, `RenderStats`, `EditorRenderSpec`
  mount/unmount.
- `MindAudio`: `BusLayout`, `StreamCache`, `VoicePool`, `MusicPlayer`, `LoopMixer`, `SharedAudioLog`.
- `MindUi`: dialog registry/stack, `pause_depth`, semantic prompt signals; `MindHud`: typed `#[var]`
  fields refreshed from the sim host.

## Invariants

- No game rules in this crate; `mind-core` owns all simulation. The crate depends only on `godot`,
  `mind-core`, `mind-stdb`, `serde_json`, `smallvec` and `log`.
- Relay/foreign commands apply only at fixed-tick starts, never mid-frame. Mouse input routes through
  `MindCamera2D::screen_to_tile`; scene lookups are by node path and warn when missing.
- Render layers are append-only `BandPlan` entries; the minimap batches tile updates and rebuilds from
  the `world_revision` counter.
- Scene `Mind*` nodes rebuild Godot-derived state in `bootstrap()` in response to the
  `EXTENSION_RELOADED` notification, because `ready()` runs only once per engine load. Every
  `bootstrap()` is idempotent: resource inserts overwrite, connectors/binders are dropped and
  reopened, and persisted band children are adopted rather than duplicated. `MindAudio` is the
  exception: its `ready()` creates the voice pool, music player and camera listener, and those
  node-tree mutations re-enter the notification callback while the node is already mutably
  borrowed (a gdext `bind_mut` re-entrancy panic), so it keeps `ready()`-only initialization with
  no `on_notification` arm.
- `mind-core` stays Godot-free and tokio-free; all Godot types stay in this crate.

## Rules

- Use `#[derive(GodotClass)]`, `#[class(base=...)]`, `#[godot_api]`, `#[func]` and `#[signal]` for any
  class surface; exported names are part of the MCP/test contract and stay append-only.
- Log through the `log` crate (`log::info!`/`warn!`/`error!`) so messages reach both Godot and
  `last_log.txt`; `clippy` denies `unwrap_used` and `expect_used`.
- Nodes created in code carry a `# code-instantiated: <reason>` comment. New files are LF/UTF-8 and
  cite ported Mindustry sources in the header.
- The editor mutex is single-instance; do not launch a second editor for in-engine probes.

## Verification

```bash
# Build the cdylib + oracle into client/bin/rust and sync scenarios
tools/build.sh

# Rust gates for this package
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check
cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
cargo check --manifest-path client/rust/Cargo.toml -p mind-gdext

# GDExtension import/parse check (no window)
bash tools/godot.sh --headless --editor --quit --path client

# Full local gate: fmt, clippy, tests, goldens, bench, build, headless import, STDB
tools/ci.sh
```

In-engine workflows (launch, node map, pid-stamped evals) go through open-godot-mcp; the repo skill
`.opencode/skills/playtest/SKILL.md` has the launch flow and recipes.
