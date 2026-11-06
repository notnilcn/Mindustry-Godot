# GAPS.md — Mindustry-Godot vs upstream Mindustry

Code-only survey (2026-11-05) of `~/g/code_examples/Mindustry` (Java, ~823 files under `core/src`, plus `server/`, `desktop/`) against this repo (`client/rust/*`, `server/spacetimedb`, `client/shaders`). Ledgers, docs, runs and screenshots were not consulted; `.claude/worktrees` was ignored.

Path shorthands: `mc/` = `client/rust/mind-core/src/`, `gx/` = `client/rust/mind-gdext/src/`, `mh/` = `client/rust/mind-headless/src/`, `stdb/` = `server/spacetimedb/src/`.

Items marked ✔ were re-verified by hand against both trees; the rest come from file-by-file subsystem sweeps and should be confirmed before fixing.

> **Status 2026-11-06.** This survey is code-only and several entries have gone
> stale. A follow-up audit re-checked every item against HEAD and the twin
> ledger; items fixed since are marked `**FIXED ...**`, and the [status
> appendix](#status-after-the-2026-11-0506-audit--fix-pass) at the end lists the
> corrected verdicts, the twin-verification items, and what remains open.

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
4. **FIXED 2026-11-06 — PAR-0471, twin-verified.** **Turrets never stopped consuming.** `TurretBehavior` had no `should_consume` override, and neither did ShockwaveTower, RegenProjector or BuildTurret, so power and coolant drained while idle. The port now mirrors `Turret.java:370-372` and the Continuous/ContinuousLiquid/Laser/PointDefense/Tractor/Shockwave/Regen/Build overrides (10 tests), including the class-hierarchy detail that PointDefense/Tractor `super` is `enabled`, not the turret reload rule.
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
12. **CODE FIXED 2026-11-06 — PAR-0475 (twin blocked on the Java reference asset/launcher setup).** **BaseShield** (`mc/world/blocks/defense/projectors.rs`):
    - it now repels/kills overlapping enemy units (`unitConsumer`: zero velocity + move out, instakill deeper than `hitSize * 1.5`, `Fx.circleColorSpark`);
    - it absorbs only `BulletDef.absorbable` enemy bullets (non-absorbable and friendly bullets pass through);
    - it no longer absorbs explosions (`mc/combat/damage/explosion.rs` now only consults `ForceProjector`, matching Java).
    Tests cover all three; the Java twin leg is blocked because `shield-projector` is `editorOnly` and the Java reference editor picker omits it (missing `block-shield-projector-ui` atlas region in this stripped build).
13. **BuildTurret** works only through explicitly queued plans, and nothing outside tests queues them. Missing: the team rebuild-queue scan, following nearby builders, deconstruct, and the `shouldConsume` check.
14. **RegenProjector:**
    - there is no `mendMap` anti-stack, so overlapping projectors stack heals;
    - heal is not scaled by `edelta`;
    - the phase item is never consumed.
15. **Logic and pickup hooks:**
    - Door `control(enabled)` and AutoDoor `sense(enabled)` are missing;
    - ForceProjector `setProp(shield)`, `overwrote` and `pickedUp` are missing;
    - `BuildingBehavior` has no `picked_up` hook at all, which also affects ShieldWall.
16. **CODE FIXED 2026-11-06 — PAR-0476 (twin pending a rebuild).** **ShockMine:** lightning colour was WHITE; it now uses `Pal.lancerLaser` and the colour is plumbed through `LightningResult` into the `Fx.lightning` event so the renderer actually draws it (previously the port emitted no lightning FX at all). Vanilla `shockMine` leaves `bullet` null, so the optional bullet burst does not apply; `shots` stays inert. Tests: `shock_mine` 4, `lightning` 6, `fx` 113, full `mind-core` lib pass.

### Production, logistics and storage
17. **FIXED 2026-11-06 — PAR-0469, twin-verified.** ✔ **ArmoredConveyor / ArmoredConduit accepted side input from non-conveyors.** The `armored` flag only affected blending. `accept_item` in `mc/world/blocks/distribution/conveyor.rs` and `accept_liquid` in `liquid/movement.rs` now carry the Java source gate (`ArmoredConveyor.java:31-33`, `ArmoredConduit.java:23-27`), with `LiquidNode.armored` and an exact `Tile.absoluteRelativeTo`.
18. **AttributeCrafter gets no attribute boost** (cultivator, silicon crucible). `efficiency_scale` in `mc/world/behavior/production.rs:~242` returns only `base_efficiency`.
19. **Fracker never consumes its item:** there is no `itemUseTime` accumulator and no `lastPump` reset.
20. **CODE FIXED 2026-11-06 — PAR-0474 (twin blocked on the Java reference setup).** **WallCrafter** had no liquid boost and no item boost, and its attribute was discarded. `WallCrafterBehavior` now ports `getEfficiency`, the `lerp(1, liquidBoostIntensity, optionalEfficiency)` and `itemBoostIntensity` terms, the graphite booster timer/`consume()`, `lastEfficiency`/`warmup`/`totalTime`, and the new `drillspeed` bar. Godot shows 7.50/s → 12.00/s with graphite; the Java leg is blocked on building a sand-wall/crusher map under synthetic editor input (see the appendix).
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
43. **FIXED 2026-11-06 — PAR-0468, twin-verified.** **PlacementFragment** had no `getUnplaceableReason` (no-builder, environment, banned and block-limit messages) and no `block.unknown` header for locked blocks. Java `PlacementFragment.java:380-432,753-761,779-782`; the port now renders the scarlet cancel row and gates the header/`?` on `unlocked()`.
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

47. **PARTIAL FIX 2026-11-06 — PAR-0472, twin-verified (RTS command drag box).** **Input selection visuals are not drawn.** `PlacementView` exposes only the cursor, line plans and break rect, so these are missing:
    - the schematic copy rect and paste preview ghosts;
    - the rebuild-select rect;
    - ~~the RTS command drag box~~ — fixed: the live signed `command_rect` is tracked and the plans overlay fills it `Pal.accent` @ 0.3 (`CommandOverlay`), Java `InputHandler.java:1354-1368`;
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

---

## Status after the 2026-11-05/06 audit + fix pass

Two read-only audits re-checked every item above against HEAD `9b04a6af` and the
twin ledger (`TWIN-VERIFICATION.json`, 467 items at the start of the pass). Full
session reports: `/tmp/gaps-audit-a.md` (#1–#31) and `/tmp/gaps-audit-b.md`
(#32–#67); these are session-local and not committed. Verdict counts:
**#1–#31: OPEN 23, PARTIAL 7, FIXED 0, BLOCKED 1** (the #21 core-item
aliasing decision); **#32–#67: OPEN 15, PARTIAL 14, FIXED 2, depth-limited 5**.

### Fixed and twin-verified in this pass

| Gap | Fix | Twin item | Evidence |
|---|---|---|---|
| 17 ArmoredConveyor/ArmoredConduit side input | Source gate in `conveyor.rs::accept_item` and `movement.rs::accept_liquid` (+ `LiquidNode.armored`, `Tile.absoluteRelativeTo`), with 11 source-gate tests | PAR-0469 | `runs/PAR-0469/`, `parity/twin/PAR-0469/` |
| 43 PlacementFragment unplaceable reason | `unplaceable_reason()` / `placement_unlocked()` in `mind-core::world::build`, `block_placement_info_json`, scarlet cancel row + `block.unknown` header | PAR-0468 | `runs/PAR-0468/`, `parity/twin/PAR-0468/` (similarity 92.4) |
| 4 Turret/defense idle consumption | `should_consume` overrides for Turret/Continuous/ContinuousLiquid/Laser/PointDefense/Tractor/Shockwave/Regen/Build, 10 tests | PAR-0471 | `runs/PAR-0471/`, `parity/twin/PAR-0471/` (similarity 87.5; hover-table caveat now fixed by PAR-0473) |
| New: Custom Rules Edit overlay stall | `defer_dialog_action()` queues every custom-rules dialog bind out of its signal (`batch_i.rs`); `MindDialogSweepCheck` regression test | PAR-0470 | `runs/PAR-0470/`, `parity/twin/PAR-0470/` (similarity 90.6) |
| New: placement hover table never rendered | `AnyDictionary` read of `hover_info_world`/`hover_info` in `placement.rs`, with `has_info_box`/`top_state_key`/STOP-guard predicates and tests | PAR-0473 | `runs/PAR-0473/`, `parity/twin/PAR-0473/` (similarity 90.2) |
| 47 (sub-item) RTS command drag box | `command_draw_rect` normalization + live signed `PlacementView.command_rect` + `CommandOverlay` accent fill; held-drag tests | PAR-0472 | `runs/PAR-0472/`, `parity/twin/PAR-0472/` (similarity 86.9) |

### Code-complete, twin-blocked (Java reference setup)

| Gap | Fix | Item | State |
|---|---|---|---|
| 20 WallCrafter/cliff-crusher boosts | Full `updateTile` port: attribute sum, liquid/item boosts, graphite timer/consume, `lastEfficiency`/`totalTime`, `drillspeed` bar; 6 tests + full `cargo test -p mind-core` pass | PAR-0474 | Godot frames show `Drill Speed 7.50/s` → `12.00/s`; Java leg blocked: the sandbox map has no sand walls, the editor `par0474` map has the wall row but synthetic map-view placement was unreliable, and the restarted client initially refused synthetic clicks. Java client was left in the editor map. |
| 12 BaseShield bullets/units/explosions | `absorbable` filter, unit repel/kill + spark, explosion absorber removed; 8+2+1 tests, full `cargo test -p mind-core` pass | PAR-0475 | Java frames captured: `runs/PAR-0475/java/20261106-105144-3{0,1}-wave-dome-enemies.png` show the Barrier Projector dome around the Tanks core during Wave 3 with enemies at the base edge. Godot leg pending (the game was paused by a debugger break; restart + place `shield-projector` + `power-source` at the core, start Wave 1, capture). |
| 16 ShockMine lightning colour | `ShockMineState.lightning_color` = `Pal.lancerLaser`; `LightningResult`/`Fx.lightning` colour payload + renderer consumption; `shock_mine` 4/`lightning` 6/`fx` 113/full lib pass | PAR-0476 | Build + editor restart pending; then place a shock-mine in the sandbox, walk the player mech onto it, burst-capture the 4 tendrils in both clients (Java lancerLaser blue; pre-fix Godot emitted no bolt). |

### Confirmed already fixed at HEAD (stale in this survey)

- **#9** `ignoreRotation` is used exactly where Java uses it (`LogicAI.java:120`);
  `rules.unitAmmo`/`ammoPerShot`/`ammoType` do not exist in this Java snapshot
  (`UnitType.ammoCapacity` is `@Deprecated`) — drop that sub-bullet.
- **#10** leg-death explosion radius (`sim/unit_runtime.rs:611`) and the tank
  crush `rules.unitDamage` + `crushDamageMultiplier` path (`unit/tank.rs:153-181`)
  are implemented; leg splash and crawler crush remain.
- **#11** arc/smite nested lightning ammo exists (`turrets/advanced.rs:385,848`);
  content IDs/visuals/ordering remain.
- **#13** BuildTurret has a live `TurretBehavior` and queued-plan path
  (`defense/behaviors.rs:802`, `turrets/mod.rs:2040-2104`); the team rebuild
  scan, following, deconstruct and `shouldConsume` remain.
- **#26** `@building` filters `breaking` (`logic/access.rs:1168`); camera/ping/
  colour/selected-rotation senses remain.
- **#38** the unlock event and toast producer exist
  (`game/runtime.rs:590-600`, `campaign.rs:2421-2432`); `logicVisible`,
  `unlockedNowHost` and `getDependencies` remain.
- **#42** the toast queue (`ui/root.rs:700-745`) and the
  `rules.unitfactoryactivation.objective` line (`ui/hud_text.rs:95-102`) are
  implemented; the in-game editor HUD panel remains.
- **#44** Mods state/detail rows, the update row, the dependency-download row and
  `@mod.requiresversion` exist; the release picker and view-content remain.
- **#46** Settings Escape/back-navigation (`batch_a.rs:403,418,476`) and the
  MapLocalesDialog controller (`batch_e.rs:5067`) are implemented; the remaining
  sub-items (game-over waiting/disconnect, savefail, minimap ping, …) are open.
- **#48** desktop line placement (`line_rules.rs`, `desktop.rs:2905-2955`) is
  implemented; only the mobile `NoLineHooks` path and the footprint `get_plan`
  remain.
- **#52** floor autotiling/tiling, `drawEdges`/`drawBlended` and the liquid
  overlay (`render/floor.rs:670-760`) are implemented; the building-cache team
  overlay remains.
- **#66** PrebuildAI is a real state/path port (`ai/types/prebuild.rs`).
- **#67** `tryRepairDerelict` (`sim/builder.rs:1049-1060`) and the
  `schematicPriority` sort (`game/schematics.rs:979-1000`) are implemented; the
  control-block shooter and `block.targetable` remain.

### Newly found during the pass (not in the original survey)

- **FIXED 2026-11-06 — PAR-0470, twin-verified.** The Godot Custom Rules
  **Edit overlay stalled the frame loop**: the `@edit`/search/overlay buttons
  bound the dialog mutably from inside their `pressed`/`text_changed` signals,
  a nested `Gd::bind_mut()` that gdext catches and turns into a pinned frame
  loop. `batch_i.rs` now queues every such bind through `defer_dialog_action()`
  and `MindDialogSweepCheck` reproduces the nested-bind hazard headlessly.
  PAR-0468's Godot leg used the rules-JSON seed as the documented fallback
  before this fix.
- **FIXED 2026-11-06 — PAR-0473, twin-verified.** The placement fragment's
  hover top table never rendered for a hovered world building: `hover_info_world`
  returns a `Dictionary<GString, Variant>` and the strict
  `try_to::<VarDictionary>()` rejected the gdext typed-dictionary metadata, so
  the hover payload was silently dropped. `update_top_table` now reads it with
  `AnyDictionary`, and the Java `hasInfoBox`/`lastDisplayState`/`Core.scene.
  hasMouse` conditions are pure predicates with tests. Both clients now show the
  hover box (PAR-0473 frames: "Core: Nucleus / Health / Capacity: 13k").
  PAR-0471's structured-power fallback was recorded before this fix and can be
  re-shot as a hover-bar pair later.

### Still open (highest-value, code-verified)

- **P0 combat/defense:** #1 lightning target/caps/insulation, #2 turret
  `limitRange`, #6 continuous/point lasers,
  #12 BaseShield filters/unit pass, #14 RegenProjector anti-stack/`edelta`/phase,
  #15 door/force-projector logic hooks, #16 ShockMine colour/burst.
- **P0 production/logistics:** #18 AttributeCrafter boost, #19 Fracker item use,
  #20 WallCrafter boosts, #22 `ConsumePowerDynamic`, #23 unit payloads,
  #24 legacy blocks, #25 logic `bullet`.
- **P1/P2:** #32–#37 server/net/TypeIO/EventType architecture, #39 player list,
  #40 unban/unadmin/trace lists, #41 schematic icon/description/files, #45
  JoinDialog version/LAN, #47 schematic/rebuild/RTS selection visuals, #49
  region/line lights, #50 planet-view sun/moons/cubemap/bloom, #53 remainder
  (debug unit hitboxes, ColoredFloor tint, environment props), #55–#62 mod/class
  parity, #63–#65 AI/indexing.
