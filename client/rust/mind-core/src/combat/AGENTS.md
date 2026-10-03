# AGENTS.md — mind-core/src/combat (combat systems)

The combat subsystem of `mind-core`: the `Bullet` component and its behavior half, bullet spawning
and per-kind dispatch, the `Damage` math, `Fires` and `Puddles` tile entities, chain `Lightning`,
deterministic target selection, the headless `CombatHarness`, and the renderer-free `view::FxSink`
seam. It is Godot-free and tokio-free; every sim random draw goes through `crate::determinism` and
every view/FX emission through `view::FxSink`. Read the root [`AGENTS.md`](../../../../../AGENTS.md)
first; the crate map is `mind-core/AGENTS.md` (`../../AGENTS.md`).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Module list, the public re-exports (`Bullet`, `BulletData`, `BulletSpawn`, `create`, `CombatHarness`, `TargetQueries`, `FxSink`, draw states) and the thin `CombatPlugin` host seam. |
| `bullet/mod.rs` | `Bullet` component, flags (`KEEP_ALIVE`/`JUST_SPAWNED`/`ABSORBED`/`HIT`/`OWNER_LOCAL`/`EXPIRED`), `BulletData`, `ShotMover`, `CombatCtx`, motion/collision/hit/despawn/finish (`update_bullet`, `collide_bullet`, `hit_bullet`, `finish_bullet`). |
| `bullet/spawn.rs` | `BulletSpawn` request and the single creation path `create` (`create_net` aliases it); applies damage/velocity/lifetime scaling and deterministic inaccuracy. |
| `bullet/behavior.rs` | `BulletBehavior` trait, `BaseBehavior`, and `behavior_for(BulletKind)` dispatch to the per-kind statics. |
| `bullet/raycast.rs` | Bullet tile raycasts over `world::raycast`: `first_wall_tile`, `first_build_tile`, `is_floor_like`. |
| `bullet/kinds/` | One module per upstream `BulletType` subclass (`basic`, `continuous`, `emp`, `empty`, `fire`, `flak`, `interceptor`, `laser`, `lightning`, `liquid`, `mass_driver`, `multi`, `point`, `sap`, `shrapnel`) plus shared `nearest_enemy_building`/`motion` helpers. |
| `damage/` | `armor` (`apply_armor`, `apply_armor_opt`, `MIN_ARMOR_DAMAGE`), `area` (`DamageOptions`, `damage_area`, `complete_damage`, `damage_entity`, `apply_health`), `line` (`collide_line`, `linecast`, `find_length`), `explosion` (`dynamic_explosion`, `tile_damage`), `status` (`StatusApply`, `StatusApplier`, `apply_status`, `status_area`). |
| `fires.rs` | `FireState` component and `Fires` behavior: `create`/`extinguish`/`remove`/`update_fires`, tile-slot cache sync, revisioned `write_fire`/`read_fire`, and `CombatEnv`. |
| `puddles.rs` | `PuddleState` component and `Puddles` behavior: `deposit`/`react_puddle`/`update_puddles`, `MAX_LIQUID`, revisioned `write_puddle`/`read_puddle`. |
| `lightning.rs` | `Lightning::create` chain builder, `LightningCounter` seed resource and `LightningResult`; chains to the farthest enemy building within range. |
| `targeting.rs` | `TargetQueries`: `BlockIndexer`-backed building lookup plus a stable unit snapshot, with `closest_target`/`closest_unit`/`find_ally_tile`. |
| `view.rs` | The `FxSink` trait and `NoopFx`/`FxHandle`/`noop_fx`/`CombatFx` aliases, `resolve_effect`, and the `BulletDrawState`/`TurretDrawState`/`ShieldDrawState`/`LaserDrawState` view structs. |
| `harness.rs` | `CombatHarness`: `BuildHarness` plus ordered bullet list, combat `tick`, `step_bullets_only`, fixture bullets, test unit/turret/defense spawners, and `checksum_value`/`checksum_hex`. |

## Key types

- `Bullet` is a plain ECS `Component` (bullets are pooled and never saved): `def`, `damage`,
  `data: BulletData`, `rotation`, `last`/`aim`/`origin`, `time`/`lifetime`, `collided`, `sticky`,
  `flags`, `frags` and an optional `ShotMover`. `finished()` reports `HIT`/`ABSORBED`/`EXPIRED`.
- `CombatCtx<'_>` bundles `world`, `content`, `grid`, `rng`, `fx`, `audio`, `seq` and the `spawned`
  buffer; `spawn` creates child bullets and records them for the host to drain in order.
- `BulletSpawn` is the complete creation request; `create` resolves the def, applies
  `damage_multiplier`/`velocity_scl`/`lifetime_scl`, angle/life/velocity randomness and the
  `create_chance` roll.
- `BulletBehavior` hooks (`init`, `update`, `hit`, `hit_tile`, `despawned`, `removed`,
  `test_collision`, `building_damage`, `shield_damage`, `continuous_damage`, `current_length`,
  `range`) default to the base bodies; `behavior_for` maps each `BulletKind` to a static instance.
- `CombatHarness` exposes `content()`, `bullet_id`/`names_map`, `place`, `building_health_at`,
  `combat_ctx`/`claim_scratch_spawned`, `lightning`, `set_fx`, and the `checksum_*` accessors.

## Bullet kinds and damage

- Each `bullet/kinds/*` module implements the hooks that differ from `BaseBehavior`: `laser` and
  `shrapnel` run `damage::line::collide_line` at `init`; `continuous` re-collides every
  `damage_interval`; `point` teleports to the endpoint; `multi` expands into `spawn_bullets`;
  `emp`/`flak`/`sap`/`interceptor`/`fire`/`liquid`/`lightning` follow their upstream subclasses.
- `mass_driver` converts a `MassDriverPayloadCarrier` fire request into a `QueuedMassDriverBolt`;
  the host drains `MassDriverBoltQueue` via `queued_spawn` so the physical bolt joins collision and
  checksum passes. `MassDriverPayload`/`MassDriverSink` let the distribution build handle delivery.
- Damage is armor-aware first: `apply_armor` returns `max(damage - armor, damage * MIN_ARMOR_DAMAGE)`
  and `apply_armor_opt` bypasses it when `pierce_armor`. `FireState` and `PuddleState` persist
  through `write_fire`/`read_fire` and `write_puddle`/`read_puddle`.

## Weapons and turrets

Weapons live in `mind-core::weapons` and turrets in `world/blocks/defense/turrets/`. Both drive
bullets through the same `CombatCtx`: `CombatHarness::update_weapons_only` calls
`weapons::update_weapons`, and `update_turrets_only` calls
`world::blocks::defense::turrets::update_turrets` against placed `TurretState`s registered by
`TurretBehavior`/`config_for`. Turret ammo bullet defs come from `turrets::register_bullets`.

## Invariants

- **Godot-free and tokio-free.** No source here may `use godot`/`use tokio`; the crate boundary is
  enforced by `cargo tree -p mind-core` and the CI `grep -RnE 'use (godot|tokio)'` gate.
- **All sim randomness through `crate::determinism`.** Uses of `SimRng`/`RngStream` (and the
  seeded `random::Rand` in lightning) keep combat replay-stable; `LightningCounter` is a monotonic
  resource rather than a process global.
- **Combat state is simulation state.** Bullets, building health, `FireState` and `PuddleState`
  contribute to `CombatHarness::checksum_value` (FNV-1a-64 via `determinism::Checksummer`).
- **Deterministic ordering.** Bullets advance in spawn order, area/line targets sort by
  `(EntitySeq, entity.index())`, and no `HashMap` iteration appears on a sim path.
- **FX is view-only.** Effects, shake, lights, sounds and trails are emitted through `view::FxSink`
  and never feed the simulation or the checksum; `NoopFx` is the headless default.

## Rules

- Add a bullet kind by implementing `BulletBehavior` in `bullet/kinds/`, adding a static instance,
  and extending `behavior_for`; read field-driven differences from the `BulletDef`.
- Route new sim randomness through `CombatCtx::rng`/`SimRng` and new view output through
  `CombatCtx::fx`; do not add a second FX dispatch path (`FxSink` is the single seam).
- Keep bullet creation on the `spawn::create` path so relayed and local creation stay identical.
- Port upstream Mindustry and cite it in the file header; files are UTF-8/LF, GPL-3.0-only.

## Verification

From `client/rust/` (or add `--manifest-path client/rust/Cargo.toml` from the repo root):

```bash
cargo test -p mind-core combat
cargo clippy -p mind-core
cargo test -p mind-core
```
