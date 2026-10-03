# AGENTS.md — mind-core/src/game (game orchestration)

This folder is the high-level game and rules orchestration layer of `mind-core`: the coarse game-state
header, match rules and their runtime behavior, teams, waves and difficulty, sector/campaign
progression, objectives, fog, saves policy and schematics. It ties content, world, entities, IO and
the `sim` spine together by holding the rule state and the ordered play-flow state machine, while the
ECS world, the fixed-step pump and byte-level serialization stay in their own modules. Read the root
[`AGENTS.md`](../../../../../AGENTS.md) first; the crate map is `mind-core/AGENTS.md` (`../../AGENTS.md`).

## Layout

| Path | Responsibility |
|---|---|
| `../game.rs` | Module root: declares the submodules and defines `State` (`Menu`/`Playing`/`Paused`) and `GameState` (`tick`, `update_id`, `advance` and the coarse-state helpers). |
| `play.rs` | Campaign play flows and the `PlaySession` match state: `play_map`/`play_new_sector`/`play_sector`, `logic_play`, `run_wave_campaign`, `check_game_state`, `sector_capture`, `game_over`/`update_game_over`/`sector_lose`; emits ordered `PlayEvent`s. |
| `rules.rs` | Runtime behavior on the persisted `Rules`: `copy`, `retain_content_fields`, `mode`, per-team multiplier accessors, bans, env masks, `checksum_part`, and the campaign/tuning constants (`TURN_DURATION_TICKS`, `SAVE_EXTENSION`, …). |
| `rules_event.rs` | Rules application: `RulesEpoch`, `RulesLoad`, `decode_rules` (100 kB cap), `guard_set_rules`, `apply_set_rules`, `apply_rules_load`, `SetRulesError`. |
| `campaign_rules.rs` | Per-planet `CampaignRules`/`Difficulty` presets and `apply` folding into `Rules`; settings `read`/`write`. |
| `gamemode.rs` | `Gamemode` presets (`Survival`/`Sandbox`/`Attack`/`Pvp`/`Editor`), `apply(Rules)` and `valid(MapView)`. |
| `universe.rs` | `Universe` campaign clock/launch state and the `Campaign` container: planets, `run_turn` production/import/invasion passes, `update_global`, `save_all`/`load_all`. |
| `planet.rs` | `Planet` runtime: orbits/positions, sectors, `apply_rules`, `update_base_coverage`/threat, `SectorNeighborhood` adjacency seam. |
| `sector.rs` | `Sector` runtime and `SectorInfoState`: base/attack/frozen/captured predicates, item storage, rolling `InfoWindows`, `prepare`/`write`, settings round-trip. |
| `stats.rs` | `CampaignStats` (per-planet lifetime totals) plus `GameStats` mutation helpers. |
| `waves.rs` | `Waves`: the built-in 28-group table and the seeded `ArcRand` generator; `WaveDifficulty`, `table_units_registered`. |
| `spawn_group.rs` | `SpawnGroup`/`ItemStack`: wave descriptor, `get_spawned`/`get_shield` scaling, JSON codec, `create_unit`. |
| `teams.rs` | `Teams` registry and `TeamData` caches: `BuildingRecord`/`UnitRecord` snapshots, quadtrees, clustered counts, team inventory, `destroy_to_derelict`, core/enemy queries. |
| `team.rs` | The 256-entry `Team` registry (`all`/`get`/`base_teams`), colors/palettes and AI predicates. |
| `quad_tree.rs` | Insertion-stable `QuadTree<T>` point index used by `TeamData`. |
| `tech_tree.rs` | Research runtime over `content::tech`: `requirements_complete`, `can_spend`/`spend`/`unlock`, `check_auto_unlocks`, `content_unlocked`, `reset_all`. |
| `objectives.rs` | Tech-tree `Objective` predicates (`Research`/`Produce`/`SectorComplete`/`OnSector`/`OnPlanet`) over `ObjectiveContext`. |
| `map_objectives.rs` | `MapObjectivesRuntime` executor: `update`/`complete`/`qualified`/`dependency_finished`, transient timer/completion state, flag application. |
| `map_markers.rs` | `MapMarkers` containers: the three index vectors, `MarkerKind` index fix-ups, `control` and save-region `write`/`read`. |
| `saves.rs` | Save-slot policy on `io::save`: `load`/listing, autosave cadence, playtime, `save_sector`, `cautious_load`, remap. |
| `schematics.rs` | `.msch` codec and the `Schematics` registry: `read`/`write`/base64, `place`/`place_loadout`, loadouts, `rotate`/`rotated`. |
| `schematic.rs` | `Schematic`/`Stile` values and `SchematicPreviewSpec`. |
| `fog.rs` | `FogControl`: static/dynamic per-team fog buffers, `is_discovered`/`is_visible`, save chunk `read`/`write`. |
| `attack_indicators.rs` | Minimap `AttackIndicators` insertion-ordered timers. |
| `world_reloader.rs` | `WorldReloader` trait plus `HostReloader`/`ClientReloader` handshakes around a world reload. |

## Key types

- State: `State`, `GameState` (`../game.rs`); `PlaySession`, `PlayEvent`, `SectorRef` (`play.rs`).
- Rules: `Rules` (re-exported from `io::json::rules`; behavior added in `rules.rs`), `Gamemode`,
  `CampaignRules`, `Difficulty`, `CampaignApply`, `RulesEpoch`, `RulesLoad`.
- Campaign: `Universe`, `Campaign`, `TurnContext`, `TurnReport`, `CampaignEvent` (`universe.rs`);
  `Planet`, `Sector`, `SectorInfoState`, `CoreSnapshot`, `WriteTarget`, `InfoWindows`, `SectorKey`.
- Teams and waves: `Teams`, `TeamData`, `BuildingRecord`, `UnitRecord`, `BlockPlan`,
  `DerelictReport`, `QuadTree`, `Team`; `Waves`, `SpawnGroup`, `ItemStack`, `WaveDifficulty`.
- Objectives and research: `Objective`, `ObjectiveContext`, `SectorStatus`;
  `MapObjectivesRuntime`, `ObjectiveEnv`, `ObjectiveRunParams`, `ObjectiveLocale`, `CompletedObjectives`;
  `SpendResult`, `SpendError`; `MapMarkers`, `MarkerKind`, `ObjectiveMarker`.
- IO-facing: `Saves`, `SavesUpdate`, `MissingModsReport`, `SaveSlot`; `Schematic`, `Stile`,
  `Schematics`, `SchematicWorld`, `EcsSchematicWorld`, `SchematicError`; `FogControl`,
  `FogData`, `FogBits`, `FogSource`; `AttackIndicators`, `Indicator`.

## Invariants

- **Fixed 60 Hz, no wall clock.** `Universe::advance` converts `delta_ticks` to seconds and turns;
  `fog` flushes are quantized to `DYNAMIC_UPDATE_INTERVAL_TICKS`, `attack_indicators` age in ticks,
  and `saves` accrues playtime from `delta_ticks`. Randomness flows only through `JavaRandom` and
  `ArcRand` (e.g. `Campaign::run_turn`, `Planet::from_def`).
- **Ordered containers on persisted/compared paths.** `Campaign.planets` is an `IndexMap` in content
  order, `MapMarkers.map` is a `BTreeMap` iterated by ascending id, `MapMarkers`' index vectors stay
  explicit, and `QuadTree` queries return insertion order.
- **Content is keyed by name.** `Rules` bans/loadout, `SpawnGroup`, `CampaignStats` and team
  inventory all reference content by name (the parity/mod ABI); `retain_content_fields` takes an
  `is_patch_content` predicate because there are no content pointers here.
- **Transient vs persisted state.** `MapObjectivesRuntime` keeps `completed`/`dep_finished`/`countup`
  out of the persisted `MapObjectives` data; `SectorInfoState` keeps the `WindowedMean` windows in a
  sibling `InfoWindows` so the persisted `SectorInfo` JSON stays unchanged.
- **No `unwrap`/`expect` on runtime data.** Fallible paths return `IoError`/`SchematicError`/
  `SetRulesError`; tests opt out via the crate-level `cfg_attr(test, …)`.

## Boundaries

- `mind-core::sim` owns `Sim`, `FixedStepRunner`, `SimClock`, the schedule and the checksum;
  `game.rs`'s `GameState` is the state header those read. `sim::logic::check_game_state` is a stub —
  the real transitions live in `play.rs` (and `event::RulesLoadEvent` / `RulesEpoch` carry the rules
  reload to the bus).
- `mind-core::io` owns the save container, `TypeIO`, `JsonIO` and settings. `game::rules` re-exports
  the persisted `Rules` and adds inherent behavior; `saves.rs`, `schematics.rs`, `map_markers.rs` and
  `fog.rs` drive their region writers/readers through `io`.
- `world`/`entities` own the ECS grid and components. `game` operates on snapshot records
  (`BuildingRecord`/`UnitRecord`) and exposes seams rather than holding a `World`: `SpawnGroup::create_unit`,
  `SchematicWorld`, `ObjectiveEnv` and `ObjectiveContext` are the adapters.

## Rules

- Add runtime behavior to existing persisted structs through inherent `impl` blocks in this folder
  (Rust permits one inherent `impl` per type per crate); do not redefine the JSON shape owned by `io`.
- Keep field order and camelCase JSON frozen; extend shapes append-only and gate new optional fields
  with `serde(default)`.
- Port upstream Mindustry and cite it in the file header; files are UTF-8/LF, GPL-3.0-only.

## Verification

Run from `client/rust/`, or add `--manifest-path client/rust/Cargo.toml` from the repo root.

```bash
cargo test -p mind-core game
cargo test -p mind-core
cargo clippy -p mind-core --all-targets -- -D warnings
```

The wave generator is pinned to the JVM golden `tests/golden/units/wave_generate.json`
(`waves::tests::generate_matches_java_golden`), and the settings/JSON round-trips
(`Universe`, `SectorInfo`, `CampaignRules`, `CampaignStats`) run under `cargo test -p mind-core game`.
