// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Commanded attacks, command-position seam and explosion/fire-damage regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// G01 T16: an attack order on a *unit* follows the moving target
/// (`CommandAI.defaultBehavior`: `targetPos.set(attackTarget)` every tick) and
/// kills it.
#[test]
fn commanded_dagger_chases_moving_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::TilePos;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let attacker = sim
        .spawn_live_unit("dagger", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("attacker");
    let target = sim
        .spawn_live_unit("dagger", 2, 50.0 * 8.0, 40.0 * 8.0, 180.0)
        .expect("target");
    // Walk the target east at full speed.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Command;
        slot.target = Some(TilePos::new(80, 40));
    }
    // The wire unit-id channel is `f32` and live seqs are ~1e9 (not exactly
    // representable), so seed the attack target component directly — the RT
    // tracking below is what this task owns.
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    sim.ecs
        .0
        .entity_mut(attacker)
        .insert(crate::ai::types::command::AttackTargetComp(Some(target)));
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(attacker)
    {
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(target_pos.x) as i16,
            crate::world::WorldGrid::to_tile(target_pos.y) as i16,
        ));
    }
    // The scenario exercises the chaser's attack-target tracking, not the
    // mutual exchange: the fleeing target holds fire (`holdFire` stance,
    // `CommandAI.shouldFire`). With the T4 velocity model a fleeing unit's
    // return fire lands (the old kinematic mover masked it), which turned the
    // exchange into a symmetric race instead of a chase.
    {
        use crate::ai::unit_stance_runtime::{UnitStancesComp, apply_stance};
        let def = sim
            .unit_runtime()
            .expect("rt")
            .content
            .unit_stance_by_name("holdfire")
            .cloned()
            .expect("holdfire stance");
        let mut stances = UnitStancesComp::default();
        assert!(apply_stance(&mut stances.0, &def));
        sim.ecs.0.entity_mut(target).insert(stances);
    }
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(target).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(target).is_err()
            || sim
                .ecs
                .0
                .get::<Health>(target)
                .is_some_and(|health| health.dead),
        "the attacker killed the fleeing target"
    );
}

/// G01 T16: the reticle seam reports the tracked entity's position.
#[test]
fn command_target_seam_reports_position() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let attacker = sim
        .spawn_live_unit("dagger", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("attacker");
    let target = sim
        .spawn_live_unit("dagger", 2, 50.0 * 8.0, 40.0 * 8.0, 180.0)
        .expect("target");
    let attacker_id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(attacker)
        .expect("seq")
        .0 as i32;
    sim.ecs
        .0
        .entity_mut(attacker)
        .insert(crate::ai::types::command::AttackTargetComp(Some(target)));
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(attacker)
    {
        slot.kind = crate::ai::controller::AiKind::Command;
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(target_pos.x) as i16,
            crate::world::WorldGrid::to_tile(target_pos.y) as i16,
        ));
    }
    sim.tick().expect("tick");
    let rows = sim.command_targets();
    let row = rows
        .iter()
        .find(|(id, _, _, _)| *id == attacker_id)
        .copied()
        .expect("the attacker reports a command target");
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    eprintln!("command target row {row:?}, target {target_pos:?}");
    assert!(row.3, "marked as an attack target");
    assert!(
        (row.1 - target_pos.x).abs() < 32.0 && (row.2 - target_pos.y).abs() < 32.0,
        "reports the tracked entity's position"
    );
}

/// G01 T14: a unit carrying blast compound explodes on death
/// (`UnitComp.destroy` -> `Damage.dynamicExplosion`): 30 blast-compound gives
/// explosiveness `2 + 1.2*30*1.53 = 57.08` -> 5 waves of 28.54 damage with a
/// 50 px final radius.
#[test]
fn unit_with_blast_compound_explodes() {
    use crate::entities::comp::unit::comp::ItemsComp;

    let mut sim = live_sim(48);
    let wall = place_at(&mut sim, 20, 20, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0 + 8.0, 20.5 * 8.0, 180.0)
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
    // Staggered waves (`Time.run(i * 2f)`): the growing radius needs ticks.
    tick_n(&mut sim, 40);
    let wall_end = health(&sim, wall);
    eprintln!("wall {wall_start} -> {wall_end:?}");
    assert!(
        wall_end.is_none_or(|hp| hp <= wall_start - 20.0),
        "the blast damaged the wall: {wall_start} -> {wall_end:?}"
    );
}

/// G01 T14: core-spawned units skip the death explosion (`spawnedByCore`).
#[test]
fn core_spawned_unit_does_not_explode() {
    use crate::entities::comp::unit::comp::{ItemsComp, UnitCore};

    let mut sim = live_sim(48);
    let wall = place_at(&mut sim, 20, 20, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0 + 8.0, 20.5 * 8.0, 180.0)
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
    if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(dagger) {
        core.spawned_by_core = true;
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
        "no explosion from a core-spawned unit"
    );
}

/// G01 T14: `rules.fire = false` blocks fire creation through the `CombatEnv`
/// sync (`Fires.create` gate).
#[test]
fn fire_disabled_rule_blocks_fire() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(24);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        fire: false,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    // One tick syncs `CombatEnv` from the session rules.
    sim.tick().expect("tick");
    let mut rng = crate::determinism::SimRng::new(7);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng);
    assert!(fire.is_none(), "rules.fire = false blocks fires");
}

/// H09 T5: a fire's damage tick burns a grounded unit for `unitDamage` and
/// applies `burning` for 300 ticks; flying units are skipped
/// (`FireComp.java:88-99`).
#[test]
fn fire_burns_ground_units() {
    use crate::entities::comp::unit::comp::StatusComp;

    let mut sim = live_sim(48);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 80.0, 80.0, 0.0)
        .expect("dagger");
    let flare = sim
        .spawn_live_unit("flare", 1, 80.0, 80.0, 0.0)
        .expect("flare");
    let mut rng = crate::determinism::SimRng::new(9);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng).expect("fire");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::combat::fires::FireState>(fire)
            .expect("fire state");
        state.damage_timer = 39.0;
    }
    let dagger_before = health(&sim, dagger).expect("dagger health");
    let flare_before = health(&sim, flare).expect("flare health");
    sim.tick().expect("tick");
    let dagger_after = health(&sim, dagger).expect("dagger health");
    assert!(
        (dagger_before - dagger_after - 3.0).abs() < 1e-3,
        "dagger took unitDamage 3 ({dagger_before} -> {dagger_after})"
    );
    let burning = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("burning")
        .expect("burning");
    let statuses = sim.ecs.0.get::<StatusComp>(dagger).expect("statuses");
    assert!(
        statuses
            .statuses
            .iter()
            .any(|entry| entry.effect == burning),
        "dagger is burning"
    );
    assert_eq!(
        health(&sim, flare),
        Some(flare_before),
        "a flying flare is untouched"
    );
}

/// H09 T5: fire tile damage goes through `apply_health`, so
/// `rules.blockHealth(team)` scales it (`BuildingComp.damage`).
#[test]
fn fire_damage_respects_block_health_multiplier() {
    let mut sim = campaign_sim(48, |rules| {
        rules.block_health_multiplier = 2.0;
    });
    let container = place_at(&mut sim, 10, 10, "container", 1);
    let mut rng = crate::determinism::SimRng::new(9);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng).expect("fire");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::combat::fires::FireState>(fire)
            .expect("fire state");
        state.damage_timer = 39.0;
    }
    let before = health(&sim, container).expect("container health");
    sim.tick().expect("tick");
    let after = health(&sim, container).expect("container health");
    assert!(
        (before - after - 0.9).abs() < 1e-3,
        "tileDamage 1.8 / blockHealth 2 = 0.9 ({before} -> {after})"
    );
}
