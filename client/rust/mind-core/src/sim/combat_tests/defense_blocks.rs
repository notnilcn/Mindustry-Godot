// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Defense-block regressions: shield/force projectors, shock mines, conveyors and building positions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// PAR-0475: a powered `shield-projector` runs the `BaseShield.unitConsumer`
/// (`BaseShield.java:38-57`) through the live `BuildingBehavior` path (it
/// borrows the content/grid/FX seam from the unit runtime): a shallow overlap
/// pushes the enemy unit out, a deep overlap kills it through
/// `unit.kill()` -> `resolve_deaths`.
#[test]
fn shield_projector_repels_and_kills_enemy_units() {
    use crate::world::blocks::defense::projectors::BaseShieldState;

    let mut sim = live_sim(48);
    place_at(&mut sim, 18, 20, "power-source", 1);
    let shield = place_at(&mut sim, 20, 20, "shield-projector", 1);
    tick_n(&mut sim, 600);
    let radius = sim
        .ecs
        .0
        .get::<BaseShieldState>(shield)
        .map(|state| state.smooth_radius)
        .expect("shield state");
    assert!(radius > 150.0, "a powered dome grows: {radius}");
    let (cx, cy) = sim
        .ecs
        .0
        .get::<Pos>(shield)
        .map(|pos| (pos.x, pos.y))
        .expect("shield pos");

    // dagger hitSize 8: `overlapDst = 4 + radius - (radius - 2) = 6 <= 12`,
    // so it is pushed out, not killed.
    let dagger = sim
        .spawn_live_unit("dagger", 2, cx + radius - 2.0, cy, 180.0)
        .expect("dagger");
    sim.tick().expect("tick");
    let pushed_radius = sim
        .ecs
        .0
        .get::<BaseShieldState>(shield)
        .map(|state| state.smooth_radius)
        .expect("shield state");
    let (px, py) = sim
        .ecs
        .0
        .get::<Pos>(dagger)
        .map(|pos| (pos.x, pos.y))
        .expect("dagger pos");
    let dst = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
    assert!(
        dst >= pushed_radius + 4.0,
        "shallow unit pushed outside the dome: dst = {dst}, radius = {pushed_radius}"
    );
    assert!(
        !sim.ecs.0.get::<Health>(dagger).expect("dagger health").dead,
        "a shallow overlap is not lethal"
    );

    // A unit stuck deep inside (`overlapDst > hitSize * 1.5`) is killed
    // (`unit.kill()`); `resolve_deaths` destroys it in the same tick.
    let stuck = sim
        .spawn_live_unit("dagger", 2, cx + radius * 0.25, cy, 180.0)
        .expect("stuck dagger");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(stuck).is_err(),
        "a deep unit is killed by the dome"
    );
}

/// G01 T14: a charged force projector absorbs the death explosion
/// (`ForceBuild.absorbExplosion`): the wall under the shield is undamaged.
#[test]
fn force_projector_absorbs_explosion() {
    use crate::entities::comp::unit::comp::ItemsComp;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    // Power-source feeds the projector; the wall sits inside its radius.
    place_at(&mut sim, 18, 20, "power-source", 1);
    let projector = place_at(&mut sim, 20, 20, "force-projector", 1);
    let wall = place_at(&mut sim, 22, 20, "copper-wall", 1);
    tick_n(&mut sim, 600);
    let (broken, radius) = sim
        .ecs
        .0
        .get::<ForceProjectorState>(projector)
        .map(|state| (state.broken, state.real_radius()))
        .expect("force projector state");
    assert!(!broken, "a powered projector is charged");
    assert!(radius > 90.0, "charged shield radius {radius}");
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 22.5 * 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    sim.tick().expect("tick");
    assert_eq!(
        health(&sim, wall),
        Some(wall_start),
        "the projector absorbed the explosion"
    );
}

/// H02 T7 (PAR-0129): a force projector next to a power source charges to
/// efficiency 1 and draws its dome (`ForceBuild.updateTile`, warmup/radscl).
#[test]
fn force_projector_powered_by_adjacent_source() {
    use crate::entities::comp::Building;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    place_at(&mut sim, 18, 20, "power-source", 1);
    let projector = place_at(&mut sim, 20, 20, "force-projector", 1);
    tick_n(&mut sim, 60);
    let efficiency = sim.ecs.0.get::<Building>(projector).map(|b| b.efficiency);
    assert!(
        efficiency.is_some_and(|e| e > 0.99),
        "powered projector efficiency {efficiency:?}"
    );
    let (warmup, radscl) = sim
        .ecs
        .0
        .get::<ForceProjectorState>(projector)
        .map(|state| (state.warmup, state.radscl))
        .expect("force projector state");
    assert!(warmup > 0.99, "warmup {warmup}");
    assert!(radscl > 0.9, "radscl {radscl}");
}

/// H02 T7: enemy bullets inside the live dome are absorbed and add buildup
/// (`ForceBuild.deflectBullets` over `Groups.bullet`).
#[test]
fn force_projector_absorbs_enemy_bullet() {
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    place_at(&mut sim, 18, 20, "power-source", 1);
    let projector = place_at(&mut sim, 20, 20, "force-projector", 1);
    tick_n(&mut sim, 120);
    // Inside the ~101.7 px dome, 60 px east of the projector centre
    // (tile centre (20,20) = world (160,160)), facing it.
    sim.spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0 + 4.0, 180.0)
        .expect("dagger");
    let mut absorbed = false;
    for _ in 0..300 {
        sim.tick().expect("tick");
        if let Some(state) = sim.ecs.0.get::<ForceProjectorState>(projector)
            && (state.buildup > 0.0 || state.hit > 0.0)
        {
            absorbed = true;
            break;
        }
    }
    assert!(absorbed, "dagger bullets are absorbed by the shield");
}

/// H02 T7: phase fabric raises `phaseHeat` and is consumed every
/// `phaseUseTime / timeScale` ticks (`ForceProjector.java:207-215`).
#[test]
fn force_projector_phase_boost() {
    use crate::world::blocks::defense::shields::ForceProjectorState;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    place_at(&mut sim, 18, 20, "power-source", 1);
    let projector = place_at(&mut sim, 20, 20, "force-projector", 1);
    let phase = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("phase-fabric")
        .expect("phase-fabric");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(projector) {
        items.add(phase, 2, 10);
    }
    tick_n(&mut sim, 30);
    let phase_heat = sim
        .ecs
        .0
        .get::<ForceProjectorState>(projector)
        .map(|state| state.phase_heat)
        .expect("force projector state");
    let count = sim
        .ecs
        .0
        .get::<ItemModule>(projector)
        .map(|items| items.get(phase))
        .unwrap_or(0);
    assert!(phase_heat > 0.9, "phase heat {phase_heat}");
    assert_eq!(count, 1, "one fabric consumed after the first timer cycle");
    tick_n(&mut sim, 400);
    let count = sim
        .ecs
        .0
        .get::<ItemModule>(projector)
        .map(|items| items.get(phase))
        .unwrap_or(0);
    assert_eq!(count, 0, "the second fabric is consumed on the next cycle");
}

/// H02 T11: a dagger standing on an enemy shock mine triggers the tendrils
/// (`ShockMineBuild.unitOn`); the mine pays `tileDamage` and cannot re-trigger
/// before `cooldown` ticks.
#[test]
fn shock_mine_zaps_enemy_dagger() {
    use crate::world::blocks::defense::shields::ShockMineState;

    let mut sim = live_sim(48);
    let mine = place_at(&mut sim, 20, 20, "shock-mine", 1);
    let mine_start = health(&sim, mine).expect("mine health");
    assert_eq!(mine_start, 50.0, "vanilla shock-mine health");
    // Tile centre: `WorldGrid::to_tile` rounds, so only `tile * 8` lands on
    // tile (20, 20) (the plan's (164,164) is the corner between tiles).
    let dagger = sim
        .spawn_live_unit("dagger", 2, 160.0, 160.0, 0.0)
        .expect("dagger");
    // Java-faithful `Lightning` chains onto the dagger with all four tendrils,
    // so one trigger lands ~150 damage: top the 150-HP dagger up so the
    // 80-tick cooldown leg below can still observe a second trigger.
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.max_health = 1_000.0;
        hp.health = 1_000.0;
    }
    let dagger_start = health(&sim, dagger).expect("dagger health");
    let hold = |sim: &mut Sim, dagger| {
        if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(dagger) {
            vel.x = 0.0;
            vel.y = 0.0;
        }
        sim.tick().expect("tick");
    };
    for _ in 0..2 {
        hold(&mut sim, dagger);
    }
    let first = sim
        .ecs
        .0
        .get::<ShockMineState>(mine)
        .and_then(|state| state.last_trigger);
    assert!(first.is_some(), "mine triggered on the unit");
    assert!(
        health(&sim, dagger).is_some_and(|hp| hp < dagger_start),
        "dagger zapped by the mine"
    );
    assert!(
        health(&sim, mine).is_some_and(|hp| hp <= mine_start - 7.0 + 0.001),
        "mine paid tileDamage: {:?}",
        health(&sim, mine)
    );
    // No re-trigger inside the 80-tick cooldown.
    for _ in 0..40 {
        hold(&mut sim, dagger);
    }
    assert_eq!(
        sim.ecs
            .0
            .get::<ShockMineState>(mine)
            .and_then(|state| state.last_trigger),
        first,
        "cooldown blocks the re-trigger"
    );
    // Past the cooldown the mine fires again.
    let mut refreshed = false;
    for _ in 0..60 {
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
        hold(&mut sim, dagger);
        if sim
            .ecs
            .0
            .get::<ShockMineState>(mine)
            .and_then(|state| state.last_trigger)
            != first
        {
            refreshed = true;
            break;
        }
    }
    assert!(refreshed, "second trigger after the cooldown");
}

/// H02 T11: a running conveyor pushes a grounded friendly unit along its
/// rotation (`ConveyorBuild.unitOn`).
#[test]
fn conveyor_carries_dagger() {
    let mut sim = live_sim(48);
    for x in 10..=20 {
        place_at(&mut sim, x, 20, "titanium-conveyor", 1);
    }
    let dagger = sim
        .spawn_live_unit("dagger", 1, 96.0, 160.0, 0.0)
        .expect("dagger");
    let start_x = sim.ecs.0.get::<Pos>(dagger).map(|p| p.x).unwrap_or(0.0);
    for i in 0..60 {
        sim.tick().expect("tick");
        if i % 10 == 0 {
            let p = sim.ecs.0.get::<Pos>(dagger).map(|p| (p.x, p.y));
            let v = sim.ecs.0.get::<Vel>(dagger).map(|v| (v.x, v.y));
            eprintln!("CONV i={i} pos={p:?} vel={v:?}");
        }
    }
    let end_x = sim.ecs.0.get::<Pos>(dagger).map(|p| p.x).unwrap_or(0.0);
    assert!(
        end_x > start_x + 8.0,
        "belt carried the dagger east: {start_x} -> {end_x}"
    );
}

/// G01 T14: `explosion_absorbed` covers the point when a live shield polygon
/// contains it and skips the ignore team's own shields.
#[test]
fn explosion_absorbed_polygon() {
    use crate::combat::damage::explosion::explosion_absorbed;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    let projector = place_at(&mut sim, 20, 20, "force-projector", 1);
    if let Some(mut state) = sim.ecs.0.get_mut::<ForceProjectorState>(projector) {
        state.radscl = 1.0;
        state.radius = 100.0;
        state.broken = false;
        state.sides = 24;
    }
    let (x, y) = (20.5 * 8.0, 20.5 * 8.0);
    assert!(
        explosion_absorbed(&mut sim.ecs.0, x, y, 30.0, Some(2)),
        "team 1's shield absorbs the wave for team 2"
    );
    assert!(
        !explosion_absorbed(&mut sim.ecs.0, x + 1000.0, y, 30.0, Some(2)),
        "a far-away point is not absorbed"
    );
    assert!(
        !explosion_absorbed(&mut sim.ecs.0, x, y, 30.0, Some(1)),
        "the shield's own team is ignored"
    );
}

/// T1: `Building.set(tile.drawx(), tile.drawy())` (BuildingComp.java:126) with
/// `drawx = tile.worldx() + block.offset` (Tile.java:166-177); the offset is
/// `((size + 1) % 2) * tilesize / 2` (Block.java:774), so odd sizes (and 1x1)
/// sit on the tile's `worldx` and even sizes shift `+4`.
#[test]
fn building_pos_uses_block_offset() {
    let mut sim = live_sim(64);
    let cases = [
        ("duo", 10, 80.0),               // 1x1, offset 0
        ("core-shard", 20, 160.0),       // 3x3, offset 0
        ("mechanical-drill", 30, 244.0), // 2x2, offset 4
        ("core-foundation", 40, 324.0),  // 4x4, offset 4
    ];
    for (name, tile, expected) in cases {
        let entity = place_at(&mut sim, tile, tile, name, 1);
        let pos = sim.ecs.0.get::<Pos>(entity).expect("pos");
        assert_eq!(
            (pos.x, pos.y),
            (expected, expected),
            "{name} at tile {tile} must sit at drawx/drawy"
        );
    }
}
