<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L04 — Unit, turret and explosion draw residuals (continuation)

This session continued the paused round-L backlog. T5 (the `ExplosionSink` host)
is landed and unit-tested; T1–T4/T6 remain open with the staged-capture blocker
recorded by the plan (T0 was attempted in `runs/20261018-s3-L04-T0/` and produced
no leg-stage frames).

## T5 — Explosion view half (PAR-0399 / PAR-0402 effects) — DONE (code)

- `GeneratorConfig` gained `explode_effects: [EffectId; 3]` and
  `explode_sound: SoundId` (`PowerGenerator.explodeEffect`/`explodeSound`,
  `PowerGenerator.java:35-36`); `ReactorExplosion::for_building` copies them.
- Filled for the vanilla reactors:
  - `thorium-reactor`: `Fx.reactorExplosion` + `Sounds.explosionReactor`
    (`NuclearReactor.java:72-73`);
  - `impact-reactor`: `Fx.impactReactorExplosion` + `Sounds.explosionReactor2`
    (`ImpactReactor.java:40-41`);
  - `flux-reactor`: `MultiEffect(Fx.bigShockwave, Fx.titanSmoke)` +
    `Sounds.explosionReactor2` (`VariableReactor.java:44-45`);
  - `neoplasia-reactor`: `MultiEffect(bigShockwave, titanSmoke,
    neoplasmSplat)` + `Sounds.explosionReactorNeoplasm`
    (`Blocks.java:2836-2837`).
- `ExplosionSink` gained a default `generatespark`; `ConsumeItemExplode` now
  emits `Fx.generatespark` through a new `explosion::generatespark` helper
  (`ConsumeItemExplode.java:20-42`).
- New host `client/rust/mind-gdext/src/fx/explosion_sink.rs`:
  - pushes each `explodeEffect` onto the `MindFx` bus as `FxEvent::Effect`;
  - pushes `FxEvent::Shake { intensity, duration }` (`Effect.shake`);
  - pushes a `scorch-<0..9>-<0|1>` decal (lifetime 3600, `Pal.rubble`,
    `Effect.scorch`);
  - emits `AudioEvent::At` through `AudioSinkRes` for `explodeSound`.
- `MindSimHost::install_fx_sink` installs the host after every Sim replacement,
  so a new world never keeps the inert default.
- Tests: `params_carry_the_explode_effect_and_sound`
  (`world/blocks/power/explosion.rs`) and
  `fx::explosion_sink::tests::{host_sink_emits_effect_shake_scorch_and_sound,
  generatespark_uses_the_generator_effect}`.
- In-engine acceptance (Java pair) still needs the T0 capture batch: the view
  code is landed, but a live thorium meltdown / blast-compound generator pair was
  not captured in this session.

## Remaining

- T1 unit `UnitType.parts` feed (`UnitDraw` has no parts/fin/ground-layer; the
  turret DrawPart path is the model).
- T2 ability draws (ForceField/EnergyField/ArmorPlate/UnitSpawn/MoveLightning/
  SuppressionField/ShieldArc); `AbilityState` lacks the view fields and
  ArmorPlate/UnitSpawn/MoveLightning are not modelled.
- T3 crawl segments, payload draws and live weapon `layerOffset` (the
  `fx::parts::weapons` path already applies it but the live unit pass bypasses
  it).
- T4 scathe/smite/malign/lustre part trees (HaloPart kind; `DrawPartSpec` has no
  halo arm). This session did not start them.
- T6 live Java leg pairs (PAR-0126 closes on a MATCH frame, not on code).

## Continuation (2026-10-18, PAR-0365/PAR-0366)

### T1 — unit `parts` feed — **DONE (code + live probe)**

- `UnitDraw` now carries `parts`, `fin` (`TimedKillComp.time / lifetime`),
  `draw_body`/`draw_cell`/`ground_layer`; `WeaponDraw` carries the resolved
  `WeaponDef` and live `WeaponMount`.
- New `mind-gdext/src/render/unit_parts.rs`: three nodes per unit band
  (`Under`/`Over`/`OverAdditive`) execute `DrawPart.draw` for every unit part
  (`UnitType.java:1561-1578`) and `Weapon.draw`'s under/over parts
  (`Weapon.java:236-278`) through the shared `PartEmit` program. The part
  params come from the part's `weaponIndex` mount (`warmup`,
  `reload/weapon.reload`, `smoothReload`, `heat`, `recoil`, `charge`, unit
  position/rotation) plus `fin`; weapon parts apply `Weapon.layerOffset`.
- `render/units.rs` gates the body/cell quads on `draw_body`/`draw_cell`.
- `mind-core` exposes `weapon_part_params`; a unit test pins the mount fields.
- Live (sandbox, display :10): quasar renders its force-field hexagon, quell
  its suppression orb, tecta its legs + `tecta-shield` arc without script
  errors. Unit-part quads themselves still need the Java pair (T0 staging).

### T2 — ability draws — **DONE (vanilla kinds) / recorded**

- `AbilityState` gained the view mirrors: `cur_stroke` (EnergyField),
  `radius_scale` + `alpha` (ForceField/ShieldArc), advanced in
  `update_unit_abilities` (`ForceFieldAbility.java:91-100`,
  `EnergyFieldAbility.java:118-124`, `ShieldArcAbility.java:199-220`).
  Test: `ability_view_state_lerps`.
- `AbilitySpec` gained the draw fields (`effect_radius`, blink/sector/rotate
  knobs, `layer`, orb/particle knobs, `angle_offset`, `draw_arc`,
  `offset_region`) with the Java defaults, plus the EnergyField `Pal.heal`
  colour and layer.
- `MindSimHost::unit_ability_draws` exposes the live rows. New
  `render/unit_abilities.rs` draws EnergyField (`Layer.bullet - 0.001`) and
  SuppressionField (`Layer.effect`, deterministic splitmix particle scatter);
  `render/shields.rs` draws ForceField polygons and ShieldArc regions/arcs in
  the existing shield group.
- Remaining: ArmorPlate/UnitSpawn/MoveLightning are mod-only kinds and are not
  modelled in the content registry (no vanilla unit uses them); record as a
  deviation. EnergyField `Drawf.light` is not routed to the light pass.

### T3 — partial

- Weapon-part `layerOffset` now lands in the parts pass. The hand-drawn weapon
  region/cell quads still bypass `layerOffset` (single mesh, no per-quad z);
  crawl segments and payload composition remain open.

### T4 — turret part trees — **DONE (code + unit test)**

- `DrawPartKind::HaloPart` and the `HaloPart` fields (`tri`, `shapes`,
  `triLength*`, `haloRadius*`, `shapeRotation`, `shapeMoveRot`,
  `haloRotateSpeed`, `haloRotation`) are on `DrawPartSpec`;
  `PartEmit::draw_part` adapts them onto the existing `draw_halo` and honours
  `under && turretShading` plus `layerOffset`.
- `PartProgressSpec` gained the time-wave `SinTime` expression.
- `turret_parts.rs` now carries `lustre` (three regions with the blended heat
  progress), `scathe` (blade + child side, mid heat wave, base missile + the
  three ammo missile variants), `smite` (four regions, two shapes, five halos)
  and `malign` (four circle shapes, spike, five regions, two halo shapes, five
  halos, three spines). `turret_parts_cover_the_vanilla_drawers` pins the names,
  counts and halo kinds. Live firing frames still need the L04 T0 Java batch.
