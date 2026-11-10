// SPDX-License-Identifier: GPL-3.0-only

//! GAPS #61: the ability pending-queue drains registered by [`Sim::new`], plus the
//! live-sim `StatusApplier` install the `shootStatus` weapon path needs.
//!
//! Upstream: none (port-only tests).

use crate::content::Rgba;
use crate::entities::comp::unit::comp::UnitTypeComp;
use crate::sim::*;

fn live_sim() -> Sim {
    let mut sim = Sim::new(7, 24, 24, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim
}

/// Item 4 (`shootStatus`): a bare harness world has no `StatusApplier`, while
/// the live sim installs the content-backed one
/// (`install_unit_runtime_from`) before any weapon/ability can apply a
/// status through `apply_status`.
#[test]
fn live_sim_installs_the_status_applier() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::combat::damage::status::StatusApplier>()
            .is_none(),
        "bare sim has no applier"
    );
    sim.install_unit_runtime().expect("unit runtime");
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::combat::damage::status::StatusApplier>()
            .is_some(),
        "live sim installs StatusApplier (shootStatus path)"
    );
}

/// `MoveLightningAbility` queues a bolt; the `UpdateUnits` drain turns it
/// into a real `Lightning.create` branch (hit bullets) and empties the queue.
#[test]
fn pending_move_lightning_becomes_a_real_branch() {
    use crate::combat::abilities::{PendingLightningStrike, PendingMoveLightning};
    let mut sim = live_sim();
    sim.ecs
        .0
        .insert_resource(PendingMoveLightning(vec![PendingLightningStrike {
            team: 1,
            color: Rgba::new(0.66, 0.85, 1.0, 1.0),
            damage: 12.0,
            x: 96.0,
            y: 96.0,
            rotation: 0.0,
            length: 6,
            vx: 0.5,
            vy: 0.0,
        }]));
    let before = sim.unit_runtime().expect("runtime").bullets_created;
    sim.tick().expect("tick");
    assert!(
        !sim.ecs.0.contains_resource::<PendingMoveLightning>(),
        "bolt queue drained"
    );
    assert!(
        sim.unit_runtime().expect("runtime").bullets_created > before,
        "Lightning.create spawned its hit bullets"
    );
}

/// `UnitSpawnAbility` queue: `Units.canCreate` gate + the tracked runtime
/// sequencer (`UnitType.spawn`).
#[test]
fn pending_ability_spawn_uses_the_runtime_sequencer() {
    use crate::combat::abilities::{PendingAbilityUnitSpawn, PendingAbilityUnitSpawns};
    let mut sim = live_sim();
    let dagger = sim
        .block_runtime()
        .expect("block runtime")
        .content()
        .unit_id("dagger")
        .expect("dagger");
    sim.ecs
        .0
        .insert_resource(PendingAbilityUnitSpawns(vec![PendingAbilityUnitSpawn {
            unit: "dagger",
            team: 1,
            x: 60.0,
            y: 60.0,
            rotation: 90.0,
        }]));
    sim.tick().expect("tick");
    assert!(
        !sim.ecs.0.contains_resource::<PendingAbilityUnitSpawns>(),
        "spawn queue drained"
    );
    let runtime = sim.unit_runtime().expect("runtime");
    let spawned = runtime
        .units
        .iter()
        .copied()
        .find(|entity| {
            sim.ecs
                .0
                .get::<UnitTypeComp>(*entity)
                .is_some_and(|comp| comp.type_id == dagger)
        })
        .expect("spawned unit tracked by the runtime");
    let pos = sim
        .ecs
        .0
        .get::<crate::entities::comp::Pos>(spawned)
        .expect("pos");
    assert!(
        (pos.x - 60.0).abs() < 1e-3 && (pos.y - 60.0).abs() < 1e-3,
        "spawned at the queued point ({}, {})",
        pos.x,
        pos.y
    );
}

/// `UnitSpawnAbility.java:48`: `Units.canCreate(team, unit)` gates the spawn.
#[test]
fn pending_ability_spawn_respects_the_unit_cap() {
    use crate::combat::abilities::{PendingAbilityUnitSpawn, PendingAbilityUnitSpawns};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim();
    let content = sim
        .block_runtime()
        .expect("block runtime")
        .content()
        .clone();
    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        unit_cap: 0,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    sim.ecs
        .0
        .insert_resource(PendingAbilityUnitSpawns(vec![PendingAbilityUnitSpawn {
            unit: "dagger",
            team: 1,
            x: 60.0,
            y: 60.0,
            rotation: 0.0,
        }]));
    crate::sim::drain_ability_queues_system(&mut sim.ecs.0);
    assert!(
        !sim.ecs.0.contains_resource::<PendingAbilityUnitSpawns>(),
        "capped spawn consumed"
    );
    assert!(
        sim.unit_runtime().expect("runtime").units.is_empty(),
        "Units.canCreate rejected the spawn at cap"
    );
}
