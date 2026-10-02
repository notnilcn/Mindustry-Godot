# Ledger — Fx effects (`content/Fx.java`, 267 entries, `Effect.all` order)

Source of truth: plan-02 seed table `client/rust/mind-core/src/content/registries/fx_meta.rs`
(name + lifetime, generated from the upstream field list) and plan 17's
`mind-core::fx::catalog`. `mind-headless fx audit` verifies this ledger's
structural invariants (registry length, name/order/lifetime parity with the seed,
classification totals, and the order hash).

## Status

| Field | Value |
|---|---|
| Entries | 267 (`none` + 266) |
| Order hash (`fx_order.txt` drift gate) | `94f48259c1412fef` (fnv1a over names, declaration order) |
| `none` | 1 (id 0, rejected by `shouldCreate`) |
| Declarative kinds | 0 in the **vanilla** catalogue (see note) |
| Composite kinds | 0 in the **vanilla** catalogue (see note) |
| Custom Rust bodies | 90 |
| Unported | 176 |
| Ported | 90/267 |

**Note (upstream fact, plan 17 R-17-11):** every field in `content/Fx.java` is a
plain `new Effect(lifetime, Cons)` construction — a grep for
`new ExplosionEffect|MultiEffect|WaveEffect|RadialEffect|WrapEffect` in
`Fx.java` returns **0**. The declarative `Particle`/`Explosion`/`Wave`/`Triangle`/
`Noise`/`Sound` kinds and the `Multi`/`Seq`/`Radial`/`Wrap` composites are
therefore exercised only by inline effects in blocks/units (mods + plan 02
`EffectSpec`), and by plan 17's own unit tests. They are implemented and tested
in `mind-core::fx::resolve` / `fx::pool`.

## Ported custom bodies (id → name)

| id | name | kind | data | notes |
|----|------|------|------|-------|
| 1 | blockCrash | Custom | Block | 2 rects, `e.fin` alpha |
| 2 | trailFade | Custom | Trail | trail geometry via gdext registry; lifetime override |
| 3 | unitSpawn | Custom | UnitType | scale + `mixcol` |
| 6 | unitControl | Custom | Unit | parent-followed; squares |
| 7 | unitDespawn | Custom | Unit | `mixcol(accent)` |
| 8 | unitSpirit | Custom | Position | `pow2In`/`pow5In` lerp |
| 9 | itemTransfer | Custom | Position | `rotate90` jitter |
| 10 | pointBeam | Custom | Position | line + light |
| 11 | pointHit | Custom | — | `fin` color lerp + circle |
| 12 | hitScepterSecondary | Custom | — | signs + seeded sparks |
| 13 | lightning | Custom | Positions | segment lines + node circles |
| 14 | coreBuildShockwave | Custom | — | lifetime override = `rotation` |
| 15 | coreBuildBlock | Custom | Block | `mixcol(accent)` |
| 16 | pointShockwave | Custom | — | `finpow` circle + spark fan |
| 17 | moveCommand | Custom | — | overlay-UI layer |
| 18 | attackCommand | Custom | — | overlay-UI layer |
| 22 | placeBlock | Custom | — | accent square |
| 24 | tapBlock | Custom | — | accent circle |
| 25 | breakBlock | Custom | — | remove square + debris |
| 26 | payloadDeposit | Custom | (simplified) | no live payload item yet |
| 27 | select | Custom | — | accent circle |
| 28 | smoke | Custom | — | gray fade circle |
| 71 | healWave | Custom | — | heal circle |
| 77 | hitBulletSmall | Custom | — | scaled ring + 5 sparks + light |
| 78 | hitBulletColor | Custom | — | `e.color` variant |
| 82 | hitBulletBig | Custom | — | cone sparks |
| 83 | hitFlameSmall | Custom | — | flame cone |
| 85 | hitLiquid | Custom | — | liquid dots |
| 98 | hitLaser | Custom | — | heal ring + light |
| 143 | shockwave | Custom | — | white→lightGray circle |
| 148 | explosion | Custom | — | ring + smoke + spark fan |
| 155 | shootSmall | Custom | — | tri pair |
| 160 | shootBig | Custom | — | tri pair |
| 190 | casing1 | Custom | — | seeded rotating rect |

### Batch F3/F4 (plan 17 M3, 56 more bodies)

Transcribed 1:1 from `content/Fx.java`; all emit deterministically (asserted by
`fx::resolve::tests::every_ported_custom_body_emits_deterministically`).

| name | notes |
|------|-------|
| commandSend | accent circle, `finpow * rotation` |
| upgradeCoreBloom | accent square, tile-scaled |
| coreLaunchConstruct | square + seeded debris |
| fallSmoke / rocketSmoke / rocketSmokeLarge / magmasmoke | gray circles |
| spawn / padlaunch | accent poly rings |
| breakProp | `e.color * 1.1` seeded circles (debris layer) |
| unitDrop / unitLand / unitDust / unitLandSmall / unitPickup / crawlDust / landShock / pickup | unit dust/pickup family (debris layer; `Lines.spikes`) |
| sparkExplosion | ring + 16 `lineAngle(offset)` rays |
| titanExplosion / titanExplosionLarge / titanExplosionSmall / titanExplosionFrag | ring + ray/tri fans |
| coreExplosion | dual 30-ray fan |
| smokeAoeCloud | 80-particle clamp-window cloud |
| scatheExplosion / scatheExplosionSmall / scatheSlash | ring + tri fans |
| scatheLight / scatheLightSmall / titanLightSmall | fill lights (`Layer.bullet + 2`) |
| dynamicSpikes / greenBomb | ring + 4 tri spikes |
| greenLaserCharge / greenLaserChargeSmall | heal charge ring/dots |
| greenCloud | `randLenVectors(fin)` dots |
| healWaveDynamic / heal / dynamicWave / shieldWave / shieldApply | wave rings |
| hitSquaresColor / hitFuse / hitFlamePlasma / hitLaserBlast / hitEmpSpark / hitLancer / hitLancerLow / hitBeam / hitFlameBeam / hitMeltdown / hitMeltHeal | hit family |
| instBomb / instTrail / instShoot / instHit | instigator bullet family |

## Unported (176)

Waves F3–F6 remain. Each is currently `EffectKind::Unported` (emits nothing and
is counted by `fx audit`). They are **not** silently dropped: `fx audit --strict`
fails while any remain, and `MindFx.effect_kind_counts()` reports the count.

## Deliberate deviations (recorded)

- **Decals capped** at 1 024, oldest dropped (`DECAL_CAPACITY`); upstream is
  pooled/unbounded (plan 17 §2.4 #6).
- **View clock** is the fixed tick counter, not wall time (deviation #7).
- **`layerDuration`** is preserved but never read (upstream dead field, #8).
- **Collection regions** on custom bodies use the `error` sentinel until plan 03
  atlas binding is wired in `mind-gdext::fx` (M5).
- **LOD particle halving** and native `Noise`/caustics blits are plan 16's
  execution layer (#9).

## Verification

- `mind-headless fx audit` — structural invariants + order hash.
- `mind-headless fx program <name> --tick N --check <golden>` —
  `tests/golden/fx/fx_program_*.json`.
- `mind-headless fx lifecycle --ticks 120 --check <golden>` —
  `tests/golden/fx/fx_lifecycle.json`.
- `mind-headless fx trail --check <golden>` — `tests/golden/fx/fx_trail.json`.
- `cargo test -p mind-core fx::` — container math, gates, lifecycle, composites,
  progressors, decals, shake, weather, trail.
