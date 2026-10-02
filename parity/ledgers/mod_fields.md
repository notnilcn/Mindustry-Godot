# Mod content field ledger (plan 20 §7a/R4)

Tracks accepted JSON field names per content kind. Unknown fields warn and are
ignored (`ignoreUnknownFields = true`); this ledger records the implemented set
so drift from upstream Java fields is visible.

Status as of 2026-10-02 (lane/20-mods): **M1 item + block core fields implemented; M2 all nine top-level kinds + unit weapons/bullets/abilities/shoot patterns parsed; M2b nested frag/interval/spawn bullets + `mind-tools mods classmap --check`; M3b patch surface (`drillMultipliers`, `attributes`, deep weapon/bullet paths, abilities).**

## Item (`mindustry.type.Item`)

| JSON field | Rust target | Status |
|---|---|---|
| `name` | bundle `item.<name>.name` + `localized_name` | implemented |
| `description` | bundle `item.<name>.description` | implemented |
| `color` | `Item.color` (hex or `[r,g,b,a]`) | implemented |
| `hardness` | `Item.hardness` | implemented |
| `cost` | `Item.cost` | implemented |
| `explosiveness` | `Item.explosiveness` | implemented |
| `flammability` | `Item.flammability` | implemented |
| `radioactivity` | `Item.radioactivity` | implemented |
| `charge` | `Item.charge` | implemented |
| `healthScaling` | `Item.health_scaling` | implemented |
| `lowPriority` | `Item.low_priority` | implemented |
| `frames` | `Item.frames` | implemented |
| `transitionFrames` | `Item.transition_frames` | implemented |
| `frameTime` | `Item.frame_time` | implemented |
| `buildable` | `Item.buildable` | implemented |
| `hidden` | `Item.hidden` | implemented |
| (all other `UnlockableContent` fields) | `UnlockFields` | TODO (M2+) |

## Block (`mindustry.world.Block` and subclasses)

| JSON field | Rust target | Status |
|---|---|---|
| `name` / `description` | bundle + `UnlockFields` | implemented |
| `type` | `BlockKind` (ClassMap, M2) | partial aliases |
| `size`, `health`, `armor` | `BlockSpec` | implemented |
| `itemCapacity`, `liquidCapacity` | `BlockSpec` | implemented |
| `solid`, `floating`, `update`, `destructible` | `BlockSpec` | implemented |
| `saveData`, `saveConfig`, `configurable`, `inEditor` | `BlockSpec` | implemented |
| `hasItems`, `hasLiquids`, `hasPower` | `BlockSpec` | implemented |
| `placeablePlayer`, `placeableLiquid`, `placeableOn` | `BlockSpec` | implemented |
| `buildVisibility`, `category` | `BlockSpec` | implemented |
| `requirements` | `BlockSpec.requirements` | implemented |
| `consumes` | `BlockDef.consumes` | implemented (patch merge; JSON parse TODO) |
| `drillMultipliers` | `BlockDef.drill_multipliers` | implemented (parse TODO; patch M3b) |
| `attributes` | `BlockDef.attributes` (name-keyed) | implemented (patch M3b) |
| `unit_plans*`, `upgrades`, `assembler plans` | `BlockDef` | TODO (M2/M3) |

## M2 top-level kinds

| Kind | Implemented JSON fields | Notes |
|---|---|---|
| `liquid` | `name`, `description`, `type` (`CellLiquid`), `color`, `gas`, `gasColor`, `barColor`, `lightColor`, `temperature`, `heatCapacity`, `viscosity`, `explosiveness`, `flammability`, `blockReactive`, `coolant`, `moveThroughBlocks`, `incinerable`, `capPuddles`, `boilPoint`, `particleSpacing`, `hidden`, `effect`, `particleEffect`, `vaporEffect`, `canStayOn` | `effect` resolves status names |
| `status` | `name`, `description`, `damageMultiplier`, `healthMultiplier`, `speedMultiplier`, `reloadMultiplier`, `buildSpeedMultiplier`, `dragMultiplier`, `transitionDamage`, `disarm`, `damage`, `intervalDamageTime`, `intervalDamage`, `intervalDamagePierce`, `effectChance`, `parentizeEffect`, `permanent`, `reactive`, `dynamic`, `show`, `color`, `effect`, `applyEffect`, `applyExtend`, `applyColor`, `parentizeApplyEffect`, `affinities`, `opposites`, `outline` | opposites/affinities via `TransitionSpec`; effect strings via `EffectId` |
| `unit` | `name`, `description`, `type` (entity keyword), `template` (ClassMap `UnitType`), `controller`, `aiController`, `health`, `speed`, `armor`, `hitSize`, `drag`, `accel`, `rotateSpeed`, `range`, `mineRange`, `buildRange`, `buildSpeed`, `mineSpeed`, `itemCapacity`, `mineTier`, `flying`, `targetAir`, `targetGround`, `useUnitCap`, `playerControllable`, `logicControllable`, `hidden`, `isEnemy`, `immunities`, `weapons` | weapons → inline `bullet` objects; requirements/reconstructor plans TODO (M3) |
| `weather` | `name`, `description`, `type`, `duration`, `opacityMultiplier`, `attrs`, `sound`, `soundVol`, `soundVolMin`, `soundVolOscMag`, `soundVolOscScl`, `hidden`, `status`, `statusDuration`, `statusAir`, `statusGround`, plus particle/rain `color`/speed/density fields | `attrs` maps `Attribute` names |
| `sector` | `name`, `description`, `planet`, `sector`, `captureWave`, `difficulty`, `startWaveTimeMultiplier`, `addStartingItems`, `noLighting`, `isLastSector`, `requireUnlock`, `showHidden`, `attackAfterWaves`, `outline`, `outlineRadius` | `planet` required; rules override TODO (plan 12) |
| `planet` | `name`, `description`, `parent`, `radius`, `sectorSize`, `mesh`, `cloudMesh`, `bloom`, `accessible`, `visible`, `hasAtmosphere`, `updateLighting`, `lightColor`, `atmosphereColor`, `iconColor`, `generator`, `startSector`, launch flags, `drawOrbit` | `sectorSize` builds the grid |
| `team` | `name`, `description`, `team` | database-only |

## Nested definables (M2 subset)

| Nested kind | Implemented fields | Notes |
|---|---|---|
| `bullet` (unit weapon) | `type` (ClassMap `BulletType`), `damage`, `speed`, `lifetime`, `hitSize`, `drawSize`, `width`, `height`, `length`, `splashDamage`, `splashDamageRadius`, `ammoMultiplier`, `reloadMultiplier`, `pierceCap`, `pierceDamageFactor`, `pierce`, `pierceBuilding`, `keepVelocity`, `collides`, `frontColor`/`backColor`/`colors`, `lightning`/`lightningLength`/`lightningLengthRand`, `status`, `frag`, `intervalBullet`, `spawnBullets` | registered recursively as non-mappable `BulletDef`; `spawnUnit` TODO (plan 11) |
| `weapon` | `name`, `mirror`, `reload`, `x`, `y`, `alternate`, `rotate`, `shootOnDeath`, `shoot`, `bullet` | `WeaponDef::from_spec`; effect/draw parts TODO |
| `shoot` | `type` (ClassMap `ShootPattern`: ShootPattern/Alternate/Spread/Helix/Barrel/Multi/Sine/Summon), `shots`, `barrels`, `barrelOffset`, `firstShotDelay`, `shotDelay`, `spread`, `scl`, `mag`, `offset`, `sineScl`, `sineMag`, `x`, `y`, `radius`, `mirror` | `ShootPatternSpec` |
| `ability` | `type` (ClassMap `Ability`), `amount`, `max`, `reload`, `range`, `healPercent`, `sameTypeHealMult`, `smartDowntime`, `rotation`, `regen`, `cooldown`, `duration`, `orbRadius`, `particleSize`, `x`, `y`, `angle`, `width`, `chanceDeflect`, `minVelocity`, `interval`, `spread`, `percentAmount`, `particles`, `maxTargets`, `sides`, `randAmount`, `smartHeal`, `active`, `whenShooting`, `teamColor`, `color`/`effectColor`/`particleColor`, `status`, `effect`, `unit`, `liquid` | `AbilitySpec`; `ShieldArcAbility`/`SuppressionFieldAbility` aliases added |
| `effect` | string names via `effect_by_name` (`effect`/`applyEffect`/`particleEffect`/`vaporEffect`) | inline `MultiEffect`/array forms TODO (plan 17 registry) |
| `draw` | not yet parsed | TODO (M2b; plan 17) |

## ClassMap replacement (M2)

`mods/json/classmap_gen.rs` (committed, **generated — do not edit**) holds
non-block aliases; block aliases derive from `BlockKind::ALL`. `ClassTagMap::resolve`
accepts both simple names and FQCNs (package-stripped) and enforces the scope
(`scope_mismatch` test). `mind-tools mods classmap [--check]` writes and verifies
`parity/mod_classmap.json` (224 entries) and `ClassTagMap::audit()` guards
duplicates/round-trip. Runtime `register_kind!` factories for plans 07/10/11/17
remain TODO.

## Patch surface (M3/M3b)

`DataPatcher` supports the nine top-level kinds, `Seq`/array (`Set`/`+`/index),
`ObjectSet` `+` (`immunities`), `ObjectFloatMap` (`drillMultipliers`), `Attributes`
(`attributes`, name-keyed incl. custom), `consumes` merge, deep
`weapons.<i>.bullet.<field>` edits, `targetFlags`, `unit.type`, `requiredPlanets`
gating, `afterPatch`, `unapply` resets and `fix_content_arrays` growth.
Remaining: reconstructor `upgrades` array model, block `drawer` parts,
created-object `postInit/load` (client), full 28-case `PatcherTests` parity.
