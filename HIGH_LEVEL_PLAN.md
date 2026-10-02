# HIGH_LEVEL_PLAN — Mindustry → Godot 4.7 full-parity port

> Status: **decisions locked 2026-10-01** (§0). **Execution: P0 (plan 00) COMPLETE; plan 01 M0–M6 COMPLETE (merged); plan 02 M1–M4 complete (all merged to `main` 2026-10-02); F2 lanes (`02` M5, `03`, `04`) dispatched 2026-10-02** — see §13. Source of intent: [`PRELIMINARY_PLAN.md`](PRELIMINARY_PLAN.md) (kept as history — where it conflicts with this file, this file wins).
> This document is the **index and constitution** for the `{nn}_{SYSTEM}_IMPLEMENTATION_PLAN.md` set in §3. Every plan inherits §0, §2, §4 (template) and §6–§9. Read order for any agent: **this file → the target system plan → the Mindustry `AGENTS.md` files named by that plan**.

---

## 0. Locked decisions (2026-10-01)

| # | Decision | Locked choice |
|---|---|---|
| **D1** | Client stack | **Pure Rust client.** `godot-rust` GDExtension (`gdext`) for all game code, `bevy_ecs` as a library for the authoritative simulation. **No C# anywhere.** `client/sstdbsdk/` (C#, 584 lines) is **rewritten in Rust** as the `mind-stdb` crate — same concepts (connector autoload, subscription waves, per-table binder), Rust types. Godot scenes + GDScript own **UI layout only**; sim, view drivers, input and net are Rust. |
| **D2** | Multiplayer authority (now) | SpacetimeDB module = **persistent state + command relay + cheap validation** (identity, ownership, rate limit, basic range/existence checks). Each client runs the full deterministic sim locally. An **authoritative STDB match sim is explicitly deferred** — the table/reducer shapes must leave room for it later. |
| **D3** | Scope | **Full 1:1 parity with the Mindustry game** — sim, all content, campaign, editor, logic, UI, audio, mod surface (data mods), multiplayer, mobile input. Strict dependency order; no system dropped by default. Known platform deviations are tracked in §9. |
| **D4** | Foundation spine | Phase 0 (plan 00) ships a minimal test rig: **Godot window + camera + one tile grid + place/break one block + state inspector + headless sim harness**. Every later plan extends this rig so each system is verifiable **headlessly and in-engine from the moment it lands**. This kills the late-integration risk of a strict dependency-order port. |
| **D5** | Verification | Every implementation plan carries a required **“Oracle & verification”** section: ported Mindustry tests, headless harness scenarios, at least one concrete MCP playtest scenario, performance budget + measurement method, exit-criteria checklist. |
| **D6** | License | Project is **GPL-3.0** (derivative of Mindustry). `LICENSE` + `THIRD_PARTY_NOTICES.md` at repo root; every ported file carries a `// Portions derived from Mindustry (GPL-3.0)` header. Asset reuse keeps original names/keys. |
| **D7** | Engine | **Godot 4.7 standard**, on PATH as `godot4` in WSL Ubuntu (4.7.2.stable). The mono build (`godot4-mono`, 4.7.2.stable.mono) is a fallback host only. Standard is used for all dev/CI/test runs; a GDExtension loads on either. |
| **D8** | Sim tick | **Fixed 60 Hz accumulator** mirroring `core/Logic.java`; view interpolation is render-only. **No Godot physics for sim entities** — custom grid + A*/flowfield in Rust (per preliminary plan §2.2). |
| **D9** | Plan numbering | `{nn}` is build order. Plans may be executed only when their `Depends on` plans’ milestones are green. |

---

## 1. What is being ported

Mindustry (`/mnt/c/Users/Clinton/g/code_examples/Mindustry/`): Java 17 + Arc; square 8 px tiles; 60 tps sim with thousands of live entities (conveyor items, bullets, units, power/fluid graphs); per-type content IDs baked into saves/network; `mindustry.gen` generated at build time by the `annotations` module.

The port keeps **behavioral parity** and **data-name parity** (content names, sprite region names, bundle keys, class semantics) but replaces:

| Mindustry | Port |
|---|---|
| Java + Arc engine | Rust (`gdext` + `bevy_ecs`; Godot owns window/render/UI/audio) |
| `mindustry.gen` annotation codegen | Rust structs/components + registries; **no build-time Java processing** |
| Arc collections (`Seq`, `IntMap`, pools) | Rust `Vec`/`SmallVec`/`hashbrown`-style maps, Bevy ECS, explicit arenas/pools |
| `@Remote` codegen + arcnet packets | STDB reducers/views relay + Rust net layer (plan 21) |
| Rhino JS / Java JAR mods | JSON data mods + patches (1:1); script mods are an open decision (§10 OD1) |
| `tools:pack` Gradle pipeline | Rust/CLI asset pipeline (plan 03) |

Ground rules that do **not** change: `ContentType` ordering and append-only IDs (`ctype/AGENTS.md`), the entity revision/save-version discipline (`io/AGENTS.md`, `annotations/AGENTS.md`), the tile-grid geometry (`Vars.tilesize = 8`, max block size 16), and all localization keys.

---

## 2. Target architecture

### 2.1 Repository layout (mirrors `main/`)

```
mindustry-godot/
  AGENTS.md                              # agent guide for this repo (root)
  HIGH_LEVEL_PLAN.md                     # this file
  PRELIMINARY_PLAN.md                    # historical intent
  LICENSE                                # GPL-3.0
  THIRD_PARTY_NOTICES.md                 # Mindustry + addon attribution
  {nn}_{SYSTEM}_IMPLEMENTATION_PLAN.md   # this plan set (§3)
  client/                                # Godot 4.7 project
    project.godot
    scenes/ ui/                          # GDScript UI + scenes (layout only)
    addons/                              # evaluated third-party addons (§8)
    rust/                                # Cargo workspace — all game Rust
      Cargo.toml
      mind-core/                         # pure sim: content, world, blocks, units, logic, campaign, io
      mind-headless/                     # test-rig binary (scenarios, dumps, benchmarks)
      mind-gdext/                        # GDExtension cdylib (view drivers, input, audio, autoloads)
      mind-stdb/                         # Rust port of sstdbsdk + generated STDB client bindings
  server/
    spacetimedb/                         # Rust STDB module (crate)
    build.sh  build.ps1                  # publish + generate bindings (bash primary)
    spacetime.json
```

The old C# tree (`client/Scripts/`, `client/sstdbsdk/`) is **reference material only**; plan 00 deletes it once the Rust equivalents land (keep the folders until then so the rewrite agent can read them).

### 2.2 Crate boundaries (hard rules)

- **`mind-core` is Godot-free and tokio-free.** All simulation, content, world, IO and campaign logic lives here. `cargo test -p mind-core` is the primary oracle. This is the single most important boundary in the project.
- **`mind-gdext`** is a `cdylib`: registers Godot classes; each frame pumps net, steps `mind-core` on the fixed accumulator, syncs views/interpolation, forwards input. It contains **no game rules**.
- **`mind-stdb`** owns connection/token/subscription/binder plumbing around `spacetimedb-sdk`; exposes a plain Rust API. Generated bindings are never hand-edited.
- **`mind-headless`** boots `mind-core` with scenarios and dumps state; used by CI, subagents and the MCP test rig.
- **`server/spacetimedb`** is a separate crate (module) — same conventions as `main/server/spacetimedb/` (`server/AGENTS.md` there is the reference for STDB Rust rules).

### 2.3 Authority & data flow

```
STDB module (durable rows + command/event relay + cheap checks)
      │  subscribes: catalogs, profiles, sectors, matches, command events
      ▼
mind-stdb ──► mind-gdext (autoload) ──► mind-core Bevy World (authoritative locally)
      ▲                                        │
      │  reducers (intent: place/break/command)  ▼
      └───────────────────────────── mind-gdext view sync ──► Godot nodes (view only)
```

- The Bevy `World` is authoritative for the live match; Godot scene tree is the view.
- Commands are applied from the STDB relay in transaction order, so every peer replays the same command stream (plan 21 defines ordering, snapshots, desync detection).
- Cheap server-side checks only (D2). No rule simulation server-side yet; table/reducer shapes must permit adding one later.

### 2.4 Determinism rules

- Content IDs: per-`ContentType` append-only; **never reorder or insert**.
- Sim: fixed 60 Hz; seeded PRNG (a Rust port of Arc's/`Random` semantics where parity matters); no `HashMap` iteration in sim code (use `IndexMap`/sorted iteration); stable entity iteration order; no wall-clock reads.
- All peers run the same build; determinism is required across platforms for lockstep command replay (plan 21 tests it).
- Render-only state (interpolation, particles, audio) may be non-deterministic and must never feed back into the sim.

---

## 3. The implementation plan set

Phase keys: P0 foundation · P1 platform/content · P2 sim core · P3 world/systems · P4 combat/units/campaign · P5 logic/UI/input · P6 render/audio · P7 editor/mods/export · P8 multiplayer · cont. = continuous.

> **Decision status:** all plan-local `NEEDS USER DECISION` items were resolved by the user on 2026-10-01; the authoritative answers are in `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` §8.1.1. Plan-local §8 rows still show the original defaults, and the register wins on any difference.

| File | Phase | Scope (what it delivers) | Depends on |
|---|---|---|---|
| `00_FOUNDATION_IMPLEMENTATION_PLAN.md` | P0 | ✅ **COMPLETE 2026-10-01 (M0–M8, P0 gate passed).** Workspace layout; Godot project reset (C# deleted, non-mono 4.7, `mobile` renderer); `gdext` + `bevy_ecs` wiring; fixed-step runner; event bus; logging; **minimal spine** (window, camera, one tile grid, place/break one block, state inspector overlay); `mind-headless` harness; MCP bridge bring-up; `LICENSE`/`THIRD_PARTY_NOTICES.md`; build scripts; CI (`cargo fmt/check/clippy/test` + headless golden run). | — |
| `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` | P1 | ✅ **COMPLETE 2026-10-01 (M0–M6, merged to `main`).** Rust rewrite of `sstdbsdk` semantics (`mind-stdb`): connector core (config/identity/token/protocol, offline-first pump), subscription waves (base/lobby/game), typed per-table binder API, frame pump; `server/spacetimedb` skeleton grown to identity/session/profile/settings/audit + relay (`relay_match`/`relay_member`/`match_command`, per-caller views, cheap validation); `spacetime generate --lang rust` workflow; **command-relay foundation with `CommandStream` ordering + two-client ping round-trip IT**; `StdbConnector` autoload + `StdbBinder` node + inspector net page; plan-21 handoff notes. Two deferrals: in-engine §7.3 MCP run on the merged tree, relay-throughput bench (plan 21 owns the load harness). | 00 |
| `02_CONTENT_IMPLEMENTATION_PLAN.md` | P1 | 🔶 **IN PROGRESS — M1–M4 complete, all merged to `main` (2026-10-02); M5–M7 in flight on `lane/02-m5`.** `ctype`/`type` equivalents in Rust (framework, dense IDs, lifecycle, name maps, bundles, unlocks); vanilla registries landed: Items (22), Liquids (11), StatusEffects (23), Bullets (6), `fx_meta` (267), commands/stances/weathers/planets/sectors/loadouts, Serpulo (223) + Erekir (152) tech trees, **all 441 `Blocks.java` entries (B1–B6, 0 unported)** with `BlockDef` metadata + `BlockKind` audit + ledger `parity/ledgers/blocks.md`. M3 re-recorded the P0 goldens (block IDs now upstream: `spine_place_break` → `375c68a53e861948`). Remaining: M5 (units), M6–M7 (audit, JVM golden — NUD-10 blocker). Content JSON parser hooks live in 20. | 00 |
| `03_ASSETS_IMPLEMENTATION_PLAN.md` | P1 | 🔶 **IN PROGRESS on `lane/03-assets` — M0–M3 complete 2026-10-02.** M0–M2: crates/migration/packer/AA + filename-only generators. M3: content-driven generators (`block-icons` outlines/team/`block-<name>-full`/UI/`block_colors.png`, `shallows`, `item-icons`, `sector-icons`, `team-icons`, `ore-icons`, content-driven `edges`), `icons.properties` writer (append-only) + `icon_codes.json`, region inventory (`mind-headless assets regions --assert-complete`: 2588/2588), full pack 4632 regions on 4 pages, byte-deterministic across runs. `unit-icons` + unit metadata **deferred to 02-M5**. Asset pipeline: `tools:pack` equivalent (sprite atlas + autotile/outline/icon generation), icon font + `Icon`/`Iconc`, bundle/localization loading and fallback chain, `Sounds`/`Musics` registration, `FileTree` (mods-first) equivalent, runtime atlas region lookup (`@Load` equivalent), sprite region naming parity. | 00, 02 |
| `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` | P1 | `SaveIO`/`SaveVersion` chain, `TypeIO` object codec, `JsonIO` (rules/settings/stats), entity revision I/O (Rust equivalent of `revisions/*.json`), `.msav` map/save read/write (format compatibility decision §10 OD2), settings persistence, backups, save slots. | 02 |
| `05_SIM_CORE_IMPLEMENTATION_PLAN.md` | P2 | `Vars`/`GameState`/`Logic` module split; fixed 60 Hz loop; Bevy schedule ordering mirroring `Logic.updateEntities()` exactly (pool cleanup → physics → players → effects → Groups → units → power graph → buildings → bullets → collisions); `Events`/`EventType`; `EntityGroup`/`EntityIndexer`/`Groups`; pooling + queue-free; `Time`; async worker physics/avoidance; reset/play flows. | 02, 04 |
| `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` | P3 | `Tile`/`Tiles`/`World`/`WorldContext`; floors/overlays/walls/props; `Edges`/multiblock indexing; `TileGen`/`WorldParams`; terrain generation pipeline (`WorldGenerator`/`BasicGenerator`/`PlanetGenerator`/`FileMapGenerator`/filters); Serpulo/Erekir/Tantros/Asteroid generators; darkness; map registry (`Maps`, previews stub — editor in 19). | 02, 04, 05 |
| `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` | P3 | `Block` base (flags/requirements/consumers/stats/bars/environment); `Building` entity + modules (`ItemModule`/`LiquidModule`/`PowerModule`); multiblock assembly; placement/breaking (`Build`/`validPlace`/`ConstructBlock`/`BuildPlan`); proximity updates; config/configured; revisioned building IO; `DrawBlock` framework base. | 06 |
| `08_LOGISTICS_IMPLEMENTATION_PLAN.md` | P3 | Item transport: `Conveyor`/`Duct`/`StackConveyor`/`Autotiler`/`TileBitmask`; `Router`/`Sorter`/`Junction`/`ItemBridge`/`DirectionBridge`/`MassDriver`/`Unloader`; `ItemBuffer`/`DirectionalItemBuffer`; payload system (`PayloadBlock`/`Payload`/conveyor/loader/unloader/router/mass driver/deconstructor/constructor/block producer); `StorageBlock`/`CoreBlock`. | 07 |
| `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` | P3 | `PowerGraph` (links, nodes, long nodes, beam nodes, batteries, diodes); generators (`PowerGenerator`, `ConsumeGenerator`, `ThermalGenerator`, `SolarGenerator`, `ImpactReactor`, `NuclearReactor`, `VariableReactor`, `HeaterGenerator`); liquid transport (`Conduit`, routers/junctions/bridges); heat network (Erekir `HeatBlock`/producer/consumer/conductor, `HeatCrafter`); power/heat rendering hooks. | 07 |
| `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` | P4 | `BulletType` hierarchy + bullet entity behavior/collision/tile raycast; `Damage` (area/line/explosions/armor); turrets (`BaseTurret`→`ReloadTurret`→`Turret`→`ItemTurret`/`LiquidTurret`/`PowerTurret`/`ContinuousTurret`/`PayloadAmmoTurret`/`LaserTurret`/`PointDefenseTurret`/`TractorBeamTurret`/`BuildTurret`); defense (`ForceProjector`/`MendProjector`/`ShieldWall`/`ShockMine`); `Fires`/`Puddles`/`Lightning`; status application hooks. | 09, 07, 05 (02, 04 transitive) |
| `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` | P4 | Entity/component framework mapped to Rust/Bevy (`comp/*` semantics: composition, `@Import`/`@Replace`/`@MethodPriority` equivalents, `@SyncField`/`@NoSerialize` equivalents); `UnitType` init/derived state; unit kinds (mech/legs/tank/hover/naval/missile/tether/crawl/segmented/payload); unit factories/assemblers/reconstructors; controllers (all `ai/types/*`); `Pathfinder` flowfields, `ControlPathfinder` HPA*, `Astar`, `BlockIndexer`; `UnitCommand`/`UnitStance`/`UnitGroup`; `RtsAI`/`BaseBuilderAI`/`BaseRegistry`; `WaveSpawner`/`SpawnGroup`/`Waves`. | 10, 08, 07, 06 (05 transitive) |
| `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | P4 | `Rules`/`Gamemode`/`Team`/`Teams`/`TeamData`; `Universe` (turns, launch, production/import); `SectorInfo`; `Saves` (slots/autosave/sector saves); `Schematic`/`Schematics` (`.msch`); `FogControl`; `MapObjectives`/`Objectives`/`MapMarkers`; `AttackIndicators`; `Difficulty`/`CampaignRules`/`CampaignStats`; tech-tree runtime; planet JSON data. | 05, 06, 07, 08, 09, 10, 11 |
| `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` | P5 | `LParser`/`LAssembler`/`LStatements`/`LExecutor`/`LVar`/`LCanvas`; `LogicIO` text (de)serialization equivalent; logic blocks (`LogicBlock`, displays, memory, message, switch, canvas, tileable display); `LAccess`/`Senseable`/`Settable`/`Controllable`; `LUnitControl` + `LogicAI`; `GlobalVars`; `LogicFx`/`LogicRule`; world-processor privilege rules; logic editor hooks (UI in 14). | 06, 07, 11, 12 |
| `14_UI_IMPLEMENTATION_PLAN.md` | P5 | Widget/theming layer over Godot `Control`; `Styles`/`Fonts`; `BaseDialog` + every dialog and fragment (settings, research, planet, mods, join, game-over, custom rules, schematics, database, about, tutorial, core info, HUD, minimap widget, chat, console); `Menus` (server-sent menus); MSUI builder DSL equivalent; toasts/info/popup; item/core displays; `FileChooser`; mobile layout branches. | 02, 05, 12 |
| `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` | P5 | `Binding`/keybinds; `InputHandler` + `DesktopInput` + `MobileInput`; `Placement` (line/rect/conveyor A*/bridges/upgrades/schematics/rebuild); build-plan queue and previews; RTS drag-select, command queue, formations; camera (pan/zoom/follow; evaluate Phantom Camera); locks/focus; input → command relay hooks. | 07, 11, 14 |
| `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` | P6 | `Renderer` frame pipeline; `Layer`/`CacheLayer` ordering; `FloorRenderer` chunk meshes (TileMapLayer/MultiMesh); `BlockRenderer` (quadtrees, sprite caches, shadows, darkness); `OverlayRenderer`; `LightRenderer`; `FogRenderer`; `MinimapRenderer`; `Pixelator`; `Shaders` (GLSL→Godot); `Pal`/`Drawf`/`Trail`; `Lod`; `EnvRenderers`; `MenuRenderer`/`LoadRenderer`; g3d planet render (Godot 3D); bloom; screenshots; cutscenes. | 02, 06, 07 |
| `17_FX_PARTS_IMPLEMENTATION_PLAN.md` | P6 | `Effect`/`EffectState`/`EffectContainer` system; full `Fx.java` catalogue port; composite effects (`MultiEffect`/`SeqEffect`/`RadialEffect`/`WrapEffect`); `DrawPart` framework (`RegionPart`/`ShapePart`/`HaloPart`/`HoverPart`/`FlarePart`/`EffectSpawnerPart`) + `PartProgress`/`PartMove`; weapon/unit/bullet part rendering; decals; screen shake; particle renderer evaluation. | 10, 11, 16 |
| `18_AUDIO_IMPLEMENTATION_PLAN.md` | P6 | `SoundControl` (playlists, dark/boss/fade, UI bus filter, pause filtering); `SoundPriority`; `AmbientSource` + audio thread equivalent; `SoundLoop`; `MusicContainer`; `Sounds`/`Musics` registration strategy; weather audio; settings/mixing parity. | 02, 03 |
| `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` | P7 | `Maps` registry/import/export/previews; `MapIO` image import/export; `MapEditor`/`EditorTile`/`DrawOperation`/`OperationStack`/`EditorTool`/`EditorRenderer`/`EditorSpriteCache`; editor dialogs (info, generate, resize, load, objectives, waves, processors, locales, banned content, assets); `SectorGenerateDialog`; playtest flow. | 04, 06, 14, 15, 16 |
| `20_MODS_IMPLEMENTATION_PLAN.md` | P7 | Mod discovery/metadata (folder/zip); JSON content parsing (`ContentParser` equivalent); `DataPatcher`/`DataManager` (patch/content/image/sound/music/bundle assets); `sprites/` + `sprites-override/`; bundle merge; `ModsDialog`; server asset loaders; `contentOrder`/dependency ordering; `ClassMap` replacement. Script mods = §10 OD1. | 02, 03, 04 |
| `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` | P8 | Full STDB schema (players/profiles/matches/sectors/schematics/tech unlocks/command events); reducers + per-caller views; command relay with cheap validation; ordered command application + snapshots/checksums/desync correction; late-join world streaming; host/join flows; admin/ban/whitelist/chat; **transport decision §10 OD3**; room for authoritative sim. | 01, 05, 06, 11, 12, 15, 16, 17 |
| `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` | P7 | Godot export presets + window/args/file dialogs; dedicated headless server binary; Discord RPC; Steam integration (**OD4**); mobile Android/iOS exports + touch specifics; save-file associations; achievements/service abstraction. | 14, 15, 16, 21 |
| `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` | cont. | The cross-cutting oracle: Mindustry test inventory → Rust test ports; golden scenarios; sim determinism tests (seed + command log → checksum); perf benchmark suite + budgets; MCP scenario catalog per plan; regression/soak harness; CI matrix; system parity checklist. | all (continuous) |

---

## 4. Required template for every `{nn}_*_IMPLEMENTATION_PLAN.md`

Every plan file **must** use exactly these top-level sections (adapt depth, not structure):

1. **Header block** — `Status`, `Phase`, `Depends on` (exact filenames), `Blocks` (what waits on this), `Sources` (Mindustry paths + `AGENTS.md` links read), `Extends spine` (what the plan adds to the plan-00 rig for this system).
2. **Scope & parity definition** — exactly which Mindustry feature set this covers; what “done” means; deliberate deviations (with reason). If something is deferred, say which plan owns it.
3. **Target design** — Rust crates/modules/types; Bevy schedule slots; Godot nodes/scenes; STDB tables/reducers/views touched; boundaries and invariants. State the **tscn-first split** (which nodes/scenes are declared in `.tscn` vs code-instantiated and why, per §6.6). Concrete names, not prose.
4. **Port map** — table: Mindustry file/class → target Rust module; notes on adaptation.
5. **Milestones & task breakdown** — ordered, each milestone verifiable through the spine/harness; state the smallest vertical slice first.
6. **Data & formats** — structs, IDs, serialization, schemas, asset formats, file paths.
7. **Oracle & verification (REQUIRED)** —
   a. **Ported tests**: Mindustry `tests/src/test/java/**` (and in-repo test helpers) → new `cargo test` names;
   b. **Headless harness scenarios**: `mind-headless` commands/seeds/expected state dumps/assertions;
   c. **MCP playtest scenario**: concrete open-godot-mcp steps (game launch, node paths or `godot_exec` evals, input, screenshot checks) — at least one;
   d. **Performance budget** + measurement method (which benchmark, what number);
   e. **Exit criteria checklist** — a ticking list.
8. **Risks & open decisions** — each with the default taken; mark load-bearing undecided items `NEEDS USER DECISION`.
9. **References** — exact files/dirs read.

Plans are living documents: check off milestones and append a short `## Changelog` when execution starts.

---

## 5. Phases & demo gates

| Phase | Plans | Gate (must be demonstrable before the next phase) |
|---|---|---|
| P0 Foundation | 00 | ✅ **GATE PASSED 2026-10-01.** Headless harness passes; in-engine spine runs (camera + grid + place/break); MCP can drive it; CI green. |
| P1 Platform & content | 01–04 | STDB connection + waves + relay skeleton; base content loads; assets pack/load; a save round-trips. |
| P2 Sim core | 05 | Deterministic 60 Hz tick runs N frames headless; event bus; entity groups; reset/play. |
| P3 World & systems | 06–09 | Generated world renders; blocks place/construct; conveyor moves an item; drill feeds; power/liquid flow. |
| P4 Combat, units, campaign | 10–12 | Turret kills a unit; waves spawn; campaign sector can be launched and saved. |
| P5 Logic, UI, input | 13–15 | mlog program runs; HUD/build menu usable; RTS commands work end-to-end via relay. |
| P6 Render, FX, audio | 16–18 | Full frame pipeline parity pass; effects play; music/ambient work. |
| P7 Editor, mods, export | 19–23 | Map editor round-trips; a data mod loads; desktop build exports; parity suite runs. |
| P8 Multiplayer | 21 | Two clients see each other’s builds/units; desync detector clean for a soak run. |

> **Phase tags are thematic, not an execution order.** Order comes from each plan’s `Depends on` header (D9): `22` (tagged P7) depends on `21` (P8), and `23` is continuous from P0. See §5.1 for the serial spine and parallel groups, and §5.2 for the subagent rules.

### 5.1 Execution graph: serial spine vs parallel groups

Reconciled against every plan’s `Depends on`/`Blocks` header (the headers are authoritative; the §3 column is a summary — rows 10/11 were corrected to match). A plan starts only when the **named milestones** of its dependencies are green, so some plans can start before a dependency’s full exit gate (exceptions at the end of this subsection).

**Serial spine (critical path):**

`00 → 02 → 04 → 05 → 06 → 07 → 09 → 10 → 11 → 12 → 14 → 15 → 21 → 22`

Branches are not optional — they are parallel prerequisites that must be green before the spine plan that consumes them (`01` before 21; `03` before 08/10/11/16/18/20; `08` before 09 M5/11/12; `13` before 14's logic-editor dialog; `16` before 17/19/21; `17` before 21; `18`/`20` before 21/22 pieces; `23` continuous).

| Group | Ready when | Run concurrently | Join / reason |
|---|---|---|---|
| F1 | 00 green | ✅ **DONE 2026-10-01:** `01` M0–M6 merged; `02` M1–M4 all merged (M3–M4 merged 2026-10-02) (+ `23` harness lane still open) | different crates: server/`mind-stdb` vs `mind-core` |
| F2 | 02 green | `03` + `04` | `04` may start against 02 stubs |
| F3 | 03 green → `18`; 03+04 → `20` | `18` + `20` alongside `05` | pull-forward lanes; `20` gates 14/19/21/22 pieces |
| F4 | 04 green | `05` | critical |
| F5 | 05 green | `06` | critical |
| F6 | 06 green | `07` | critical |
| F7 | 07 green | `08` + `09` + `16` | 08↔09 two-way contract, land it first; `10` waits on `09` |
| F8 | 09 green | `10` | 08/16 continue |
| F9 | 10 + 08 payload API green | `11` | critical |
| F10 | 11 + 08 + 09 green | `12` + `17` | 12 joins 08/09/10/11; 17 joins 10/11/16 |
| F11 | 12 green | `13` + `14` | 14's logic editor joins 13 |
| F12 | 14 green | `15` | critical |
| F13 | 15 + 16 + 17 green | `19` + `21` | 19 also uses 20's `DataManager`; 21 needs 15/16/17 |
| F14 | 21 green | `22` | M0–M2 may start much earlier (below) |
| cont. | 00 green | `23` | owns every gate report |

**Milestone-level early starts** (dispatch the subagent on the ready milestone, not the whole plan):
- `00`: M1 (engine hello) and M2/M3 (headless core) share no code and may run as two lanes after M0.
- `04`: most of the plan can land against the 02 stub registry (its header).
- `08`/`09`: land 08's `Autotiler`/`TileBitmask` contract before 09's liquid milestone; 09's power/module core unlocks 10 (heat/liquid milestones may trail).
- `22`: M0/M2 (version, args, headless server) are pure Rust and can start right after 00; M1 needs 00 + export templates; only M3–M7 wait on 21.

**Practical concurrency:** 2–3 subagents in F1–F3, 4–6 in F7–F13 (plus `23`); beyond that, merge conflicts and the shared MCP/STDB resources dominate.

### 5.2 Parallel-execution rules (subagents)

1. **One writer per artifact.** A subagent owns one plan file (its checkboxes/Changelog) and that plan’s module tree. Only the orchestrator edits `HIGH_LEVEL_PLAN.md`, `AGENTS.md`, other plan files, workspace `Cargo.toml`, shared `lib.rs` glue, `project.godot`, `.gdextension` and CI — schedule those edits between groups.
2. **Gate evidence first.** Dispatch a plan only after its dependency milestones are checked with evidence; join plans (`12`, `19`, `21`, `22`) also merge predecessor work and reconcile the "not written at plan-write time" contracts (09/10/11/13/14/17/21/22 headers).
3. **Isolate parallel lanes.** Use a git branch/worktree per lane when two lanes can touch the same package; merge at joins. In a shared worktree, keep to disjoint paths and never run repo-wide `cargo fmt`/`clippy` across another lane’s files.
4. **Single-resource mutexes (serialize across agents):**
   - MCP editor/game: one editor, bridge 127.0.0.1:6970 — queue all in-engine scenarios; the identity preflight in plan 00 §7c applies before every run. Headless oracles are concurrent-safe.
   - SpacetimeDB: one local `spacetime start`; every publish wipes data — one publisher at a time.
   - Godot import cache (`client/.godot/`): one editor/import at a time.
   - Cargo: concurrent builds are safe but serialize on the target-dir lock — don't run build storms from multiple lanes.
   - Ports: STDB 3000, server socket 6859, MCP bridge 6970, DAP 6006, LSP 6005, game 7070.
5. **Interface reconciliation rule.** When a predecessor’s real API differs from a consumer’s assumption, the consumer updates its plan first, then the code; the orchestrator records it in §12, never both sides silently.
6. **`23` gates every join.** Register new scenarios and run `parity gate --phase Pn` before dispatching the next group.

---

## 6. Cross-cutting conventions

### 6.1 Rust & module hygiene
- `rustfmt` defaults + `clippy` clean (workspace lints); modules mirror Mindustry package paths (`mind_core::world::blocks::production`), names `snake_case` of the Java class.
- Comments cite the ported source (`// Ported from core/src/mindustry/.../Turret.java:...`).
- No `unwrap()`/`expect()` on runtime data in sim paths; `Result` + structured logging. Panics only for violated invariants, and only in debug.
- Hot paths allocation-free (mirror the `Tmp`/`Pools` discipline with `SmallVec`, scratch buffers, ECS queries).

### 6.2 Generated code & schema rules
- Generated STDB client bindings are **checked in** at `client/rust/mind-stdb/src/module_bindings/` (clean builds need no `spacetime` CLI) and are **never hand-edited**; regenerate via `server/build.sh`, with `server/build.sh --check` as the CI drift gate (`01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` §6). Any other codegen output follows the same never-hand-edit rule.
- Entity/component field changes are a serialization change: a Rust revision entry is appended (mirror `annotations/AGENTS.md`), old revisions never edited.
- Content IDs per type: append-only. Content names/sprite regions/bundle keys are parity ABI (mods, saves).

### 6.3 STDB rules (module side)
- Follow `main/server/AGENTS.md` Rust rules (reducers deterministic, `ctx.sender()`, views vs tables, index naming, spread updates).
- Reducers never return data; clients read rows. Cheap validation only (D2).
- Every publish may wipe dev data; seeds live in code and re-run.

### 6.4 Licensing & attribution
- `LICENSE` = GPL-3.0; `THIRD_PARTY_NOTICES.md` lists Mindustry (and any addon ported/adapted) with license text.
- Every ported source file starts with: `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
- Ported assets keep their license/credit entries (Mindustry credits bundle).

### 6.5 Repo hygiene
- WSL2 Ubuntu-first dev (login shell; Windows 11 host, WSLg for GUI); cross-platform script twins: bash `build.sh` primary, PowerShell `build.ps1` parity for Windows.
- Keep each file’s existing line endings; new files LF.
- No secrets/tokens committed; STDB token files are gitignored.

### 6.6 Godot conventions (workflow rule, user 2026-10-01)
- **`.tscn`-first.** Prefer nodes and scenes declared in `.tscn` files over trees constructed in code. Static UI, world scaffolding, autoloads, menus and debug views belong in scenes, so a human can open the project in the Godot editor and navigate/inspect the game.
- **Rust owns behavior, not static scene construction.** `mind-gdext` adds dynamic children only when runtime data demands it.
- **Justify every code instantiation.** Any node created in code (spawned units/buildings/bullets, pooled FX, data-driven list entries) carries a `// code-instantiated: <specific reason>` comment (GDScript: `# code-instantiated: ...`) at the instantiation site. “Easier in code” is not a reason; “spawned by `WaveSpawner` at runtime; count/position are data-driven” is.
- Each plan’s §3 Target design states its tscn-first split; MCP oracles assert editor navigability for new screens.

---

## 7. Verification & tooling

### 7.1 Headless harness (`mind-headless`) — the spine oracle
- Commands: `run <scenario>`, `replay <command-log>`, `dump <path>` (JSON state), `bench`, `sim <ticks>`.
- Scenarios are Rust-registered fixtures (seed, world params, scripted commands, expected checksums/dumps). Every plan contributes scenarios named `{system}_{case}`.
- `cargo test -p mind-core` must not require Godot or a network.

### 7.2 In-engine (MCP) — the integration oracle
- Tools: `open-godot-mcp` (`godot_health`, `godot_game play/stop`, `godot_input`, `godot_exec`, `godot_runtime_state`, `godot_screenshot`, `godot_log`, `godot_editor_*`).
- The playtest skill at `/mnt/c/Users/Clinton/g/.opencode/skills/playtest/` currently contains **main-project** recipes; plan 00 creates the mindustry-godot analog (launch flow, node map, pid-stamp checks) as a repo skill.
- Godot static checks use the `godot-compositor-testing` skill (`/mnt/c/Users/Clinton/g/.opencode/skills/godot-compositor-testing/`): headless `--import` parse check for GDScript/scenes; windowed scripted screenshot runs.

### 7.3 SpacetimeDB
- Local: `spacetime start`; publish via `server/build.sh` (login shell); inspect with `spacetime sql/logs`. Dev DB wipes are expected.
- Server-side unit tests typecheck with `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` (cannot link in place — same constraint as `main/`).

### 7.4 Performance budgets
- Baseline: 60 tps ⇒ 16.6 ms/frame; target sim tick ≤ 4 ms at mid-game entity loads (measured by `mind-headless bench`).
- Each system plan states its own budget and the benchmark that measures it. Regressions block phase gates.

---

## 8. Addon evaluation policy

Per preliminary plan §2.4: evaluate, then keep, or rewrite in Rust where performance demands. Candidates and default posture:

| Addon | Default posture |
|---|---|
| BlastBullets2D | Evaluate in plan 10/17 for bulk bullet FX. If adopted, it is driven from sim data; bullet *simulation* stays in Rust. |
| LimboAI | Likely **not** used — Mindustry AI is bespoke deterministic controllers (plan 11); parity wins. |
| Phantom Camera | Evaluate in plan 15 for RTS camera; fallback is a small custom Rust camera rig. |
| TileMapDual | Likely **not** used — Mindustry is square-tile 47-slice autotiling (`TileBitmask`); implement in plan 16. |
| Minimap/fog addons | Evaluate in plan 16; fog needs sim parity (`FogControl`, plan 12). |

Addon adoption is recorded (with license) in `THIRD_PARTY_NOTICES.md`.

---

## 9. Parity ledger & known deviations

| Area | Deviation | Tracking |
|---|---|---|
| Java JAR mods / Rhino scripts | Cannot run JVM code; JSON data mods + patches are 1:1. Scripting is OD1. | §10, plan 20 |
| Steam Workshop / Discord RPC / Steam networking / achievements | Godot-specific integration; deferred. | §10 OD4, plan 22 |
| Android/iOS launchers | Godot export replaces Arc backends; Android touch input parity kept (plan 15). **iOS is cut from this phase** (user decision 2026-10-01, NUD-52=B). Store flows/robustness are export work. | plan 22 |
| `logicids.dat` (Anuken-only generation) | Skip; logic content lookups resolve by name. | plan 13 |
| Version/build checking | Own scheme; no cross-build with Java clients. | plan 01 |
| `.msav` byte compatibility with upstream Mindustry | Feature-parity own format by default; import shim is OD2. | plan 04 |
| Exact float determinism vs Java | Only Rust↔Rust determinism is required (all peers same build). | plan 21 |

---

## 10. Open decisions (deferred — defaults recorded)

> **All open decisions here and the 53-item NUD register were resolved by the user on 2026-10-01.** The authoritative per-item answer log is `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` §8.1.1. Defaults below are retained unless overridden there; the overrides are: renderer `mobile` everywhere, legacy C# tree deleted outright, standard Godot host + aliases, snake_case STDB names, hybrid shader translation, iOS cut from this phase. NUD-23 and NUD-25 were not explicitly answered and remain on their defaults.

| # | Decision | Default being planned against | Needs user? |
|---|---|---|---|
| OD1 | Mod scripting engine (replaces Rhino/JS) | JSON-only mods in 20; script hook kept behind a trait; candidate engines (GDScript, WASM, Lua) evaluated when 20 executes | **yes** at plan-20 execution |
| OD2 | Read upstream `.msav` saves/maps | Own versioned format first; optional importer later | yes |
| OD3 | Multiplayer transport | STDB relay for commands + events only (D2); bulk snapshot streaming over STDB event tables first, direct ENet/UDP only if measured too slow | yes at plan-21 execution |
| OD4 | Steam/Discord | Deferred to plan 22; optional | yes |
| OD5 | Mobile targets | Keep input parity (plan 15); export/polish deferred (plan 22) | no |
| OD6 | Addon adoption per §8 | Per-system evaluation | no |
| OD7 | Godot standard vs mono build | **DECIDED 2026-10-01; host updated to WSL:** standard 4.7 (`godot4` on PATH in WSL Ubuntu); mono alias `godot4-mono` is fallback | locked |
| OD8 | Crate/module names (`mind-core` etc.) | As in §2.1 | no |
| OD9 | Content JSON schema stability | Preserve upstream JSON field names/classes | no |

---

## 11. How to execute this plan set

1. Pick the lowest-numbered plan whose `Depends on` gates are green — or, with multiple agents, any ready group from §5.1 (gate evidence first; honor the §5.2 mutexes).
2. Read this file, the plan, then every source doc/AGENTS.md named in its header.
3. Implement milestone-order; after each milestone run the plan’s **Oracle & verification** commands (headless + MCP) and check the box.
4. Never mark a milestone done on intent — evidence (test output, dump, screenshot path) goes in the plan’s changelog.
5. If reality contradicts a plan, fix the plan first, then the code; if a locked decision is challenged, stop and ask the user.
6. Keep `PRELIMINARY_PLAN.md` untouched as history; this file is the single source of truth for scope and order.

---

## 12. Reconciliation log (2026-10-01, after the plan set was written)

The 24 plans were written in parallel, so cross-plan conflicts were expected. The following are now **locked**; where a plan’s text disagrees, this section wins until that plan is updated at execution. `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` §8 aggregates the ~40 `NEEDS USER DECISION` items (NUD-01…40) — treat that table as the live user-decision queue.

| # | Conflict | Resolution |
|---|---|---|
| C1 | Node paths: 05/08/09/11/12 used `/root/Main/*`; 00/23 define `/root/Spine/*`. | **Plan 00 §3.5 is authoritative.** Stale usage sites in 04/05/08/09/11/12 were normalized in this pass. Residual `/root/Main` mentions in 06/16/17/18/19/23 are historical conflict notes only; the executor fixes any remaining reference when the owning plan lands (23 R-01). |
| C2 | Sim checksum: 00 P0 xxh3 `DUMP_FORMAT` stream vs 05 canonical `Checksum` (FNV-1a, `CHECKSUM_VERSION`). | **05 is canonical.** 00’s stream is the P0 dump hash only; at 05 M8 it is replaced and all P0 goldens are re-recorded once. `parity/checksum_registry.json` owns versioning (23 R-02). |
| C3 | STDB bindings committed vs gitignored. | **Checked in** with `server/build.sh --check` drift gate (01 §6; noted in §6.2 above). |
| C4 | Duplicate upstream-test ownership. | **23 §7a-1/7a-2 is the arbiter** (one `primary` owner per upstream test; composition rows for the rest); see 23 R-04. |
| C5 | Data root: 00 OD-R9 (XDG app-data) vs 04 R3 (portable `./data`) vs 22. | **22 §6.1 resolves it**: app-data by default, portable `./data` when a marker/writable dir is present, `--data-dir`/env override, server `./config`; NUD-30 stays user-facing. |
| C6 | Fx ownership: 02 `fx_meta` seed table vs 17 catalogue vs 10 `CombatFx`. | **17 owns** the Fx registry/behaviors and is the single playback path; 10’s `CombatFx` re-exports 17’s `FxSink`; 02 keeps only the seed table (17 §3, 02 R6). |
| C7 | Music types: 12 `MusicContainerRef` vs 18 `MusicRef`; 05 missing `MusicRegisterEvent`. | **18’s `MusicRef` supersedes**; 05 adds `MusicRegisterEvent` at M6 (18 §8). |
| C8 | `mind-headless` server mode: 22 needs a lib target. | 00 extends `mind-headless` with a lib target + `server` mode at M1 (00 §3.10, 22 §3.6). |
| C9 | `Platform::autosave_allowed` (05 trait vs 12 policy). | Policy lives in **12**; 05’s trait method is a thin provider hook (12 §8). |
| C10 | `WorldContext` trait (04 defines, 06 implements); `WindowedMean` (08/09/12); building quadtree (09/11/12); `BaseRegistry`/`Waves` (06/11). | Single owner per plan: 04/06, 09, 09, 11 respectively; consumers import, never fork (23 R-03/R-09/R-13 and the plans’ cross-reference notes). |

**Canonical node paths** (plan 00 §3.5/§5; new nodes register in plan 00’s extension contract before use): root `/root/Spine`, scene `res://scenes/spine.tscn`; `/root/Spine/SimHost`, `/root/Spine/World/{TileGrid,Camera2D,Renderer}`, `/root/Spine/Ui/StateInspector/*`, `/root/Spine/{Input,MindUnits,MindCampaign,MindRender,MindFx,MindAudio,MindEditor}`; autoloads `/root/StdbConnector`, `/root/MindUi`, `/root/MindIo`.

**Plan-set status (updated 2026-10-02):** all 24 plans exist and follow §4. Plans 18–22 are now on disk (23 §8's "not on disk" note and 14/22's parallel-authorship notes are stale — the assumed interfaces should be reconciled against the real files at execution). All `NEEDS USER DECISION` items were resolved 2026-10-01 (register: 23 §8.1.1). Execution order from here is §13; `lane/02-m3` (02 M3–M4) is merged (2026-10-02) with goldens re-recorded — the in-engine MCP re-record and plan-01 §7.3 two-instance run remain queued on the MCP mutex. F2 is dispatched: `02` M5–M7 on `lane/02-m5`, `03` on `lane/03-assets`, `04` on `lane/04-io`.

---

## 13. Execution log (append-only; newest last)

**2026-10-01 — P0 COMPLETE (plan 00 M0–M8).** Spine, headless oracle (`mind-headless` + 4 canonical scenarios), STDB skeleton publishable locally, C# tree deleted, CI + repo playtest skill. Evidence in `00_FOUNDATION_IMPLEMENTATION_PLAN.md` Changelog. P0 goldens at that time: `spine_place_break` → `e53c9277bb8c28d1`, `spine_determinism` → `e435247bbe23afb1`.

**2026-10-01 — Plan 01 M0–M6 COMPLETE, merged to `main`** (branch `lane/01-stdb`). `mind-stdb` connector/waves/binders/relay client, server identity + relay schema (11 tables, 7 views), `StdbConnector` autoload (single pump per process; sim-host `--db` facade removed), `StdbBinder`, inspector net page, `stdb_*` headless scenarios, `mind-stdb/AGENTS.md` + plan-21 handoff. `mind-stdb` tests: 23 passed + doc-test; 3 ignored ITs pass with `MIND_STDB_IT=1`. Local DB is `mindustry` (integration `mindustry-it`); STDB 2.10.1 rejects underscores in *database* names, crate/module stays `mindustry_godot`. **Plan 01's own header still reads "Draft, not started" — stale; this log wins.** Deferred to the integrator: §7.3 MCP two-instance run on the merged tree; relay-throughput bench (plan 21 load harness).

**2026-10-01 — Plan 02 M1–M2 complete, merged to `main`** (branch `lane/02-content`); **M3–M4 complete on `lane/02-m3`, NOT yet merged.** M1: content framework + Items/Liquids/Statuses/Bullets/`fx_meta` (49 `mind-core` tests). M2: commands/stances/weathers/planets/sectors/loadouts + both tech trees verbatim via `parity/tools/gen_trees.py` (62 tests). M3: `BlockDef` framework + B1–B2 blocks (256) via `parity/tools/gen_blocks.py` + ledger `parity/ledgers/blocks.md`; **P0 goldens re-recorded** (`spine_place_break` → `375c68a53e861948`, `spine_determinism` → `52edd459bfa28b41`, `spine_many_commands` → `086e7c26935c2acb`) — in-engine MCP scenarios and `AGENTS.md`'s golden still quote the old values until re-recorded after merge. M4: all B3–B6 waves — **441/441 blocks, 0 unported** (68 tests); goldens stable across M4. Remaining: M5 (units), M6–M7; JVM golden still pending (NUD-10, M7 blocker).

**Open integration items (working tree, uncommitted):** `mind-gdext/src/stdb.rs` renames Godot-visible `connect`/`disconnect` to Rust `connect_db`/`disconnect_db` (`Object::connect` collision; Godot names pinned via `#[func(rename)]`); `client/project.godot` has a re-added `[dotnet] project/assembly_name` block (likely editor artifact — violates 00 M6, remove before commit).

**2026-10-02 — Integration + `lane/02-m3` merged; F2 dispatched.** Housekeeping commit `d988389`: the two open integration items above are resolved (`stdb.rs` rename committed; `[dotnet]` block removed from `client/project.godot`). `lane/02-m3` merged to `main` (`21734a5`, merge `--no-ff`): plan 02 M3–M4 — `BlockDef` framework, all **441/441 blocks** across B1–B6 (0 unported), `BlockKind` audit, ledger `parity/ledgers/blocks.md`, generator `parity/tools/gen_blocks.py`, re-recorded goldens (`spine_place_break` → `375c68a53e861948`, `spine_determinism` → `52edd459bfa28b41`, `spine_many_commands` → `086e7c26935c2acb`). Root `AGENTS.md` golden reference updated to match. `lane/02-m3` branch + old worktree pruned. **F2 lanes provisioned** (worktrees under `../mg-lanes/`): `02` M5–M7 (units, audit, JVM golden) on `lane/02-m5`; `03` assets on `lane/03-assets`; `04` IO/serialization on `lane/04-io`. Agents dispatch as soon as the `tools/ci.sh` gate on this merge reports green. Lane agents update this file (§3 row + §13) on their branches per milestone; the orchestrator reconciles §13 at each merge and runs `tools/ci.sh` as the join gate. Still queued on the MCP single-editor mutex: 02 M3–M4 in-engine MCP re-record, plan-01 §7.3 two-instance MCP run on merged tree.

**2026-10-02 — 03-M0 (lane/03-assets): workspace, migration, provenance.** New crates `mind-atlas` + `mind-tools` (registered in `client/rust/Cargo.toml` on the branch); `mind-core::assets` doc skeleton; `mind-headless assets migrate-check`. Vendored upstream `core/assets` + `core/assets-raw` (2977 files, 61M+4.2M, upstream commit `2cd7aeec`) into `assets/`/`assets-raw/` per NUD-11 with trim rules (`mind-atlas/src/migrate.rs`); provenance `assets/ASSET_PROVENANCE.md`, manifest byte-identical across two runs. Evidence in `03_ASSETS_IMPLEMENTATION_PLAN.md` Changelog. Deferred: `assets-pack` CI job (orchestrator owns CI), unit-icon generator passes (await `lane/02-m5` merge).

**2026-10-02 — 03-M1 (lane/03-assets): packer core + vertical slice.** `mind-atlas` packer landed: Arc `Pixmap`/`Pixmaps` ports, deterministic PNG codec, `pack.json` inheritance, ninepatch splits/pads, `stripWhitespaceCenter`, alias dedupe, MaxRects + BinarySearch page allocator with Arc's `rectanglesToCheckWhenPruning` fast path, page render (`duplicatePadding`, bleed) and §6.3 `sprites.atlas.json` (A1). `mind-core::assets::atlas` (`AtlasIndex`, `find`→`Option`, `loadIcon` chain); `mind-headless assets index`; `mind-tools pack` staging/pack/pack-fallback/manifest stages. Verified: 44+8 tests green; `pack_slice` byte-determinism; real pack → 4 pages (main/environment/rubble/ui); atlas JSON byte-identical across two full runs; probes `copper-wall`/`error` found, negative probe not found.

**2026-10-02 — 03-M2 (lane/03-assets): filename-only generators.** `Generators.run()` filename-only passes ported: autotiles (embedded layout, sha256-pinned), splashes, bubbles, gas-frames (Simplex tiled), cliffs (256 masks), cracks, edges (skeleton), scorches (`ScorchGenerator`, seeds pinned FNV-1a per §3.5 — upstream is time-seeded/non-reproducible). `PackAtlas` staging mutation (`save`/`replace`/`delete`, flattened names, duplicate-basename error), AA pass with the exact gradle skip predicate, `move-ui-icons`. `TileBitmask` table at `mind-core::world::blocks::tile_bitmask`. Full pipeline: 3764 regions on 4 typed pages (main 1554 / environment 1985 / rubble 96 / ui 129), manifest byte-identical across two runs. Unit-icon passes deferred (`lane/02-m5` not merged).

**2026-10-02 — 03-M3 (lane/03-assets): content-driven generators.** Metadata contract in `mind-tools/src/generate/metadata.rs` (from upstream sources): team palettes/`hasPalette`, per-block variants/blend groups/`wallOre`/shallow `liquidBase`+`floorBase`, autotile flags, `getGeneratedIcons` per class incl. `DrawBlock` drawer compositions and `PayloadBlock.findFactoryRegion` fallbacks, `getRegionsToOutline` (turret `RegionPart`s + PayloadMassDriver left/right/cap), `makeIconRegions` (empty in vanilla), `outlineColor`/`outlineRadius`/`outlineIcon`/`outlinedIcon`, `Vars.tilesize`. New passes: `block-icons` (outlines, per-team recolor with the three magic colors, `block-<name>-full`, `ui/block-<name>-ui` scaled to <=128, `block_colors.png` 441x1), `shallows`, `item-icons` (status tint + 6px container + `Pal.gray` outline), `sector-icons`, `team-icons` (derelict `b7b8c9`), `ore-icons`, and content-driven `edges`; the `icons.properties` writer is byte-stable/append-only (628 entries, +0) and `icon_codes.json` (138 glyphs) is written at pack time. `mind-headless assets regions --assert-complete` checks the generated `build/assets/region_inventory.json`: **2588/2588 expected regions resolve, 0 missing**. Full pack: **4632 regions on 4 pages** (main/environment/ui/rubble; fallback 7 pages), byte-identical across two full runs (`sprites.atlas.json` sha256 `a2cb204d…6528`, `inputsHash 548fed93…afb27`). Tests: mind-atlas 44, mind-core 73, mind-tools 18+2. **Deferred: `unit-icons` and all unit region/part/weapon/tread/segment metadata — plan 02 M5 (`lane/02-m5`) not merged (marked `UNIT_METADATA_DEFERRED` in `metadata.rs` and in the plan Changelog).**
