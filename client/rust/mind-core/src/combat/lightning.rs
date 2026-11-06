// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Lightning` chain helper (`core/src/mindustry/entities/Lightning.java`).
//!
//! Ports the full `createLightningInternal` body:
//!
//! - each step spawns a `hitCreate` bullet, then draws one jittered polyline
//!   point (`Mathf.range(3f)`);
//! - consecutive points are raycast (`World.raycastEach`); an enemy insulated
//!   building snaps the last point to that tile's corner and ends the branch;
//! - the chain snaps to the *farthest* enemy unit in the `hitRange` box
//!   (`Units.nearbyEnemies` + `Geometry.findFurthest`) when it is within
//!   `hitRange * 2`; otherwise it walks along a randomized angle;
//! - a `hitter` bullet gates the candidates with `UnitType.checkTarget`
//!   (`collidesAir`/`collidesGround`), caps the link count with `pierceCap`
//!   and scales the branch damage by `Bullet.damageMultiplier()`;
//! - after `maxChain` links the candidates are empty, so the branch walks
//!   randomly (upstream clears `entities` once `hit.size >= maxChain`).
//!
//! Java's static `lastSeed++` becomes a monotonic [`LightningCounter`] resource
//! so replays are stable (plan 10 §3.2). The branch also emits the
//! `Fx.lightning` view event (polyline + caller colour) exactly like
//! `Lightning.java:113`.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::query::{With, Without};
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::registries::bullets::DAMAGE_LIGHTNING;
use crate::content::registries::fx_meta::EffectRef;
use crate::content::{BulletId, EffectId, Rgba};
use crate::entities::comp::unit::HitboxComp;
use crate::entities::comp::unit::comp::{BlockUnitComp, StatusComp, UnitTypeComp};
use crate::entities::comp::{Building, Health, Pos, TeamComp, Unit};
use crate::fx::EffectData;
use crate::random::Rand;
use crate::world::block::{BlockTable, TILE_SIZE};
use crate::world::{WorldGrid, raycast};

use super::bullet::{BulletSpawn, CombatCtx};

/// Monotonic lightning seed (`Lightning.lastSeed`), reset on match reset.
#[derive(Debug, Clone, Copy, Default, Resource)]
pub struct LightningCounter(pub i32);

/// Chain limits (`Lightning.maxChain`).
const MAX_CHAIN: usize = 8;
/// `Lightning.hitRange`.
const HIT_RANGE: f32 = 30.0;

/// The `hitter` bullet's chain parameters (`Lightning.create(Bullet, ...)`).
///
/// Mirrors the three `BulletType` fields the Java helper reads from the bullet
/// that created the branch: the air/ground target gate (`collidesAir`/
/// `collidesGround`), the chain cap (`pierceCap`) and the instance damage
/// multiplier (`BulletComp.damageMultiplier()`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightningHitter {
    /// `BulletType.collidesAir`.
    pub collides_air: bool,
    /// `BulletType.collidesGround`.
    pub collides_ground: bool,
    /// `BulletType.pierceCap` (`<= 0` = no cap).
    pub pierce_cap: i32,
    /// `Bullet.damageMultiplier()` (status + `rules.unitDamage`/`blockDamage`).
    pub damage_multiplier: f32,
    /// `BulletType.lightningType` (the per-step `hitCreate` bullet).
    pub lightning_type: Option<BulletId>,
}

/// Result of one lightning branch (test/inspection output).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LightningResult {
    /// Endpoint of the branch.
    pub end: (f32, f32),
    /// Number of chained targets.
    pub links: usize,
    /// Chained target entities in order.
    pub hit: Vec<Entity>,
    /// Requested bolt tint (`Lightning.create(color)`; `Pal.lancerLaser` for a
    /// shock mine). Carried into the `Fx.lightning` event.
    pub color: Rgba,
    /// Whether an enemy insulated building ended the branch
    /// (`Lightning.bhit`).
    pub snapped_to_insulation: bool,
    /// Tile-corner point the last polyline segment snapped to
    /// (`Lightning.java:62-65`); `None` without an insulated hit.
    pub snap_point: Option<(f32, f32)>,
}

/// Creates a lightning branch (`Lightning.create(team, ...)`).
///
/// `team` should be the caster's team; pass a team that matches nothing to hit
/// everyone (upstream `Team.derelict`).
#[allow(clippy::too_many_arguments)]
pub fn create(
    ctx: &mut CombatCtx<'_>,
    team: u8,
    color: Rgba,
    damage: f32,
    x0: f32,
    y0: f32,
    rotation0: f32,
    length: i32,
) -> LightningResult {
    create_internal(
        ctx,
        None,
        DAMAGE_LIGHTNING,
        team,
        color,
        damage,
        x0,
        y0,
        rotation0,
        length,
    )
}

/// Creates a lightning branch from a `hitter` bullet
/// (`Lightning.create(Bullet bullet, ...)`).
///
/// The branch team, air/ground gate, `pierceCap`, damage multiplier and
/// `lightningType` all come from `bullet` (`Lightning.java:34-36`).
#[allow(clippy::too_many_arguments)]
pub fn create_from_bullet(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    color: Rgba,
    damage: f32,
    x0: f32,
    y0: f32,
    rotation0: f32,
    length: i32,
) -> LightningResult {
    let Some(state) = ctx.bullet(bullet) else {
        return LightningResult::default();
    };
    let def_id = state.def;
    let owner = state.owner;
    let team = ctx.team(bullet);
    let Some(def) = ctx.content.bullet(def_id) else {
        return LightningResult::default();
    };
    let hitter = LightningHitter {
        collides_air: def.collides_air,
        collides_ground: def.collides_ground,
        pierce_cap: def.pierce_cap,
        damage_multiplier: owner_damage_multiplier(ctx.world, team, owner),
        lightning_type: def.lightning_type,
    };
    let hit_create = def.lightning_type.unwrap_or(DAMAGE_LIGHTNING);
    create_internal(
        ctx,
        Some(hitter),
        hit_create,
        team,
        color,
        damage,
        x0,
        y0,
        rotation0,
        length,
    )
}

/// `BulletType.damageMultiplier(Bullet)` (`BulletType.java:547-551`): a unit
/// owner scales by its status damage multiplier and `rules.unitDamage`, a
/// building owner by `rules.blockDamage`; no owner leaves `1`.
fn owner_damage_multiplier(world: &World, team: u8, owner: Option<Entity>) -> f32 {
    let Some(owner) = owner else {
        return 1.0;
    };
    let mut damage = world
        .get::<StatusComp>(owner)
        .map(|status| status.damage_multiplier)
        .unwrap_or(1.0);
    if let Some(rules) = crate::game::runtime::rules_of(world) {
        if world.get::<UnitTypeComp>(owner).is_some() {
            damage *= rules.unit_damage(team);
        } else if world.get::<Building>(owner).is_some() {
            damage *= rules.block_damage(team);
        }
    }
    damage
}

/// `createLightningInternal` (`Lightning.java:42-116`).
#[allow(clippy::too_many_arguments)]
fn create_internal(
    ctx: &mut CombatCtx<'_>,
    hitter: Option<LightningHitter>,
    hit_create: BulletId,
    team: u8,
    color: Rgba,
    damage: f32,
    x0: f32,
    y0: f32,
    rotation0: f32,
    length: i32,
) -> LightningResult {
    let seed = next_seed(ctx);
    let mut rand = Rand::new(seed as i64 as u64);
    let mut x = x0;
    let mut y = y0;
    let mut rotation = rotation0;
    let mut hit: Vec<Entity> = Vec::new();
    let mut lines: SmallVec<[(f32, f32); 8]> = SmallVec::new();
    let mut snapped_to_insulation = false;
    let mut snap_point: Option<(f32, f32)> = None;

    // `Units.nearbyEnemies` walks the present team trees; the port keeps the
    // stable unit snapshot in entity index order (deterministic ordering rule).
    // Unit positions do not move during one branch, so one snapshot is enough;
    // the per-step `hitRange` box is applied against it below.
    let units = unit_snapshot(ctx.world);
    let branch_damage = damage * hitter.map_or(1.0, |h| h.damage_multiplier);

    let steps = (length / 2).max(0);
    for _ in 0..steps {
        let spawn = BulletSpawn {
            def: hit_create,
            x,
            y,
            angle: rotation,
            team,
            damage: branch_damage,
            ..BulletSpawn::default()
        };
        let _ = ctx.spawn(&spawn);
        lines.push((x + rand.range_symmetric(3.0), y + rand.range_symmetric(3.0)));

        // `Lightning.java:55-70`: raycast the previous -> current polyline
        // segment; an enemy insulated building snaps the segment end to that
        // tile's world corner and terminates the branch.
        if lines.len() > 1 {
            let from = lines[lines.len() - 2];
            let to = lines[lines.len() - 1];
            let mut snap: Option<(f32, f32)> = None;
            {
                let world: &World = ctx.world;
                let grid = ctx.grid;
                raycast::raycast_each_world(from.0, from.1, to.0, to.1, |wx, wy| {
                    let Some((_, other_team)) = insulated_building_at(world, grid, wx, wy) else {
                        return false;
                    };
                    if other_team == team {
                        return false;
                    }
                    snap = Some((wx as f32 * TILE_SIZE, wy as f32 * TILE_SIZE));
                    true
                });
            }
            if let Some(point) = snap {
                if let Some(last) = lines.last_mut() {
                    *last = point;
                }
                snapped_to_insulation = true;
                snap_point = Some(point);
                break;
            }
        }

        // `Lightning.java:72-81`: collect nearby enemy units unless the chain
        // cap is already reached (then `entities` stays empty and the branch
        // walks randomly).
        let mut candidates: Vec<Entity> = Vec::new();
        if hit.len() < MAX_CHAIN {
            for &unit in &units {
                // `Units.nearbyEnemies`: only other teams.
                if ctx.world.get::<TeamComp>(unit).map(|t| t.team) == Some(team) {
                    continue;
                }
                if hit.contains(&unit) {
                    continue;
                }
                if let Some(h) = hitter
                    && !unit_passes_hitter(ctx.world, unit, h)
                {
                    continue;
                }
                if !in_hit_box(ctx.world, unit, x, y) {
                    continue;
                }
                candidates.push(unit);
            }
        }

        // `Lightning.java:83-85`: `pierceCap` ends the branch *after* the
        // current step's bullet/polyline point were emitted.
        if let Some(h) = hitter
            && h.pierce_cap > 0
            && hit.len() >= h.pierce_cap as usize
        {
            break;
        }

        // `Geometry.findFurthest` over the candidate list (strictly-farther
        // replacement keeps the first entity on ties).
        let mut best: Option<(f32, Entity)> = None;
        for &unit in &candidates {
            let Some(pos) = ctx.world.get::<Pos>(unit) else {
                continue;
            };
            let d2 = (pos.x - x) * (pos.x - x) + (pos.y - y) * (pos.y - y);
            if best.is_none_or(|(bd, _)| d2 > bd) {
                best = Some((d2, unit));
            }
        }

        // `Lightning.java:87-95`: chain when the farthest unit is within
        // `hitRange * 2`; otherwise random-walk `hitRange / 2` along a jittered
        // rotation.
        if let Some((_, target)) = best
            && let Some(pos) = ctx.world.get::<Pos>(target)
            && (pos.x - x) * (pos.x - x) + (pos.y - y) * (pos.y - y) <= (HIT_RANGE * 2.0).powi(2)
        {
            hit.push(target);
            x = pos.x;
            y = pos.y;
        } else {
            rotation += rand.range_symmetric(20.0);
            let rad = rotation.to_radians();
            x += rad.cos() * HIT_RANGE / 2.0;
            y += rad.sin() * HIT_RANGE / 2.0;
        }
    }

    // `Fx.lightning.at(x, y, rotation, color, lines)` (`Lightning.java:113`):
    // the view body draws the polyline in the requested tint.
    ctx.fx.effect_data(
        &EffectRef::Named(EffectId::LIGHTNING),
        x,
        y,
        rotation,
        color,
        EffectData::Positions(lines),
    );

    LightningResult {
        end: (x, y),
        links: hit.len(),
        hit,
        color,
        snapped_to_insulation,
        snap_point,
    }
}

/// Stable unit snapshot, entity-index order (`Groups.unit` stand-in).
///
/// Detached block-unit proxies (possessed turrets) are never in Java's
/// `Groups.unit`, so they are excluded — same filter as
/// [`crate::combat::targeting::TargetQueries`].
fn unit_snapshot(world: &mut World) -> Vec<Entity> {
    let mut units: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, (With<Unit>, Without<BlockUnitComp>)>();
        query.iter(world).collect()
    };
    units.sort_unstable_by_key(|entity| entity.index());
    units.dedup();
    units
}

/// `UnitComp.checkTarget(air, ground)` (`UnitComp.java:76-78`).
fn unit_passes_hitter(world: &World, unit: Entity, hitter: LightningHitter) -> bool {
    let flying = super::bullet::unit_is_flying(world, unit);
    let grounded = super::bullet::unit_is_grounded(world, unit);
    (grounded && hitter.collides_ground) || (flying && hitter.collides_air)
}

/// `Units.nearbyEnemies(team, rect, cons)` rectangle test: the unit's
/// `hitSize` square intersects the `hitRange` square centred on `(x, y)`.
///
/// Dead units are skipped: Java's `Groups.unit` tree drops them on
/// `Unit.remove()`.
fn in_hit_box(world: &World, unit: Entity, x: f32, y: f32) -> bool {
    if !world.get::<Health>(unit).is_some_and(|h| !h.dead) {
        return false;
    }
    let half = world
        .get::<HitboxComp>(unit)
        .map_or(0.0, |h| h.hit_size / 2.0);
    let Some(pos) = world.get::<Pos>(unit) else {
        return false;
    };
    let reach = HIT_RANGE / 2.0 + half;
    (pos.x - x).abs() <= reach && (pos.y - y).abs() <= reach
}

/// The enemy building on a tile, when its block is `insulated`
/// (`BuildingComp.isInsulated`, `Lightning.java:58-68`).
fn insulated_building_at(
    world: &World,
    grid: &WorldGrid,
    tx: i32,
    ty: i32,
) -> Option<(Entity, u8)> {
    if !grid.tiles.in_bounds(tx, ty) {
        return None;
    }
    let build = grid.tiles.get(tx, ty).build?;
    let team = world.get::<TeamComp>(build).map(|t| t.team)?;
    let block = world.get::<Building>(build).map(|b| b.block)?;
    let insulated = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
        .is_some_and(|instance| instance.def.insulated);
    insulated.then_some((build, team))
}

/// Increments and returns the monotonic lightning seed.
fn next_seed(ctx: &mut CombatCtx<'_>) -> i32 {
    if ctx.world.get_resource::<LightningCounter>().is_none() {
        ctx.world.insert_resource(LightningCounter(0));
    }
    let Some(mut counter) = ctx.world.get_resource_mut::<LightningCounter>() else {
        return 0;
    };
    let seed = counter.0;
    counter.0 = counter.0.wrapping_add(1);
    seed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;
    use crate::entities::comp::unit::comp::PhysicsComp;

    fn ground_unit(h: &mut CombatHarness, x: f32, y: f32, team: u8) -> Entity {
        h.spawn_test_unit(x, y, team, Vec::new())
    }

    fn flying_unit(h: &mut CombatHarness, x: f32, y: f32, team: u8) -> Entity {
        let entity = h.spawn_test_unit(x, y, team, Vec::new());
        h.build.world.entity_mut(entity).insert(PhysicsComp {
            flying: true,
            ..PhysicsComp::default()
        });
        entity
    }

    #[test]
    fn chain_is_deterministic_for_same_seed() {
        let mut a = CombatHarness::new(48, 48, 5);
        let mut b = CombatHarness::new(48, 48, 5);
        let (ax, ay) = CombatHarness::tile_center(4, 4);
        ground_unit(&mut a, ax + 8.0, ay, 2);
        ground_unit(&mut b, ax + 8.0, ay, 2);
        let ra = a.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 10);
        let rb = b.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 10);
        assert_eq!(ra, rb);
        assert_eq!(ra.links, 1, "chained to the enemy unit");
    }

    /// `Lightning.java:72-90`: `Units.nearbyEnemies` + `Geometry.findFurthest`
    /// chain to **units**; a building in range is not a chain target.
    #[test]
    fn chains_to_units_not_buildings() {
        let mut h = CombatHarness::new(48, 48, 5);
        let wall = h.content().block_id("copper-wall").expect("wall");
        assert!(h.place(5, 4, wall, 1, true), "enemy wall beside the start");
        let (ax, ay) = CombatHarness::tile_center(4, 4);
        let unit = ground_unit(&mut h, ax + 10.0, ay, 2);
        let result = h.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 4);
        assert_eq!(result.hit, vec![unit], "the unit is the only chain target");
        assert_eq!(result.links, 1);
        assert!(!result.snapped_to_insulation);
    }

    /// `Lightning.java:55-70`: the polyline raycast hard-stops on an enemy
    /// insulated building and snaps the segment end to that tile's corner.
    #[test]
    fn insulated_wall_stops_the_chain() {
        let mut h = CombatHarness::new(48, 48, 5);
        let plastanium = h.content().block_id("plastanium-wall").expect("wall");
        // A ring of enemy insulated walls around the branch origin: every
        // `hitRange/2` walk leaves the origin tile and crosses one of them.
        for (x, y) in [
            (3, 3),
            (4, 3),
            (5, 3),
            (3, 4),
            (5, 4),
            (3, 5),
            (4, 5),
            (5, 5),
        ] {
            assert!(h.place(x, y, plastanium, 1, true), "wall at ({x},{y})");
        }
        // A tile corner rounds to tile (4, 4) (`World.toTile`).
        let (ax, ay) = CombatHarness::tile_origin(4, 4);
        let result = h.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 20);
        assert!(
            result.snapped_to_insulation,
            "the branch ends on the insulated wall"
        );
        assert_eq!(result.links, 0, "no chain target past the wall");
        let ts = TILE_SIZE;
        let snap = result.snap_point.expect("tile-corner snap point");
        assert_eq!(
            snap.0 % ts,
            0.0,
            "snapped to a tile corner (Lightning.java:64): {snap:?}"
        );
        assert_eq!(snap.1 % ts, 0.0);
        assert!(
            (3.0 * ts..=5.0 * ts).contains(&snap.0) && (3.0 * ts..=5.0 * ts).contains(&snap.1),
            "snapped into the wall ring: {snap:?}"
        );
        // `Fx.lightning.at(x, y, ...)` keeps the last chain position, not the
        // snap point (`Lightning.java:113`).
        assert_ne!(result.end, snap, "the FX position is the last chain point");
        assert!(result.end.0 > 0.0 && result.end.1 > 0.0);
    }

    /// A friendly insulated building never stops the branch
    /// (`tile.team() != team`).
    #[test]
    fn friendly_insulated_wall_never_stops_the_chain() {
        let mut h = CombatHarness::new(48, 48, 5);
        // `BuildHarness::place` uses `rules.default_team`; make the ring the
        // caster's own team so the `tile.team() != team` gate rejects it.
        h.build.rules.default_team = 1;
        let plastanium = h.content().block_id("plastanium-wall").expect("wall");
        for (x, y) in [
            (3, 3),
            (4, 3),
            (5, 3),
            (3, 4),
            (5, 4),
            (3, 5),
            (4, 5),
            (5, 5),
        ] {
            assert!(h.place(x, y, plastanium, 1, true), "wall at ({x},{y})");
        }
        let (ax, ay) = CombatHarness::tile_origin(4, 4);
        let result = h.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 20);
        assert!(!result.snapped_to_insulation, "own team's wall is ignored");
    }

    /// `Lightning.java:73-77`: the `hitter` gate accepts only units matching
    /// `collidesAir`/`collidesGround` through `UnitType.checkTarget`.
    #[test]
    fn hitter_air_ground_filter() {
        let mut h = CombatHarness::new(48, 48, 5);
        let (ax, ay) = CombatHarness::tile_center(4, 4);
        let air = flying_unit(&mut h, ax + 8.0, ay, 2);
        let ground = ground_unit(&mut h, ax - 8.0, ay, 2);

        let ground_only = LightningHitter {
            collides_air: false,
            collides_ground: true,
            pierce_cap: -1,
            damage_multiplier: 1.0,
            lightning_type: None,
        };
        let result = {
            let mut ctx = h.combat_ctx();
            create_internal(
                &mut ctx,
                Some(ground_only),
                DAMAGE_LIGHTNING,
                1,
                Rgba::WHITE,
                10.0,
                ax,
                ay,
                0.0,
                4,
            )
        };
        h.claim_scratch_spawned();
        assert_eq!(result.hit, vec![ground], "air unit filtered out");

        let air_only = LightningHitter {
            collides_air: true,
            collides_ground: false,
            ..ground_only
        };
        let result = {
            let mut ctx = h.combat_ctx();
            create_internal(
                &mut ctx,
                Some(air_only),
                DAMAGE_LIGHTNING,
                1,
                Rgba::WHITE,
                10.0,
                ax,
                ay,
                0.0,
                4,
            )
        };
        h.claim_scratch_spawned();
        assert_eq!(result.hit, vec![air], "ground unit filtered out");
        let _ = ground;
    }

    /// `Lightning.java:83-85`: `pierceCap` terminates the branch once the hit
    /// count reaches it.
    #[test]
    fn pierce_cap_limits_links() {
        let mut h = CombatHarness::new(64, 64, 5);
        let (ax, ay) = CombatHarness::tile_center(4, 8);
        for i in 0..6 {
            ground_unit(&mut h, ax + 8.0 + i as f32 * 10.0, ay, 2);
        }
        let hitter = LightningHitter {
            collides_air: true,
            collides_ground: true,
            pierce_cap: 2,
            damage_multiplier: 1.0,
            lightning_type: None,
        };
        let result = {
            let mut ctx = h.combat_ctx();
            create_internal(
                &mut ctx,
                Some(hitter),
                DAMAGE_LIGHTNING,
                1,
                Rgba::WHITE,
                10.0,
                ax,
                ay,
                0.0,
                40,
            )
        };
        h.claim_scratch_spawned();
        assert_eq!(result.links, 2, "pierceCap 2 caps the chain");
    }

    /// `Lightning.java:87-95` + `:72-81`: once `maxChain` links are reached the
    /// candidate list is empty, so the rest of the branch random-walks.
    #[test]
    fn random_walk_after_chain_cap() {
        let mut h = CombatHarness::new(96, 96, 5);
        let (ax, ay) = CombatHarness::tile_center(8, 8);
        for i in 0..12 {
            ground_unit(&mut h, ax + 6.0 + i as f32 * 8.0, ay, 2);
        }
        let result = h.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 60);
        assert_eq!(result.links, MAX_CHAIN, "stops chaining at maxChain");
        // The 30-step branch ends past the last chained unit (random walk).
        assert!(
            result.end.0 > ax + 8.0 * MAX_CHAIN as f32,
            "random walk continues after the cap: end {:?}",
            result.end
        );
    }

    /// `Lightning.java:50`: every step bullet is scaled by
    /// `hitter.damageMultiplier()`.
    #[test]
    fn hitter_damage_multiplier_scales_chain_bullets() {
        use crate::combat::bullet::Bullet;
        use crate::combat::bullet::BulletSpawn;

        let mut h = CombatHarness::new(48, 48, 5);
        let (ax, ay) = CombatHarness::tile_center(4, 4);
        let shooter = ground_unit(&mut h, ax, ay, 1);
        h.build.world.entity_mut(shooter).insert(StatusComp {
            damage_multiplier: 2.0,
            ..StatusComp::default()
        });
        let def = h.bullet_id("fuse").expect("fuse");
        let bullet = {
            let mut ctx = h.combat_ctx();
            ctx.spawn(&BulletSpawn {
                def,
                x: ax,
                y: ay,
                team: 1,
                owner: Some(shooter),
                damage: 10.0,
                ..BulletSpawn::default()
            })
        }
        .expect("hitter bullet");
        h.claim_scratch_spawned();
        let result = {
            let mut ctx = h.combat_ctx();
            create_from_bullet(&mut ctx, bullet, Rgba::WHITE, 10.0, ax, ay, 0.0, 6)
        };
        h.claim_scratch_spawned();
        assert!(result.links == 0);
        let chain_bullets: Vec<f32> = h
            .bullets
            .iter()
            .filter_map(|&entity| h.build.world.get::<Bullet>(entity))
            .filter(|state| state.def == DAMAGE_LIGHTNING)
            .map(|state| state.damage)
            .collect();
        assert_eq!(chain_bullets.len(), 3, "length 6 -> 3 lightning bullets");
        for damage in chain_bullets {
            assert_eq!(damage, 20.0, "status damageMultiplier 2 doubles it");
        }
    }

    /// `Lightning.java:72-77`: a `hitter` with no air/ground targets still
    /// chains to nothing (the gate is exact, not a fallback).
    #[test]
    fn hitter_gate_with_no_matching_target_chains_to_nothing() {
        let mut h = CombatHarness::new(48, 48, 5);
        let (ax, ay) = CombatHarness::tile_center(4, 4);
        ground_unit(&mut h, ax + 8.0, ay, 2);
        let none = LightningHitter {
            collides_air: false,
            collides_ground: false,
            pierce_cap: -1,
            damage_multiplier: 1.0,
            lightning_type: None,
        };
        let result = {
            let mut ctx = h.combat_ctx();
            create_internal(
                &mut ctx,
                Some(none),
                DAMAGE_LIGHTNING,
                1,
                Rgba::WHITE,
                10.0,
                ax,
                ay,
                0.0,
                4,
            )
        };
        h.claim_scratch_spawned();
        assert_eq!(result.links, 0);
    }

    /// `Lightning.java:113`: the branch ends with
    /// `Fx.lightning.at(x, y, rotation, color, lines)` — the caller's colour
    /// (`Pal.lancerLaser` for a shock mine) and the polyline reach the view
    /// sink unchanged.
    #[test]
    fn lightning_emits_fx_with_the_requested_color() {
        use std::sync::Arc;

        use crate::content::registries::pal;
        use crate::fx::{FxBus, FxEvent};

        let mut harness = CombatHarness::new(48, 48, 5);
        let bus = Arc::new(FxBus::new());
        harness.set_fx(bus.clone());
        let (x, y) = CombatHarness::tile_center(4, 4);
        // A non-default tint must survive the round trip.
        let color = pal::SURGE;
        let result = harness.lightning(1, color, 10.0, x, y, 0.0, 10);
        assert_eq!(result.color, color, "result carries the requested tint");
        let events = bus.drain();
        assert_eq!(events.len(), 1, "one Fx.lightning per branch");
        match &events[0] {
            FxEvent::EffectWithData {
                effect: EffectRef::Named(EffectId::LIGHTNING),
                color: emitted,
                data: EffectData::Positions(points),
                ..
            } => {
                assert_eq!(*emitted, color, "FX event keeps the tint");
                assert_eq!(points.len(), 5, "length 10 -> length/2 points");
            }
            other => panic!("expected Fx.lightning with a polyline: {other:?}"),
        }
    }
}
