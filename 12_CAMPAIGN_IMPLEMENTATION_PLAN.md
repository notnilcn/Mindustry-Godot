# 12 — CAMPAIGN, RULES, TEAMS, SCHEMATICS, OBJECTIVES & FOG IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Ready to execute after 05–11 milestones are green; 3 items flagged `NEEDS USER DECISION`/`NEEDS ORCHESTRATOR RECONCILE` in §8 (R2/R3/R8); all have defaults so execution continues. |
| **Phase** | P4 — Combat, units, campaign (`HIGH_LEVEL_PLAN.md` §5: this plan plus 10/11 is the P4 gate “campaign sector can be launched and saved”). |
| **Depends on** | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`Sim`/schedule/`Groups`/`Events`/`Time`/`SimCommand`/`SimRng`/`EntityIds`/`Tmp`; the minimal `Rules`/`Teams` boundary is **replaced** here, §3.1), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`WorldGrid`/`Tiles`/`Tile`, `WorldContext`, planet generators, `Maps`, `Edges`), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (`Build.validPlace`, `ConstructBlock`, `BuildPlan`, building entities), `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (`CoreBlock`/`StorageBlock`/`Unloader`, `TeamInventory` decision R2), `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (`QuadTree` port R4, `WindowedMean`, `PowerModule`, reactor/lighting fields), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (`Rules` damage accessors, `Team`/`TeamData.present`), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (`Rules.spawns`, `WaveSpawner`, `TeamData.build_ai`/`rts_ai`, unit caps, `Waves::generate` call sites). |
| **Blocks** | `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` (markers/`LMarkerControl`, global flags, world-processor guards, `LExecutor` for `completionLogicCode`), `14_UI_IMPLEMENTATION_PLAN.md` (research dialog, planet/sector map, custom rules, schematics list, game-over, save slots, objectives HUD), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (map metadata, editor objectives/waves dialogs, preview pixels/textures, locales), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (`set_rules`/`complete_objective`/`research`/sector capture relays, campaign persistence in STDB), `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (save-file associations, dedicated server policy), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (goldens/bench registration). Consumed (not blocked) by `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (fog/markers/minimap/attack indicators). |
| **Sources (read in full where noted)** | Mindustry AGENTS docs: `core/src/mindustry/game/AGENTS.md`, `type/AGENTS.md`, `core/AGENTS.md`, `io/AGENTS.md`, `ai/AGENTS.md`, `net/AGENTS.md` (WorldReloader), `ui/AGENTS.md` (ResearchDialog flow only), `content/AGENTS.md` (tech trees), `tests/AGENTS.md`. Code: `game/Rules.java` (443), `Gamemode.java` (86), `Team.java` (183), `Teams.java` (540), `Universe.java` (303), `Saves.java` (502), `Schematic.java` (158), `Schematics.java` (773), `SectorInfo.java` (366), `SpawnGroup.java` (196), `Waves.java` (skim; 11 owns), `CampaignRules.java` (44), `Difficulty.java` (43), `CampaignStats.java` (31), `GameStats.java` (37), `FogControl.java` (591), `MapObjectives.java` (1587), `Objectives.java` (155), `MapMarkers.java` (128), `AttackIndicators.java` (66), `EventType.java` (campaign/rules/marker events + 41 triggers; 05 owns the enum), `core/GameState.java` (122), `core/Control.java` (`playMap`/`playSector`/`playNewSector`/`checkAutoUnlocks`/`AttackIndicators` wiring, 396–592), `core/Logic.java` (`play`/`reset`/`runWave`/`checkGameState`/`sectorCapture`/`updateGameOver`/`gameOver`/`researched`, 269–453), `net/WorldReloader.java` (61), `type/Planet.java` runtime half (240–430), `type/Sector.java` (252), `type/SectorPreset.java` (128), `content/TechTree.java` (204), `ui/dialogs/ResearchDialog.java` spend/unlock flow (551–625), `Vars.java` constants (`turnDuration=2*Time.toMinutes`, `baseInvasionChance=1/100`, `invasionGracePeriod=20`, `maxSchematicSize=64`, `maxLoadoutSchematicPad=5`, `saveExtension=msav`, `schematicExtension=msch`), `tests/src/test/java/ApplicationTests.java` (`writeRules`, `writeRules2`, `playMap`, `save`, `saveLoad`, `testSectorValidity`, `initBuilding`, legacy save loads). |
| **Extends spine** | Replaces 05's placeholder `Rules`/`Teams` with the full structs (explicit swap §3.1) and wires campaign scheduling/events. `mind-headless` gains `campaign rules│sector│turn│schematic│tech│objectives│fog│bench-campaign`. State inspector gains `Campaign`, `Teams`, `Objectives` tabs. GDExtension autoload `MindCampaign` (§3.9) exposes save/load, sector start, rules JSON, research, schematic place, and fog queries for MCP. |

**Locked inputs treated as constants:** GPL-3.0 (D6); pure Rust/GDExtension (D1); fixed 60 Hz sim (D8); full parity (D3); `mind-core` Godot-free/tokio-free (HLP §2.2); no `HashMap` iteration on sim/checksum paths; content IDs append-only; rules JSON field names camelCase (OD9); server = relay + cheap validation only (D2, 21 may add authority later).

---

## 2. Scope & parity definition

### 2.1 In scope

1. **`Rules` (full).** Every upstream field, `TeamRule`, `TeamRules`, `copy`, `retainContentFields`, `mode`, `isBanned`, `hasEnv`, all per-team multiplier accessors, `buildRadius`, `unitActivationDelay`, `isInfiniteResources`, JSON round-trip via plan 04, and `RulesLoadEvent`/reset semantics. Replaces the minimal 05 boundary in `mind-core/src/game/rules.rs` (state the swap in the changelog of 05).
2. **`Gamemode` presets.** `survival`/`sandbox`/`attack`/`pvp`/`editor` with exact `apply(Rules)` table, `valid(Map)` (map spawns/teams), `hidden`, and mode detection `Rules::mode()`.
3. **`Team`/`Teams`/`TeamData`.** The 256-team registry with deterministic placeholder palettes, all `Teams` queries, `TeamData` caches (`cores`, `buildings`, `units`, `players`, `unitsByType`, `typeCounts`, `unitCount`, `unitCap`, `plans`, `buildingTree`/`turretTree`/`unitTree`, `buildingTypes`, `coreEnemies`), `updateTeamStats`, `registerCore`/`unregisterCore`, `updateEnemies`, `destroyToDerelict`/`timeDestroy`/derelict scheduling, `BlockPlan`, `hasAI`, `getClustered`, and the `TeamInventory` accessor (08 R2 default).
4. **Campaign runtime.** `Universe` (seconds/turns/launch resources/loadouts/production/imports/invasions/save cadence), `Planet` runtime half (positions, campaign rules/stats persistence, `updateBaseCoverage`, last sector, `applyRules`), `Sector` runtime half (save/info/light/threat/items/`isCaptured`/`hasBase`/`isAttacked`), `SectorInfo` (`prepare()`/`write()`/`update()`/`refreshImportRates`/`ExportStat`), `Saves` policy (slots/autosave/sector saves/remap/mod checks; file format in 04), `CampaignRules`/`Difficulty`/`CampaignStats`/`GameStats`.
5. **`Schematic`/`Schematics`.** `.msch` byte-parity read/write, base64 (`readBase64`/`writeBase64`, always starts `bXNjaAB`), creation from a world selection, placement (`place`, `placeLoadout`, `placeLaunchLoadout`), loadout validation/caching, rotation, tags/labels, `contentMap` remap, legacy v0 config mapping.
6. **Tech-tree runtime.** `TechNode` `finishedRequirements`/`save`/`reset`, research spend + unlock flow (`spend`/`unlock` semantics from `ResearchDialog`, state mutation only), `checkAutoUnlocks`, persistence via plan-04 `UnlockStore` + MP `Rules.researched`, `ResearchEvent`/`UnlockEvent`, and the `Objectives` requirement classes (`Research`, `Produce`, `SectorComplete`, `OnSector`, `OnPlanet`). `SectorComplete` auto-insertion is performed by 02's `TechNode` constructor (see §3.4); 12 owns the requirement evaluation.
7. **Objectives & markers.** `MapObjectives` executor (all 13 objective types), `MapMarkers` containers, all 8 marker types + `TextureHolder` + legacy `Minimap` alias, class-tag registration through 04, completion remote equivalent (`complete_objective` → 21 relay), `clear`/`clearObjectives`, `ObjectiveMarker::control` for the logic API (13), `fetchText`/`setText`/localization hooks.
8. **FogControl.** Per-team static exploration + dynamic visibility bits, discovery API, deterministic (tick-quantized, thread-free) update model, `forceUpdate`, reset/stop, and the registered `static-fog-data` save custom chunk (04 container).
9. **AttackIndicators.** Packed indicator ring with dedupe, 15 s lifetime, `add`/`update`/`clear`; consumed by the minimap (16).
10. **Start/play flows.** `play_map`, `play_sector`, `play_new_sector`, `WorldReloader` semantics, `Logic::play`/`reset` campaign halves, `run_wave` campaign difficulty scaling, sector capture/loss/campaign game-over, `checkGameState` campaign branch.
11. **Server-authoritative rules edits.** `SimCommand::SetRules` handling + `set_rules` relay contract (21), campaign-only guards, and the read-only `rules_epoch` used by 21/23.

### 2.2 “Done” means

- `cargo test -p mind-core` passes every §7a test headless with no Godot/network.
- `mind-headless run rules_roundtrip`, `run sector_save_load_turn --planet serpulo --sector <n>`, `run schematic_place`, `run tech_unlock_gating`, `run fog_reveal`, `run campaign_sector_cycle` pass with committed goldens.
- A generated Serpulo sector can be launched, played 600 ticks, saved as `sector-serpulo-<id>.msav`, process-reset, reloaded with identical checksum, and a production turn can be run between two owned sectors with import/export values matching the golden.
- The MCP scenario §7c runs end-to-end: start sector → research an unlock → capture + save → verify via inspector state and screenshot.
- `mind-core` stays Godot-free/tokio-free; every ported file carries the GPL header; §7d budgets are met.

### 2.3 Explicit boundaries (who owns what)

| Area | This plan (12) owns | Deferred to |
|---|---|---|
| Content definitions (`PlanetDef`/`SectorPresetDef`/`Planet`/`Sector` **content-time** fields, `TechTree`/`TechNode` graph construction, `Loadouts` raw base64, `Rules` JSON schema shape) | runtime halves only; consumes 02 records by ID | `02_CONTENT_IMPLEMENTATION_PLAN.md` |
| Save container/format/`TypeIO`/`JsonIO`/revisions/`SaveSlot` files/preview paths/settings store | policy on top (`Saves`, `SectorInfo::prepare/write`, marker/fog custom-chunk payloads) | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| `Rules`/`Teams` **minimal boundary** already shipped in 05 | full merge (`game/rules.rs`, `game/teams.rs`) — ownership transferred (see §3.1) | this plan replaces them |
| `Tiles`/`WorldGrid`/generators/`Maps`/`Map` metadata | sector/planet selection logic only | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` |
| Block behavior, placement validity, `ConstructBlock`, building IO | placement calls via `Build`/`Tile`; `SectorInfo`/`BlockPlan` data | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` |
| `CoreBlock`/`StorageBlock`/`Unloader` behavior, `ItemModule`, payloads | `TeamInventory` accessors + campaign hooks (`CoreCampaignHooks` impl) | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` |
| `QuadTree`, `WindowedMean`, power/lighting internals | consumes `world::spatial::quad_tree` and `WindowedMean` (09 R4/§6.3) | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` |
| Damage/Rules damage accessors consumers | provides `Rules::{unit_damage, block_damage, …}` | `10`, `11` consume |
| Wave table/generation/spawner, unit caps enforcement, `TeamData` AI handles | `Rules.spawns` type, `TeamData.{build_ai,rts_ai}` fields, cap inputs | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| `LMarkerControl` enum, `LExecutor.runLogicScript`, `GlobalVars` flag access, `Senseable` impls | `objectiveFlags`, marker `control` semantics, completion logic hook call | `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` |
| All dialogs (research, planet map, custom rules, schematics, game-over, saves) | data + mutation APIs + events | `14_UI_IMPLEMENTATION_PLAN.md` |
| Input/placement/selection/camera | `Schematics.create` reads finalized world state; `place` is the shared mutation | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` |
| Rendering (fog, markers, minimap pings, planet g3d, schematic previews) | data/read APIs + `FogEvent` stream | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Map editor, editor objectives/waves dialogs, map locales, preview pixels/PNG | `MapObjectives`/`MapMarkers` data used by editor | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| JSON mod parsing/patching (`DataPatcher`, `contentMap` from mods) | `.msch` `contentMap` fallback resolution via 02/20 | `20_MODS_IMPLEMENTATION_PLAN.md` |
| STDB schema, reducers, views, ordered relay, desync/snapshots | command/event shapes + relay contract | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Dedicated-server save policy/export presets/achievements | `Saves` engine hooks | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` |
| Golden/bench/CI registry | contributes scenarios/goldens | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |

### 2.4 Deliberate deviations (reason stated)

| # | Deviation | Reason |
|---|---|---|
| 1 | **`Rules::copy` is `Clone`**, not a JSON round-trip; `copy`/`retainContentFields` keep exact semantics. | Plan 04 §2.3.8; avoids per-edit alloc churn. JSON remains the persistence format. |
| 2 | **FogControl runs in the sim tick, not on background threads.** Static events are processed in queue order each tick; dynamic flushes are quantized to `DYNAMIC_UPDATE_INTERVAL_TICKS = 40` (1000/25 ms at 60 tps). | HLP §2.4 (no wall-clock, deterministic lockstep); behavior (25 Hz visibility, event semantics) preserved. Threads/handles are not portable and not replayable. |
| 3 | **`Call.*` remotes become host-side functions + events; 21 relays the intent.** `sectorCapture`, `completeObjective`, `clearObjectives`, `setRules`, `researched`, `destroyPayload`, `gameOver`/`updateGameOver`, `createWeather` are `SimCommand`s/events, not packets. | D2; preserves the authoritative-sim seam. Cheap validation lives in 21. |
| 4 | **`MapObjectives` class tags use 04's `ClassTagRegistry`** (`Strings::camelize` + exact class name), unknown tags warn + skip the element, lowercase legacy tag yields an empty executor (04 §2.3.6). | Plan 04 owns JSON; strictly more resilient, same visible behavior for valid maps. |
| 5 | **`Saves`/`SectorInfo` never touch the filesystem directly** — all IO through 04's `FileSystem`/`SaveIo`/`SaveSlot`. Preview textures (`SavePreviewLoader`) are 04 paths + 19 pixels; `mind-core` exposes only `PreviewRequest`s. | D1/HLP §2.2; testability with `MockFs`. |
| 6 | **Schematic preview rendering (`getBuffer`/`savePreview`/`getPreview`) is 16/19.** `mind-core` keeps the `Schematic`, the read/write/rotate/place logic, and a `SchematicPreviewSpec` (tiles → plans) consumed by 19. | Godot `FrameBuffer`/`Texture2D` are render state; D1. |
| 7 | **`Team` placeholder palettes are deterministic but must not claim Java bit parity.** The seed sequence (`Mathf.rand.setSeed(8)` + 3 warm-up draws + HSV generation) is ported through 05's `JavaRandom`/`Rand`; the golden is captured from the Rust build once (plan 23). | HLP §9 (Rust↔Rust determinism only); palette names/indices are ABI. |
| 8 | **`TeamData` quadtrees are `Vec`-slab `QuadTree` values supplied by 09** (`world::spatial::quad_tree`), not Arc `QuadTree` with comparator objects. Iteration order is insertion-stable (09 R4). | 09's port is the shared implementation; avoids duplicate semantics. |
| 9 | **`universe.updateGlobal()` replaces the 05 `UniverseGlobal` stub system**, running planet positions every tick (even in menu), exactly as upstream. | 05 §3.4 slot 2 reserved for 12. |
| 10 | **`Rules` participates in the sim checksum** through `Rules::checksum_part()` (canonical, sorted field hash). Upstream has no `Rules` checksum. | Desync diagnosis needs config state; `CHECKSUM_VERSION` bump is joint with 05/23 (§8 R8). |
| 11 | **Autosave/playtime cadence is a `Saves::update_policy` system** gated on `!headless || platform.allows_autosave()`, tick-driven (`saveinterval * 60 * 60` ticks, not `Time.delta`). | 05 has no wall clock in sim; upstream uses frame delta. Same interval in practice. |
| 12 | **`research` spend consumes the active sector’s core inventory**, mirroring `ResearchDialog.spend`; UI stays in 14. `TechNode.save()` writes only changed `req-<content>-<item>` settings. | Upstream; keeps UI/state split. |

---

## 3. Target design

All names are final unless marked. `mind-core` is Godot-free/tokio-free; `bevy_ecs` pinned by 00. No `HashMap` iteration in sim/checksum paths; ordered `IndexMap`/`BTreeMap`/`Vec` only. Content is referenced by typed IDs (02), never by pointer.

### 3.1 Module layout

```
client/rust/mind-core/src/game/
  mod.rs                    # GamePlugin (systems/events/resources registration), re-exports
  rules.rs                  # FULL Rules/TeamRule/TeamRules; accessors; checksum_part; serde (04 shims)
  gamemode.rs               # Gamemode presets + valid() + mode()
  team.rs                   # Team registry (all[256]/baseTeams[6]), palette seed parity, Senseable (13)
  teams.rs                  # Teams resource, TeamData, BlockPlan, TeamInventory, update_team_stats/update_enemies
  campaign_rules.rs         # CampaignRules, Difficulty
  stats.rs                  # GameStats, CampaignStats
  universe.rs               # Universe resource, TurnEvent/SectorInvasionEvent staging, production/import, launch
  planet.rs                 # Planet runtime: position/children, campaignRules/stats persistence, lastSector,
                            #   updateBaseCoverage, applyRules/applyDefaultRules, preset() wiring
  sector.rs                 # Sector runtime half, SectorInfo, ExportStat, prepare/write/update
  saves.rs                  # Saves resource: slots policy, autosave, sector saves, remap, mod checks
  schematic.rs              # Schematic, Stile, value ops (requirements/power/core), preview spec
  schematics.rs             # Schematics resource: .msch IO, base64, load/loadouts/place/rotate/create
  tech_tree.rs              # TechNode runtime (finishedRequirements/spend/unlock/save/reset), check_auto_unlocks
  objectives.rs             # Objectives::Objective requirement types (Research/Produce/SectorComplete/OnSector/OnPlanet)
  map_objectives.rs         # MapObjectives executor + 13 objective types + registries + completion
  map_markers.rs            # MapMarkers + marker registry + 8 markers + TextureHolder + MarkerSetter logic
  fog.rs                    # FogControl, FogData, FogEvent packing, static-fog-data CustomChunk
  attack_indicators.rs      # AttackIndicators
  play.rs                   # play_map/play_sector/play_new_sector/check_game_state/sector_capture/game_over
  world_reloader.rs         # WorldReloader host/client semantics (relay trait impl'd by 21)
  rules_event.rs            # RulesLoadEvent handling/application + rules_epoch
client/rust/mind-core/src/entities/comp/building.rs  # (07 owns) TeamData hooks only via public fns
client/rust/mind-core/assets/                         # nothing new (content stays in 02)
```

`GamePlugin` registration (one plugin; 05 never edited again — HLP §2.2):

```rust
impl Plugin for GamePlugin {
    fn build(&self, sim: &mut SimBuilder) {
        sim.insert_resource(Rules::default());          // replaces 05 stub resource
        sim.insert_resource(Teams::new());              // replaces 05 stub resource
        sim.insert_resource(Universe::new());
        sim.insert_resource(Saves::new());
        sim.insert_resource(Schematics::new());
        sim.insert_resource(FogControl::new());
        sim.insert_resource(AttackIndicators::new());
        sim.register_event::<RulesLoadEvent>();
        sim.register_event::<PlayEvent>();
        sim.register_event::<WaveEvent>();
        sim.register_event::<TurnEvent>();
        sim.register_event::<SectorLaunchEvent>();
        sim.register_event::<SectorInvasionEvent>();
        sim.register_event::<SectorCaptureEvent>();
        sim.register_event::<SectorLoseEvent>();
        sim.register_event::<GameOverEvent>();
        sim.register_event::<ResearchEvent>();
        sim.register_event::<UnlockEvent>();
        sim.register_event::<SaveLoadEvent>();
        sim.register_event::<SaveWriteEvent>();
        sim.register_event::<SchematicCreateEvent>();
        // systems: see §3.2
    }
}
```

### 3.2 Schedule slots (exact anchors into 05's schedule)

| 05 slot | Systems registered by 12 | Run condition |
|---|---|---|
| `TickSet::UniverseGlobal` | `universe::update_global` (planet positions; replaces the 05 stub) | always |
| `TickSet::Frame` | `saves::update_policy` (autosave/playtime; client/dedicated policy) | `platform.autosave_allowed()` |
| `TickSet::StateClock` | `sector::update_sector_info` dispatch (`SectorInfo::update`, 60-tick refresh) | `is_game && !paused && !client` |
| `TickSet::TeamStats` | `teams::update_team_stats` (rebuilds `present`/caches/quadtrees/`bosses`), then `teams::update_enemies` if dirty | `is_game && !paused` |
| `TickSet::Fog` | `fog::update` (static queue drain + dynamic flush + discovery writes) | `is_game && !paused && rules.fog` |
| `TickSet::Campaign` | `universe::update` (seconds/turn/lighting) | `is_game && !paused && !client` |
| `TickSet::Objectives` | `map_objectives::update` (collect completions; host applies, client waits for relay) | `is_game && !editor` |
| `TickSet::RunWave` | (11 spawner); 12 listens to `WaveEvent` for `SectorInfo`/hiscore | `rules.waves` |
| `TickSet::AfterGameUpdate` | `attack_indicators::update` | always in game |
| `TickSet::GameStateCheck` | `play::check_game_state` (campaign win/capture/game-over/attack-mode branches) | host, `!editor`, `rules.can_game_over` |
| `IoSet::Apply` (04) | `rules_event::apply_from_save` (parse rules JSON → `Rules`, retain fields, fire `RulesLoadEvent`) | queued loads |
| `IoSet::Capture` (04) | `sector::prepare_for_write` (SaveVersion hook), `saves::on_save_written` | save writing |

Ordering constraints (assert in `trace order` golden): `TeamStats` before `Fog`; `Campaign` before `Objectives`; `GameStateCheck` after `RunWave` and `EntityUpdate`; `rules_event` before `play` on load.

### 3.3 `Rules` full struct (merge of the 05 boundary)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rules {
    // --- mode / core ---
    pub allow_edit_rules: bool,
    pub infinite_resources: bool,
    pub core_build_and_config: bool,
    pub teams: TeamRules,
    pub wave_timer: bool,
    pub wave_sending: bool,
    pub waves: bool,
    pub air_use_spawns: bool,
    pub waves_spawn_at_cores: bool,
    pub pvp: bool,
    pub pvp_auto_pause: bool,
    pub pause_disabled: bool,
    pub wait_enemies: bool,
    pub attack_mode: bool,
    pub editor: bool,
    pub derelict_repair: bool,
    pub can_game_over: bool,
    pub core_capture: bool,
    pub reactor_explosions: bool,
    pub possession_allowed: bool,
    pub schematics_allowed: bool,
    pub damage_explosions: bool,
    pub fire: bool,
    pub random_wave_ai: bool,
    pub unit_payload_update: bool,
    pub unit_payloads_explode: bool,
    pub unit_cap_variable: bool,
    pub hide_spawns: bool,
    // --- multipliers / balance ---
    pub solar_multiplier: f32,
    pub unit_build_speed_multiplier: f32,
    pub unit_cost_multiplier: f32,
    pub unit_damage_multiplier: f32,
    pub unit_health_multiplier: f32,
    pub unit_crash_damage_multiplier: f32,
    pub unit_mine_speed_multiplier: f32,
    pub unit_factory_activation_delay: f32,
    pub ghost_blocks: bool,
    pub show_other_team_pings: bool,
    pub logic_unit_control: bool,
    pub logic_unit_build: bool,
    pub logic_unit_deconstruct: bool,
    pub world_processor_player_link: bool,
    pub allow_edit_world_processors: bool,
    pub disable_world_processors: bool,
    pub block_health_multiplier: f32,
    pub block_damage_multiplier: f32,
    pub build_cost_multiplier: f32,
    pub build_speed_multiplier: f32,
    pub deconstruct_refund_multiplier: f32,
    pub objective_timer_multiplier: f32,
    pub enemy_core_build_radius: f32,
    pub polygon_core_protection: bool,
    pub place_range_check: bool,
    pub cleanup_dead_teams: bool,
    pub only_deposit_core: bool,
    pub allow_core_unloaders: bool,
    pub item_deposit_cooldown: f32,
    pub core_destroy_clear: bool,
    pub hide_banned_blocks: bool,
    pub allow_environment_deconstruct: bool,
    pub instant_build: bool,
    pub block_whitelist: bool,
    pub unit_whitelist: bool,
    pub drop_zone_radius: f32,
    pub wave_spacing: f32,
    pub initial_wave_spacing: f32,
    pub win_wave: i32,
    pub unit_cap: i32,
    pub disable_unit_cap: bool,
    pub drag_multiplier: f32,
    // --- env / planet / campaign ---
    pub env: i32,
    pub attributes: Attributes,                 // 06
    pub sector: Option<SectorRef>,              // runtime handle resolved via Planet/Sector registry
    pub ambient_music: Option<Vec<MusicContainerRef>>,   // audio names resolved by 18
    pub dark_music: Option<Vec<MusicContainerRef>>,
    pub always_play_music: bool,
    pub disable_music: bool,
    pub music_volume: f32,
    pub spawns: Vec<SpawnGroup>,                // 11 type
    pub loadout: Vec<ItemStack>,                // 02 type; default copper x100
    pub weather: Vec<WeatherEntry>,             // 02 type
    pub block_limits: IndexMap<BlockId, i32>,
    pub banned_blocks: IndexSet<BlockId>,
    pub banned_units: IndexSet<UnitTypeId>,
    pub revealed_blocks: IndexSet<BlockId>,
    pub researched: IndexSet<UnlockableRef>,    // MP campaign unlocks
    pub objectives: MapObjectives,
    pub objective_flags: IndexSet<String>,
    pub fog: bool,
    pub static_fog: bool,
    pub static_color: Rgba,
    pub dynamic_color: Rgba,
    pub lighting: bool,
    pub ambient_light: Rgba,
    pub unit_light: bool,
    pub default_team: TeamId,
    pub wave_team: TeamId,
    pub cloud_color: Rgba,
    pub mode_name: Option<String>,
    pub mission: Option<String>,
    pub core_incinerates: bool,
    pub border_darkness: bool,
    pub limit_map_area: bool,
    pub limit_x: i32, pub limit_y: i32, pub limit_width: i32, pub limit_height: i32,
    pub disable_outside_area: bool,
    pub tags: IndexMap<String, String>,
    pub custom_background_callback: Option<String>,
    pub background_texture: Option<String>,
    pub background_speed: f32,
    pub background_scl: f32,
    pub background_offset_x: f32, pub background_offset_y: f32,
    pub planet_background: Option<PlanetParamsRef>,   // 16 data
    pub planet: PlanetId,
    pub allow_logic_data: bool,
}
```

- `Rules::copy() = self.clone()`; `retain_content_fields(&mut self, source: &Rules)` ports the three forced fields (`spawns`, `objectives`, `weather`) and the patch-content conditionals for `banned_blocks`/`banned_units`/`loadout` (02's `is_patch_content`).
- `mode()` ports the if-chain (`pvp → editor → attack_mode → infinite_resources → survival`).
- Accessors: `build_radius(team)`, `unit_build_speed/unit_cost/unit_damage/unit_health/unit_crash_damage/unit_mine_speed/block_health/block_damage/build_speed(team)`, `unit_activation_delay(team)`, `is_infinite_resources(team)`, `is_banned(BlockId|UnitTypeId)` honoring the whitelist flags, `has_env`.
- `TeamRule`/`TeamRules`: array `[Option<TeamRule>; 256]` with lazy `get(team)` (derelict defaults `protect_cores=false`, `check_placement=false`), JSON written with team-id keys in ascending order exactly like `TeamRules.write/read`.
- `RulesLoadEvent { rules_epoch: u32, from_save: bool }`: fired (a) by 04 after non-map saves per 04 §3.2 and (b) by `play_map`/`play_sector`/`play_new_sector`; listeners fill `GameState`, `TeamRule`s, fog reset, block visibility caches.

### 3.4 `Gamemode`, `Team`, `Teams`/`TeamData`

- `Gamemode` enum + `apply(&mut Rules)` table (survival: `wave_timer/waves`; sandbox: `infinite_resources/allow_edit_rules/waves/wave_timer=false`; attack: `attack_mode/wave_timer`, `wave_spacing = 2 * TIME_TO_MINUTES`, `wave_team.rules().infinite_resources = true`; pvp: `pvp/enemy_core_build_radius=600/build_cost_multiplier=1/build_speed_multiplier=1/unit_build_speed_multiplier=2/attack_mode`; editor: hidden, `infinite_resources/instant_build/editor/waves=false/wave_timer=false`). `valid(map)` = `spawns > 0` / `teams.len > 1`.
- `Team`: `[Team; 256]` in a `OnceLock` table (or resource-adjacent const-built registry), `base_teams[6]`, `TeamId(u8)`, `Team::get(id) = all[(id as i8) as u8 as usize]` mask semantics, `rules()`, `data()`, `core()`, `items()`, `is_ai()`, `is_only_ai()`, `needs_flow_field()`, `activate_unit_factories()`, `cores()`, `localized()`, `colored_name()`, `set_palette`, `Senseable` impl for 13 (`id`, `color`, `name`). Placeholder palette loop uses `JavaRandom` with `set_seed(8)`, three warm-up `random()` draws, then per team `hsv_to_rgb(360*rand, 100*rand(0.4,1), 100*rand(0.6,1))`; afterward reseed from a fresh `Rand::next_long()` (same discard semantics).
- `Teams` resource: `map: Box<[Option<TeamData>; 256]>` (or `Vec<Option<TeamData>>`), `active`, `present`, `bosses`; queries `closest_enemy_core`, `closest_core`, `any_enemy_cores_within_build_radius`, `any_enemy_cores_within`, `each_enemy_core`, `get`, `get_or_null`, `player_cores`, `cores(team)`, `is_active`, `can_interact`, `get_active` (prunes inactive), `update_active`, `register_core`/`unregister_core`, `update_team_stats`, `update_enemies`. `Teams::new()` pre-adds `crux`.
- `TeamData`: exact field set from source (§6.3), with `build_ai: Option<BaseBuilderAI>` (11), `rts_ai: Option<RtsAI>` (11), `plans: VecDeque<BlockPlan>`, `building_tree`/`turret_tree`/`unit_tree: Option<QuadTree<Entity>>` (09's `world::spatial::quad_tree`), `unit_cap`, `unit_count`, `type_counts`, `building_types: IndexMap<BlockId, Vec<Entity>>`, `units`, `players`, `buildings`, `units_by_type: Vec<Vec<Entity>>`, `core_enemies`, `present_flag`, `clustered_counts`, `last_cluster_update_timer`. Methods `get_buildings`, `get_count`, `destroy_to_derelict`, `time_destroy`, `schedule_derelict`, `finish_schedule_derelict`, `unit_cache`, `update_count`, `tree()`, `count_type`, `active`, `has_core`, `is_alive`, `no_cores`, `core`, `has_ai`, `get_clustered` (70 px cluster key, 10-tick refresh). `BlockPlan { x: i16, y: i16, rotation: i8, block: BlockId, config: ConfigValue, removed: bool }`.
- `TeamInventory` (08 R2 default): `Teams` owns `inventories: IndexMap<TeamId, ItemModule>`; accessors `inventory(team)`, `inventory_mut(team)`, `Team::items()` delegates; `CoreBuild`/`StorageBuild` route through it; `destroy_to_derelict` clears the team inventory when no cores remain.
- `update_team_stats` port order: clear `present`/`bosses`; per team reset flags/caches/`units`/`players`; keep `last_core`; clear `unit_tree`/`type_counts`/`units_by_type`; insert every `Groups.unit` into `tree()` + `units` + `units_by_type[type]`, mark wave-team bosses; recursive payload unit counting; players; append present teams (`present_flag || active`). `update_enemies` rebuilds `core_enemies` for `active` and re-adds the wave team when `rules.waves`.

### 3.5 Campaign state

- `Universe` resource: `seconds`/`net_seconds`/`second_counter`/`turn`/`turn_counter` (all loaded from 04 `SettingsStore` keys `utimei`/`turn`), `last_loadout: Option<SchematicHandle>`, `last_launch_resources: ItemSeq`. `update_global` positions parentless planets recursively (`position` = sum of parent offsets); `update` advances counters (host only), auto-runs a turn at `TURN_DURATION = 7200` ticks, saves every 10 s, and applies sector lighting (`ambient_light.a = 1 - alpha`, `lighting = alpha != 1`) via `Sector::get_light`; `run_turn` ports the three passes: legacy launch-pad import/export means (`ExportStat.mean * seconds_passed`), `minutes_captured`/frozen-attacked sectors, production with `storage_capacity` cap, negative clamp, `save_info`, invasion queueing (`SectorInvasionEvent`, `win_wave = max(winWave, wave) + rand(2,4)*5`, live rules update + `SetRules` host call), `TurnEvent`, `save`. `get_launch_resources`/`update_launch_resources`/`clear_loadout_info`/`update_loadout`/`get_last_loadout`/`get_loadout(core)` port the settings-key scheme (`launch-resources-seq`, `lastloadout-<core>`).
- `CampaignRules` fields + `apply(planet, rules)` exact port (fog/static_fog, hide_spawns, random_wave_ai, pause_disabled, `objective_timer_multiplier`, `TeamRule.{rts_ai,rts_max_squad,block/unit health/unit cost/build speed}`; controller reset when `rts_ai` toggles and `state.is_game()`). `Difficulty` enum table (`casual/easy/normal/hard/eradication`) + `info()`/`localized()`/percent formatting.
- `Planet` runtime (`planet.rs`): `position`/`children`/`total_radius`/`get_orbit_angle`/`get_rotation` (uses `universe.secondsf()` + `Mathf.randomSeed(id+1, 360)` via 05 `JavaRandom`)/`get_world_position`; `save_rules`/`load_rules` (`<name>-campaign-rules`), `stats`/`load_stats`/`save_stats`/`clear_stats` (`<name>-campaign-stats`, `statParent` delegation); `get_start_sector`/`get_last_sector`/`set_last_sector` (`<name>-last-sector`); `preset(index, preset)`; `apply_rules(rules, custom_game)`/`apply_default_rules`; `update_base_coverage` (threat formula); `has_grid`/`is_landable`.
- `Sector` runtime (`sector.rs`): `save: Option<SaveSlotHandle>`, `preset`, `shield_target`, `info: SectorInfo`, `threat`, `generate_enemy_base`; `near()`/`is_near` (SectorRect adjacency), `display_threat`, `unlocked()`, `allow_launch_schematics/loadout`, `save_info`/`load_info`/`clear_info` (settings `<planet>-s-<id>-info`), `is_shielded`/`is_attacked`/`has_base`/`is_frozen`/`has_enemy_base`/`is_being_played`, `name`/`set_name`, `is_captured`, `has_save`, `locked`, `get_light`, `get_size`, `remove_items`/`remove_item`/`add_items`/`items()`, `sector_data_matches`.
- `SectorInfo`: all fields + `prepare(sector)`, `write()`, `update()` (60-tick refresh; caps production by raw production, export by raw + core deltas, imports by import rate), `handle_core_item`/`handle_production`/`handle_item_export`/`handle_item_import`/`get_export`/`has_export`, `refresh_import_rates`/`get_import_rates`/`get_import_rate`, `export_rates`/`any_exports`/`import_stats`/`each_import`, `ExportStat { counter, means: WindowedMean, loaded, mean }` (WindowedMean from 09).
- `Saves` resource: `Vec<SaveSlot>` (04 type), `current`, `last_sector_save`, `saving`, `time`, `total_playtime`, `last_timestamp`; `load()` (parallel meta reads through 04; legacy megabase cleanup port; sector remap two-phase algorithm; `hadSerpuloRemaps` flag); `get_last_sector`/`get_current`/`update` (autosave policy §3.2)/`get_total_playtime`/`reset_save`/`is_saving`/`get_sector_file`/`save_sector`/`add_save`/`import_save`/`get_next_slot_file`/`get_save_slots`/`delete_all`; `StateChangeEvent(to = menu)` clears current/playtime. Mod checks (`cautious_load`) return a `MissingModsReport` consumed by 14.
- `GameStats`/`CampaignStats`: exact field sets; objective consumers read `state.stats`; `CampaignStats` maps keyed by `ItemId`/`BlockId`/`UnitTypeId`; `Planet::stats()` picks `stat_parent` when set.

### 3.6 Schematics

- `Schematic { tiles: Vec<Stile>, labels: Vec<String>, tags: IndexMap<String, String>, width: i32, height: i32, file: Option<Fixture>, mod: Option<ModId> }`; `Stile { block: BlockId, x: i16, y: i16, config: ConfigValue, rotation: i8 }`. Value ops `power_production` (02/09 `PowerGenerator` metadata), `power_consumption`, `requirements() -> ItemSeq`, `has_core`, `find_core`, `name`, `description`, `save`/`add_steam_id`/`remove_steam_id` (22 stubs), `compare_to`.
- `Schematics` resource: `all: Vec<Schematic>`, `loadouts: IndexMap<BlockId, Vec<SchematicHandle>>`, `default_loadouts: IndexMap<BlockId, SchematicHandle>`, `errored: IndexSet<u32>`, rotate scratch buffers. API port: `load()` (data dir via 04 FS + mod files via 20 + workshop stub 22), `load_loadouts`, `load_file`, `overwrite`, `all`, `save_changes`, `to_plans(schem, x, y, check_hidden)` (centered, filter invisible/un-unlocked/non-core, sort by `-schematic_priority`), `check_loadout` (core exists; ≤1 core; width/height ≤ `get_max_launch_size`; no `sandboxOnly`; all unlocked), `get_max_launch_size = core.size + MAX_LOADOUT_SCHEMATIC_PAD*2`, `find_file` (sanitize + `_n` suffix), `add`, `remove`, `create(x,y,x2,y2)` (visible/discovered build scan, multiblock extents, `ConstructBlock.current` unwrap, `lastConfig`), `write_base64`/`read_base64`, `write`/`read` (byte parity §6.4), `map_config` (legacy v0), `rotate`/`rotated` (config `point_config` + `Mathf.mod` rotation), and static `place_launch_loadout`/`place_loadout`/`place`. Placement defers to 06 tile ops and 07 `configure_any`/`register_core`.
- Rendering (`get_buffer`/`save_preview`/`get_preview`/`has_preview`) is replaced by `SchematicPreviewSpec { plans: Vec<BuildPlan>, width, height }` (19 owns pixels/textures; 07 `draw_plan_region` produces `DrawCommands`).

### 3.7 Tech tree runtime & objectives

- `tech_tree.rs`: `TechStore` (owned by 02's `ContentRegistry`) gains runtime methods: `node_state(TechNodeRef)`, `spend(node, sector_items: &mut ItemModule) -> SpendResult` (port of `ResearchDialog.spend`: take `min(req-completed, available)`, mark complete, then `unlock(node)`), `unlock(node)` (unlocks content + all parents for MP, fires `ResearchEvent`, sets `content.unlocked`), `node.save()` (`req-<content>-<item>` deltas only), `reset()`, `check_auto_unlocks()` (port of `Control.checkAutoUnlocks`: no parent-unlocked, zero requirements, no incomplete objectives → `content.unlock()`), `selectable/locked/can_spend`, `completed_requirements`.
- Persistence split: single-player uses 02's `UnlockStore` (`<name>-unlocked` booleans + `req-` ints); MP uses `Rules.researched` (host-authoritative through 21's `research` reducer). `unlocked_host()`/`unlocked_now()` semantics from 02 pick the right source.
- `Objectives` requirement classes: `Research { content }`, `Produce { content }`, `SectorComplete { preset }`, `OnSector { preset }`, `OnPlanet { planet }` with `complete()`/`display()` matching source; `SectorComplete` only evaluates in campaign (`preset.sector.save.is_some() && is_captured() && has_base()`). `TechNode` constructor auto-inserts `SectorComplete` when the parent content is a `SectorPreset` (02 §3.8 `TechTree.java:39-139`); 12 supplies evaluation + display strings via bundle keys.

### 3.8 Objectives & markers

- `MapObjectives` resource: `all: Vec<MapObjective>` (flattened, parents stripped), static registries `ALL_OBJECTIVE_TYPES` (13 constructors) and `ALL_MARKER_TYPES` (8) + `marker_name_to_type` + `all_marker_type_names` + legacy `Minimap` alias. `register_objective`/`register_marker` also register camelized + exact class tags into 04's `ClassTagRegistry`. `update()` iterates `each_running` and, host-side, calls `complete(index)` per true result; clients only run `update()` for timer state (no completion). `add`/`flatten`, `get`, `any`, `clear`, `each_running`, iteration.
- `MapObjective` base: `hidden`, `details`, `completion_logic_code`, `flags_added`/`flags_removed`, `markers`, transient `parents`/`children`, `editor_x/y`, `completed`, `dep_finished`; `update()` (13 concrete impls), `reset()`, `done()` (remove/add flags, `completed = true`, call 13's `LExecutor::run_logic_script`), `dependency_finished()`, `is_completed()`, `qualified()`, builder chain `child`/`parent`/`details`/`flags_added`/`flags_removed`/`markers`, `text()`/`details()`/`type_name()`/`validate()`.
- Objective types (13): `Research`, `Produce`, `Item` (`default_team.items().has`), `CoreItem` (`state.stats.core_item_count`), `BuildCount` (`stats.placed_block_count`), `UnitCount` (`default_team.data().count_type`), `DestroyUnits` (`stats.enemy_units_destroyed`), `Timer` (`Time.delta` accumulation × `objective_timer_multiplier`; localized `@`/map-locale text), `DestroyBlock`/`DestroyBlocks` (tile lookup by pos/team/block with progress), `CommandMode` (headless-true / `control.input.selected_units` hook → 15), `Flag` (`rules.objective_flags`), `DestroyCore` (`wave_team.cores().is_empty()`).
- `MapMarkers`: `world_markers`/`map_markers`/`light_markers` parallel index vectors + `map: IntMap<u32, ObjectiveMarkerHandle>`; `add`/`remove`/`update_marker`/`set_marker`/`remove` index-fixup port; `write`/`read` through 04's `JsonIO` bytes with `-1` sentinel handling; iteration order = insertion order.
- `ObjectiveMarker` base: `world`/`minimap`/`light` index-bools, `autoscale`, `draw_layer` (16 data), `control(LMarkerControl, p1, p2, p3)` (NaN-ignore; delegates to `update_marker`/`autoscale`/`draw_layer`), `set_text`/`set_texture`, `type_name`, JSON field read/write incl. legacy boolean→±1 fixup and `textureName`. `PosMarker` (`pos: Vec2`, `control pos` in tiles). Markers: `ShapeText`, `Point`, `Shape`, `Text`, `Line` (`endPos`, `posi`/`colori` triple ops), `Texture` (region lookup: string region via 03, `Unlockable` full icon, logic display buffer (13), canvas texture (13), else error), `Quad` (`vertices`, UV/pos/color ops), `Light`. Logic side: 13 calls `marker.control(...)` from `LExecutor` (`LMarkerControl` enum owned by 13; reconcile R9).
- Completion relay (21): `complete_objective { index, rules_epoch }`, `clear_objectives`; host functions `complete(index)`/`clear_all()` validate index/epoch, then `done()`. `SectorCaptureEvent` handler clears markers/objectives (`Call.clearObjectives` equivalent).

### 3.9 FogControl, AttackIndicators, play flows, Godot/STDB

- `FogControl` resource: `fog: Box<[Option<FogData>; 256]>`, `static_events: Vec<u64>` (packed `FogEvent`), `dynamic_event_queue`, `unit_event_queue`, `dynamic_events`, `loaded_static`, `just_loaded`, `last_entity_update_index`. `FogData { read: FogBits, write: FogBits, static_data: FogBits, last_dynamic_tick, dynamic_updated }`. API: `get_discovered(team)`, `is_discovered(team, x, y)`, `is_visible(team, x, y)`, `is_visible_tile`, `reset_fog`, `data`, `stop`, `push_static_blocks(initial)`, `push_event`, `force_update`, `update` (static drain in order; dynamic flush when `dynamic_updated && tick - last >= 40`; chunked building visibility refresh — 5 slices/rotating index, exact math; unit event queue rebuilt per present team; `just_loaded` forces immediate dynamic update), `update_dynamic(cleared)`, `circle`/`hline` (midpoint circle + clipped hline into `FogBits`), chunk `write`/`read`/`should_write`. `FogEvent` packing `(x:16 | y:16 | radius:16 | team:8)` preserved. View notification (`renderer.fog.handle_event`) is a `ClientHooks` call (16), never a sim dependency. Save chunk registered in 04's `CustomChunk` registry under name `static-fog-data`.
- `AttackIndicators` resource: `Vec<Indicator>` (packed pos/time) + `IntMap<i32, u32>`; `add(x, y)` resets existing, else push; `update()` advances time and compacts (15 s = 900 ticks); `clear()`; `list()` for the minimap (16). Fed from `BuildDamageEvent` listener (`build.team == local player team`); local player team injected via `ClientHooks`/platform (no global `Vars.player`).
- `play.rs`:
  - `play_map(map, rules, playtest)`: `logic.reset()` → `world.load_map` (06) → `retain_content_fields(old_rules)` → install rules + clear `sector`/`editor` → fire `RulesLoadEvent(from_save=false)` → `logic.play()` → optional create-save (`savecreate`) → `Trigger::new_game`.
  - `play_sector(origin, sector, reloader)`: save current slot; `preset.quiet_unlock()`; `set_last_sector`; branch on `sector.save`/`clearSectorOnLose`; load with `world.make_sector_context(sector)`; no-core path: `play_new_sector`; damaged-core path: spawn default core, `SectorDamage::apply(1)`, snapshot plans/buildings/derelicts, reset, `play_new_sector` with `core_position_override` and a `before_play` that re-derives derelicts/buildings/power/plans (exact port of `Control.playSector` 489–542); normal path fires `RulesLoadEvent(from_save=true)` + `State::playing`; `SaveException` → delete slot, retry once.
  - `play_new_sector(origin, sector, reloader, params, before_play)`: reloader begin → `world.load_sector` (06) → sector fields/`attempts++` → fire `RulesLoadEvent` → `logic.play()` → `saves.save_sector(sector)` → `SectorLaunchEvent` → `Trigger::new_game` → reloader end → `State::playing`.
  - `logic_play_campaign()`: `initial_wave_spacing`/`wave_spacing × difficulty.wave_time_multiplier`, reset `GameStats`, `PlayEvent`, loadout fill per `allow_launch_loadout`/`addStartingItems`, core heal.
  - `check_game_state()`: campaign branch (player cores zero → `GameOverEvent(wave_team)`; no spawns → `rules.waves = false`; win wave reached / attack-mode enemy dead → `attackAfterWaves` handling or `sector_capture()`), non-campaign branch (attack-mode alive count, win wave).
  - `sector_capture()`: `rules.waves = false`; campaign: initial flag, `info.was_captured = true`, `SectorCaptureEvent`, `attack_mode = false`, `disable_world_processors = true`, clear markers + objectives, `save_sector` (host, non-headless client split per 21).
  - `game_over(winner)`/`update_game_over(winner)`: stats/won bookkeeping; restart dialog is 14.
  - `run_wave_campaign()`: wraps 11's spawner + `RunWave` difficulty multiplier + `WaveEvent`.
- `world_reloader.rs`: `pub trait WorldReloader { fn begin(&mut self, sim); fn end(&mut self, sim); }` with a host impl (snapshot players/clear units/`logic.reset`/world-begin relay) and a client impl (disconnect/reset); 21 supplies the relay bodies. `play_sector` accepts `&mut dyn WorldReloader`.
- **Godot (plan-00 rig extension).** New autoload class `MindCampaign` (`/root/Spine/MindCampaign`, Rust) with `#[func]`: `start_sector(planet: GString, sector: i32) -> bool`, `start_map(name: GString) -> bool`, `get_rules_json() -> GString`, `set_rules_json(json: GString) -> bool`, `research(content: GString) -> bool`, `get_tech_state() -> Dictionary`, `complete_objective(index: i64) -> bool`, `write_schematic_selection(x0,y0,x1,y1) -> GString`, `place_schematic_base64(b64: GString, x: i32, y: i32) -> bool`, `list_schematics() -> Array[Dictionary]`, `save_slot(name: GString) -> bool`, `load_slot(name: GString) -> bool`, `get_sector_state() -> Dictionary`, `fog_query(team: i64, x: i32, y: i32) -> Dictionary`, `run_turn() -> bool`, `get_objective_state() -> Array[Dictionary]`. Inspector tabs `/root/Spine/StateInspector/{Campaign,Teams,Objectives}`; no campaign logic in GDScript.
- **STDB (plan 21 owns schema).** This plan fixes the Rust-side shapes/touchpoints only: `SetRules { rules_blob, rules_epoch }` command; `ResearchUnlock { content_name }`; `CompleteObjective { index, rules_epoch }`; `SectorCapture { }`; read models `SectorInfoRow`, `UnlockRow`, `CampaignStatsRow`, `SchematicRow`. Cheap validation in 21: rules JSON ≤ `MAX_RULES_BYTES` (100 000, 04), content names resolvable, index range, `rules_epoch` match, sender owns an active player in the match. No sim authority (D2).

### 3.10 Boundaries & invariants

1. `Rules` is the only match-config object; `GameState.rules` is replaced wholesale (event `RulesLoadEvent`) — never cached across `ResetEvent`/`WorldLoadEvent`.
2. `TeamId` is `u8`; every `Team::get` masks with `& 0xff`; `TeamRules` indexed by id, never hashed.
3. `Universe`/`SectorInfo`/`Saves`/`Schematics` are campaign-only paths; custom maps must not touch them (guarded by `GameState::is_campaign`).
4. All campaign persistence goes through 04's `SettingsStore`/`FileSystem`; no direct `std::fs`, no Godot paths in `mind-core`.
5. No `HashMap` iteration in `teams::update_team_stats`, `fog::update`, `universe::run_turn`, or any checksum path; `IndexMap`/`BTreeMap`/slot `Vec` only.
6. `Rules::retain_content_fields` runs exactly once per play flow, after map content exists and before `RulesLoadEvent`.
7. `MapObjectives` completions are host-decided; clients never mutate objective state except through relayed `complete_idempotent(index)`.
8. `Schematic` placement is `TypeIO`-safe: every `config` must encode through 04's `TypeValue`; placement goes through 06 tile ops + 07 configure.
9. `FogControl` reads sim state only; renderer notifications are `ClientHooks` and never feed back.
10. Every ported file carries the GPL header; team/mode/objective/marker names and bundle keys are ABI.

---

## 4. Port map

### 4.1 `game/*` and core play flows → Rust

| Mindustry source | Rust target | Notes on adaptation |
|---|---|---|
| `game/Rules.java` | `game/rules.rs` | Full struct + `TeamRule`/`TeamRules`; `copy` = `Clone` (dev §2.4.1); `retain_content_fields` takes `&Rules`; `mode()`; accessors; `checksum_part()`. **Replaces the 05 stub** (§3.1). |
| `game/Gamemode.java` | `game/gamemode.rs` | Enum order `survival/sandbox/attack/pvp/editor`; `apply(&mut Rules)`; `valid(&Map)` via `spawns`/`teams`; `hidden`. |
| `game/Team.java` | `game/team.rs` | 256-team table + `base_teams`; placeholder palette seed parity via 05 `JavaRandom`; `Senseable` for 13. |
| `game/Teams.java` | `game/teams.rs` | `Teams`, `TeamData`, `BlockPlan`, `TeamInventory`; quadtrees from 09; `destroyPayload` remote → host fn + 21 relay. |
| `game/Universe.java` | `game/universe.rs` | Resource; settings keys `utimei`/`turn`; `TurnEvent`/`SectorInvasionEvent`; `save` every 10 s; lighting update. |
| `game/Saves.java` | `game/saves.rs` (+ 04 `save/slot.rs`) | Split: 04 owns `SaveSlot` file/meta/preview paths; 12 owns the policy/remap/autosave/mod-check types. |
| `game/Schematic.java` | `game/schematic.rs` | `Schematic`/`Stile`; `Publishable`/Steam hooks stubbed for 22. |
| `game/Schematics.java` | `game/schematics.rs` | `.msch` byte format + base64 + load/loadouts/place/rotate/create; preview rendering replaced by `SchematicPreviewSpec` (19). |
| `game/SectorInfo.java` | `game/sector.rs` (`SectorInfo`, `ExportStat`) | 60-tick refresh; `WindowedMean` from 09; `prepare`/`write`; import/export means. |
| `game/CampaignRules.java` | `game/campaign_rules.rs` | Exact `apply()` incl. `rts_ai` swap → unit controller reset via 11 hook. |
| `game/Difficulty.java` | `game/campaign_rules.rs` | Enum table + `info()` formatting. |
| `game/CampaignStats.java` | `game/stats.rs` | ID-keyed `IndexMap`s; settings JSON key `<planet>-campaign-stats` (04). |
| `game/GameStats.java` | `game/stats.rs` | Exact counters/maps; objective consumers. |
| `game/FogControl.java` | `game/fog.rs` | Threads → deterministic tick model (dev §2.4.2); `FogBits`; `static-fog-data` chunk. |
| `game/MapObjectives.java` | `game/map_objectives.rs` | Executor + 13 objectives; class tags via 04; `Call.completeObjective` → host fn + 21. |
| `game/Objectives.java` | `game/objectives.rs` | Requirement classes; `SectorComplete` evaluation. |
| `game/MapMarkers.java` | `game/map_markers.rs` | Containers + index fixups + 8 markers + `control`. |
| `game/AttackIndicators.java` | `game/attack_indicators.rs` | Packed ring; ticks in sim, consumed by 16 minimap. |
| `game/EventType.java` (campaign classes) | additions to 05's `sim/events.rs` | `RulesLoadEvent` (rules_epoch/from_save), `PlayEvent`, `TurnEvent`, `SectorLaunch/Loadout/Invasion/Capture/Lose`, `GameOverEvent`, `ResearchEvent`, `UnlockEvent`, `SaveLoad/SaveWrite`; `Trigger::new_game` reused. |
| `game/SpawnGroup.java`, `game/Waves.java` | 11 owns; 12 consumes `Rules.spawns` + `Waves::generate` call sites | Campaign sectors assign `rules.spawns` in generators (06) using 11 algorithms. |
| `core/GameState.java` | 05's `sim/state.rs` extended by 12 | 12 adds `stats`, `teams`, `markers`, `wave`, `wavetime`, `game_over`, `after_game_over`, `won`, `enemies`, `is_campaign`/`get_sector`/`get_planet`; `mapLocales` is 19. |
| `core/Control.java` (campaign parts) | `game/play.rs`, `game/saves.rs`, `game/attack_indicators.rs` | `playMap`/`playSector`/`playNewSector`/`checkAutoUnlocks`/indicators wiring; UI/input/authost parts stay 14/15/21. |
| `core/Logic.java` (`play`/`reset`/`runWave`/`checkGameState`/`sectorCapture`/`updateGameOver`/`gameOver`/`researched`) | `game/play.rs` (campaign halves) + 05 `sim/reset.rs` | `Call.*` → host fns + events (dev §2.4.3); `Logic.reset` world half stays 05/06. |
| `net/WorldReloader.java` | `game/world_reloader.rs` | Trait + host/client semantics; relay bodies from 21. |
| `type/Planet.java` (runtime half) | `game/planet.rs` | Positions, campaignRules/stats persistence, lastSector, base coverage, applyRules. |
| `type/Sector.java` | `game/sector.rs` | Runtime half + `SectorRect` projections (06 owns generation rect usage). |
| `type/SectorPreset.java` (runtime half) | `game/sector.rs` + `game/tech_tree.rs` | `initialize` wiring is 02; `quiet_unlock`/`unlocked` runtime here. |
| `content/TechTree.java` (`TechNode` runtime) | `game/tech_tree.rs` | `finishedRequirements`/`save`/`reset` + research spend/unlock; graph/builder in 02. |
| `ui/dialogs/ResearchDialog.java` (`spend`/`unlock`) | `game/tech_tree.rs` | State mutation only; dialog in 14. |
| `content/Planets.java` `sectorCaptureReplacements` | consumed by `teams::time_destroy` | Map data from 02. |
| `io/SaveFileReader` fallback (`modContentNameMap`) | consumed (02/04) | No reimplementation. |
| `io/SaveVersion` custom chunk registry | 04's `add_custom_chunk`; 12 registers `static-fog-data` | Marker/objective payloads travel in the `markers` region (04). |
| `Vars.turnDuration/baseInvasionChance/invasionGracePeriod/maxSchematicSize/…` | `mind_core::constants` additions | Values in §6.6. |

### 4.2 Sibling reconciliation (by filename)

| Plan | Interface consumed/provided | Note |
|---|---|---|
| `02_CONTENT` | `PlanetDef`/`SectorPresetDef`/`Sector` content fields, `TechTree`/`TechNode` graph, `Loadouts` base64, `ObjectiveSpec` enum, `Unlockable` unlock store keys, `SectorRemapProvider` (§3.7 of 02), `Attributes` | 02 owns definitions/tech graph; 12 owns runtime research/unlock and requirement evaluation. `TechNode.finishedRequirements` is runtime state stored in 12, not 02 §6.1. |
| `04_IO` | `SaveIo`/`SaveVersion`/`TypeIO`/`JsonIO`/`SettingsStore`/`FileSystem`/`SaveSlot`/`SaveMeta`/`CustomChunk`; 12 registers `static-fog-data`, `SectorInfo` prepare hook, `Saves` policy, markers payload | 04 §2.4 explicitly defers these to 12. |
| `05_SIM_CORE` | `Sim`/schedule/`EventBus`/`Time`/`SimCommand`/`SimRng`/checksum; **05 §3.12/§8 R4 hand-off** | 12 replaces `game/rules.rs`/`game/teams.rs` stubs; adds systems at reserved slots (`UniverseGlobal`, `TeamStats`, `Fog`, `Campaign`, `Objectives`, `GameStateCheck`). |
| `06_WORLD` | `WorldGrid`/`Tile`/`Tiles`/`WorldContext`/`load_map`/`load_sector`/`make_sector_context`/`SectorShape`/`SectorView`/`Waves::generate` hook/RulesWriter | 06 §2.4 defers `Rules`/`Sector`/`Universe`/`Schematics` to 12; 12 implements `SectorView`/`Waves::generate` assignment. `add_darkness` sector-polygon hook supplied here. |
| `07_BLOCKS` | `Build.validPlace`/`ConstructBlock`/`BuildPlan`/`Building`/`register_core` calls | 07 §2.3 defers rules/limits/derelict to 12; `BlockCounter` reads `rules.block_limits`, `CoreRadiusProvider` reads `TeamRule`. |
| `08_LOGISTICS` | `CoreBlock`/`StorageBuild` behavior; `TeamInventory` (08 R2), `CoreCampaignHooks { handle_core_item, is_campaign, default_team, core_incinerates, allow_core_unloaders }` | 12 implements the `CoreCampaignHooks` trait + `TeamInventory` accessors; capture/loss behavior in `play.rs`. |
| `09_POWER` | `world::spatial::quad_tree`, `WindowedMean`, `PowerModule`, `Rules.{solar_multiplier,lighting,ambient_light,reactor_explosions,damage_explosions,env}` | 09 R4 expects 12 to consume its quadtree; `TeamData` fields use it (reconcile R3). |
| `10_COMBAT` | `Rules.{fire,damage_explosions,unit_damage,block_damage,unit_health,infinite_resources,build_cost_multiplier,fog}`, `Team`/`TeamData.present` | 10 §3.14 says “12 not written; minimal surface promised by 05” — 12 M1 must land the full accessors before 10 exits. |
| `11_UNITS` | `Rules.spawns`, `TeamData.{build_ai,rts_ai,unit_cap}`, `Waves::generate`, cap hook `Units::can_create`, `CampaignRules.apply` controller reset | 11 §2.3 defers `Rules`/`TeamData`/campaign difficulty to 12; 12 calls 11's `WaveSpawner` from `RunWave`. |
| `13_LOGIC` | `LExecutor::run_logic_script`, `LMarkerControl`, `GlobalVars` flags, `Senseable` | 12 calls; marker `control` semantics owned here (reconcile R9). |
| `14_UI` | Research/planet/rules/schematic/game-over/saves dialogs | 14 renders 12's data + invokes 12's mutations. |
| `19_MAPS_EDITOR` | map metadata/rules tags, editor objectives/waves dialogs, locales (`MapLocales`), preview pixels/textures | 12 exposes `MapObjectives`/`MapMarkers`/`Schematic`; editor mutates via these APIs. |
| `21_MULTIPLAYER` | `set_rules`/`complete_objective`/`clear_objectives`/`research_unlock`/`sector_capture`/`save_sector` relay + campaign persistence | 12 defines intents and host fns; 21 defines schema/ordering/validation. |

---

## 5. Milestones & task breakdown

Every milestone ends with `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, plus the named harness command; evidence goes in the Changelog. Order is strict.

**M0 — Rules swap + Gamemode + JSON round-trip (smallest vertical slice).**
Deliver `game/rules.rs` (full struct, accessors, `TeamRules`), `game/gamemode.rs`, `game/rules_event.rs`; delete the 05 minimal boundary (record the hand-off in 05's Changelog). Wire 04's serde (`#[serde(default, rename_all = "camelCase")]`) and `TypeIO` rules codec tests.
*Verify*: `cargo test -p mind-core game::rules` (ported `writeRules`/`writeRules2`); `mind-headless run rules_roundtrip`; `trace order` unchanged.

**M1 — Teams/TeamData/TeamInventory.**
Deliver `game/team.rs`, `game/teams.rs`; register `TickSet::TeamStats` systems; implement `update_team_stats`/`update_enemies`/core registration/destroy-to-derelict/`BlockPlan`; `TeamInventory` accessors (08 R2 default); consume 09's quadtree.
*Verify*: `cargo test -p mind-core game::teams` (`update_stats_counts`, `register_core`, `destroy_to_derelict`, `team_palette_seed_parity`); `mind-headless run teams_stats_bench`.

**M2 — Tech-tree runtime + objectives requirement system.**
Deliver `game/tech_tree.rs`, `game/objectives.rs`; `check_auto_unlocks`; spend/unlock; `req-` persistence through 04's `SettingsStore`; MP branch reads `Rules.researched`.
*Verify*: `run tech_unlock_gating`; `cargo test -p mind-core game::tech`.

**M3 — Campaign core (Universe/Planet/Sector/SectorInfo/stats).**
Deliver `game/campaign_rules.rs`, `game/stats.rs`, `game/universe.rs`, `game/planet.rs`, `game/sector.rs`; register `UniverseGlobal`/`Campaign`; lighting; production/import passes; `SectorInfo::{prepare,write,update}`.
*Verify*: `run campaign_sector_cycle --planet serpulo --sector ground-zero`; `mind-headless campaign turn --turns 10` golden means; `cargo test -p mind-core game::universe game::sector`.

**M4 — Saves policy.**
Deliver `game/saves.rs` on top of 04's `SaveSlot`; autosave; sector saves; remap; mod checks; playtime/`StateChangeEvent` handling.
*Verify*: `run sector_save_load_turn --save` (06 world + 04 IO); `cargo test -p mind-core game::saves`.

**M5 — Schematics.**
Deliver `game/schematic.rs`, `game/schematics.rs`; `.msch` byte format; base64; create from selection; loadouts; place/rotate; `Schematics` load from data/mods.
*Verify*: `run schematic_place`; `cargo test -p mind-core game::schematics` (golden `.msch` fixture byte-compare, `bXNjaAB` prefix, `create_from_selection`, rotate config point fixes).

**M6 — Objectives & markers.**
Deliver `game/map_objectives.rs`, `game/map_markers.rs`; all 13 objectives + 8 markers; `control`; completion host fn; class tags with 04; `call.clear_objectives` path in `sector_capture`.
*Verify*: `run objectives_completion`; `cargo test -p mind-core game::objectives game::markers`.

**M7 — FogControl + AttackIndicators.**
Deliver `game/fog.rs`, `game/attack_indicators.rs`; register `TickSet::Fog`; custom save chunk; deterministic dynamic cadence; `BuildDamageEvent` listener.
*Verify*: `run fog_reveal`; `cargo test -p mind-core game::fog` (circle clipping, RLE round-trip, discovery/visibility, chunk should_write); `fog_update_bench`.

**M8 — Play flows + game over/capture + relay contracts.**
Deliver `game/play.rs`, `game/world_reloader.rs`; `SetRules` command handler with campaign guards; MCP autoload `MindCampaign`; relay shapes handed to 21.
*Verify*: `run campaign_sector_cycle` (launch → 600 ticks → capture → game-over variants); MCP §7c; `cargo test -p mind-core game::play` (`play_map_event_order`, `sector_capture_flags`, `game_over_winner`, `set_rules_guard`).

**M9 — Perf, docs, exit gate.**
Budgets, goldens, `trace order` update, reconciliation notes to 10/11/13/14/19/21 owners, exit checklist green.

---

## 6. Data & formats

### 6.1 `Rules` JSON (parity ABI, camelCase, `#[serde(default)]`)

- Field set exactly as §3.3; `TeamRules` serializes as `{ "<teamId>": TeamRule, … }` ascending id, only entries that exist (lazy). `TeamRule` field names: `aiCoreSpawn`, `protectCores`, `checkPlacement`, `cheat`, `fillItems`, `infiniteResources`, `prebuildAi`, `buildAi`, `buildAiTier`, `rtsAi`, `rtsMinSquad`, `rtsMaxSquad`, `rtsMinWeight`, `unitFactoryActivationDelay`, `unitBuildSpeedMultiplier`, `unitDamageMultiplier`, `unitCrashDamageMultiplier`, `unitMineSpeedMultiplier`, `unitCostMultiplier`, `unitHealthMultiplier`, `blockHealthMultiplier`, `blockDamageMultiplier`, `buildSpeedMultiplier`, `extraCoreBuildRadius`.
- Content fields serialize by **name** (04 content serializers); `sector` as `"<planet>-<id>"`; `Color` as `#rrggbb[aa]`; `ItemStack` `{"item","amount"}`; `spawns`/`objectives` per 11/12 rules.
- `Rules::copy` is `Clone`; `retainContentFields` semantics per §3.3. `TypeIO` wire codec is 04 (`write_rules`/`read_rules`, 100 000 cap).

### 6.2 Settings keys (parity ABI; 04 `SettingsStore`)

`utimei`, `turn`, `<planet>-last-sector`, `<planet>-campaign-rules`, `<planet>-campaign-stats`, `<planet>-s-<id>-info`, `last-sector-save`, `save-<n>-name`, `save-<n>-autosave`, `saveinterval`, `launch-resources-seq`, `lastloadout-<core>`, `req-<content>-<item>`, `<name>-unlocked`, `savecreate`, `disableSave` runtime flag.

### 6.3 `TeamData` field table

`team`, `build_ai`, `rts_ai`, `present_flag`, `clustered_counts`, `last_cluster_update_timer`, `core_enemies`, `plans`, `cores`, `last_core`, `building_tree`, `turret_tree`, `unit_tree`, `unit_cap`, `unit_count`, `type_counts`, `building_types`, `units`, `players`, `buildings`, `units_by_type`. Engine-maintained: read-only for other plans; mutations only via `register_core`/unit add-remove hooks/`update_team_stats`.

### 6.4 `.msch` format (byte parity)

```
0    'm','s','c','h'          header
4    u8 version = 1
5..  zlib deflate (DeflaterOutputStream) wrapping:
     i16 width, i16 height            (max 128 each, ≤128*128 tiles)
     u8 tag_count; (UTF key, UTF value)*
       - tags always include "labels" (JSON string array) and
         "contentMap" (JSON: {contentTypeOrdinal: {name: id}})
     u8 block_dictionary_count; UTF block name*     (first-seen order)
     i32 total
     total × { u8 dict index, i32 Point2.pack(x,y),
               TypeIO.writeObject(config), u8 rotation }
```

Read: header → version ≤ 1 → inflate → dims guard → tags → `contentMap` mapper resolution via 02 names (fallback `SaveFileReader.fallback`) → block dictionary (unknown/`LegacyBlock` → `air`) → tiles (v0 `mapConfig` sniffing for `Sorter`/`Unloader`/`ItemSource`/`LiquidSource`/`MassDriver`/`ItemBridge`/`LightBlock`). `readBase64` trims/decode; `writeBase64` always starts `bXNjaAB`. Schematic registry/preview paths: `data/schematics/**`; preview PNGs are 19.

### 6.5 Fog custom chunk (`static-fog-data`)

```
u8  used_teams
i16 world_w, i16 world_h
used × { u8 team_id,
         RLE over w*h bits: repeated bytes, bit7 = value, bits0-6 = run (1..127) }
```

`should_write = rules.fog && rules.static_fog && fog.is_some()`; unknown chunk names skipped by 04. Bits semantics: `FogBits` is a `Vec<u64>` with `get/set(from,to)/clear` and `len = w*h`, rebuilt on `WorldLoadEvent`.

### 6.6 Campaign constants (add to `mind_core::constants`)

`TURN_DURATION_TICKS = 7200` (2 min), `INVASION_GRACE_PERIOD = 20`, `BASE_INVASION_CHANCE = 0.01`, `MAX_SCHEMATIC_SIZE = 64` (config-overridable), `MAX_SCHEMATIC_DIMENSION = 128`, `MAX_LOADOUT_SCHEMATIC_PAD = 5`, `MAX_PREVIEWS_MOBILE = 32`, `DYNAMIC_UPDATE_INTERVAL_TICKS = 40`, `ATTACK_INDICATOR_LIFETIME_TICKS = 900`, `CLUSTER_CHUNK_SIZE = 70.0`, `SAVE_EXTENSION = "msav"`, `SCHEMATIC_EXTENSION = "msch"`, `TIME_TO_MINUTES = 3600.0`.

### 6.7 STDB-shared shapes (schema owned by 21)

Rust structs (serde, stable names): `SectorInfoRow { planet, sector, items: Vec<(String,i32)>, production/export/imports: Vec<ImportStatRow>, wave, win_wave, waves, attack, minutes_captured, spawn_position, last_preset_name, was_captured, playtime }`, `UnlockRow { content_name, unlocked, requirements_finished: Vec<(String,i64)> }`, `CampaignStatsRow`, `SchematicRow { name, base64, tags }`, `RulesBlob { json, epoch }`. Commands: `SetRules`, `ResearchUnlock`, `CompleteObjective`, `SectorCapture`, `SaveSector`. Generated bindings never hand-edited (00 §3.3).

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (`Mindustry/tests/src/test/java/**` → `cargo test -p mind-core`)

| Mindustry test | Rust test | Notes |
|---|---|---|
| `ApplicationTests.writeRules` | `game::rules::tests::typeio_roundtrip` | TypeIO codec from 04; attackMode/buildSpeed values. |
| `ApplicationTests.writeRules2` | `game::rules::tests::json_roundtrip_and_field_tolerance` | tags map + forward/backward tolerance. |
| `ApplicationTests.playMap` | `game::play::tests::play_map_loads_and_sets_playing` | asserts `RulesLoadEvent` order + `State::playing` (world load from 06). |
| `ApplicationTests.save` / `saveLoad` | `game::saves::tests::slot_save_load_checksum` | 04 container + 06 world; campaign rules/tags survive. |
| `ApplicationTests.load77/85/108/114/152/152_be` | `io::legacy::*` (`msav-import` feature, 04) + `game::saves::tests::legacy_slot_meta` | campaign meta spot-check only; full reader owned by 04. |
| `ApplicationTests.testSectorValidity` (campaign half) | `campaign::tests::presets_rule_validity` | No infinite resources / no rule editing / no world-processor editing / default team sharded / wave team per planet / timer objectives localized / boss or win wave present / spawns non-zero through boss wave / one player core / spawn points exist. Generator+placement assertions remain joint with 06/11. |
| `ApplicationTests.initBuilding` | `game::teams::tests::register_core_and_inventory` | `core.build == default_team.data().core()` + inventory routing. |
| `ApplicationTests.spawnWaves` (campaign half) | `game::universe::tests::run_wave_campaign_difficulty` | spawner is 11; asserts wave increment + spacing × difficulty + `WaveEvent`. |
| New (no upstream test) | `game::rules::tests::retain_content_fields`, `mode_matches_presets`, `team::palette_seed_parity`, `teams::destroy_to_derelict_chunks`, `universe::run_turn_production_export`, `sector::sector_info_prepare_write`, `schematics::msch_golden_roundtrip`, `schematics::create_from_selection`, `schematics::rotate_point_config`, `tech::auto_unlocks_and_gating`, `objectives::sector_complete_eval`, `markers::add_remove_reindex`, `fog::circle_clip_and_rle`, `fog::is_discovered_visible`, `play::sector_capture_flags`, `play::game_over_winner`, `play::set_rules_guard` | Golden artifacts committed under `tests/golden/`. |

### 7b. Headless harness scenarios (`mind-headless`)

```text
mind-headless run rules_roundtrip --map serpulo/groundZero --dump out/rules.json
  # JSON→Rules→TypeIO→JSON equality; RulesLoadEvent fired once (from_save=false); checksum stable.

mind-headless run sector_save_load_turn --planet serpulo --sector groundZero --seed 7 --ticks 600
  # generate (06) → play → deposit core items → save sector-<planet>-<id>.msav → process reset
  # → load → checksum equality with tests/golden/sector_groundZero.checksum → 10 production turns
  # across two owned sectors → import/export means equal tests/golden/sector_turns.json.

mind-headless run schematic_place --base64 tests/fixtures/schemes/basic-loadout.msch.b64 --x 64 --y 64
  # readBase64 (prefix bXNjaAB) → placeLoadout → assert core registered, configs applied,
  # requirements sum equals golden; rotate 1× and re-place with point-config fixes.

mind-headless run tech_unlock_gating --fresh-profile
  # locked node research rejected → satisfy dependencies/requirements (spend items from sector core)
  # → unlock + ResearchEvent once → parent auto-unlock in MP mode → check_auto_unlocks only for
  # zero-requirement parentless nodes → settings req- keys round-trip.

mind-headless run fog_reveal --planet serpulo --sector groundZero --team sharded
  # fog on + static fog: place hasFogRadius building + unit, tick 40/120 → assert discovered vs
  # visible bits differ, circle clipping at edges → save custom chunk → reload → bits equal.

mind-headless run campaign_sector_cycle --planet serpulo --sector groundZero --ticks 1200
  # launch → rules event → wave run with difficulty scaling → win/capture → SectorCaptureEvent,
  # waves disabled, markers/objectives cleared, sector save written; lose variant: core destroyed
  # → GameOverEvent(wave_team) → clearSectorOnLose path.

mind-headless campaign turn --turns 10 --json out/turn.json
mind-headless campaign bench --profile {rules,teams,fog,turn} --ticks 3600 --json out/bench_campaign.json
```

All scenarios write `format: 1` dumps (00 §6.3) with checksums; goldens committed.

### 7c. MCP playtest scenario (concrete; open-godot-mcp)

Preconditions: spine scene running with `MindCampaign` autoload; plan-06 terrain generator live; a Serpulo sector with a start sector defined.

1. `godot_health` → `{ok: true}`.
2. `godot_game play` → client starts; wait for `ClientLoadEvent` (log line).
3. `godot_exec eval` `get_node("/root/Spine/MindCampaign").start_sector("serpulo", 0)` → `true`; wait ~1 s.
4. `godot_runtime_state inspect /root/Spine/MindCampaign properties ["get_sector_state"]` (or `godot_exec eval get_node("/root/Spine/MindCampaign").get_sector_state()`) → sector `groundZero` (or configured start), `has_core == true`, `wave == 1`, `rules.waves == true`.
5. Grant launch items via host eval (`Teams::inventory_mut(default_team)` add copper/lead) then `godot_exec` `get_node("/root/Spine/MindCampaign").research("<content>")` where `<content>` is a node whose parents are unlocked but whose items are now available → `true`; inspect `get_tech_state()` → content `unlocked == true`, `req-` amounts persisted.
6. Force capture: `godot_exec` `get_node("/root/Spine/MindCampaign").complete_objective(0)` (if an objective exists) or run `Logic.sector_capture` equivalent eval `get_node("/root/Spine/MindCampaign").capture_sector()`; inspect `get_sector_state()` → `was_captured == true`, `waves == false`, `objectives == []`.
7. `godot_exec` `get_node("/root/Spine/MindCampaign").save_slot("mcp-sector-test")` → `true`; `list_saves()` contains the slot; `load_slot("mcp-sector-test")` → `true`; inspector `checksum` equals the pre-save checksum.
8. `godot_screenshot` → path recorded; expected: sector terrain, HUD, researched content visible in build menu/inspector tab.
9. `godot_log errors` → empty (warnings allowed only for stubbed view features).
10. `godot_game stop`.

### 7d. Performance budget + measurement

Baseline: HLP §7.4 (60 tps, sim tick ≤ 4 ms mid-game). Campaign-specific budgets (dev machine, p99; `mind-headless campaign bench --json`):

| System / bench | Profile | Budget |
|---|---|---|
| `teams::update_team_stats` | `teams` (8 teams, 600 buildings, 300 units, 2 000 type-cache entries) | ≤ 0.35 ms/tick |
| `fog::update` | `fog` (static+dynamic, 2 player teams, 2 000 buildings, 400 units) | ≤ 0.60 ms/tick |
| `universe::run_turn` | `turn` (all 7 planets, 46 sector infos, 20 owned sectors) | ≤ 0.50 ms/turn; `universe::update` ≤ 0.05 ms/tick |
| `map_objectives::update` | `objectives` (32 running objectives) | ≤ 0.10 ms/tick |
| `schematic` read+write | 64×64 max loadout | ≤ 1.0 ms per op |
| `sector_save` (incl. `SectorInfo::prepare`) | groundZero-shaped world | within 04's ≤ 30 ms save budget |
| Allocation | all above | zero steady-state allocs after warmup (05 alloc-audit harness) |

Measurement: `mind-headless campaign bench --profile <p> --ticks 3600 --warmup 600`; correctness guard: same-seed checksum equals golden. Regressions block the P4 gate.

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core` green without Godot/network; every §7a row implemented or `#[ignore = "plan NN"]` with an owner.
- [ ] `cargo fmt --check` + `cargo clippy -p mind-core -- -D warnings` clean; no `HashMap` iteration in `game/`.
- [ ] 05's `game/rules.rs`/`game/teams.rs` stubs replaced; hand-off recorded in 05's Changelog; `trace order` golden updated with the new systems.
- [ ] `rules_roundtrip`, `sector_save_load_turn`, `schematic_place`, `tech_unlock_gating`, `fog_reveal`, `campaign_sector_cycle` all pass with committed goldens.
- [ ] Sector save→process reset→load yields identical sim checksum.
- [ ] Production turn: 10 turns produce expected import/export means; friendly/attacked/frozen sector rules honored.
- [ ] Schematic `.msch` golden byte-match + base64 prefix + loadout validation (no multi-core, no sandbox-only, max size).
- [ ] Tech gating: cannot research locked node; dependencies/objectives enforced; `SectorComplete` auto-insert evaluated; auto-unlock only for zero-requirement parentless nodes.
- [ ] Fog: discovery≠visibility semantics, RLE custom chunk round-trips, dynamic cadence deterministic (golden after N ticks).
- [ ] Sector capture clears objectives/markers, disables waves/world processors; campaign game-over/lose paths fire `GameOverEvent` with the correct winner.
- [ ] `set_rules` command rejects edits when `!allow_edit_rules` in campaign (guard test).
- [ ] MCP §7c executed; inspector state + screenshot + logs attached to the Changelog.
- [ ] §7d budgets met; alloc-audit zero after warmup.
- [ ] Reconciliation notes delivered to 10/11/13/14/19/21 owners (§4.2 table) and R2/R3/R8 resolved or explicitly deferred.
- [ ] `mind-core` Godot-free/tokio-free; GPL headers on every new file.

---

## 8. Risks & open decisions

Each item states the default this plan proceeds with. `NEEDS USER DECISION`/`NEEDS ORCHESTRATOR RECONCILE` items are load-bearing; execution continues on the default unless overridden.

| # | Risk / decision | Default taken | Status |
|---|---|---|---|
| R1 | **Rules/Teams merge ownership vs 05** (05 §3.12/§8 R4 reserved the files). | M0/M1 replace the stubs wholesale; field names identical so 05 systems compile unchanged; hand-off changelog entry required. | Orchestrator must accept (planned). |
| R2 | **Core inventory sharing model** (08 §8 R2). Java aliases `items` pointers across cores/linked storages. | `TeamInventory` resource in `Teams` (`IndexMap<TeamId, ItemModule>`); `Team::items()`/cores/storage route through accessors; campaign ownership here. | **NEEDS USER DECISION** (inherited from 08 R2; default continues). |
| R3 | **`TeamData` quadtree ownership** (09 §3.13/§8 R4 expects 09 to port Arc `QuadTree` and 12/11 to consume). | 12 consumes `world::spatial::quad_tree` for `building_tree`/`turret_tree`/`unit_tree`; if 09 ships without it, 12 provides `game::quad_fallback` behind the same type alias. | **NEEDS ORCHESTRATOR RECONCILE** with 09/11. |
| R4 | **Fog background threads → deterministic tick model** (deviation §2.4.2). | Tick-queued static drain + 40-tick dynamic cadence; behavior preserved, replayable. | Accepted (HLP §2.4 mandates). |
| R5 | **`Rules` in the sim checksum** (upstream has none). | `Rules::checksum_part()` canonical field hash; bump `CHECKSUM_VERSION` jointly with 05/23 so all peers/goldens move together. | **NEEDS ORCHESTRATOR RECONCILE** with 05/23. |
| R6 | **`unlocked_host`/`unlocked_now` dual source** (02 `UnlockStore` vs MP `Rules.researched`). | SP/editor: settings keys; MP: `Rules.researched` is authoritative and relayed; `unlocked_now` checks rules, `unlocked_host` checks the active store. | Reconcile with 02 at M2. |
| R7 | **`LMarkerControl` enum + `LExecutor` ownership** (13 not written). | 12 consumes `mind_core::logic::{LMarkerControl, LExecutor}`; ships a `NoopLogicRunner` trait default until 13 lands (marker `control` fully works without it; `completion_logic_code` is a no-op + debug counter). | Reconcile with 13. |
| R8 | **Saves preview textures / `SavePreviewLoader`** (04 §3.8 paths, pixels in 19). | 12 emits `PreviewRequest`s and keeps delete-and-requeue resilience; no textures in core. | Reconcile with 04/19. |
| R9 | **`MapLocales` ownership** (19). | Objective/marker text uses a `LocaleView` trait; default falls back to bundle keys; 19 supplies map locales. | Reconcile with 19. |
| R10 | **`WindowedMean` ownership** (09 §6.3). | `SectorInfo::ExportStat` consumes 09's type; if absent at M3, 12 ports it in `game/stats.rs` and 09 re-exports. | Reconcile with 09. |
| R11 | **`Call.*` relay shapes** (21 not written). | 12 exports host fns + `SimCommand`/event intents + serialized `SectorInfoRow`/`UnlockRow`/`RulesBlob`; 21 owns schema, ordering, validation, persistence. | Reconcile with 21. |
| R12 | **Autosave policy on dedicated/headless server**. | `saves::update_policy` runs only when `platform.autosave_allowed()` (client default true; headless default false unless `--autosave`); sector saves are explicit host calls. | Default chosen. |
| R13 | **Java palette/pseudorandom parity** for placeholder teams and planet rotation offsets. | Exact port through 05 `JavaRandom` (`Mathf.randomSeed(id+1, 360)`), golden captured once from the Rust build. | Accepted (HLP §9). |
| R14 | **`Schematics.create` visibility/discovery** needs fog state and `was_visible`. | Consumes 12 fog + 07 `Building::was_visible`; headless selection uses `team = None` (all visible) exactly like upstream `headless`. | Accepted. |
| R15 | **`data/schematics` load in headless**; mod schematics. | 04 `FileSystem` + 20 mod file tree; missing dir is empty, never fatal. | Accepted. |

---

## 9. References

Read in full for this plan:

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 locked decisions, §2 architecture, §3 row for 12, §4 template, §5 P4 gate, §6–§9 conventions, §10 OD1–OD9).
- `mindustry-godot/PRELIMINARY_PLAN.md`.
- Sibling plans: `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (spine/scenario/dump formats, extension contract), `02_CONTENT_IMPLEMENTATION_PLAN.md` (§2.3 boundaries, §3.4 lifecycle, §3.6 interfaces, §3.7 integration, §3.8 gotchas, §6.1 records incl. `PlanetDef`/`SectorPresetDef`/`TechNode`), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (§2.4 deferred ownership, §3.2 save engine, §3.4 TypeIO, §3.6 JsonIO, §3.7 settings, §3.8 slots, §6 formats, §8 R2/R3), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§2.4, §3.2 resources, §3.4 schedule, §3.6 Groups, §3.7 events, §3.8 Time, §3.11 determinism, §3.12 Rules/Teams boundary, §6.4 SimCommand, §8 R4), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§2.3/2.4, §3.2 WorldGrid, §3.3–3.6 tile ops/darkness, §3.9 Maps, §3.10–3.11 generators, §3.12–3.13 boundaries), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (§2.3 boundaries, §2.5 06 contract, §3.4 Building, §3.6 Build/limits, §3.10 building IO, §3.14 sibling reconciliation), `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (§3.7 storage/core, `CoreCampaignHooks`, §8 R2), `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (§3.13 R2/R4, §6.3 WindowedMean), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (§2.3 Rules fields, §3.14 cross-plan ledger), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (§2.3 Rules/TeamData ownership, §3.4 caps, §3.9 Team AI, §3.10 Waves, §6.5 SimCommand).
- Mindustry AGENTS docs: `core/src/mindustry/game/AGENTS.md`, `core/src/mindustry/type/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/io/AGENTS.md`, `core/src/mindustry/ai/AGENTS.md`, `core/src/mindustry/net/AGENTS.md` (WorldReloader), `core/src/mindustry/ui/AGENTS.md` (ResearchDialog), `core/src/mindustry/content/AGENTS.md` (tech trees), `tests/AGENTS.md`, `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`.
- Mindustry sources: all files listed in the §1 Sources row (Rules, Gamemode, Team, Teams, Universe, Saves, Schematic, Schematics, SectorInfo, SpawnGroup, Waves skim, CampaignRules, Difficulty, CampaignStats, GameStats, FogControl, MapObjectives, Objectives, MapMarkers, AttackIndicators, EventType, GameState, Control, Logic, WorldReloader, Planet runtime, Sector, SectorPreset, TechTree, ResearchDialog spend/unlock, Vars constants, ApplicationTests).
- Skills: `C:\Users\Clinton\g\.opencode\skills\playtest\SKILL.md`, `C:\Users\Clinton\g\.opencode\skills\godot-compositor-testing\SKILL.md`.

## Changelog

- 2026-10-01 — Plan written (P4). No implementation started.
- 2026-10-03 — **M0 (rules/gamemode/rules_event) + M1 (teams/teamdata) landed** on `lane/12-campaign` (base `main` @ `377451a`; commit `4d7a45b`).
  - M0: `game/rules.rs` adds runtime behavior on plan-04's JSON `Rules` (the full struct already lives in `io::json::rules`; 12 re-exports it under `game::rules` and adds `mode()`/`copy()`/`retain_content_fields`/accessors/`is_banned_*`/`has_env`/`checksum_part`). `game/gamemode.rs` is the exact preset table + `valid`/`hidden`. `game/rules_event.rs` adds `RulesEpoch` + the `SetRules` campaign guard and JSON decode (plan 05's blob seam closed; command shape unchanged). Campaign constants live in `game::rules` (shared `constants.rs` untouched).
  - M1: `game/team.rs` (256-team registry, exact base colors, deterministic `ArcRand` palette seed), `game/teams.rs` (`Teams`/`TeamData`/`BlockPlan`/`TeamInventory`, `update_team_stats`/`update_enemies`/`register_core`/`unregister_core`/`destroy_to_derelict` chunked), `game/quad_tree.rs` (insertion-stable fallback per R3 — plan 09 did not ship `world::spatial::quad_tree`). Building caches re-derive from deterministic snapshot records; the `Groups`→record ECS adapter is the joint plan-11 wiring step.
  - Evidence: `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` **995 passed / 2 ignored** at M1. `checksum_part()` is deliberately **not** wired into `Sim::checksum` (R5 awaits the joint `CHECKSUM_VERSION` bump with 05/23) so all existing goldens are unchanged.
  - Reconciliation to record: plan 05 has no `game/rules.rs`/`game/teams.rs` stubs (plan 10/11 never added them; plan 04 shipped the full `Rules` JSON shape at M6). §3.1's "replace the 05 stubs" therefore became "add `impl Rules` + a `game::rules` re-export"; no contract change.
- 2026-10-03 — **M2 (tech-tree runtime + objectives) landed + headless `campaign` scenarios** (`9f6c31a`).
  - `game/tech_tree.rs`: `spend` (exact `ResearchDialog.spend`), `unlock` (content + parents for MP), `can_spend`/`dependencies_unlocked`/`objectives_complete`, `check_auto_unlocks` (exact `Control.checkAutoUnlocks`, single ordered pass), `reset_all`, `content_unlocked`/`content_unlocked_with_rules` (SP settings vs MP `Rules.researched`), all-type `unlock_fields`/`unlock_fields_mut` including `ContentType::Unit` (plan 02's `with_unlock_fields` omits units), and harness `push_node`.
  - `game/objectives.rs`: `Objective` for `Research`/`Produce`/`SectorComplete`/`OnSector`/`OnPlanet` over an `ObjectiveContext` (`SectorStatus`), with bundle display keys.
  - `mind-headless campaign rules|tech` (`campaign_scenarios.rs`) + committed goldens `tests/golden/campaign/{rules_roundtrip,tech_unlock_gating}.json`; `campaign rules` checksum `69fff976f239bfd9`. Evidence: `cargo test -p mind-core` **1005 passed / 2 ignored**; `cargo test -p mind-headless` **38 passed**; fmt + workspace clippy `-D warnings` clean.
  - **Flagged to plan 02:** vanilla `TechNode.requirements` are empty (the builder does not materialize `UnlockableContent.researchRequirements()`), so research costs are not represented; `push_node` covers the harness spend path until 02 fixes it.
- 2026-10-03 — **M3–M9 NOT started.** Deferred: M3 campaign core (`Universe`/`Planet`/`Sector`/`SectorInfo`/`CampaignRules`/`Difficulty`/`CampaignStats`), M4 `Saves`, M5 `Schematic`/`Schematics`, M6 `MapObjectives`/`MapMarkers`, M7 `FogControl`/`AttackIndicators`, M8 `play`/`WorldReloader`/`MindCampaign` gdext, M9 perf/docs. In-engine MCP §7c recorded **DEFERRED** (single-editor mutex). R2 (`TeamInventory`) proceeded on the plan-08 default; R3 consumed the fallback; R5 pending orchestrator reconcile.
- 2026-10-03 — **M3 (campaign core) landed** (`118c928`).
  - `game/campaign_rules.rs`: `CampaignRules` + `Difficulty` (exact enum table, `apply` fold incl. RTS-AI wave-team toggle returning `CampaignApply { rts_swapped, rts_enabled }` for the plan-11 controller-reset hook, `info`/`percent_stat`, `<planet>-campaign-rules` persistence).
  - `game/stats.rs`: `CampaignStats` (name-keyed `IndexMap` fields, camelCase JSON, mutation helpers) + `GameStats` mutation helpers over plan-04's persisted shape.
  - `game/planet.rs`: `Planet` runtime (orbit/rotation via `JavaRandom`, `add_parent_offset`/`world_position`, rules/stats persistence, last-sector, `apply_rules`/`apply_default_rules`, `update_base_coverage` via the `SectorNeighborhood` plan-06 seam).
  - `game/sector.rs`: `Sector` runtime (`has_base`/`is_attacked`/`is_captured`/`has_enemy_base`/`is_shielded`/`add_items`/save-info settings) + `SectorInfoState` with `prepare`/`write`/`update`/`refresh_import_rates`/`handle_core_item` and `InfoWindows` (`WindowedMean` from plan 09) — the transient `ExportStat` windows are kept out of the plan-04 persisted `SectorInfo` (deviation; avoids dropping its `Copy`).
  - `game/universe.rs`: `Universe` clock/launch/loadout (`utimei`/`turn`/`launch-resources-seq`) + `Campaign` container (`from_registry`, `update_global` positioning, `run_turn` production/import/invasion passes with `TurnContext`/`TurnReport`/`CampaignEvent`).
  - Harness: `mind-headless campaign sector|turn` + goldens `tests/golden/campaign/{sector_cycle,turn}.json` (`campaign sector … --ticks 600` pass; `campaign turn --turns 10` → 3000 items/sector, checksum `b02c01ef73ded6da`).
  - Evidence: `cargo fmt --all -- --check` clean; workspace `clippy -D warnings` clean; `cargo test -p mind-core` **1092 passed / 2 ignored**; `cargo test -p mind-headless` **65 passed**.
  - **Deferred in M3:** plan-06 Serpulo/Erekir generators (blank/real sector generation) — `campaign_sector_cycle` uses the seam (`EmptyNeighborhood`) and the content-time sector grid; world-spanning save/load of a generated land is M8 + plan 06.
- 2026-10-03 — **M4 (`Saves` policy) landed** (`0d2f268`).
  - `game/saves.rs`: `Saves` over plan-04's `SaveSlot` — listing (`list_save_slots`), legacy megabase cleanup, two-phase sector remap (`plan_remap` + file migration + `<planet>-s-<id>-info` move), tick-driven autosave/playtime (`Saves::update`, `saveinterval`), `save_sector`/`add_save`/`import_save`/`get_next_slot_file`/`delete_all`, `StateChangeEvent` reset and `cautious_load` → `MissingModsReport`.
  - Tests cover MockFs listing, autosave cadence, state-change reset, sector naming, import/delete, remap decision. `cargo test -p mind-core game::saves` green.
  - **Deferred in M4:** `sector_save_load_turn --save` (needs a real generated world + `WriteContext` from plan 06/M8); `Saves::update` performs the save through the caller-supplied context rather than owning the world.
- 2026-10-03 — **M5 (`Schematic`/`Schematics`) landed** (`4a48809`).
  - `game/schematic.rs`: `Schematic`/`Stile`, `requirements`, `has_core`/`find_core`, `SchematicPreviewSpec`, steam-id tags.
  - `game/schematics.rs`: exact `.msch` reader/writer (raw header + version, zlib default-level deflate matching the vanilla payloads, first-seen block dictionary, `TypeIO` configs, v0 legacy `map_config`), `write_base64`/`read_base64` (`bXNjaA…`), `Schematics` registry (`load_loadouts` decoding the 4 vanilla base64 schematics, `load_file`, `check_loadout`, `to_plans`, selection `create` via the `SchematicWorld` seam, `rotate`/`rotated` point-config fixes).
  - Harness `campaign schematic` + golden `schematic.json` (4 loadouts decoded, round-trip, checksum `e7d30b25b03690c2`).
  - Tests: `.msch` round-trip/header, base64 prefix, **vanilla loadout base64 decode**, `create_from_selection`, rotate point-config, multicore rejection. `cargo test -p mind-core game::schematic*` green.
  - **Deferred in M5:** placement (`place`/`placeLoadout`/`placeLaunchLoadout`) against plan-06 tile ops + plan-07 `configure_any`; `search`/preview pixels (plan 19). `contentMap` is emitted empty (Rust↔Rust dense IDs); `schematic_priority` sorting is by block id.
- 2026-10-03 — **M7 (`FogControl`/`AttackIndicators`) landed** (`aa820dd`).
  - `game/fog.rs`: `FogBits`, packed `FogEvent` (`x:16|y:16|radius:16|team:8`), `FogData` read/write double buffers, exact midpoint `circle`/`hline` clipping, deterministic static drain + 5-slice building refresh + 40-tick dynamic flush, `static-fog-data` RLE `write`/`read` (§6.5), `should_write`.
  - `game/attack_indicators.rs`: insertion-ordered indicators with dedupe/reset, 900-tick timeout compaction and head reindex (upstream tail-index staleness preserved).
  - Harness `campaign fog` + golden `fog.json` (checksum `170e5095d482f982`); tests cover packing, edge clipping, discovery≠visibility, RLE round-trip, deterministic cadence, indicator lifecycle.
- 2026-10-03 — **M6/M8/M9 NOT started.** Deferred with owners: M6 `MapObjectives` executor (13 objective types) + `MapMarkers` (8 marker types) — plan-04 already owns the JSON shapes/class-tag registry; M8 `play`/`WorldReloader`/`SetRules` handler + gdext `MindCampaign` autoload (MCP §7c remains DEFERRED on the single-editor mutex); M9 perf/docs/budgets. R5 (`Rules` in `Sim::checksum`) still pending the joint `CHECKSUM_VERSION` bump with 05/23. R7 (`LMarkerControl`/`LExecutor`) and R9 (`MapLocales`) remain open for M6. All four new headless goldens are additive; no P0 golden changed.
