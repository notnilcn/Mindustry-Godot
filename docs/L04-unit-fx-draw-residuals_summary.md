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
