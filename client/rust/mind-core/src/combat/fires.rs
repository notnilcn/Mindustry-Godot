// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Fires` tile entities (`core/src/mindustry/entities/Fires.java` +
//! `entities/comp/FireComp.java`).
//!
//! A [`FireState`] component is the sim half of `FireComp`; the tile slot
//! (`Tiles::fires`, plan 06) is a cache synced by [`update_fires`] each tick so
//! bullets that hold a read-only grid borrow can still create fires. View data
//! (animation frames, light) and sound are plan 17.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::determinism::{RngStream, SimRng};
use crate::entities::comp::{Health, Pos, TeamComp};

use super::view::FxSink;

/// `Geometry.d4` neighbor offsets.
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// `Fires.baseLifetime`.
pub const BASE_LIFETIME: f32 = 1000.0;
/// `FireComp.frames`.
pub const FRAMES: f32 = 40.0;
/// `FireComp.duration`.
pub const DURATION: f32 = 90.0;
/// `FireComp.ticksPerFrame = duration / frames` (2.25).
pub const TICKS_PER_FRAME: f32 = DURATION / FRAMES;
/// `FireComp.warmupDuration`.
pub const WARMUP_DURATION: f32 = 20.0;
/// `FireComp.spreadDelay`.
pub const SPREAD_DELAY: f32 = 22.0;
/// `FireComp.fireballDelay`.
pub const FIREBALL_DELAY: f32 = 40.0;
/// `FireComp.damageDelay`.
pub const DAMAGE_DELAY: f32 = 40.0;
/// `FireComp.tileDamage`.
pub const TILE_DAMAGE: f32 = 1.8;
/// `FireComp.unitDamage`.
pub const UNIT_DAMAGE: f32 = 3.0;

/// Advances a fire's view animation by one tick (`animation += delta /
/// ticksPerFrame; animation %= frames`, `FireComp.java:41-43`). View-only.
pub fn advance_animation(animation: f32) -> f32 {
    (animation + 1.0 / TICKS_PER_FRAME) % FRAMES
}

/// Marker resource set by [`create`] when a new fire is born (G05 T4b
/// `Trigger.fireCreate` hint event); drained by `Sim::tick`.
#[derive(Debug, Clone, Copy, Default, bevy_ecs::resource::Resource)]
pub struct FireCreated;

/// Fire state (`FireComp` sim fields; draw/animation are plan 16/17).
#[derive(Debug, Clone, Copy, Component)]
pub struct FireState {
    /// Owning tile.
    pub tile: (i16, i16),
    /// Ticks lived.
    pub time: f32,
    /// Ticks to live.
    pub lifetime: f32,
    /// Puddle flammability contribution this tick.
    pub puddle_flammability: f32,
    /// Spread accumulator (`FireComp.spreadTimer`).
    pub spread_timer: f32,
    /// Fireball accumulator (`FireComp.fireballTimer`).
    pub fireball_timer: f32,
    /// Damage accumulator (`FireComp.damageTimer`).
    pub damage_timer: f32,
    /// Warmup (view).
    pub warmup: f32,
    /// Animation frame (view).
    pub animation: f32,
}

impl FireState {
    /// Normalized life fraction (`Timedc.fin`).
    pub fn fin(&self) -> f32 {
        if self.lifetime <= 0.0 {
            1.0
        } else {
            (self.time / self.lifetime).clamp(0.0, 1.0)
        }
    }
}

/// Returns whether `(tx, ty)` has a live fire (`Fires.has`).
pub fn has(world: &World, tx: i16, ty: i16) -> bool {
    find_at(world, tx, ty).is_some()
}

/// Finds the fire entity at a tile (`Fires.get`).
pub fn find_at(world: &World, tx: i16, ty: i16) -> Option<Entity> {
    world
        .iter_entities()
        .find(|entity_ref| {
            entity_ref
                .get::<FireState>()
                .is_some_and(|fire| fire.tile == (tx, ty))
        })
        .map(|entity_ref| entity_ref.id())
}

/// Creates/refreshes a fire (`Fires.create`).
///
/// The `Env.oxygen`/`rules.fire` gate is read from the optional [`CombatEnv`]
/// resource (default: enabled), matching upstream until plan 12 owns `Rules`.
pub fn create(world: &mut World, tx: i16, ty: i16, rng: &mut SimRng) -> Option<Entity> {
    if !env_allows_fire(world) {
        return None;
    }
    if let Some(existing) = find_at(world, tx, ty) {
        if let Some(mut fire) = world.get_mut::<FireState>(existing) {
            fire.lifetime = BASE_LIFETIME;
            fire.time = 0.0;
        }
        return Some(existing);
    }
    let ts = crate::config::TILESIZE as f32;
    let entity = world
        .spawn((
            Pos {
                // `fire.set(tile.worldx(), tile.worldy())` (Fires.java:26).
                x: tx as f32 * ts,
                y: ty as f32 * ts,
            },
            TeamComp { team: 0 },
            FireState {
                tile: (tx, ty),
                time: 0.0,
                lifetime: BASE_LIFETIME,
                puddle_flammability: 0.0,
                spread_timer: rng.range(RngStream::Sim, 0.0, SPREAD_DELAY),
                fireball_timer: rng.range(RngStream::Sim, 0.0, 40.0),
                damage_timer: rng.range(RngStream::Sim, 0.0, DAMAGE_DELAY),
                warmup: 0.0,
                animation: rng.range(RngStream::Sim, 0.0, 39.0),
            },
        ))
        .id();
    // G05 T4b: `Trigger.fireCreate` marker for the hint driver.
    world.insert_resource(FireCreated);
    Some(entity)
}

/// Extinguishes a fire by shortening its life (`Fires.extinguish`).
pub fn extinguish(world: &mut World, tx: i16, ty: i16, intensity: f32) -> bool {
    let Some(entity) = find_at(world, tx, ty) else {
        return false;
    };
    if let Some(mut fire) = world.get_mut::<FireState>(entity) {
        fire.time += intensity;
        return fire.time < fire.lifetime;
    }
    false
}

/// Removes a fire (`FireComp.remove`/`Fires.remove`).
pub fn remove(world: &mut World, entity: Entity) {
    let _ = world.despawn(entity);
}

/// `Tile.getFlammability()` (`Tile.java:142-160`).
///
/// Air: the floor's `liquidDrop` flammability. A building: the item term
/// (`sum(item.flammability * amount) / max(itemCapacity, 1)` times
/// `clamp(itemCapacity / 2.4, 1, 3)`) plus the liquid term
/// (`sum(liquid.flammability * amount / 1.6) / max(liquidCapacity, 1)` times
/// `clamp(liquidCapacity / 30, 1, 2)`). Other blocks return 0; the puddle
/// contribution is added by the caller from the stored
/// `FireComp.puddleFlammability`.
pub fn tile_flammability(
    world: &World,
    tiles: &crate::world::tiles::Tiles,
    content: &crate::content::ContentRegistry,
    tx: i32,
    ty: i32,
) -> f32 {
    if !tiles.in_bounds(tx, ty) {
        return 0.0;
    }
    let tile = tiles.get(tx, ty);
    if tile.block == crate::content::BlockId::AIR {
        return tile.get_flammability(content);
    }
    let Some(build) = tile.build else {
        return 0.0;
    };
    let Some(def) = content.block(tile.block) else {
        return 0.0;
    };
    let mut result = 0.0f32;
    if def.has_items
        && let Some(items) = world.get::<crate::world::modules::ItemModule>(build)
        && items.total() > 0
    {
        // `build.items.sum((item, amount) -> item.flammability * amount)`.
        let sum: f32 = content
            .items()
            .iter()
            .map(|item| item.flammability * items.get(item.id) as f32)
            .sum();
        result += sum / (def.item_capacity.max(1) as f32)
            * (def.item_capacity as f32 / 2.4).clamp(1.0, 3.0);
    }
    if def.has_liquids
        && let Some(liquids) = world.get::<crate::world::modules::LiquidModule>(build)
    {
        // `build.liquids.sum((liquid, amount) -> liquid.flammability * amount / 1.6f)`.
        let sum: f32 = content
            .liquids()
            .iter()
            .map(|liquid| liquid.flammability * liquids.get(liquid.id) / 1.6)
            .sum();
        result += sum / def.liquid_capacity.max(1.0) * (def.liquid_capacity / 30.0).clamp(1.0, 2.0);
    }
    result
}

/// Stored puddle flammability contribution (`Puddle.getFlammability() / 3f`),
/// refreshed only on the damage tick (`FireComp.java:90-91`).
fn puddle_flammability_at(
    world: &World,
    content: &crate::content::ContentRegistry,
    tx: i16,
    ty: i16,
) -> f32 {
    super::puddles::find_at(world, tx, ty)
        .and_then(|p| world.get::<super::puddles::PuddleState>(p).copied())
        .map(|p| {
            content
                .liquid(p.liquid)
                .map(|def| def.flammability * p.amount / 3.0)
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0)
}

/// `Damage.damageUnits(null, x, y, tilesize, unitDamage, pred, acceptor)`
/// (`Damage.java:436-454`): every unit whose square hitbox overlaps the
/// `2 * tilesize` square centred on `(cx, cy)` that is not flying and not
/// immune to `burning` takes [`UNIT_DAMAGE`] and gets `burning` for `60 * 5`
/// ticks. Iterates in `EntitySeq` order.
fn damage_units_square(
    world: &mut World,
    content: &crate::content::ContentRegistry,
    cx: f32,
    cy: f32,
) {
    let Some(burning) = content.status_id("burning") else {
        return;
    };
    let half = crate::config::TILESIZE as f32;
    let mut units: Vec<(u64, Entity)> = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let Some(hitbox) = entity_ref.get::<crate::entities::comp::unit::comp::HitboxComp>() else {
            continue;
        };
        if entity_ref.get::<Health>().is_none() {
            continue;
        }
        // `unit -> !unit.isFlying() && !unit.isImmune(StatusEffects.burning)`.
        let entity = entity_ref.id();
        if crate::combat::bullet::unit_is_flying(world, entity) {
            continue;
        }
        let immune = entity_ref
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
            .and_then(|comp| content.unit(comp.type_id))
            .is_none_or(|def| def.immunities.contains(&burning));
        if immune {
            continue;
        }
        let hittable = entity_ref
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
            .and_then(|comp| content.unit(comp.type_id))
            .is_none_or(|def| def.hittable);
        if !hittable {
            continue;
        }
        // `entity.hitbox(hitrect); hitrect.overlaps(rect)` with the outer rect a
        // `size * 2` square (`Arc Rect.overlaps` uses strict comparisons).
        let reach = half + hitbox.hit_size / 2.0;
        if (pos.x - cx).abs() >= reach || (pos.y - cy).abs() >= reach {
            continue;
        }
        let seq = entity_ref
            .get::<crate::ecs::EntitySeq>()
            .map(|seq| seq.0)
            .unwrap_or(u64::MAX);
        units.push((seq, entity));
    }
    units.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    for (_, unit) in units {
        crate::combat::damage::area::damage_entity(world, content, unit, UNIT_DAMAGE, false, 1.0);
        crate::combat::damage::status::apply_status(world, unit, burning, 60.0 * 5.0);
    }
}

/// Updates every fire one tick (`FireComp.update`, `FireComp.java:39-100`).
///
/// `water_attr` is `state.envAttrs.get(Attribute.water)` (plan 12); `tiles` is
/// mutated to keep the `Tiles::fires` cache in sync with live entities.
/// Returns the fireball origins queued this tick
/// (`Bullets.fireball.createNet`); the caller spawns them with a
/// [`crate::combat::bullet::CombatCtx`].
pub fn update_fires(
    world: &mut World,
    tiles: &mut crate::world::tiles::Tiles,
    content: &crate::content::ContentRegistry,
    rng: &mut SimRng,
    fx: &dyn FxSink,
    water_attr: f32,
) -> Vec<(f32, f32)> {
    // Reset the slot cache; re-register live fires below.
    tiles.clear_fire_slots();
    let mut entities: Vec<Entity> = world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<FireState>().is_some())
        .map(|entity_ref| entity_ref.id())
        .collect();
    entities.sort_by_key(|entity| entity.index());
    let mut fireballs: Vec<(f32, f32)> = Vec::new();
    for entity in entities {
        let Some(mut fire) = world.get::<FireState>(entity).copied() else {
            continue;
        };
        // `animation += delta / ticksPerFrame; warmup += delta; animation %= frames`
        // (`FireComp.java:41-43`; H03 T7 — the earlier rate advanced one full
        // frame per tick, cycling the flame 2.25x too fast).
        fire.animation = advance_animation(fire.animation);
        fire.warmup += 1.0;
        let speed_multiplier = 1.0 + (water_attr * 10.0).max(0.0);
        fire.time = (fire.time + speed_multiplier).clamp(0.0, fire.lifetime);

        let (tx, ty) = fire.tile;
        let in_bounds = tiles.in_bounds(tx as i32, ty as i32);
        if fire.time >= fire.lifetime || !in_bounds || !fire.lifetime.is_finite() {
            // `remove()` hook: fire-remove FX.
            if let Some(pos) = world.get::<Pos>(entity) {
                fx.effect(
                    &crate::content::registries::fx_meta::EffectRef::Named(
                        crate::content::EffectId::FIRE_REMOVE,
                    ),
                    pos.x,
                    pos.y,
                    0.0,
                    crate::content::Rgba::WHITE,
                );
            }
            remove(world, entity);
            continue;
        }
        let build = tiles.get(tx as i32, ty as i32).build;
        let world_x = tx as f32 * crate::config::TILESIZE as f32;
        let world_y = ty as f32 * crate::config::TILESIZE as f32;

        // `float flammability = tile.getFlammability() + puddleFlammability`.
        let flammability = tile_flammability(world, tiles, content, tx as i32, ty as i32)
            + fire.puddle_flammability;

        // Not on a building: bare ground with nothing to burn dies 8x faster.
        if build.is_none() && flammability <= 0.0 {
            fire.time += 8.0;
        }
        // A burning building feeds the fire.
        if build.is_some() {
            fire.lifetime += (flammability / 8.0).clamp(0.0, 0.6);
        }

        // Flammable tiles spread to `d4` neighbors.
        if flammability > 1.0 {
            fire.spread_timer += (flammability / 5.0).clamp(0.3, 2.0);
            if fire.spread_timer >= SPREAD_DELAY {
                fire.spread_timer = 0.0;
                let pick = rng.random(RngStream::Sim, 4) as usize;
                let (dx, dy) = D4[pick % 4];
                let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
                if tiles.in_bounds(nx, ny) {
                    let _ = create(world, nx as i16, ny as i16, rng);
                }
            }
        }

        // Throw fireballs while anything is burning.
        if flammability > 0.0 {
            fire.fireball_timer += (flammability / 10.0).clamp(0.0, 0.5);
            if fire.fireball_timer >= FIREBALL_DELAY {
                fire.fireball_timer = 0.0;
                fireballs.push((world_x, world_y));
            }
        }

        // Damage the building and nearby ground units every `damageDelay`.
        fire.damage_timer += 1.0;
        if fire.damage_timer >= DAMAGE_DELAY {
            fire.damage_timer = 0.0;
            fire.puddle_flammability = puddle_flammability_at(world, content, tx, ty);
            if let Some(build) = build {
                crate::combat::damage::area::apply_health(world, build, TILE_DAMAGE);
            }
            damage_units_square(world, content, world_x, world_y);
        }

        if let Some(mut slot) = world.get_mut::<FireState>(entity) {
            *slot = fire;
        }
        // Keep the tile-slot cache in sync.
        tiles.set_fire(tiles.index(tx as i32, ty as i32), Some(entity));
    }
    fireballs
}

/// `Fire` save revision (`plan 10 §6.3`).
pub const FIRE_REVISION: u8 = 1;

/// Writes the `Fire` save payload (`FireComp.write`; `tile`/`time`/`lifetime`).
pub fn write_fire(
    state: &FireState,
    w: &mut crate::world::BuildingWriter,
) -> Result<(), crate::io::IoError> {
    w.s(state.tile.0);
    w.s(state.tile.1);
    w.f(state.time);
    w.f(state.lifetime);
    Ok(())
}

/// Reads the `Fire` save payload (revision-tolerant; all fields present since v1).
pub fn read_fire(
    state: &mut FireState,
    r: &mut crate::world::BuildingReader,
    revision: u8,
) -> Result<(), crate::io::IoError> {
    if revision >= 1 {
        state.tile = (r.s()?, r.s()?);
        state.time = r.f()?;
        state.lifetime = r.f()?;
    }
    Ok(())
}

/// Whether `rules.fire` and `Env.oxygen` allow fire (default enabled).
fn env_allows_fire(world: &World) -> bool {
    world
        .get_resource::<CombatEnv>()
        .map(|env| env.fire && env.oxygen)
        .unwrap_or(true)
}

/// Combat environment gates owned by plan 12 (`Rules.fire`, `Env.oxygen`).
#[derive(Debug, Clone, Copy, bevy_ecs::prelude::Resource)]
pub struct CombatEnv {
    /// `Rules.fire`.
    pub fire: bool,
    /// `Rules.hasEnv(Env.oxygen)`.
    pub oxygen: bool,
    /// `state.envAttrs.get(Attribute.heat)` (`Liquid.willBoil`).
    pub heat: f32,
}

impl Default for CombatEnv {
    fn default() -> Self {
        Self {
            fire: true,
            oxygen: true,
            heat: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn create_refresh_and_extinguish() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let fire = create(&mut harness.build.world, 4, 4, &mut rng).expect("fire");
        assert!(has(&harness.build.world, 4, 4));
        // Refresh keeps the same entity.
        let again = create(&mut harness.build.world, 4, 4, &mut rng).expect("refresh");
        assert_eq!(fire, again);
        // Extinguish eventually removes.
        loop {
            if !extinguish(&mut harness.build.world, 4, 4, 200.0) {
                break;
            }
        }
        harness.fire_tick(0.0);
        assert!(!has(&harness.build.world, 4, 4));
    }

    #[test]
    fn water_attribute_speeds_up_burn() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let e = create(&mut harness.build.world, 4, 4, &mut rng).expect("fire");
        harness.fire_tick(0.2);
        let time = harness
            .build
            .world
            .get::<FireState>(e)
            .map(|f| f.time)
            .unwrap_or(0.0);
        // speedMultiplier = 1 + 0.2*10 = 3, then the bare-ground branch adds 8
        // (`FireComp.java:67-69`: `if(!damage && flammability <= 0) time += 8`).
        assert!((time - 11.0).abs() < 1e-4, "time={time}");
    }

    /// H09 T5: a fire on bare, non-flammable ground burns out in
    /// `ceil(BASE_LIFETIME / 9)` ticks (`time += 8` on top of the normal 1).
    #[test]
    fn fire_on_bare_ground_dies_fast() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let _ = create(&mut harness.build.world, 4, 4, &mut rng).expect("fire");
        for _ in 0..111 {
            let _ = harness.fire_tick(0.0);
            assert!(has(&harness.build.world, 4, 4), "alive before tick 112");
        }
        let _ = harness.fire_tick(0.0);
        assert!(!has(&harness.build.world, 4, 4), "removed on tick 112");
    }

    /// H09 T5: `Tile.getFlammability` reads the building's stored items; a
    /// container with a full load of coal has flammability 3, feeds lifetime and
    /// throws a fireball every `fireballDelay` accumulation
    /// (`FireComp.java:71-85`).
    #[test]
    fn fire_on_coal_container_lives_longer_and_throws_fireballs() {
        let mut harness = CombatHarness::new(16, 16, 7);
        let mut rng = SimRng::new(7);
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(8, 8, container, 0, true));
        let entity = harness.build_at(8, 8).expect("container entity");
        let coal = harness.content().item_id("coal").expect("coal");
        {
            let mut items = harness
                .build
                .world
                .get_mut::<crate::world::modules::ItemModule>(entity)
                .expect("items");
            items.set(coal, 300);
        }
        let flammability = tile_flammability(
            &harness.build.world,
            &harness.build.grid.tiles,
            &harness.build.content,
            8,
            8,
        );
        assert!(
            (flammability - 3.0).abs() < 1e-4,
            "300 coal -> 3.0 ({flammability})"
        );

        let fire = create(&mut harness.build.world, 8, 8, &mut rng).expect("fire");
        {
            let mut state = harness
                .build
                .world
                .get_mut::<FireState>(fire)
                .expect("fire");
            state.fireball_timer = 0.0;
        }
        let mut fireballs = Vec::new();
        for _ in 0..100 {
            fireballs = harness.fire_tick(0.0);
        }
        let lifetime = harness
            .build
            .world
            .get::<FireState>(fire)
            .map(|f| f.lifetime)
            .unwrap_or(0.0);
        // `lifetime += clamp(3 / 8, 0, 0.6) = 0.375` per tick.
        assert!(
            (lifetime - 1037.5).abs() < 1e-3,
            "lifetime after 100 ticks = 1037.5 ({lifetime})"
        );
        assert!(fireballs.is_empty(), "no fireball within 100 ticks");
        // Timer grows 0.3/tick -> the first fireball lands on tick 134.
        for _ in 0..33 {
            fireballs = harness.fire_tick(0.0);
        }
        assert!(fireballs.is_empty(), "no fireball at tick 133");
        fireballs = harness.fire_tick(0.0);
        assert_eq!(fireballs.len(), 1, "one fireball at tick 134");
        assert_eq!(fireballs[0], (64.0, 64.0), "fireball at the tile origin");
    }

    #[test]
    fn spread_timer_ignites_neighbor_with_flammable_puddle() {
        let mut harness = CombatHarness::new(16, 16, 5);
        let mut rng = SimRng::new(5);
        let oil = harness.content().liquid_id("oil").expect("oil");
        // Ignite the tile and keep a large oil puddle there so `flammability > 1`.
        let _ = create(&mut harness.build.world, 8, 8, &mut rng).expect("fire");
        for _ in 0..400 {
            let _ = super::super::puddles::deposit(
                &mut harness.build.world,
                &harness.build.grid.tiles,
                &harness.build.content,
                8,
                8,
                oil,
                60.0,
                &mut rng,
            );
            harness.fire_tick(0.0);
        }
        let lit_neighbors = [(7, 8), (9, 8), (8, 7), (8, 9)]
            .iter()
            .filter(|(x, y)| has(&harness.build.world, *x, *y))
            .count();
        assert!(has(&harness.build.world, 8, 8));
        assert!(lit_neighbors >= 1, "spread to a d4 neighbor");
    }

    #[test]
    fn fire_save_roundtrip() {
        use crate::io::wire::{WireReader, WireWriter};
        let src = FireState {
            tile: (3, 4),
            time: 12.5,
            lifetime: BASE_LIFETIME,
            puddle_flammability: 2.0,
            spread_timer: 5.0,
            fireball_timer: 7.0,
            damage_timer: 8.0,
            warmup: 0.0,
            animation: 0.0,
        };
        let mut bytes = Vec::new();
        {
            let mut w = WireWriter::new(&mut bytes);
            write_fire(&src, &mut w).expect("write");
        }
        let mut dst = src;
        dst.tile = (0, 0);
        dst.time = 0.0;
        dst.lifetime = 0.0;
        let mut r = WireReader::new(&bytes);
        read_fire(&mut dst, &mut r, FIRE_REVISION).expect("read");
        assert_eq!(dst.tile, (3, 4));
        assert_eq!(dst.time, 12.5);
        assert_eq!(dst.lifetime, BASE_LIFETIME);
    }

    /// T1: `fire.set(tile.worldx(), tile.worldy())` (Fires.java:26).
    #[test]
    fn fire_sits_on_tile_world() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let fire = create(&mut harness.build.world, 5, 7, &mut rng).expect("fire");
        let pos = harness.build.world.get::<Pos>(fire).expect("pos");
        assert_eq!((pos.x, pos.y), (40.0, 56.0));
    }

    /// H03 T7: `duration / frames = 2.25` ticks per frame, so one flame cycle
    /// (`frames` 40) takes `duration` 90 ticks (`FireComp.java:23-43`).
    #[test]
    fn fire_animation_cycles_every_90_ticks() {
        let mut animation = 0.0f32;
        for _ in 0..90 {
            animation = advance_animation(animation);
        }
        // The f32 accumulation may land just short of a full cycle: compare
        // modulo the frame count.
        let err = animation.min(FRAMES - animation);
        assert!(err < 1e-3, "animation after 90 ticks: {animation}");
        // And one tick advances exactly one frame step (1 / 2.25).
        let one = advance_animation(0.0);
        assert!((one - 1.0 / 2.25).abs() < 1e-6, "one-tick advance: {one}");
    }
}
