# AGENTS.md — mind-gdext (Godot GDExtension)

`mind-gdext` is the `cdylib` that Godot 4.7 loads as the GDExtension, bridging `res://` scenes and
the native UI to `mind-core` and `mind-stdb`. It registers the `Mind*` classes, drives the fixed-step
pump, translates Godot input into core commands, runs the render/audio/FX layers, and wires the
SpacetimeDB and platform seams. It holds **no game rules**: every simulation mutation crosses into
`mind-core`, whose guides own the rules and view data. Read the root [`AGENTS.md`](../../../AGENTS.md)
first; the project- and workspace-level maps are [`client/AGENTS.md`](../../AGENTS.md) and
[`client/rust/AGENTS.md`](../AGENTS.md). The `MindNet` API surface is documented in
[`NET_API.md`](NET_API.md).

## Layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Crate root: module declarations, public class re-exports, `MindExtension` `ExtensionLibrary`, boot hooks, `MIND_VERSION`. |
| `src/sim_host/` | `MindSimHost` — owns `mind_core::Sim`, the fixed 60 Hz pump, player control and the MCP/test API. `mod.rs` keeps the class, `pump`/`state`/`player` the tick and state seams, `commands`/`command_panel`/`plans` the order surface, `world_load`/`campaign_link`/`io` the load and campaign seams, `render_blocks`/`render_units`/`render_light`/`bar_rows` the render reads, and `inspect`/`world_query` the probes. |
| `src/camera.rs` | `MindCamera2D` — pan/zoom/edge-pan/shake; screen↔tile transforms over `mind_core::input::CameraState`. |
| `src/tile_grid.rs` | `MindTileGrid` — debug grid lines plus one quad per non-air block. |
| `src/render.rs` | `MindWorldRenderer` (frame pipeline, band children, `RenderStats`, `build_menu_texture` menu-world bake) and the `MindRender` probe facade. |
| `src/fx.rs` | `MindFx` — effect/decal/trail/shake pools and the `FxBus` drain; geometry helpers in `fx/`. |
| `src/logic.rs` | `MindLogic` — mlog statement metadata plus parse/compile/run probes over `mind_core::logic`. |
| `src/campaign/` | `MindCampaign` — sector/play/rules/tech/schematic facade over `mind_core::game`. `mod.rs` keeps the class and its `#[func]` surface; `pump.rs` the runtime steps, `runtime.rs` the Godot-side seams, `views.rs` the typed read-model producers, `launch.rs`/`sector_launch.rs` the launch flow, `saves.rs`/`lifecycle.rs` the save and boot flows, `research.rs`/`schematics.rs`/`rules.rs` the remaining feature facades. |
| `src/mods.rs` | `MindMods` — scans `user://mods`, persists enable flags, exposes listing/import/errors. |
| `src/stdb.rs` | `StdbConnector` autoload and `StdbBinder` node over `mind-stdb`. |
| `src/content_boot.rs` | One per-session `ContentRegistry` builder shared by `MindSimHost`, `MindCampaign` and `MindEditor`; resolves enabled mods once. |
| `src/settings_hub.rs`, `src/settings.rs` | Process-wide `Core.settings` store (`init`/`with_settings`/`save`, shared by UI, campaign, mods, input); `settings.rs` is the legacy P0 `user://settings.json` reader/writer reserved for the full port and not restored at boot. |
| `src/display_context.rs` | View-side icon/glyph image provider for the CPU logic-display rasterizer. |
| `src/log_bridge.rs` | `log` → Godot console + `user://last_log.txt` bridge and panic hook. |
| `src/assets/` | `MindAssets` plus atlas, bundle, decorator, font, audio and path loaders. |
| `src/audio/` | `MindAudio` (buses, decorator, stream cache, voice pool) over `mind_core::audio` state machines. |
| `src/editor/` | `MindEditor` (state/command/palette/map-view facade, `#[func]`s split across `api_*.rs` by feature), `MindEditorRenderer` (editor view) and `MindPreview` (map preview textures). |
| `src/fx/` | `draw_turret.rs`, `env.rs`, `explosion_sink.rs` — turret/environment geometry plus the reactor-explosion → FX/audio sink. |
| `src/input/` | `MindInput` plus event translation (`events`, `bindings`, `gesture`), the `#[func]` surface split across `api_*.rs`, and the desktop/mobile bridges (`desktop/mod.rs` keeps the type definitions, the behaviour is chunked across its children, `glue`/`glue_mobile`/`effects`). |
| `src/net/` | `MindNet` multiplayer node and the `relay.rs` runtime mapping. |
| `src/platform/` | `MindPlatform` plus args, crash, desktop, dialogs, discord, service, update, uri and workshop host glue. |
| `src/render/` | Render passes mounted under the `BandPlan` children: terrain/light `floor`, `blocks`, `block_extras`, `building_cache`, `cached_cracks`, `shadow`, `light`, `fog`, `weather`, `env`; units/bullets `units`, `unit_parts`, `unit_build`, `unit_overlays`, `unit_abilities`, `bullets`, `turret`, `shields`, `status`, `hazards`; overlays/UI `overlays`, `overlay_top`, `command_overlay`, `plans`, `links`, `logic_displays`, `objective_markers`, `pods`, `build_beam`, `debug`, `minimap`, `menu`, `g3d`, `load`, `pixelate`; shared helpers `canvas`, `atlas_bind`, `shaders`. |
| `src/gdutil/` | Shared Godot-side helpers: `json` (Variant→JSON), `variant` (lenient Variant/VarDictionary coercion) and `coords` (screen/tile conversions). |
| `src/ui/` | The native UI: `MindUi` (dialog registry, pause governor, prompt/toast signals), the `MindHud` read-only surface, `MindDialog` plus the dialog controllers in `ui/dialogs/<group>/` (campaign, database, dev, editor, game, logic, maps, menu, mods, net, pickers, rules, schematics, settings; shared helpers in `dialogs/common/`), the 14 fragments, the `kit/` widgets/theme/text/scroll/layout, the roots (`ui/root/` = `MindUiRoot`, `ui/planet/` = `MindPlanetView`, `ui/landing.rs` = `MindLandingCore`, menu background/flyers, editor views) and the four headless oracles (`checks.rs`). `MindDialog::campaign()` is the dialog-side typed campaign accessor; `campaign_views`/`research_tree`/`launch_plan`/`sector_state` return T01's typed read models. Guides: `src/ui/PORTING.md` (API contract) and `PORT-STATUS.md` (inventory + cutover record). |

## Where things live

- **A Godot class may split across files.** The `#[godot_api] impl` that owns every `#[signal]`,
  `#[rpc]` and any fn named by a `#[var(get = …, set = …)]` attribute is the primary block and
  stays in the class's `mod.rs`. Additional `#[godot_api(secondary)] impl` blocks hold the
  `#[func]` groups in `api_*.rs`; plain `impl` blocks hold private helpers. Signals never move
  out of the primary block, and one `#[func]` name may appear only once across all blocks.
- **Dialogs live in `ui/dialogs/<group>/`.** One file per Java dialog class, named
  `<java_snake>.rs` (e.g. `editor/MapResizeDialog.java` → `dialogs/editor/map_resize.rs`); the
  group's `mod.rs` owns its `dispatch(id)` and declares the leaves. Group-local helpers live in
  the group's `util.rs`; helpers two or more groups share live in `dialogs/common/`
  (`defer` = deferred actions/frame ticks, `facade` = `MindUi` calls and rich labels, `prompt` =
  prompt-signal wiring). Controllers are registered in `ui/dialogs/mod.rs::controller_for`, and
  `every_manifest_id_resolves_exactly_once` proves one owner per manifest id.
- **The `Mind*` facades split by topic, not by Godot concern.** `campaign/`, `sim_host/`,
  `editor/`, `input/` and `net/` keep their class declaration and `ready`/`process`/`bootstrap`
  in `mod.rs` and move behaviour into a named child file; `#[func]` groups go in `api_*.rs`.
- **Deferred work goes through `dialogs/common/defer`** (`defer_action`, `Deferred`,
  `install_frame_tick`) rather than a fresh `call_deferred` closure, so a nested `bind_mut` panic
  cannot happen (see [`src/ui/PORT-STATUS.md`](src/ui/PORT-STATUS.md)).

## Responsibilities

- **Fixed-step frame pump.** `MindSimHost` owns a `mind_core::sim::FixedStepRunner` built with
  `for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame)` and advances it in `_process`.
  `advance_steps` drains `pending_commands` at
  each tick start, ticks the sim (or `ScenarioPlayer`) and `MindFx::tick_view`, then marks the world
  dirty. `_process`/`step` emit `state_changed(tick, checksum)`/`world_changed`; `set_paused` rebuilds
  the runner, resetting the accumulator.
- **Class registry.** Each `#[derive(GodotClass)]` node is registered with the engine at load.
  Exported class names, `#[func]` signatures and `#[signal]` names are the GDScript/MCP ABI and stay
  append-only; the scene files (`game.tscn`, autoload scenes) name the instances.
- **Render execution split.** `mind_core::render` owns the data/scan side (`Layer`, `BandPlan`,
  `RenderQueue`, `CameraView`, `Lod`, `RulesRenderView`, minimap math). `MindWorldRenderer` owns GPU
  execution: one `Node2D` per append-only `BandPlan` entry plus the pass children above; `MindRender`
  is the stable probe facade. The renderer reads the sim and never feeds it.
- **Input translation.** `MindInput` captures `InputEvent`s into `mind_core::input::RawEvent`s and holds
  `BindingState`/`InputLocks`/`FocusState`; `MindCamera2D` polls pan/zoom and maps screen↔tile;
  `MindSimHost::unhandled_input` turns unclaimed mouse clicks into `Command::Place`/`Command::Break`.
- **Net and STDB wiring.** `MindNet` owns the `MatchSession`, `Connector`, `RelayRuntime` and
  `CommandSender`; foreign `SimCommand`s queue into `MindSimHost.pending_commands` so they apply only
  at tick boundaries, and scoped checksums are published while in game. `StdbConnector` owns the
  single `mind-stdb` connector pump; `StdbBinder` mirrors one table's row signals.
- **Platform glue.** `MindPlatform` snapshots capabilities, applies launch/window args and exposes
  native-dialog, URI, crash, file-import and service hooks; `platform::discord` is feature-gated.

## Key types

- `MindExtension` (`lib.rs`): on `InitStage::Scene` sets the data dir, installs `log_bridge` and the
  chained crash hook, and (outside the editor) records the launch.
- `MindSimHost`: `sim`, `runner`, `player`, `pending_commands: VecDeque<SimCommand>`, `world_dirty`,
  `revision`, `audio_log`.
- `MindWorldRenderer` / `MindRender` / `MindMinimap`: `BandPlan`, `RenderQueue`, `RenderStats`,
  `mount_editor_view`/`unmount_editor_view`, minimap texture/region updates.
- `MindAudio`: `BusLayout`, `StreamCache`, `VoicePool`, `MusicPlayer`, `LoopMixer`,
  `SharedAudioLog`. `MindUi`: dialog registry/stack, `pause_depth`, prompt signals; `MindHud`: typed
  `#[var]` fields refreshed from the sim host.

## Invariants

- No game rules in this crate; `mind-core` owns all simulation. The crate depends only on `godot`,
  `mind-core`, `mind-stdb`, `bevy_ecs`, `serde_json`, `indexmap`, `smallvec` and `log`.
- Relay/foreign commands apply only at fixed-tick starts, never mid-frame. Mouse input routes through
  `MindCamera2D::screen_to_tile`.
- **Scene-spine deps are injected, not looked up.** The migrated scene-spine classes take collaborators as
  `#[export] Option<Gd<T>>` fields wired from `client/scenes/game.tscn` (`node_paths` + `NodePath`, relative to
  the node carrying the property) and adopt them in `ready`/`bootstrap` through `wiring::{required, optional}`.
  A missing required dep warns once (``{class}: missing required `{property}` (wire it in the owning scene)``)
  and the class degrades; optional deps stay silent. UI dialog/fragment controllers and the code-built
  render-pass children still resolve nodes through the `kit::*`/path helpers — deferred, see
  [`docs/GODOT-SCENES-REFACTOR.md`](../../../docs/GODOT-SCENES-REFACTOR.md).
- Render layers are append-only `BandPlan` entries; the minimap batches tile updates and rebuilds
  from the `world_revision` counter.
- Scene `Mind*` nodes rebuild Godot-derived state in `bootstrap()` in response to the
  `EXTENSION_RELOADED` notification, because `ready()` runs only once per engine load. Every
  `bootstrap()` is idempotent: resource inserts overwrite, connectors/binders are dropped and
  reopened, and persisted band children are adopted rather than duplicated. `MindAudio` is the
  exception: its `ready()` creates the voice pool, music player and camera listener, and those
  node-tree mutations re-enter the notification callback while the node is already mutably
  borrowed, so it keeps `ready()`-only initialization with no `on_notification` arm.
  The native UI (`MindUiRoot`, `MindDialog`, the fragments) has no reload bootstrap either:
  rebuilding the extension while an editor session is open needs an editor restart. In
  particular, a class that changes its own `visible` must not implement `on_notification`
  (the notification is delivered synchronously from inside the bound `ready`/`show_dialog`
  and re-enters `bind_mut`; see `src/ui/PORT-STATUS.md`).
- All Godot types stay in this crate; `mind-core` remains Godot-free and tokio-free.

## Rules

- Use `#[derive(GodotClass)]`, `#[class(base=...)]`, `#[godot_api]`, `#[func]` and `#[signal]` for any
  class surface; exported names are part of the MCP/test contract and stay append-only.
- **No new spine path lookups.** Never add a `/root/Spine` or `"../"` lookup to a migrated scene-spine
  class; wire the dependency in the owning scene instead (`node_paths` + `NodePath` on the node that owns
  the property). Required deps go through `wiring::required` — exactly one
  ``{class}: missing required `{property}` (wire it in the owning scene)`` warning, then degrade — and
  optional deps through `wiring::optional` (silent). Wire new deps as additions only; class, `#[func]`
  and `#[signal]` names stay append-only.
- Log through the `log` crate (`log::info!`/`warn!`/`error!`) so messages reach both Godot and
  `last_log.txt`.
- Nodes created in code carry a `# code-instantiated: <reason>` comment.
- The editor mutex is single-instance; do not launch a second editor for in-engine probes.

## Verification

Run from the repo root.

```bash
# Build this cdylib + the oracle into client/bin/rust and sync scenarios
bash tools/build.sh

# Crate gate
cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
```

Workspace gates live in [`client/rust/AGENTS.md`](../AGENTS.md); the Godot import/parse check is
owned by [`client/AGENTS.md`](../../AGENTS.md). In-engine launch, node maps and pid-stamped evals go
through `open-godot-mcp`; use the `playtest` skill for the launch flow and the CLICK/HOVER/DRAG
recipes.
