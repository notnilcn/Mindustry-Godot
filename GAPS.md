# GAPS.md — Mindustry-Godot vs upstream Mindustry

Code-only survey (2026-11-05) of `~/g/code_examples/Mindustry` (Java, ~823 files under `core/src`, plus `server/`, `desktop/`) against this repo (`client/rust/*`, `server/spacetimedb`, `client/shaders`). Ledgers, docs, runs and screenshots were not consulted; `.claude/worktrees` was ignored.

Path shorthands: `mc/` = `client/rust/mind-core/src/`, `gx/` = `client/rust/mind-gdext/src/`, `mh/` = `client/rust/mind-headless/src/`, `stdb/` = `server/spacetimedb/src/`.

Items marked ✔ were re-verified by hand against both trees; the rest come from file-by-file subsystem sweeps and should be confirmed before fixing.

## What is already at parity (for scale)

- **Content name sets and load order:** every block (447), unit (69), item, liquid, status effect, planet, sector preset, weather, Fx (267, lifetimes/clip identical), tech-tree node (223 Serpulo / 152 Erekir), loadout and UnitCommand/Stance exists with the Java name and order. Unit/item/liquid/status/block-requirement numeric fields match.
- **Logic:** all 54 `@RegisterStatement` opcodes, all `LExecutor` instructions, `LAccess`, `LogicOp`, `ConditionOp`, `LogicRule`, `LUnitControl`, `LogicFx`.
- **Editor:** all 28 classes, all 7 tools with alt modes.
- **Saves:** SaveIO/SaveVersion and every legacy reader (Save1–13, Legacy*, ShortChunk). These sit behind the `msav-import` feature, which mind-gdext turns on.
- **Rules:** every field; all 14 MapObjective and 10 marker types; all 41 Triggers; Universe turn/capture/invasion.
- **Input:** all 88 keybinds; `Placement` (bridges, lines); desktop input.
- **Shaders:** all 36 Java shaders except `shockwave`, which is also unused upstream.
- **Most block families:** power, drills, conveyors, ducts, bridges, MassDriver, StackConveyor, Unloader, storage, heat, sandbox, logic blocks, campaign blocks, Reconstructor, UnitAssembler, RepairTower.
- **Most UI:** about 62 of 84 ui classes, including CustomRules (all 84 rows), Settings (61 of 64 keys), Research, Database, Keybind, Host.

---

## P0 — gameplay-visible divergences in vanilla play

### Combat
1. ✔ **Lightning chains to buildings, not units.** `mc/combat/lightning.rs:81` uses `farthest_enemy_building`, but Java (`entities/Lightning.java:74-85`) uses `Units.nearbyEnemies` + `Geometry.findFurthest` on units. Also missing:
   - the insulated-building raycast;
   - the `hitter` air/ground filter, `pierceCap`, damage multiplier and `hitCreate`;
   - the random walk after the chain cap.

   This affects every lightning weapon, arc, and lightning-on-hit.
2. ✔ **Turret ammo has no `limitRange`.** Java `Turret.limitRange` (`Turret.java:248`) recomputes ammo lifetime from range. The Rust port has no equivalent, so ranges are wrong for duo, scatter, hail, swarmer, salvo, cyclone, spectre and six Erekir turrets. For example, duo copper lifetime is 60 in Rust vs 70 in Java, and salvo copper is 60 vs 83.6.
3. ✔ **Erekir turret-ammo rebalance is skipped.** `mc/content/tech/ekir.rs` `rebalance()` part (b) is a documented no-op. Breach, diffuse, sublimate, titan, disperse, afflict, lustre, scathe, smite and malign ammo do about 33% too much damage (breach beryllium is 85 vs 63.75).
4. **Turrets never stop consuming.** `TurretBehavior` has no `should_consume` override, and neither do ShockwaveTower, RegenProjector or BuildTurret. Power and coolant drain while idle. Java overrides it in Turret, ContinuousTurret, LaserTurret, PointDefense, TractorBeam and ContinuousLiquid.
5. **`cheating()` (team cheat rule) is ignored** by turret ammo and heat logic: there is no infinite ammo, no heat bypass, and no ItemTurret auto-fill.
6. **Continuous and point lasers:**
   - `mc/combat/bullet/kinds/continuous.rs` ignores the ContinuousLaser `fadeTime` and ContinuousFlame `lengthInterp`.
   - PointLaser (lustre) does line damage from the muzzle instead of `Damage.collidePoint` at the aim point. `collidePoint` has no port.
7. **LiquidTurret fire extinguishing** (`findTarget` scanning `Fires`) is missing.
8. **TractorBeamTurret** subtracts health directly instead of using `damageContinuousPierce` (no shields, death path, `blockDamage`), and can't distinguish air from ground targets.
9. **Weapons:**
   - `shootStatus`/`shootStatusDuration` are parsed but never applied (vela and the quasar/conquer lines);
   - `ignoreRotation` is unused;
   - unit ammo (`rules.unitAmmo`, `ammoPerShot`, `ammoType`) is entirely absent.
10. **Movement damage:**
    - legs: `legSplashDamage` and the leg-death `dynamicExplosion` are missing (arkyid, toxopid, Erekir legged units);
    - crawlers: `crushDamage` is missing;
    - tanks: the crush ignores `rules.unitDamage` and `disarmed`, and bypasses `build.damage`.
11. **Bullet content-ID space differs.** Turret ammo is installed after load by `install_turret_ammo`, in a non-Java order, from hand-written tables in `mc/world/blocks/defense/turrets/{mod,advanced}.rs`. Consequences:
    - there are two synthetic extras (`tractor_dummy`, `pointdefense_dummy`);
    - arc and smite probably each lack a nested lightning bullet;
    - ammo visuals (width, height, effects, colours, trails) are dropped;
    - mods and patches can't reach the ammo.

### Defense blocks
12. **BaseShield** (`mc/world/blocks/defense/projectors.rs`):
    - it does not repel or kill units, because the `unitConsumer` is missing;
    - it despawns every enemy bullet instead of only `absorbable` ones;
    - it wrongly absorbs explosions (`mc/combat/damage/explosion.rs:292`).
13. **BuildTurret** works only through explicitly queued plans, and nothing outside tests queues them. Missing: the team rebuild-queue scan, following nearby builders, deconstruct, and the `shouldConsume` check.
14. **RegenProjector:**
    - there is no `mendMap` anti-stack, so overlapping projectors stack heals;
    - heal is not scaled by `edelta`;
    - the phase item is never consumed.
15. **Logic and pickup hooks:**
    - Door `control(enabled)` and AutoDoor `sense(enabled)` are missing;
    - ForceProjector `setProp(shield)`, `overwrote` and `pickedUp` are missing;
    - `BuildingBehavior` has no `picked_up` hook at all, which also affects ShieldWall.
16. **ShockMine:** lightning colour is WHITE where Java uses `Pal.lancerLaser`, and the optional bullet burst is missing.

### Production, logistics and storage
17. ✔ **ArmoredConveyor / ArmoredConduit accept side input from non-conveyors.** The `armored` flag only affects blending. `accept_item` in `mc/world/blocks/distribution/conveyor.rs` and `accept_liquid` in `liquid/movement.rs` lack the Java gate.
18. **AttributeCrafter gets no attribute boost** (cultivator, silicon crucible). `efficiency_scale` in `mc/world/behavior/production.rs:~242` returns only `base_efficiency`.
19. **Fracker never consumes its item:** there is no `itemUseTime` accumulator and no `lastPump` reset.
20. **WallCrafter** has no liquid boost and no item boost, and its attribute is discarded.
21. **Core items are not shared across a team's cores.** Java does `items = other.items`; the port only sums capacity (`mc/world/blocks/storage/core_block.rs:236`, flagged "R2 NEEDS USER DECISION").
22. **ConsumePowerDynamic is missing.** The power graph always requests the static usage, so PayloadLoader charging is wrong.
23. **Unit payloads are a stub:**
    - `dump_unit_payload` always returns false (`payload_block.rs:293`), so unit payloads never leave a block;
    - unit payload IO returns `Unsupported`, which stops payload-block reads, including in saves;
    - PayloadSource never creates units;
    - UnitFactory spawns its unit directly instead of moving out a `UnitPayload`, so the factory→reconstructor payload chain is not 1:1.
24. **Legacy blocks** (LegacyBlock, MechPad, UnitFactory, CommandCenter) are never removed or replaced on map load. `MapHooks::legacy_remove_self` is never called, and their `read()` byte-skips aren't ported, so old saves may misparse.

### World processor and logic
25. ✔ **The `bullet` instruction (`SpawnBulletI`) spawns nothing.** The event is emitted (`mc/logic/world.rs:1233`) but `mc/sim/mod.rs` has no consumer arm. There is also no Bullet `LogicObject` and no bullet sense/`setprop`.
26. **Unit sense** returns 0/null for camera, ping, selected block and rotation, breaking, and color. Building camera sense is always 0.
27. `payEnter` is an empty arm. `sync` and `clientdata` events have no network sink.

### Map generation
28. **`trimDark` passes a darkness of 0** (`serpulo.rs:1647`, `erekir.rs:717`, `tantros.rs:190`), so sector-edge walls differ from Java.
29. **`addWeather`** leaves out the sector-circle term because Arc's 2-D noise isn't ported.
30. The **CoreSpawnFilter** and **SpawnPathFilter** map filters are no-op stubs (`mc/maps/filters/builtin.rs:1517,1656`). RandomItemFilter is also a stub, but it is unused upstream.
31. **Sector submission order:** `registerSurvival(202)` runs after the megabases in `sectors.rs:398-425`, which shifts 8 sector-preset IDs.

---

## P1 — multiplayer and server

32. **The dedicated server doesn't run a game.** `mh/server/host.rs` `LocalHost::host` loads a `FixtureWorld`, not a real Sim:
    - built-in and sector maps fail with `NeedsWorldLoader`;
    - `runwave` only increments a counter;
    - `fillitems`, `shuffle`, `nextmap`, `gameover`, `reloadassets` and `mods` are message-only stubs;
    - map rotation, `roundExtraTime` and auto-pause can't happen.
33. **Missing pieces of NetServer:**
    - `assignTeam` (PvP balancing);
    - `clientSnapshot`-style movement validation (`stdb/relay/player_state.rs` only checks seq, finite floats and rate);
    - chat commands `/help`, `/sync`, `/votekick`, `/vote`;
    - an admin chat channel (`ChatKind` is All/Team/System only);
    - `fixName`, `checkColor`, `kickAll`.
34. **Missing pieces of Administration:**
    - action filters (`addActionFilter`/`ActionType`);
    - PlayerInfo history (names, IPs, join/kick counts) and `findByName`/`searchNames`;
    - trace info and `adminRequest(trace)`;
    - the kick cooldown;
    - name-ban enforcement (`banned_name_patterns` is stored but never enforced);
    - enforcement for `enableVotekick`, `interactRateLimit` and `messageSpamKick`;
    - `checkUuidChanges`.
35. **Server→client `@Remote` calls with no channel:** `infoMessage`, `followUpMenu`/`hideFollowUpMenu`, `setCameraPosition`, `sound`/`soundAt`/`effect`, `playMusic`, `traceInfo`, `removeWorldLabel`, and the `*Reliable` menu variants. There is no `KickReason` enum; kick reasons are free-form strings.
36. **TypeIO network-only codecs:** Unit, Building, Tile, PosEntity, Entity, Controller, Kick, TraceInfo, Sound, MenuResult, MarkerControl, UiBuilder, StringArray, Action.
37. **EventType:** only about 28 of about 95 event classes exist, and there is no subscribe bus. Missing include:
    - Unit Create/Destroy/Damage/Drown/Spawn/Change/Control;
    - Player Join/Leave/Chat/Connect/Ban/Unban;
    - Config, Deposit, Withdraw, Pickup, PayloadDrop, CoreChange, LaunchItem, Research, Unlock, BuildSelect;
    - the BulletDestroy pair, GeneratorPressureExplode, MenuOptionChoose, TextInput, ServerLoad, SchematicCreate, MapMake.
38. **UnlockableContent hooks** are missing: `onUnlock`, `showUnlock`, `logicVisible`, `unlockedNowHost`/`unlockedHost` (a no-op at `mc/sim/builder.rs:1052`), and `getDependencies` on the trait. `unlock()` doesn't fire `UnlockEvent`.

## P1 — UI

39. **PlayerListFragment** is a stub (`gx/ui/fragments/player_list.rs`). Its data is always empty (`RelayUnavailable`), and it has no per-player ban/kick/trace/team/admin menu, votekick prompt, count header, avatars or icons.
40. **Bans, Admins and Trace dialogs** (`gx/ui/dialogs/misc.rs`):
    - Bans and Admins have no unban/unadmin buttons or confirms;
    - Trace has no IP/name lists and its strings are unformatted.
41. **SchematicsDialog:**
    - there is no tag editor (rename, delete, reorder), no icon tags and no per-schematic tags;
    - the edit view has no description field;
    - import and export are clipboard-only, with no `.msch` file support and no export dialog.
42. **HudFragment:**
    - the in-game editor-mode panel is missing (team selector, block palette with search and favorites, `setPlayerTeamEditor`);
    - the toast queue is missing;
    - `rules.unitfactoryactivation.objective` is missing.
43. **PlacementFragment** has no `getUnplaceableReason` (no-builder, environment, banned and block-limit messages) and no `block.unknown` header for locked blocks.
44. **Mods and ModBrowser dialogs:** missing the release picker, the dependency status dialog, view-content, and the incompatibility and version messages.
45. **JoinDialog** shows no server version or outdated strings and has no LAN discovery.
46. Smaller UI gaps:
    - GameOverDialog has no multiplayer waiting or disconnect states and no `difficulty.guide`;
    - LoadDialog doesn't handle corrupted saves or missing assets;
    - the `savefail` dialog is missing;
    - the Planet dialog has no SectorSubmissions link;
    - the minimap has no ping (`Binding.ping`, `Call.pingLocation`);
    - PlanConfigFragment has no caller;
    - MapLocalesDialog is an empty placeholder (`batch_e.rs:5070`);
    - MapObjectivesDialog edits some fields as raw text;
    - EffectsDialog shows no live preview;
    - Settings has no ESC back-navigation;
    - the content-picker and sound-picker in the logic statement editor are text fields.

## P1 — rendering, input, audio

47. **Input selection visuals are not drawn.** `PlacementView` exposes only the cursor, line plans and break rect, so these are missing:
    - the schematic copy rect and paste preview ghosts;
    - the rebuild-select rect;
    - the RTS command drag box;
    - `drawOverlapCheck`;
    - other players' plans.
48. **Mobile line placement** passes `NoLineHooks` (`mc/input/mobile.rs:660`, `line.rs:173`), so it has no junction replacement, bridge auto-link or node chaining. `get_plan` matches the exact tile, not the footprint.
49. **Lighting** draws circles only. Region lights, line lights (lasers, continuous bullets) and custom lights are missing, and per-block light overrides use Java defaults.
50. **Planet view** (`gx/ui/planet.rs`):
    - no sun, other planets or moons are drawn;
    - the skybox is a billboard instead of a cubemap;
    - there is no planet bloom;
    - the `cubemap`, `mesh`, `unlit` and `unlitwhite` shaders are dead.
51. **LoadRenderer** is essentially a stub: 58 lines against Java's 524.
52. **FloorRenderer** doesn't apply 47-slice autotiling and draws floors at TILESIZE instead of the region's natural size. Team overlays in the building cache are deferred.
53. Smaller rendering and input gaps:
    - DebugCollisionRenderer has no unit hitboxes;
    - the full-map screenshot (`ScreenshotPlan`) is unused;
    - possibly `fluidFrames` (animated liquid regions);
    - the Drawf dash helpers and `flameFront` are missing;
    - MenuRenderer simplifies its flyers;
    - fx noise, shader and polyline primitives are deferred (`gx/fx.rs:797`);
    - ColoredFloor/ColoredWall tint is never applied in the renderer;
    - environment-prop draws are unconfirmed: SeaBush, Tree, TallBlock, Cliff, Rune/Character overlays and SteamVent fx.
54. **Audio:** `SoundLoop` per-key fade voices are folded into one mixer, the BurstDrill ambient factor is deferred, and LaunchPad has no sound, shake or fx.

## P2 — data-driven and mod parity

55. **Block behaviour is dispatched by vanilla name, not class.** Any mod or JSON block of a class without a name registration gets `NoopBehavior` (`mc/world/behavior/mod.rs:560,714-765`). That includes mod turrets, conveyors, routers, conduits and crafters.
56. **Per-family knobs are not in content data.** Values such as `craftTime`, outputs, drill tier and time, pump amount and conveyor speed come from `apply_vanilla_knobs` (`mc/world/block_kind_data.rs:633,773`), keyed by about 64 block names. Mod blocks get empty defaults, and patches can't change them.
57. **Draw chains** are hand-written for about 15 vanilla crafters (`vanilla_chain`). Parsed `DrawerKind` specs are never converted to `DrawDesc`, so the Erekir crafters and every mod block fall back to DrawDefault. DrawPumpLiquid and DrawBlockParts have no implementation. `world/draw.rs` is a third, unclear framework.
58. **Consumers:** only Items, Liquid, Liquids, Power and Coolant are generic. These have no generic runtime: ItemFilter, Flammable, Radioactive, Explosive, Charged, List, Efficiency, Explode, ItemDynamic, LiquidFilter, LiquidFlammable, LiquidsDynamic, PayloadDynamic, PayloadFilter, Payloads, PowerDynamic and PowerCondition.
59. **ContentParser** (`mc/mods/json/mod.rs`):
    - it reads only about 30 base Block fields and about 30 unit fields, and no subclass fields;
    - `research` tech-node insertion is not handled;
    - named bullet references are dropped;
    - bullet `spawnUnit` is ignored;
    - ParticleEffect (the default effect type), RadialEffect, SeqEffect, SoundEffect and NoiseEffect are missing.
60. **DataPatcher** covers about 14 block fields and a few unit fields; Java patches any field by reflection.
61. **Abilities and unit components:**
    - ArmorPlate, MoveLightning, UnitSpawn and EmptyData abilities are missing;
    - Segment, WaterCrawl, UnderwaterMove, Shielder and Child are markers only;
    - LaunchCoreComp is missing;
    - TiledFloor, TiledWall and WobbleProp have no `BlockKind`;
    - ShockMine bullet bursts are missing;
    - SingleBlockProducer is missing.
62. **Stub helper traits:** RotBlock, UnitTetherBlock, ExplosionShield and ControlBlock are declared but have no implementors. Missing `Block` methods: `placeEnded`, `flipRotation`, `getPlanConfig`.

## P2 — AI and indexing

63. **BlockIndexer** is rebuild-based, with no incremental `addIndex`/`removeIndex`. `getFlaggedFloors`, `getAllPresentOres`, range `eachBlock`, `allBuildings` and `findTile` are missing.
64. **BuilderAI** doesn't scan for build-turret hold positions.
65. **BaseBuilderAI** returns core fill and unit spawn as actions, and it's unconfirmed that the host executes them.
66. **PrebuildAI** is a stub, but upstream marks it "not functional".
67. Smaller AI gaps:
    - a logic-controlled turret's missile doesn't get its shooter set (`combat/bullet/spawn.rs:174`);
    - `block.targetable` is omitted (`sim/unit_runtime.rs:5768`);
    - `tryRepairDerelict` is deferred in the input layer;
    - the Schematics loadout path doesn't sort by `schematicPriority` (`mc/game/schematics.rs:231`).

## P3 — platform services (some intentionally dropped)

68. **Achievements and stats:** `Achievement` (69 entries), `SStat` and the `GameService` tracking logic are absent. `NullService` is a no-op.
69. **Steam:** SNet lobbies, SStats, SUser, SWorkshop (a stub returning empty) and the Maps Workshop publish methods are missing. Discord is a stub.
70. **BeControl updater** is stubbed on purpose. PerfCounter, IntelGpuCheck and NvGpuInfo are missing. `async_work/physics.rs` is a dead leftover stub; the real physics is in `sim/unit_runtime.rs`.
71. **Documented deviations (decide whether they stay):**
    - JS scripting (`Scripts.java`) is replaced by `NoScriptHost`, so script mods are marked Unsupported;
    - Java mods and plugins (`Mod`, `Plugin`, `ModClassLoader`) are JSON and assets only;
    - IP and subnet bans are dropped under STDB;
    - the community server list isn't fetched from the Java URLs;
    - Pixelator resamples the screen instead of rendering to a second buffer.

## Stale comments that misreport status (cleanup)

These comments say something is missing that is actually implemented:
- `mc/maps/generators/planet/serpulo.rs:8,1787,1814` and `asteroid.rs:10` say ruins and loadout are skipped; they are implemented in `place_structures`.
- `mc/combat/bullet/kinds/mod.rs:6` and `kinds/lightning.rs:6-8` call the kinds "not yet" done or "inert".
- `mc/sim/unit_runtime.rs:6213` says Predict is "not ported".
- `mc/ai/rts_ai.rs`, `mc/ai/ai_controller.rs` and the `mc/ai/block_indexer.rs` headers describe pieces as plan-deferred that now exist.
- `gx/render/env.rs` says "tint only".
