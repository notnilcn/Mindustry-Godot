// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Targeting regressions: air/ground, prediction, per-mount targeting and re-aiming.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// T2: `AIController.updateWeapons` searches per mount
/// (`findTarget(mountX, mountY, weapon.range(), bullet.collidesAir,
/// bullet.collidesGround)`, AIController.java:216-222), so a ground-only weapon
/// ignores the flyer the unit's main target search picked. The unit is the
/// first content unit whose first weapon bullet does not collide air (printed
/// by the test).
#[test]
fn ground_weapon_ignores_air_target_per_mount() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        wave_team: 2,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let name = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        content
            .units()
            .iter()
            .find(|unit| {
                unit.weapons.first().is_some_and(|weapon| {
                    content
                        .bullet(weapon.bullet.id)
                        .is_some_and(|bullet| !bullet.collides_air && bullet.collides_ground)
                })
            })
            .map(|unit| unit.name.clone())
            .expect("a ground-only unit exists")
    };
    eprintln!("T2 ground-only unit: {name}");

    let wall = place_at(&mut sim, 29, 25, "copper-wall", 1);
    let unit = sim
        .spawn_live_unit(&name, 2, 200.0, 200.0, 0.0)
        .expect("ground-only unit");
    // A flying enemy at the same distance as the wall (`hitSize/2` apart).
    let flare = sim
        .spawn_live_unit("flare", 1, 230.0, 200.0, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    tick_n(&mut sim, 120);
    let target = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(unit)
        .and_then(|weapons| weapons.mounts.first().and_then(|mount| mount.target));
    assert_eq!(
        target,
        Some(wall),
        "the ground-only mount searched its own target (the wall), not the flyer"
    );
}

/// T2: the shoot/invalidate range adds the TARGET's `hitSize/2`
/// (`mount.target.within(mountX, mountY, wrange + target.hitSize/2)`,
/// AIController.java:239; `Units.invalidateTarget`, Units.java:151-157). A 3x3
/// core-shard adds 12 px where the pre-T2 port added the shooter's `hitSize/2`.
///
/// The per-mount search radius is `weapon.range() + |shootY|` (a tighter gate
/// than the shoot range), so the shoot gate is exercised with the mount target
/// set directly on a non-`singleTarget` unit.
#[test]
fn dagger_engages_core_at_target_hitsize() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let (name, weapon_x, weapon_y, range, shooter_hit) = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        let def = content
            .units()
            .iter()
            .find(|unit| {
                !unit.single_target
                    && !unit.flying
                    && unit
                        .weapons
                        .first()
                        .and_then(|weapon| content.bullet(weapon.bullet.id))
                        .is_some_and(|bullet| bullet.collides_ground)
            })
            .expect("a non-singleTarget ground unit exists");
        let weapon = def.weapons.first().expect("weapon");
        (
            def.name.clone(),
            weapon.x,
            weapon.y,
            weapon.range(),
            def.hit_size,
        )
    };
    eprintln!(
        "T2 hitSize unit: {name} range {range} weapon ({weapon_x}, {weapon_y}) hit {shooter_hit}"
    );
    let core = place_at(&mut sim, 20, 20, "core-shard", 1);
    // core-shard Pos = (160, 160); hitSize = 24 -> the shoot/invalidate range is
    // `range + 12`. `rotation 0` puts the mount at `unit - (weapon.y, weapon.x)`.
    let target_dist = range + 12.0 - 4.0;
    let unit = sim
        .spawn_live_unit(
            &name,
            2,
            160.0 + target_dist + weapon_y,
            160.0 + weapon_x,
            0.0,
        )
        .expect("unit");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    // Tick 1 runs the first (search) tick; the search radius is `range + |shootY|`.
    sim.tick().expect("tick");
    let set_target = |sim: &mut Sim, target: Option<Entity>| {
        if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(unit)
            && let Some(mount) = weapons.mounts.first_mut()
        {
            mount.target = target;
        }
    };
    let mount_state = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(unit)
            .and_then(|weapons| {
                weapons
                    .mounts
                    .first()
                    .map(|mount| (mount.target, mount.shoot))
            })
    };
    set_target(&mut sim, Some(core));
    sim.tick().expect("tick");
    let (target, shoot) = mount_state(&sim).expect("mounts");
    assert_eq!(
        target,
        Some(core),
        "the target survives the invalidate check"
    );
    assert!(shoot, "engages at range + target.hitSize/2 - 4");
    // Move 5 px further: beyond `range + 12` -> invalidated and not shooting.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(unit) {
        pos.x += 5.0;
    }
    set_target(&mut sim, Some(core));
    sim.tick().expect("tick");
    let (target, shoot) = mount_state(&sim).expect("mounts");
    assert_eq!(
        target, None,
        "beyond range + target.hitSize/2 is invalidated"
    );
    assert!(!shoot, "does not shoot past range + target.hitSize/2");
}

/// T2: `FlyingAI.findTarget` keeps the in-range main target
/// (FlyingAI.java:34-39): a flare whose main target (the core) is in range does
/// not switch to a closer dagger.
#[test]
fn flying_mount_keeps_in_range_main_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let core = place_at(&mut sim, 20, 20, "core-shard", 1);
    let dagger = place_at(&mut sim, 24, 20, "duo", 1);
    let flare = sim
        .spawn_live_unit("flare", 2, 200.0, 190.0, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::Flying;
    }
    // A team-1 dagger (closer than the core) must not steal the mount target.
    let bait = sim
        .spawn_live_unit("dagger", 1, 175.0, 190.0, 180.0)
        .expect("bait");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(bait) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    tick_n(&mut sim, 130);
    let target = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(flare)
        .and_then(|weapons| weapons.mounts.first().and_then(|mount| mount.target));
    assert!(
        target == Some(core) || target == Some(dagger),
        "the flare engaged a building ({target:?}), not the closer dagger"
    );
}

/// T2: `autoDropBombs` (AIController.java:225-229): a horizon with no valid
/// main target drops its bomb weapon on the ground unit below it
/// (`Units.closestTarget(.., u -> !u.isFlying(), t -> true)`).
#[test]
fn horizon_auto_drops_bombs_on_conveyor() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        wave_team: 2,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // A concrete ground victim: a copper wall on a building tile.
    let wall = place_at(&mut sim, 20, 20, "copper-wall", 1);
    let before = health(&sim, wall).expect("wall health");
    let horizon = sim
        .spawn_live_unit("horizon", 2, 20.0 * 8.0, 20.0 * 8.0 - 14.0, 90.0)
        .expect("horizon");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(horizon) {
        slot.kind = AiKind::Flying;
    }
    tick_n(&mut sim, 600);
    let after = health(&sim, wall).unwrap_or(0.0);
    eprintln!("T2 horizon bombs: wall {before} -> {after}");
    assert!(after < before, "the horizon dropped bombs on the wall");
}

/// T2: a `!controllable` mount is never overwritten by the AI's main target
/// (`if(!weapon.controllable || weapon.noAttack) continue;`,
/// AIController.java:214).
#[test]
fn non_controllable_mount_not_overwritten() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let name = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        content
            .units()
            .iter()
            .find(|unit| {
                unit.weapons
                    .iter()
                    .any(|weapon| !weapon.controllable && !weapon.no_attack)
            })
            .map(|unit| unit.name.clone())
            .expect("a unit with a non-controllable weapon exists")
    };
    eprintln!("T2 non-controllable weapon unit: {name}");
    // The first such unit is naval (oxynoe): give it water so the
    // environment pass does not kill it on land.
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("deep-water")
        .expect("deep-water");
    for x in 11..=14 {
        sim.grid.tiles.get_mut(x, 20).floor = water;
    }
    let _wall = place_at(&mut sim, 28, 20, "copper-wall", 1);
    let unit = sim
        .spawn_live_unit(&name, 2, 100.0, 160.0, 0.0)
        .expect("unit");
    // A close enemy unit so the AI main target is Some.
    let bait = sim
        .spawn_live_unit("dagger", 1, 120.0, 160.0, 180.0)
        .expect("bait");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(bait) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    tick_n(&mut sim, 5);
    let weapons = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(unit)
        .expect("weapons");
    let any_non_controllable = weapons
        .weapons
        .iter()
        .zip(weapons.mounts.iter())
        .filter(|(def, _)| !def.controllable)
        .all(|(_, mount)| mount.target != Some(bait));
    assert!(
        any_non_controllable,
        "a non-controllable mount was overwritten with the AI main target"
    );
}

/// H02 T16: the duo aims at the intercept point for its 2.5 px/tick copper
/// bullet, not at the target's current position (`Turret.targetPosition`,
/// `Turret.java:459-479`; `Predict.intercept`, `Predict.java:27-52`).
#[test]
fn turret_leads_moving_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place_at(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + 100.0, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 2.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // `EntitySet::UpdateUnits` runs before `UpdateBuildings`, so the flare's
    // own `VelComp.update` (T4) already integrated its velocity and applied
    // drag before the turret aimed this tick.
    let flare_now = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    let flare_vel = sim
        .ecs
        .0
        .get::<Vel>(flare)
        .map(|v| (v.x, v.y))
        .expect("flare vel");
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    // `duo_copper` bullet speed (`register_bullets`).
    let expected = crate::combat::targeting::intercept_positions(
        duo_pos,
        (0.0, 0.0),
        flare_now,
        flare_vel,
        true,
        2.5,
    );
    assert!(
        state.target_pos.1 > flare_now.1,
        "the aim leads the northbound flare: {:?} vs {flare_now:?}",
        state.target_pos
    );
    assert!(
        (state.target_pos.0 - expected.0).abs() < 0.01
            && (state.target_pos.1 - expected.1).abs() < 0.01,
        "aim equals Predict.intercept: {:?} vs {expected:?}",
        state.target_pos
    );
}

/// H02 T16: the aim point is recomputed every tick even between retargets
/// (`Turret.updateTile` calls `targetPosition` on every tick, not only in
/// `findTarget`).
#[test]
fn turret_reaims_every_tick() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place_at(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + 80.0, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // Move the target 20 px north; the retarget timer stays high.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(flare) {
        pos.y -= 20.0;
    }
    let moved = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    sim.tick().expect("tick");
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    assert!(
        (state.target_pos.0 - moved.0).abs() < 0.01 && (state.target_pos.1 - moved.1).abs() < 0.01,
        "aim follows the moved target: {:?} vs {moved:?}",
        state.target_pos
    );
}

/// H02 T16: firing is gated by `within(target, range() + hitSize/1.9)`
/// (`Turret.java:587`), evaluated against the target's own position.
#[test]
fn turret_holds_fire_out_of_range() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place_at(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let hit_size = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name("flare")
        .expect("flare def")
        .hit_size;
    let far = 160.0 + hit_size / 1.9 + 2.0;
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + far, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
        state.rotation = 0.0; // already facing the target
        state.reload_counter = 1000.0; // ready to fire if allowed
    }
    let before = sim.unit_runtime().expect("rt").bullets_created;
    sim.tick().expect("tick");
    let after = sim.unit_runtime().expect("rt").bullets_created;
    assert_eq!(
        before, after,
        "out-of-range target holds fire (Turret.java:587)"
    );
}

/// H02 T16: `scathe` does not predict (`Blocks.java:5248`), so its aim point
/// is the target's current position.
#[test]
fn scathe_does_not_predict() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(64);
    let scathe = place_at(&mut sim, 20, 20, "scathe", 1);
    // `Turret.targetPosition` returns without ammo (`Turret.java:460`).
    feed(&mut sim, scathe, "carbide", 1);
    let scathe_pos = sim
        .ecs
        .0
        .get::<Pos>(scathe)
        .map(|p| (p.x, p.y))
        .expect("scathe pos");
    assert!(
        !sim.ecs
            .0
            .get::<TurretState>(scathe)
            .expect("scathe state")
            .config
            .predict_target,
        "scathe predictTarget = false"
    );
    let flare = sim
        .spawn_live_unit("flare", 2, scathe_pos.0 + 100.0, scathe_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 2.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(scathe) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // `EntitySet::UpdateUnits` runs before `UpdateBuildings`: the flare's own
    // `VelComp.update` (T4) integrated its velocity before the scathe aimed.
    let flare_now = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    let state = sim.ecs.0.get::<TurretState>(scathe).expect("state");
    assert!(
        (state.target_pos.0 - flare_now.0).abs() < 0.01
            && (state.target_pos.1 - flare_now.1).abs() < 0.01,
        "no lead without predictTarget: {:?} vs {flare_now:?}",
        state.target_pos
    );
}
