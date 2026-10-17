<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L04 — Unit, turret and explosion draw residuals

**Goal.** Finish the `UnitType.draw` / `DrawTurret` composition that round J started, and make
explosions visible.

**Evidence.** Ledger PAR-0363 (residual), PAR-0365–PAR-0369, PAR-0399, PAR-0402 (generator
effect half); round-J notes in `docs/J03-turret-unit-draw_summary.md` and
`docs/J04-combat-production-sim_summary.md`; round-L `godot/52-legs.png`.

**Build.** This plan is Rust (`mind-gdext/src/fx`, `render/`, `mind-core`): iterate with
`cargo check -p mind-gdext` and targeted tests; run `tools/build.sh` once before the
in-engine Java-pair check.

## T0 — Stage every Java capture in one batch — HIGH, before any code

Every task below is accepted against a Java capture of the same unit or block. Staging is
in-depth gameplay (place the factory/reconstructor, feed it from item sources), which is why
round L could not pair these — so stage **all** of them up front, in one Java sandbox session:
- T1: merui, cleroi, anthicus, tecta, collaris, elude, avert, obviate (idle + firing);
- T2: quasar, oct, aegires, quell, disrupt (per ability);
- T3: renale/latum crawl, mega/quad/oct payload carry;
- T4: scathe, smite, malign, lustre firing frames (+ the already-landed nine for comparison);
- T5: combustion generator + blast compound, thorium reactor meltdown;
- T6: atrax, spiroct, arkyid, toxopid (walking + standing).

Use the Core Database unit preview for static poses where it suffices. Record the recipe per
item. Then implement T1–T6 against the staged frames.

## T1 — Unit `parts` (PAR-0365) — HIGH

- Java: `UnitType.draw` runs `parts` after the body/weapons/shield (`type/UnitType.java:1560`)
  with `DrawPart.params` set from the unit: `warmup`, `reload` (`smoothReload` per mount),
  `heat`, `recoil`, `charge`, `life`. 31 parts on Erekir units, plus weapon `parts`
  (`Weapon.parts`, `type/Weapon.java`).
- Port: the turret path already feeds the `DrawPart` program live (`mind-gdext/src/fx/
  draw_turret.rs`, J03 T1). Add a unit feed from `WeaponMount` state and draw at the unit's
  layer.
- Accept: merui, cleroi, anthicus, tecta, collaris, elude, avert and obviate idle and firing,
  against Java.

## T2 — Ability draws (PAR-0366) — HIGH

Port `draw(Unit)` for `ForceFieldAbility`, `EnergyFieldAbility`, `ArmorPlateAbility`,
`UnitSpawnAbility`, `MoveLightningAbility`, `SuppressionFieldAbility` and `ShieldArcAbility`
(`entities/abilities/*.java`). Shield visuals share the shield pass
(`mind-gdext/src/render/shields.rs`). Accept against quasar, oct, aegires, quell, disrupt and
the units that carry each ability.

## T3 — Crawl segments, payload draws, weapon `layerOffset` (PAR-0367/0368/0369) — MEDIUM

- `UnitType.drawCrawl` + `segmentRegions/segmentCellRegions/segmentOutlineRegions` for
  `renale`/`latum` (`UnitTypes.java:4316,4335`).
- `UnitType.drawPayload` for `Payloadc` units (mega/quad/oct carrying blocks and units), read
  from the PAR-0138 payload state.
- `Weapon.layerOffset` (`type/Weapon.java`) applied to the weapon draw z.

## T4 — Turret part trees left by J03 (PAR-0363 residual) — MEDIUM

Port the remaining hand-written part trees: scathe, smite, malign, lustre and any others with
`parts`/`ammoParts` that J03 skipped (`grep -n "parts.add\|ammo(" content/Blocks.java` against
the port's `turret_parts.rs`). Capture firing frames for these and the already-landed nine.

## T5 — Explosion view half: `ExplosionSink` host (PAR-0399 + PAR-0402 effects) — HIGH

- Port: `mind-core/src/world/blocks/power/explosion.rs:125-160` defines `ExplosionSink`, but
  no `mind-gdext` node installs `ExplosionSinkRes`. Thorium-reactor and generator explosions
  therefore apply damage, puddles and fires with **no** visible effect, sound, shake or scorch.
  The `Fx.generatespark`/`explodeEffect` generator sparks (J04 T2) have no host either.
- Java: `PowerGenerator.createExplosion` (`world/blocks/power/PowerGenerator.java`) and
  `ConsumeItemExplode.update`: `explodeEffect.at`, `explodeSound.at`,
  `Effect.shake(explosionShake, explosionShakeDuration, …)`, `Damage.dynamicExplosion` (its
  own effect/scorch), `Fx.generatespark`.
- Change: implement `ExplosionSink` in `mind-gdext` and route it into the existing `MindFx`
  (effect + decal), `MindAudio` and camera shake seams. Install it in `MindSimHost` on world
  load.
- Accept: a combustion generator fed blast compound sparks, and a thorium reactor meltdown in a
  sandbox shows Java's effect, scorch and shake (capture both).

## T6 — Legged units against Java — MEDIUM (verification)

Round J landed legs (`9f381d49`). Round L shows feet and joints, but nothing pairs them with
Java. Capture atrax, spiroct, arkyid and toxopid (walking and standing) in both clients. If the
leg segments differ, compare the leg region draw with `UnitType.drawLegs`
(`type/UnitType.java:1826-…`): `legRegion` stretched from base to joint, `legBaseRegion` joint
to foot, `legExtension`, `legSplashDamage` FX. Close PAR-0126 on a match.

## Done criteria

T1, T2 and T5 landed with Java-paired frames. T3/T4/T6 landed or recorded. PAR-0126 and
PAR-0363 closed when their remaining parts match. Summary in
`docs/L04-unit-fx-draw-residuals_summary.md`.
