# GAPS.md — Mindustry-Godot vs upstream Mindustry

Code-only survey (2026-11-05) of `~/g/code_examples/Mindustry` (Java, ~823 files under `core/src`, plus `server/`, `desktop/`) against this repo (`client/rust/*`, `server/spacetimedb`, `client/shaders`). Ledgers, docs, runs and screenshots were not consulted; `.claude/worktrees` was ignored.

Path shorthands: `mc/` = `client/rust/mind-core/src/`, `gx/` = `client/rust/mind-gdext/src/`, `mh/` = `client/rust/mind-headless/src/`, `stdb/` = `server/spacetimedb/src/`.

Items marked ✔ were re-verified by hand against both trees; the rest come from file-by-file subsystem sweeps and should be confirmed before fixing.

> **Status 2026-11-07.** This survey is code-only and several entries have gone
> stale. A follow-up audit re-checked every item against HEAD and the twin
> ledger; items fixed since are marked `**FIXED ...**`. The
> [2026-11-07 headless pass](#status-after-the-2026-11-07-headless-fix-pass)
> closed most of the remaining list (57 gaps, 15 commits `c150184e`..`ff278592`)
> and records the ledger ids and headless evidence per item; the earlier
> 2026-11-06 closures are kept in a separate table below it. What remains is
> documented/blocked (#4, #11, #12, #13, #16, #31, #69 Steam, #71), partial
> (#38, #44, #46, #47, #53) or carries a filed residual (#34, #37, #39) — see
> the appendix.

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
   **FIXED 2026-11-07 — PAR-0529 (verified, headless).**
   - the insulated-building raycast;
   - the `hitter` air/ground filter, `pierceCap`, damage multiplier and `hitCreate`;
   - the random walk after the chain cap.

   This affects every lightning weapon, arc, and lightning-on-hit.
2. ✔ **Turret ammo has no `limitRange`.** Java `Turret.limitRange` (`Turret.java:248`) recomputes ammo lifetime from range. The Rust port has no equivalent, so ranges are wrong for duo, scatter, hail, swarmer, salvo, cyclone, spectre and six Erekir turrets. For example, duo copper lifetime is 60 in Rust vs 70 in Java, and salvo copper is 60 vs 83.6.
   **FIXED 2026-11-07 — PAR-0530 (verified, headless).**
3. ✔ **Erekir turret-ammo rebalance is skipped.** `mc/content/tech/ekir.rs` `rebalance()` part (b) is a documented no-op. Breach, diffuse, sublimate, titan, disperse, afflict, lustre, scathe, smite and malign ammo do about 33% too much damage (breach beryllium is 85 vs 63.75).
   **FIXED 2026-11-07 — PAR-0531 (verified, headless).**
4. **FIXED 2026-11-06 — PAR-0471, twin-verified.** **Turrets never stopped consuming.** `TurretBehavior` had no `should_consume` override, and neither did ShockwaveTower, RegenProjector or BuildTurret, so power and coolant drained while idle. The port now mirrors `Turret.java:370-372` and the Continuous/ContinuousLiquid/Laser/PointDefense/Tractor/Shockwave/Regen/Build overrides (10 tests), including the class-hierarchy detail that PointDefense/Tractor `super` is `enabled`, not the turret reload rule.
5. **`cheating()` (team cheat rule) is ignored** by turret ammo and heat logic: there is no infinite ammo, no heat bypass, and no ItemTurret auto-fill.
   **FIXED 2026-11-07 — PAR-0532 (verified, headless).**
6. **Continuous and point lasers:**
   **FIXED 2026-11-07 — PAR-0533 (verified, headless).**
   - `mc/combat/bullet/kinds/continuous.rs` ignores the ContinuousLaser `fadeTime` and ContinuousFlame `lengthInterp`.
   - PointLaser (lustre) does line damage from the muzzle instead of `Damage.collidePoint` at the aim point. `collidePoint` has no port.
7. **LiquidTurret fire extinguishing** (`findTarget` scanning `Fires`) is missing.
   **FIXED 2026-11-07 — PAR-0534 (verified, headless).**
8. **TractorBeamTurret** subtracts health directly instead of using `damageContinuousPierce` (no shields, death path, `blockDamage`), and can't distinguish air from ground targets.
   **FIXED 2026-11-07 — PAR-0535 (verified, headless).**
9. **Weapons:**
   **FIXED 2026-11-07 — PAR-0542 (verified, headless); the `ignoreRotation`/unit-ammo sub-bullets were already resolved at HEAD.**
   - `shootStatus`/`shootStatusDuration` are parsed but never applied (vela and the quasar/conquer lines);
   - `ignoreRotation` is unused;
   - unit ammo (`rules.unitAmmo`, `ammoPerShot`, `ammoType`) is entirely absent.
10. **Movement damage:**
   **FIXED 2026-11-07 — PAR-0543 (verified, headless); the leg-death explosion and tank crush were already at HEAD.**
    - legs: `legSplashDamage` and the leg-death `dynamicExplosion` are missing (arkyid, toxopid, Erekir legged units);
    - crawlers: `crushDamage` is missing;
    - tanks: the crush ignores `rules.unitDamage` and `disarmed`, and bypasses `build.damage`.
11. **Bullet content-ID space differs.** Turret ammo is installed after load by `install_turret_ammo`, in a non-Java order, from hand-written tables in `mc/world/blocks/defense/turrets/{mod,advanced}.rs`. Consequences:
   **BLOCKED — needs an append-only content-ID migration (see appendix).**
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
   **DOCUMENTED — PAR-0528 (verified, headless) landed those paths; the live-Sim placement/deconstruct-refund residuals are listed in the appendix.**
14. **RegenProjector:**
   **FIXED 2026-11-07 — PAR-0526 (verified, headless).**
    - there is no `mendMap` anti-stack, so overlapping projectors stack heals;
    - heal is not scaled by `edelta`;
    - the phase item is never consumed.
15. **Logic and pickup hooks:**
   **FIXED 2026-11-07 — PAR-0527 (verified, headless).**
    - Door `control(enabled)` and AutoDoor `sense(enabled)` are missing;
    - ForceProjector `setProp(shield)`, `overwrote` and `pickedUp` are missing;
    - `BuildingBehavior` has no `picked_up` hook at all, which also affects ShieldWall.
16. **CODE FIXED 2026-11-06 — PAR-0476 (twin pending a rebuild).** **ShockMine:** lightning colour was WHITE; it now uses `Pal.lancerLaser` and the colour is plumbed through `LightningResult` into the `Fx.lightning` event so the renderer actually draws it (previously the port emitted no lightning FX at all). Vanilla `shockMine` leaves `bullet` null, so the optional bullet burst does not apply; `shots` stays inert. Tests: `shock_mine` 4, `lightning` 6, `fx` 113, full `mind-core` lib pass.

### Production, logistics and storage
17. **FIXED 2026-11-06 — PAR-0469, twin-verified.** ✔ **ArmoredConveyor / ArmoredConduit accepted side input from non-conveyors.** The `armored` flag only affected blending. `accept_item` in `mc/world/blocks/distribution/conveyor.rs` and `accept_liquid` in `liquid/movement.rs` now carry the Java source gate (`ArmoredConveyor.java:31-33`, `ArmoredConduit.java:23-27`), with `LiquidNode.armored` and an exact `Tile.absoluteRelativeTo`.
18. **AttributeCrafter gets no attribute boost** (cultivator, silicon crucible). `efficiency_scale` in `mc/world/behavior/production.rs:~242` returns only `base_efficiency`.
   **FIXED 2026-11-07 — PAR-0514 (verified, headless).**
19. **Fracker never consumes its item:** there is no `itemUseTime` accumulator and no `lastPump` reset.
   **FIXED 2026-11-07 — PAR-0515 (verified, headless).**
20. **CODE FIXED 2026-11-06 — PAR-0474 (twin blocked on the Java reference setup).** **WallCrafter** had no liquid boost and no item boost, and its attribute was discarded. `WallCrafterBehavior` now ports `getEfficiency`, the `lerp(1, liquidBoostIntensity, optionalEfficiency)` and `itemBoostIntensity` terms, the graphite booster timer/`consume()`, `lastEfficiency`/`warmup`/`totalTime`, and the new `drillspeed` bar. Godot shows 7.50/s → 12.00/s with graphite; the Java leg is blocked on building a sand-wall/crusher map under synthetic editor input (see the appendix).
21. **Core items are not shared across a team's cores.** Java does `items = other.items`; the port only sums capacity (`mc/world/blocks/storage/core_block.rs:236`, flagged "R2 NEEDS USER DECISION").
   **FIXED 2026-11-07 — PAR-0575 (verified, headless).**
22. **ConsumePowerDynamic is missing.** The power graph always requests the static usage, so PayloadLoader charging is wrong.
   **FIXED 2026-11-07 — PAR-0516 (verified, headless).**
23. **Unit payloads are a stub:**
   **FIXED 2026-11-07 — PAR-0518 (verified, headless).**
    - `dump_unit_payload` always returns false (`payload_block.rs:293`), so unit payloads never leave a block;
    - unit payload IO returns `Unsupported`, which stops payload-block reads, including in saves;
    - PayloadSource never creates units;
    - UnitFactory spawns its unit directly instead of moving out a `UnitPayload`, so the factory→reconstructor payload chain is not 1:1.
24. **Legacy blocks** (LegacyBlock, MechPad, UnitFactory, CommandCenter) are never removed or replaced on map load. `MapHooks::legacy_remove_self` is never called, and their `read()` byte-skips aren't ported, so old saves may misparse.
   **FIXED 2026-11-07 — PAR-0517 (verified, headless).**

### World processor and logic
25. ✔ **The `bullet` instruction (`SpawnBulletI`) spawns nothing.** The event is emitted (`mc/logic/world.rs:1233`) but `mc/sim/mod.rs` has no consumer arm. There is also no Bullet `LogicObject` and no bullet sense/`setprop`.
   **FIXED 2026-11-07 — PAR-0521 (verified, headless).**
26. **Unit sense** returns 0/null for camera, ping, selected block and rotation, breaking, and color. Building camera sense is always 0.
   **FIXED 2026-11-07 — PAR-0218 (verified): unit/building logic senses read `LogicPlayerView`; the gdext host push is a filed follow-up (see appendix).**
27. `payEnter` is an empty arm. `sync` and `clientdata` events have no network sink.
   **FIXED 2026-11-07 — PAR-0524 (verified, headless).**

### Map generation
28. **`trimDark` passes a darkness of 0** (`serpulo.rs:1647`, `erekir.rs:717`, `tantros.rs:190`), so sector-edge walls differ from Java.
   **FIXED 2026-11-07 — PAR-0510 (verified, headless).**
29. **`addWeather`** leaves out the sector-circle term because Arc's 2-D noise isn't ported.
   **FIXED 2026-11-07 — PAR-0511 (verified, headless).**
30. The **CoreSpawnFilter** and **SpawnPathFilter** map filters are no-op stubs (`mc/maps/filters/builtin.rs:1517,1656`). RandomItemFilter is also a stub, but it is unused upstream.
   **FIXED 2026-11-07 — PAR-0512 (verified, headless).**
31. **Sector submission order:** `registerSurvival(202)` runs after the megabases in `sectors.rs:398-425`, which shifts 8 sector-preset IDs.
   **BLOCKED — PAR-0513: renumbering the 8 SectorPreset IDs breaks the append-only content-ID ABI (see appendix).**

---

## P1 — multiplayer and server

32. **The dedicated server doesn't run a game.** `mh/server/host.rs` `LocalHost::host` loads a `FixtureWorld`, not a real Sim:
   **FIXED — PAR-0124 (verified 2026-11-06) + PAR-0583/PAR-0584 (verified, headless): real host Sim/rotation/auto-pause, mod-registry boot/reload and host-clock autosave.**
    - built-in and sector maps fail with `NeedsWorldLoader`;
    - `runwave` only increments a counter;
    - `fillitems`, `shuffle`, `nextmap`, `gameover`, `reloadassets` and `mods` are message-only stubs;
    - map rotation, `roundExtraTime` and auto-pause can't happen.
33. **Missing pieces of NetServer:**
   **FIXED 2026-11-07 — PAR-0545/PAR-0585/PAR-0600 (verified, headless; module + host halves).**
    - `assignTeam` (PvP balancing);
    - `clientSnapshot`-style movement validation (`stdb/relay/player_state.rs` only checks seq, finite floats and rate);
    - chat commands `/help`, `/sync`, `/votekick`, `/vote`;
    - an admin chat channel (`ChatKind` is All/Team/System only);
    - `fixName`, `checkColor`, `kickAll`.
34. **Missing pieces of Administration:**
   **FIXED 2026-11-07 — PAR-0557/PAR-0558 (verified, headless; module side); the client UI remainder is PAR-0603 (`fixing`), and `checkUuidChanges` stays blocked (PAR-0559, no trusted client address under D2).**
    - action filters (`addActionFilter`/`ActionType`);
    - PlayerInfo history (names, IPs, join/kick counts) and `findByName`/`searchNames`;
    - trace info and `adminRequest(trace)`;
    - the kick cooldown;
    - name-ban enforcement (`banned_name_patterns` is stored but never enforced);
    - enforcement for `enableVotekick`, `interactRateLimit` and `messageSpamKick`;
    - `checkUuidChanges`.
35. **Server→client `@Remote` calls with no channel:** `infoMessage`, `followUpMenu`/`hideFollowUpMenu`, `setCameraPosition`, `sound`/`soundAt`/`effect`, `playMusic`, `traceInfo`, `removeWorldLabel`, and the `*Reliable` menu variants. There is no `KickReason` enum; kick reasons are free-form strings.
   **FIXED 2026-11-07 — PAR-0546/PAR-0576/PAR-0601 (verified, headless; call channel + host call producers).**
36. **TypeIO network-only codecs:** Unit, Building, Tile, PosEntity, Entity, Controller, Kick, TraceInfo, Sound, MenuResult, MarkerControl, UiBuilder, StringArray, Action.
   **FIXED 2026-11-07 — PAR-0547/PAR-0578 (verified, headless).**
37. **EventType:** only about 28 of about 95 event classes exist, and there is no subscribe bus. Missing include:
   **FIXED 2026-11-07 — PAR-0548/PAR-0577 (verified, headless): all 89 EventType classes, payload codec, game-flow fire sites and typed relay publication; host-side drain/fire sites remain as filed residuals.**
    - Unit Create/Destroy/Damage/Drown/Spawn/Change/Control;
    - Player Join/Leave/Chat/Connect/Ban/Unban;
    - Config, Deposit, Withdraw, Pickup, PayloadDrop, CoreChange, LaunchItem, Research, Unlock, BuildSelect;
    - the BulletDestroy pair, GeneratorPressureExplode, MenuOptionChoose, TextInput, ServerLoad, SchematicCreate, MapMake.
38. **UnlockableContent hooks** are missing: `onUnlock`, `showUnlock`, `logicVisible`, `unlockedNowHost`/`unlockedHost` (a no-op at `mc/sim/builder.rs:1052`), and `getDependencies` on the trait. `unlock()` doesn't fire `UnlockEvent`.
   **PARTIAL — `onUnlock`/`showUnlock`, `getDependencies` and `unlockedNowHost` now have implementors; `logicVisible` remains (see appendix).**

## P1 — UI

39. **PlayerListFragment** is a stub (`gx/ui/fragments/player_list.rs`). Its data is always empty (`RelayUnavailable`), and it has no per-player ban/kick/trace/team/admin menu, votekick prompt, count header, avatars or icons.
   **FIXED 2026-11-07 — PAR-0567 (verified, headless); the client UI remainder is PAR-0603 (`fixing`: admin relay listener, roster refresh, row authority gates, `Binding.chat`).**
40. **Bans, Admins and Trace dialogs** (`gx/ui/dialogs/misc.rs`):
   **FIXED 2026-11-07 — PAR-0536 (verified, headless).**
    - Bans and Admins have no unban/unadmin buttons or confirms;
    - Trace has no IP/name lists and its strings are unformatted.
41. **SchematicsDialog:**
   **FIXED 2026-11-07 — PAR-0574 (verified, headless).**
    - there is no tag editor (rename, delete, reorder), no icon tags and no per-schematic tags;
    - the edit view has no description field;
    - import and export are clipboard-only, with no `.msch` file support and no export dialog.
42. **HudFragment:**
   **FIXED 2026-11-07 — PAR-0556 (verified, headless); the toast queue and `rules.unitfactoryactivation.objective` line were already at HEAD.**
    - the in-game editor-mode panel is missing (team selector, block palette with search and favorites, `setPlayerTeamEditor`);
    - the toast queue is missing;
    - `rules.unitfactoryactivation.objective` is missing.
43. **FIXED 2026-11-06 — PAR-0468, twin-verified.** **PlacementFragment** had no `getUnplaceableReason` (no-builder, environment, banned and block-limit messages) and no `block.unknown` header for locked blocks. Java `PlacementFragment.java:380-432,753-761,779-782`; the port now renders the scarlet cancel row and gates the header/`?` on `unlocked()`.
44. **Mods and ModBrowser dialogs:** missing the release picker, the dependency status dialog, view-content, and the incompatibility and version messages.
   **PARTIAL — release picker, dependency row and version/incompatibility strings are already at HEAD (`batch_g.rs`); `mods.viewcontent` (View Content) still has no implementor.**
45. **JoinDialog** shows no server version or outdated strings and has no LAN discovery.
   **FIXED 2026-11-07 — PAR-0549 (verified, headless): version/`@server.outdated` strings + `safeConnect` gates. LAN discovery is a documented transport deviation (STDB has no LAN equivalent; the local relay is the default server).**
46. Smaller UI gaps:
   **PARTIAL FIX 2026-11-07 — PAR-0550/PAR-0551/PAR-0552/PAR-0553/PAR-0554/PAR-0555 (verified, headless): savefail, objective/vertices, live Effects preview, minimap ping, PlanConfig caller and the list pickers; Settings/MapLocales were already at HEAD. Still open: GameOverDialog multiplayer waiting/disconnect + `difficulty.guide`, LoadDialog corrupt/missing-asset states, and the Planet SectorSubmissions link.**
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

47. **PARTIAL FIX 2026-11-06 — PAR-0472, twin-verified (RTS command drag box).** **Input selection visuals were not drawn (2026-11-06 survey).** `PlacementView` exposed only the cursor, line plans and break rect, so these were missing:
   **PARTIAL FIX 2026-11-07 — PAR-0472 (twin-verified) + PAR-0592 (verified, headless): RTS drag box, F/B selection rects, paste ghosts, overlap check and other-plan geometry/draw assembly landed (8 focused gdext tests). Only other players' live plan previews remain, blocked on net preview-plan sync.**
    - the schematic copy rect and paste preview ghosts;
    - the rebuild-select rect;
    - ~~the RTS command drag box~~ — fixed: the live signed `command_rect` is tracked and the plans overlay fills it `Pal.accent` @ 0.3 (`CommandOverlay`), Java `InputHandler.java:1354-1368`;
    - `drawOverlapCheck`;
    - other players' plans.
48. **Mobile line placement** passes `NoLineHooks` (`mc/input/mobile.rs:660`, `line.rs:173`), so it has no junction replacement, bridge auto-link or node chaining. `get_plan` matches the exact tile, not the footprint.
   **FIXED 2026-11-07 — PAR-0568 (verified, headless).**
49. **Lighting** draws circles only. Region lights, line lights (lasers, continuous bullets) and custom lights are missing, and per-block light overrides use Java defaults.
   **FIXED 2026-11-07 — PAR-0569/PAR-0581/PAR-0582 (verified, headless): region/line/custom lights, beam line lights and the `Fx.pointBeam` light-pass consumer; per-block `lightRadius`/`lightColor` were already data-driven (PAR-0467). In-engine visual/Java twin still pending.**
50. **Planet view** (`gx/ui/planet.rs`):
   **FIXED 2026-11-07 — PAR-0570/PAR-0580 (verified, headless).**
    - no sun, other planets or moons are drawn;
    - the skybox is a billboard instead of a cubemap;
    - there is no planet bloom;
    - the `cubemap`, `mesh`, `unlit` and `unlitwhite` shaders are dead.
51. **LoadRenderer** is essentially a stub: 58 lines against Java's 524.
   **FIXED 2026-11-07 — PAR-0571 (verified, headless).**
52. **FloorRenderer** doesn't apply 47-slice autotiling and draws floors at TILESIZE instead of the region's natural size. Team overlays in the building cache are deferred.
   **FIXED 2026-11-07 — PAR-0520 (verified, headless).**
53. Smaller rendering and input gaps:
   **PARTIAL FIX 2026-11-07 — PAR-0519 (verified): debug unit hitboxes landed; the remaining sub-items below are unchanged.**
    - DebugCollisionRenderer has no unit hitboxes;
    - the full-map screenshot (`ScreenshotPlan`) is unused;
    - possibly `fluidFrames` (animated liquid regions);
    - the Drawf dash helpers and `flameFront` are missing;
    - MenuRenderer simplifies its flyers;
    - fx noise, shader and polyline primitives are deferred (`gx/fx.rs:797`);
    - ColoredFloor/ColoredWall tint is never applied in the renderer;
    - environment-prop draws are unconfirmed: SeaBush, Tree, TallBlock, Cliff, Rune/Character overlays and SteamVent fx.
54. **Audio:** `SoundLoop` per-key fade voices are folded into one mixer, the BurstDrill ambient factor is deferred, and LaunchPad has no sound, shake or fx.
   **FIXED 2026-11-07 — PAR-0540/PAR-0565 (verified, headless).**

## P2 — data-driven and mod parity

55. **Block behaviour is dispatched by vanilla name, not class.** Any mod or JSON block of a class without a name registration gets `NoopBehavior` (`mc/world/behavior/mod.rs:560,714-765`). That includes mod turrets, conveyors, routers, conduits and crafters.
   **FIXED 2026-11-07 — PAR-0560 (verified, headless).**
56. **Per-family knobs are not in content data.** Values such as `craftTime`, outputs, drill tier and time, pump amount and conveyor speed come from `apply_vanilla_knobs` (`mc/world/block_kind_data.rs:633,773`), keyed by about 64 block names. Mod blocks get empty defaults, and patches can't change them.
   **FIXED 2026-11-07 — PAR-0538/PAR-0572/PAR-0573/PAR-0588/PAR-0590 (verified, headless).**
57. **Draw chains** are hand-written for about 15 vanilla crafters (`vanilla_chain`). Parsed `DrawerKind` specs are never converted to `DrawDesc`, so the Erekir crafters and every mod block fall back to DrawDefault. DrawPumpLiquid and DrawBlockParts have no implementation. `world/draw.rs` is a third, unclear framework.
   **FIXED 2026-11-07 — PAR-0562/PAR-0589 (verified, headless).**
58. **Consumers:** only Items, Liquid, Liquids, Power and Coolant are generic. These have no generic runtime: ItemFilter, Flammable, Radioactive, Explosive, Charged, List, Efficiency, Explode, ItemDynamic, LiquidFilter, LiquidFlammable, LiquidsDynamic, PayloadDynamic, PayloadFilter, Payloads, PowerDynamic and PowerCondition.
   **FIXED 2026-11-07 — PAR-0561 (verified, headless).**
59. **ContentParser** (`mc/mods/json/mod.rs`):
   **FIXED 2026-11-07 — PAR-0537 (verified, headless).**
    - it reads only about 30 base Block fields and about 30 unit fields, and no subclass fields;
    - `research` tech-node insertion is not handled;
    - named bullet references are dropped;
    - bullet `spawnUnit` is ignored;
    - ParticleEffect (the default effect type), RadialEffect, SeqEffect, SoundEffect and NoiseEffect are missing.
60. **DataPatcher** covers about 14 block fields and a few unit fields; Java patches any field by reflection.
   **FIXED 2026-11-07 — PAR-0539 (verified, headless).**
61. **Abilities and unit components:**
   **FIXED 2026-11-07 — PAR-0563 (verified, headless).**
    - ArmorPlate, MoveLightning, UnitSpawn and EmptyData abilities are missing;
    - Segment, WaterCrawl, UnderwaterMove, Shielder and Child are markers only;
    - LaunchCoreComp is missing;
    - TiledFloor, TiledWall and WobbleProp have no `BlockKind`;
    - ShockMine bullet bursts are missing;
    - SingleBlockProducer is missing.
62. **Stub helper traits:** RotBlock, UnitTetherBlock, ExplosionShield and ControlBlock are declared but have no implementors. Missing `Block` methods: `placeEnded`, `flipRotation`, `getPlanConfig`.
   **FIXED 2026-11-07 — PAR-0564 (verified, headless).**

## P2 — AI and indexing

63. **BlockIndexer** is rebuild-based, with no incremental `addIndex`/`removeIndex`. `getFlaggedFloors`, `getAllPresentOres`, range `eachBlock`, `allBuildings` and `findTile` are missing.
   **FIXED 2026-11-07 — PAR-0522 (verified, headless).**
64. **BuilderAI** doesn't scan for build-turret hold positions.
   **FIXED 2026-11-07 — PAR-0523 (verified, headless).**
65. **BaseBuilderAI** returns core fill and unit spawn as actions, and it's unconfirmed that the host executes them.
   **FIXED 2026-11-07 (confirmed at HEAD) — `game/runtime.rs:1501-1585` executes `fill_core_items`/`spawn_core_unit`; PAR-0156.**
66. **PrebuildAI** is a stub, but upstream marks it "not functional".
   **ALREADY IMPLEMENTED at HEAD — `ai/types/prebuild.rs` is a real state/path port (see appendix).**
67. Smaller AI gaps:
   **FIXED 2026-11-07 — PAR-0525/PAR-0544 (verified, headless).**
    - a logic-controlled turret's missile doesn't get its shooter set (`combat/bullet/spawn.rs:174`);
    - `block.targetable` is omitted (`sim/unit_runtime.rs:5768`);
    - `tryRepairDerelict` is deferred in the input layer;
    - the Schematics loadout path doesn't sort by `schematicPriority` (`mc/game/schematics.rs:231`).

## P3 — platform services (some intentionally dropped)

68. **Achievements and stats:** `Achievement` (69 entries), `SStat` and the `GameService` tracking logic are absent. `NullService` is a no-op.
   **FIXED 2026-11-07 — PAR-0566/PAR-0579/PAR-0595 (verified, headless).**
69. **Steam:** SNet lobbies, SStats, SUser, SWorkshop (a stub returning empty) and the Maps Workshop publish methods are missing. Discord is a stub.
   **OUT OF SCOPE — Steam half seeded as PAR-0591 (OD4 deferred); the platform-neutral half is PAR-0596 (verified, headless).**
70. **BeControl updater** is stubbed on purpose. PerfCounter, IntelGpuCheck and NvGpuInfo are missing. `async_work/physics.rs` is a dead leftover stub; the real physics is in `sim/unit_runtime.rs`.
   **PRACTICAL FIX 2026-11-07 — PAR-0598/PAR-0599 (verified, headless): PerfCounter, Intel/NV GPU probes, offline update check and GPU crash notice; the updater download/dialog half stays intentionally unported.**
71. **Documented deviations (decided: they stay):**
   **DECISION (2026-11-07): the deviations stay.** The rationale is already in
   the repo; this pass did not re-open any of them. See the appendix for the
   per-bullet references.
    - JS scripting (`Scripts.java`) → `NoScriptHost` (OD1; `mods/script.rs`,
      `mods/mod.rs:467`, `mods/AGENTS.md`: "Scripts and Java are surfaced, not
      executed"), so script mods are marked Unsupported;
    - Java mods and plugins (`Mod`, `Plugin`, `ModClassLoader`) → JSON and
      assets only (same AGENTS.md rule; an enabled Java/`main` mod keeps its
      JSON/assets and reports `JavaModUnsupported`);
    - IP and subnet bans → dropped under STDB (plan-21 D2: reducers have no
      client address; `mind-headless/src/server/admin.rs:11,43`,
      `server/host.rs:99`);
    - the community server list isn't fetched from the Java URLs: JoinDialog
      lists the active server's public `all_matches` rows instead
      (`batch_b.rs:1586-1587`), gated by the `communityservers` setting;
    - Pixelator resamples the screen instead of rendering to a second buffer
      (deviation S16-9, `gx/render/pixelate.rs`).

## Stale comments that misreport status (cleanup)

Re-checked against HEAD `ff278592`; the 2026-11-07 docs pass fixed the remaining
stale comments (comment-only, no behavior change):

- `mc/maps/planet/serpulo.rs` header + the three `generate()` notes, and
  `mc/maps/planet/asteroid.rs` header + its `generate()` note: ruins and the
  launch loadout now run in
  `place_structures` (`place_ruins` + `place_launch_loadout`); the tile-only
  `generate()` pass leaves structures to that hook instead of calling them
  skipped. (The old survey path `mc/maps/generators/planet/` was wrong; the
  files live under `mc/maps/planet/`.)
- `mc/combat/bullet/kinds/mod.rs:6`: every `BulletKind` now resolves to a
  behavior, so the header no longer calls the remaining kinds "not yet"
  implemented.
  `kinds/lightning.rs` was already corrected in this pass (it no longer calls
  the bullet "inert").
- `mc/sim/unit_runtime.rs` (`face_target`): the `Predict.intercept` lead is
  implemented via `combat::targeting::intercept_positions`; the "not ported"
  note was removed.
- `mc/ai/rts_ai.rs` and `mc/ai/ai_controller.rs` headers: the `TeamData`
  wiring/`estimateStats` and `AIController` helpers are live (`game/runtime.rs`
  RTS pass, `ai_controller.rs` helpers); the plan-deferred wording was removed.
  `mc/ai/block_indexer.rs` was already correct (incremental maintenance
  documented).
- `gx/render/env.rs`: the plan-17 bodies (underwater caustics/rays/particles and
  the scorching noise layers) live in `gx/fx/env.rs`, so the deviation note now
  points there instead of implying the tint is all that exists.

---

## Status after the 2026-11-07 headless fix pass

The 2026-11-07 pass re-checked the survey against HEAD `ff278592` and the twin
ledger. It landed 15 commits (`c150184e`..`ff278592`) covering combat, defense,
production, logic/AI, mapgen, server/net, UI, render, audio, mod and platform
items. Each closure is recorded in the twin ledger (headless evidence:
`cargo test`/`cargo check`/`mind-headless` scenarios), with `twin-verified` used
only where a Java reference capture already existed
(PAR-0468/0469/0470/0471/0472/0473). No new Java leg was run in this pass.

Ledger at the end of the pass: **603 items** — 300 `verified`, 29
`twin-verified`, 261 `reference-verified`, 7 `fixing`, 2 `blocked`, 2
`wontfix`, 2 `seeded`. This pass closes or code-completes 57 gaps:
#1–#3, #5–#10, #14/#15, #18/#19, #21–#30, #33–#37, #39–#42, #44 (partial),
#45–#51, #54–#65, #67/#68 and #70 (practical). This pass also confirms #32
(`PAR-0124` + residuals), #52, the #53 hitbox sub-item and #66 at HEAD. Eight
stay documented/blocked (#4, #11, #12, #13, #16, #31, #69 Steam, #71); #38,
#44, #46 and #47 keep partial residuals and the #53 remainder stays open;
#34/#37/#39 keep filed residuals.

### Closed in the 2026-11-07 pass

| Area | Gaps | Ledger | What landed |
|---|---|---|---|
| Combat | #1 | PAR-0529 | Full `createLightningInternal`: farthest **unit** chain, insulated-building raycast, `hitter` air/ground + `pierceCap` + damage multiplier + `hitCreate`, jittered polyline, random walk after the cap. |
| Combat | #2 | PAR-0530 | `Turret.limitRange` with the Java per-block margins, applied to every turret ammo `BulletDef` after registration. |
| Combat | #3 | PAR-0531 | Erekir rebalance part (b): 0.75 ammo scaling for turrets whose requirements include a non-Serpulo item, ItemTurret/ContinuousLiquid/Continuous only. |
| Combat | #5 | PAR-0532 | `cheating()` wired into turret ammo/heat: infinite ammo, heat bypass, ItemTurret auto-fill. |
| Combat | #6 | PAR-0533 | ContinuousLaser `fadeTime` + ContinuousFlame `lengthInterp`; `Damage.collidePoint` and PointLaser aim-point damage. |
| Combat | #7 | PAR-0534 | LiquidTurret `findTarget` fire-extinguish branch. |
| Combat | #8 | PAR-0535 | TractorBeam `damageContinuousPierce` (shield/death/`blockDamage` path) + air/ground target gate. |
| Combat | #9 | PAR-0542 | `Weapon.shootStatus`/`shootStatusDuration` applied to the shooter on fire (vela, quasar/conquer). `ignoreRotation`/unit-ammo were already resolved. |
| Combat | #10 | PAR-0543 | Leg splash damage + crawler crush damage; leg-death explosion and tank crush were already at HEAD. |
| Defense | #14 | PAR-0526 | RegenProjector `mendMap` anti-stack, `edelta` heal scale, phase-item consumption. |
| Defense | #15 | PAR-0527 | Door `control`, AutoDoor `sense`, ForceProjector `setProp`/`overwrote`/`pickedUp`, `BuildingBehavior::picked_up`. |
| Production | #18 | PAR-0514 | AttributeCrafter attribute sum + `efficiencyMultiplier` (`AttributeCrafter.java:93-124`). |
| Production | #19 | PAR-0515 | Fracker `itemUseTime` accumulator + `lastPump` reset. |
| Storage | #21 | PAR-0575 | Team core-item pool (`CoreBlock.items` semantics) via `storage/team_items.rs`; every core resolves through the canonical holder. |
| Power | #22 | PAR-0516 | `ConsumePowerDynamic` (`PowerDynamicUsage`) + PayloadLoader live request. |
| Payload | #23 | PAR-0518 | Unit payload carry/dump/move-out, PayloadSource/UnitFactory flow, nested payload save body. |
| World | #24 | PAR-0517 | Legacy block removal on both map-load paths + `read()` byte skips. |
| Logic | #25 | PAR-0521 | `bullet` instruction spawns a real bullet; Bullet `LogicObject` senses/`setprop`. |
| Logic | #27 | PAR-0524 | `payEnter` core selection; `sync`/`clientdata` executor + relay sinks. |
| Mapgen | #28 | PAR-0510 | `trimDark` darkness (border + generated-sector polygon) for Serpulo/Erekir/Tantros. |
| Mapgen | #29 | PAR-0511 | `addWeather` border + sector-polygon + static-wall terms. |
| Mapgen | #30 | PAR-0512 | CoreSpawnFilter and SpawnPathFilter (A* carve) are real filters. |
| Server | #32 | PAR-0124, PAR-0583, PAR-0584 | Real host Sim + mod-registry boot/reload + host-clock autosave rotation (the main host was already at HEAD). |
| Server | #33 | PAR-0545, PAR-0585, PAR-0600 | PvP `assignTeam` + clientSnapshot validation, chat commands/votekick/admin channel, fixName/checkColor, client team adoption. |
| Server | #34 | PAR-0557, PAR-0558; PAR-0603 fixing; PAR-0559 blocked | Action filters + interact rate, PlayerInfo history/trace/name-ban/kick cooldown; the client-side admin/player-list remainder is PAR-0603; `checkUuidChanges` unenforceable under D2. |
| Net | #35 | PAR-0546, PAR-0576, PAR-0601 | `CallPacket` channel for 23 server→client @Remote calls + KickReason; relay publication + host call producers (infoMessage/kick/kickReason/traceInfo). |
| Net | #36 | PAR-0547, PAR-0578 | Network-only TypeIO codecs + `writeObject`-tagged overloads/entity-plan sync. |
| Net | #37 | PAR-0548, PAR-0577 | All 89 EventType classes + JSON payload codec + game-flow fire sites + typed relay publication; the host-side drain/fire sites are filed residuals. |
| UI | #39 | PAR-0567; PAR-0603 fixing | PlayerListFragment rows/actions/votekick over MindNet; TraceDialog roster. Client-side admin relay/authority/chat toggle remain in PAR-0603. |
| UI | #40 | PAR-0536 | Bans/Admins unban/unadmin confirms; Trace IP/name lists + formatted strings. |
| UI | #41 | PAR-0574 | Schematic tag editor, per-schematic tags, edit description, `.msch` import/export. |
| UI | #42 | PAR-0556 | In-game editor-mode HUD panel (team selector, palette search/favorites, `setPlayerTeamEditor`). |
| UI | #44 (partial) | already at HEAD (`batch_g.rs:3288-3308`) | Release picker, dependency row and version/incompatibility strings are present; `mods.viewcontent` (View Content) remains. |
| UI | #45 | PAR-0549 | JoinDialog server version + `@server.outdated` strings + `safeConnect` gates; LAN discovery is a documented STDB transport deviation. |
| UI | #46 (partial) | PAR-0550..0555 | `savefail` modal, objective/vertices interpreters, live Effects preview, minimap ping, PlanConfig caller, pickers; Settings/MapLocales already at HEAD. Still open: GameOverDialog multiplayer waiting/disconnect + `difficulty.guide`, LoadDialog corrupt/missing-asset states, Planet SectorSubmissions link. |
| Input/render | #47 (partial) | PAR-0472, PAR-0592 | RTS drag box (twin-verified); copy/rebuild/paste-ghost/overlap/other-plan geometry + a gdext draw path (PAR-0592 verified, 8 focused tests). Only other players' plan previews remain, blocked on net preview-plan sync. |
| Input | #48 | PAR-0568 | Mobile line placement hooks + footprint-aware `get_plan`. |
| Render | #49 | PAR-0569, PAR-0581, PAR-0582 | Region/line/custom lights + continuous-beam line lights; beam bullets skip the default circle light; the `Fx.pointBeam` light-pass consumer is wired end-to-end. In-engine/Java twin still pending. |
| Render | #50 | PAR-0570, PAR-0580 | Cubemap skybox, SunMesh/other-body draws, planet-body rows. |
| Render | #51 | PAR-0571 | LoadRenderer data half (bars, layout, asset log, progress matrix). |
| Render | #52 | PAR-0520 | Enemy/derelict team `block-border` corner baked into the building cache. |
| Render | #53 (partial) | PAR-0519 | DebugCollisionRenderer unit hitboxes; the remaining sub-items are listed below. |
| Audio | #54 | PAR-0540, PAR-0565 | Per-key SoundLoop fade voices, BurstDrill shake/drillEffect, LaunchPad audio hooks. |
| Mods | #55 | PAR-0560 | Class-keyed behavior dispatch (`kind` resolution + class defaults). |
| Mods | #56 | PAR-0538, 0572, 0573, 0588, 0590 | Family knobs in `BlockKindData` defaults, TiledFloor/TiledWall/WobbleProp kinds, full `parse_block` surface, packed tiling/wobble draw. |
| Mods | #57 | PAR-0562, PAR-0589 | All 41 DrawerKind→DrawDesc, DrawPumpLiquid/DrawBlockParts, DrawRegion/Turret/Weapon parts. |
| Mods | #58 | PAR-0561 | 17 consumer classes lowered to a generic runtime. |
| Mods | #59 | PAR-0537 | ContentParser inline effects, bullet unit refs, research nodes, broader block/unit fields. |
| Mods | #60 | PAR-0539 | DataPatcher general Block/Floor scalar surface. |
| Mods | #61 | PAR-0563 | ArmorPlate/MoveLightning/UnitSpawn/EmptyData + WaterCrawl/UnderwaterMove/Shielder/LaunchCore. |
| Mods | #62 | PAR-0564 | RotBlock/UnitTetherBlock/ExplosionShield/ControlBlock implementors + placeEnded/flipRotation/getPlanConfigs. |
| AI | #63 | PAR-0522 | BlockIndexer incremental maintenance, flagged floors/ores, range scans. |
| AI | #64 | PAR-0523 | BuilderAI build-turret hold positions. |
| AI | #65 | PAR-0156; `game/runtime.rs:1501-1585` | BaseBuilderAI core fill / core-unit spawn actions are executed by the host. |
| AI | #67 | PAR-0525, PAR-0544 | Missile shooter from ControlBlock + `Block.targetable`. |
| Platform | #68 | PAR-0566, 0579, 0595 | Achievement/SStat tables, ServiceTracker injected into the live campaign, Database achievement/stat tabs. |
| Platform | #70 (practical) | PAR-0598, PAR-0599 | PerfCounter + MindPlatform perf probe, Intel/NV GPU probes, offline update check, GPU crash notice; the updater download/dialog half stays intentionally unported. |

### Closed in the 2026-11-06 pass (kept)

| Gap | Fix | Twin item | State |
|---|---|---|---|
| 4 Turret/defense idle consumption | `should_consume` overrides for Turret/Continuous/ContinuousLiquid/Laser/PointDefense/Tractor/Shockwave/Regen/Build, 10 tests | PAR-0471 | `twin-verified` — left documented, not re-opened |
| 12 BaseShield bullets/units/explosions | `absorbable` filter, unit repel/kill + spark, explosion absorber removed | PAR-0475 | `verified`; Java twin blocked (editorOnly `shield-projector`; stripped Java atlas lacks `block-shield-projector-ui`) |
| 16 ShockMine lightning colour | `Pal.lancerLaser` through `LightningResult`/`Fx.lightning` + renderer | PAR-0476 | `verified`; twin pending a .so rebuild + editor restart + shock-mine walk capture |
| 17 ArmoredConveyor/ArmoredConduit side input | Source gate + `LiquidNode.armored` + `Tile.absoluteRelativeTo` | PAR-0469 | `twin-verified` |
| 20 WallCrafter boosts | Full `updateTile` port (attribute/liquid/item, graphite consume, `drillspeed` bar) | PAR-0474 | `verified`; Java twin blocked on the sand-wall/editor map setup |
| 43 PlacementFragment unplaceable reason | `unplaceable_reason()`/`placement_unlocked()` + scarlet cancel row + `block.unknown` | PAR-0468 | `twin-verified` |
| New: Custom Rules Edit overlay stall | `defer_dialog_action()` for every custom-rules dialog bind + `MindDialogSweepCheck` | PAR-0470 | `twin-verified` |
| New: placement hover table | `AnyDictionary` read of `hover_info_world`/`hover_info` + STOP-guard predicates | PAR-0473 | `twin-verified` |
| 66 PrebuildAI | Real state/path port (`ai/types/prebuild.rs`), already at HEAD | — | confirmed |

### Documented / blocked (not closed)

| Gap | State and precise reason |
|---|---|
| #4 | Already closed 2026-11-06 (`PAR-0471`, twin-verified); left documented here as closed, not re-opened. |
| #11 | **Blocked.** The bullet content-ID space still differs from Java: `install_turret_ammo` installs after load in a non-Java order from hand-written tables, with synthetic `tractor_dummy`/`pointdefense_dummy` extras (`turrets/advanced.rs:394-397`). Reordering needs an append-only content-ID migration and save compatibility work. |
| #12 | **Code fixed, Java twin blocked.** `PAR-0475` verified headless; `shield-projector` is `editorOnly` and the Java reference editor picker omits it (stripped atlas). |
| #13 | **Code landed, documented residual.** `PAR-0528` verified headless (team rebuild queue, following builders, deconstruct, `shouldConsume`). `Build.beginPlace/beginBreak` need the live `SimRuntime` (unreachable from the `CombatCtx` pass), so a plan whose tile has no `ConstructBuild` is discarded, and deconstruct refunds are not distributed. |
| #16 | **Code fixed, twin pending.** `PAR-0476` verified headless; needs `.so` rebuild + editor restart, then a shock-mine walk capture in both clients. |
| #31 | **Blocked (ABI).** `registerSurvival(202)` runs after the megabases, shifting 8 SectorPreset dense ids (246→67, 244→68, 242→69, 243→70, 247→71, 245→72, 27→73, 202→74). Renumbering breaks the append-only content-ID ABI; `sector_submission_order_is_pinned` records the pinned order (`PAR-0513`, blocked). |
| #69 | **Steam out of scope, practical half closed.** `PAR-0591` is `seeded`: SNet/SStats/SUser/SWorkshop, Steam init and rich presence need the Steamworks SDK (OD4 deferred; no Steam code compiled, `MindPlatform::service_enabled()` stays false). The platform-neutral `GameService` surface is complete (`PAR-0596` verified). |
| #71 | **Decision recorded: the deviations stay** (see the per-item rationale above). Not re-opened by this pass. |

### Filed residuals and still-open items

- **#34/#39** (`PAR-0603` `fixing`): client-side remainder — `MindUi` admin
  relay listener + roster refresh, player-list row authority gates and the
  `Binding.chat` toggle.
- **#37** (`PAR-0577` verified): host-side work remains — `MindCampaign` drain of
  `PlaySession.event_rows` + bus ownership, PlayerJoin/Leave/Research host fire
  sites, Config/Deposit/BuildSelect/BulletDestroy bus seams, and the relay
  MatchCreate/Play host edges.
- **#47** (`PAR-0592` verified): only the other-players' plan previews remain;
  they need network preview-plan sync, not a render hook.
- **#44** (partial): `mods.viewcontent` (View Content, bundle key at
  `assets/bundles/bundle.properties:152`) still has no Rust implementor.
- **#46** (partial): the GameOverDialog multiplayer waiting/disconnect states and
  `difficulty.guide` (bundle keys exist, no Rust use), the LoadDialog
  corrupt/missing-asset states (no `mod.missing` use; `@save.corrupted` is only
  shown on sector resume), and the Planet SectorSubmissions link
  (`sectors.viewsubmission`, no Rust use) remain.
- **#38** (partial): `onUnlock`/`showUnlock`, `getDependencies` (blocks/units)
  and `unlockedNowHost` (`unit_unlocked_now_host`, UI campaign) now exist;
  `logicVisible` has no implementor.
- **#26**: the core senses are verified (`PAR-0218`), but `mind-gdext` still
  does not call `Sim::set_logic_player_view`, so the host half is a follow-up.
- **#53** (partial): hitboxes landed (`PAR-0519`). Not re-verified in this
  pass: `fluidFrames`, the Drawf dash helpers/`flameFront`, `MenuRenderer`
  flyers, the `gx/fx.rs` noise/shader/polyline primitives, ColoredFloor/Wall
  renderer tint and the environment-prop draws (SeaBush/Tree/TallBlock/Cliff/
  Rune/Character/SteamVent). `ScreenshotPlan` exists in
  `mc/render/snapshot.rs`.
- **#12/#16/#13** also belong to this list for their twin/refund residuals, but
  are kept in the documented table above because they are not open code work.

### Not re-checked / caveats

- The closures are **headless** (`cargo test`/`cargo check`, `mind-headless`).
  For the 2026-11-07 items, `verified` means headless-verified (Java twin not
  run in this pass), not `twin-verified`.
- `#65` was already implemented before this pass (`game/runtime.rs` executes
  the `BaseBuilderAI` actions); it is recorded here because the survey was
  stale.
- The prior audits' session-local reports (`/tmp/gaps-audit-*.md`) are no
  longer referenced; the ledger and this appendix are the source of truth.
