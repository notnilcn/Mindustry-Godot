<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L06 — Simulation and campaign residuals

**Goal.** Port the gameplay behaviours round J recorded as "not ported". Every task needs a
failing `mind-core`/`mind-headless` test first. These are mostly headless; in-engine checks
follow the round-J recipes.

**Build.** Rust (`mind-core` sim + `mind-gdext`): iterate with `cargo check` + targeted
`cargo test -p mind-core <test>`; `tools/build.sh` only before in-engine checks.

**Evidence.** PAR-0402, PAR-0391, PAR-0296; `runs/20261017-s2-J04-T6/report.md`,
`runs/20261017-s5-J07-T4/notes.md`, `docs/J04-*_summary.md`, `docs/J07-*_summary.md`.

## T1 — Unit assembler output gate and hand-off — HIGH

Java `UnitAssembler` (`world/blocks/units/UnitAssembler.java`):
- `checkSolid`/`wasOccupied` blocks spawning while the output area is occupied (`:466-472`);
- the produced payload is handed to `targetBuild` when a payload conveyor or similar sits on the
  output (`:516-532`);
- `commandPos` is passed to the spawned unit (`:560-562`).

The port spawns directly at `getUnitSpawn` (I01 T11 deviations 2-3, J04 T6 record). Port all
three; the command hand-off needs the `UnitCommandRuntime` seam.

## T2 — `router` ControlBlock + controllable blocks — MEDIUM

Java's `Router` implements `ControlBlock` (`world/blocks/distribution/Router.java`), next to
`Turret` and `BuildTurret` (ported in I01 T12). `Turret.playerControllable` is also unported
(I01 T12 deviation 1). Port the router's control unit and the `playerControllable` field.

## T3 — Bullet suppression and laser/pierce length pre-pass — MEDIUM

- `BulletType.suppressionRange/suppressionDuration/suppressionEffectChance`
  (`entities/bullet/BulletType.java:329-335,562-580`) → `Damage.applySuppression`: suppresses
  enemy *healing blocks* (menders/regen projectors) in range.
- `findPierceLength`/`findLaserLength` (`entities/Damage.java:165-200`), which shorten lasers at
  `laserAbsorb`/absorbing buildings (plastanium walls). The port draws and damages full length
  (`combat/bullet/kinds/laser.rs:40`).

## T4 — Tethered units and `EmptyDataAbility` I/O — LOW

`UnitTetherComp.spawnerUnitId` and `EmptyDataAbility` round-trip through saves (`TypeIO`). Test
a save/load with `UnitSpawnAbility` children: they must stay tethered.

## T5 — `DrawLiquidOutputs` side regions — LOW

J04 T3 landed the direction data. Draw the per-side output regions
(`world/draw/DrawLiquidOutputs.java`; port `render/draw_desc.rs:678` is region-only) for the
electrolyzer and other crafters with `outputLiquids`.

## T6 — Campaign rules and generators (PAR-0391) — MEDIUM

1. `Planet.prebuildBase` (`type/Planet.java:133`) and `allowCampaignRules` (`:139`, `:221`
   `isVanilla()`): add both fields and gate the `CampaignRulesDialog` and difficulty button on
   them (`PlanetDialog.java:738`).
2. `ErekirPlanetGenerator`: apply `state.rules.*`, `Schematics.placeLaunchLoadout` and
   `Waves.generate` (`maps/planet/ErekirPlanetGenerator.java`; port header
   `mind-core/src/maps/planet/erekir.rs:7-13`). I04 T17 is the Serpulo precedent.
3. Abandon semantics: Java hides the planet page, kills the sector's cores and plays the
   game-over path (`PlanetDialog.java:1166-1177`). The port deletes the record and the save
   (I04 T10 deviation).
4. `SectorPreset.showSectorLandInfo` and the land-info gates.
5. Planet clouds: `Planet.drawClouds`/`cloudMesh` (`PlanetRenderer.java:149`; J07 deviation 2).

## T7 — Interplanetary Accelerator (PAR-0296) — MEDIUM

Use the J07 T4 record (`runs/20261017-s5-J07-T4/notes.md`). Vanilla never fills
`Planet.launchCandidates`, so the Accelerator is a **same-planet** core launch
(`PlanetDialog.canLaunchTo`, `allowSelfSectorLaunch`, `:610`). Port:
- the progress/item-capacity state;
- `canLaunch` + the config button (`Accelerator.java:42,129-…`);
- a launch mode in the planet dialog;
- the core and resource carry;
- the `LaunchAnimator` cutscene (`:322-…`, reusing the I02 T1 pod/cutscene path).

Accept: a charged accelerator on a captured sector launches to another sector, against a Java
capture.

## Done criteria

T1, T3, T6.1–T6.3 and T7 landed with tests. The rest are landed or recorded. Moved goldens
are regenerated in the batched pass with L02 (L-INDEX speed rule 3) under the L08 T1 protocol.
Summary in `docs/L06-sim-campaign-residuals_summary.md`.
