// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live unit command, status and AI regressions: command switch, Mace lead, Crawler ram, builder repair, boost kinds and movement.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

/// PAR-0140: every registered command id is accepted for a commandable unit
/// and stored on it (`CommandAI.command`, CommandAI.java:57-64); move orders
/// keep the tile path.
#[test]
fn live_unit_command_switch_is_stored() {
    use crate::ai::controller::{AiKind, ControllerSlot, UnitCommandComp};
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
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let unit = sim
        .spawn_live_unit("poly", 1, 100.0, 100.0, 0.0)
        .expect("poly");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(unit).expect("seq").0 as i32;
    let (repair_id, poly_allows) = {
        let runtime = sim.unit_runtime().expect("rt");
        let repair = runtime
            .content
            .unit_command_by_name("repair")
            .expect("repair command");
        let allows = runtime
            .content
            .unit_id("poly")
            .and_then(|id| runtime.content.unit(id))
            .is_some_and(|def| crate::ai::unit_command_runtime::allows_command(def, repair.id));
        (repair.id.raw(), allows)
    };
    assert!(poly_allows, "poly lists the repair command");
    assert_eq!(
        sim.apply_live_unit_command(&[id], repair_id, 0.0, 0.0, false),
        1,
        "the repair command is accepted"
    );
    assert_eq!(
        sim.ecs.0.get::<UnitCommandComp>(unit).copied(),
        Some(UnitCommandComp(repair_id)),
        "the active command is stored on the unit"
    );
    // A move order keeps the ordered tile (and clears nothing else).
    assert_eq!(
        sim.apply_live_unit_command(
            &[id],
            crate::sim::command::MOVE_COMMAND.raw(),
            200.0,
            100.0,
            false
        ),
        1,
        "move order accepted"
    );
    assert!(
        sim.ecs
            .0
            .get::<ControllerSlot>(unit)
            .and_then(|slot| slot.target)
            .is_some(),
        "move target set"
    );
}

/// T12: the live AI aims with the `Predict.intercept` lead
/// (`AIController.updateWeapons`, AIController.java:231): a slow shooter
/// connects with a target moving perpendicular at full speed.
#[test]
fn mace_leads_a_moving_dagger() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::TilePos;

    let mut sim = live_sim(64);
    let shooter = sim
        .spawn_live_unit("mace", 1, 100.0, 100.0, 0.0)
        .expect("mace");
    let target = sim
        .spawn_live_unit("dagger", 2, 140.0, 100.0, 0.0)
        .expect("target");
    // Walk the target straight down (-y) at full speed.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Command;
        slot.target = Some(TilePos::new(17, 20));
    }
    let target_start = health(&sim, target).expect("target health");
    let mut killed = false;
    for _ in 0..600 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(target).is_err() {
            killed = true;
            break;
        }
    }
    let target_end = health(&sim, target);
    // The mace (74 damage) kills a 150 hp dagger in 3 connected shots; a
    // no-lead aim hits nothing once the target has moved a bullet-flight away.
    assert!(
        killed || target_end.is_some_and(|hp| hp <= target_start - 3.0 * 74.0),
        "the lead lets the mace connect (start {target_start}, end {target_end:?}, shooter {:?})",
        health(&sim, shooter)
    );
}

/// T14: the live dispatch runs `SuicideAI` for crawlers (`AiKind::Suicide`):
/// the crawler rams a target and its death weapon fires on contact
/// (`ai/types/suicide.rs`, `UnitTypes.crawler`).
#[test]
fn crawler_rams_and_explodes() {
    let mut sim = live_sim(48);
    let target = sim
        .spawn_live_unit("dagger", 1, 150.0, 100.0, 0.0)
        .expect("target dagger");
    let target_start = health(&sim, target).expect("target health");
    let crawler = sim
        .spawn_live_unit("crawler", 2, 100.0, 100.0, 0.0)
        .expect("crawler");
    let mut ticks = 0;
    while sim.ecs.0.get_entity(crawler).is_ok() && ticks < 400 {
        sim.tick().expect("tick");
        ticks += 1;
    }
    assert!(
        sim.ecs.0.get_entity(crawler).is_err(),
        "the crawler rammed the target and died ({ticks} ticks); pos {:?} weapons {:?}",
        sim.ecs.0.get::<Pos>(crawler).map(|p| (p.x, p.y)),
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(crawler)
            .map(|w| w
                .mounts
                .iter()
                .map(|m| (m.total_shots, m.shoot, m.target.is_some()))
                .collect::<Vec<_>>())
    );
    assert!(
        health(&sim, target).is_none_or(|hp| hp < target_start),
        "the crawler's death explosion damaged the target: {:?}",
        health(&sim, target)
    );
}

/// T14/T10c: a builder unit (`AiKind::Builder`) picks up the ghost plan of a
/// destroyed friendly wall and rebuilds it (`BuilderAI` +
/// `Sim::update_builders`, G01 T10c semantics).
#[test]
fn commanded_builder_repairs_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("copper")
        .expect("copper");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(core) {
        items.add(copper, 500, 4000);
    }
    let wall = place(&mut sim, 16, 12, "copper-wall", 1);
    // Destroy it so `resolve_deaths` queues the team ghost plan.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    let builder = sim
        .spawn_live_unit("alpha", 1, 100.0, 100.0, 0.0)
        .expect("alpha");
    // `alpha`'s content controller is `BuilderOrCommand` -> `AiKind::Builder`
    // for spawned units.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(builder) {
        slot.kind = AiKind::Builder;
    }
    for _ in 0..1200 {
        sim.tick().expect("tick");
    }
    let rebuilt = sim.grid.tiles.get(16, 12).build.is_some();
    assert!(rebuilt, "the builder rebuilt the destroyed wall");
}

/// T14: the `Boost` kind enables `UnitCore.boosting`, which lifts a mech
/// (`BoostAI.defaultBehavior` + `UnitComp.updateBoosting`).
#[test]
fn boost_kind_lifts_mech() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::unit::comp::UnitCore;

    let mut sim = live_sim(48);
    let nova = sim
        .spawn_live_unit("nova", 1, 100.0, 100.0, 0.0)
        .expect("nova");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(nova) {
        slot.kind = AiKind::Boost;
    }
    for _ in 0..20 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs
            .0
            .get::<UnitCore>(nova)
            .is_some_and(|core| core.elevation >= 1.0),
        "the boost kind lifted the mech: {:?}",
        sim.ecs.0.get::<UnitCore>(nova).map(|core| core.elevation)
    );
}

/// NEW-1: a status applied through the live applier ticks damage every tick
/// (`StatusComp.update` in the live unit loop).
#[test]
fn live_status_burning_damages_unit() {
    use crate::combat::damage::status::apply_status;
    let mut sim = live_sim(32);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let burning = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("burning")
        .expect("burning");
    let start = health(&sim, dagger).expect("health");
    apply_status(&mut sim.ecs.0, dagger, burning, 300.0);
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let end = health(&sim, dagger).expect("alive");
    assert!(end < start, "burning ticked live damage ({start} -> {end})");
}

/// NEW-1: a live `RepairFieldAbility` (`poly`) heals a damaged ally through
/// the live ability update in the unit loop.
#[test]
fn live_ability_repair_field_heals() {
    let mut sim = live_sim(48);
    let _poly = sim
        .spawn_live_unit("poly", 1, 200.0, 200.0, 0.0)
        .expect("poly");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 216.0, 200.0, 0.0)
        .expect("dagger");
    sim.ecs
        .0
        .get_mut::<Health>(dagger)
        .expect("dagger health")
        .health = 50.0;
    // Poly's `RepairFieldAbility` reload is 480 ticks (`standard.rs` poly spec).
    for _ in 0..520 {
        sim.tick().expect("tick");
    }
    let end = health(&sim, dagger).expect("alive");
    assert!(end > 50.0, "repair field healed live (50 -> {end})");
}

/// PAR-0137: `UnitComp.speed()` — a ground unit on `deep-water`
/// (`speedMultiplier 0.2`) moves at most 0.25x its stone speed; a flying unit
/// ignores the floor entirely.
#[test]
fn dagger_slowed_on_deep_water() {
    let probe = live_sim(1);
    let content = &probe.unit_runtime().expect("rt").content;
    let stone = content.block_id("stone").expect("stone").get();
    let deep = content.block_id("deep-water").expect("deep-water").get();
    let air = BlockId::AIR.get();
    drop(probe);

    let displacement = |name: &str, floor: u16| -> f32 {
        let mut sim = Sim::new(1, 40, 40, BlockId::new(floor), BlockId::new(air));
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let unit = sim
            .spawn_live_unit(name, 2, 100.0, 100.0, 0.0)
            .expect("unit");
        crate::entities::comp::unit::lifecycle::set_move_target(
            &mut sim.ecs.0,
            unit,
            crate::world::TilePos::new(36, 12),
        );
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        let pos = sim
            .ecs
            .0
            .get::<Pos>(unit)
            .map(|pos| (pos.x, pos.y))
            .expect("pos");
        ((pos.0 - 100.0).powi(2) + (pos.1 - 100.0).powi(2)).sqrt()
    };

    let dagger_stone = displacement("dagger", stone);
    let dagger_water = displacement("dagger", deep);
    let flare_stone = displacement("flare", stone);
    let flare_water = displacement("flare", deep);
    eprintln!(
        "dagger stone {dagger_stone} water {dagger_water}; flare stone {flare_stone} water {flare_water}"
    );
    assert!(
        dagger_water < 0.25 * dagger_stone,
        "deep water slows the dagger ({dagger_water} vs {dagger_stone})"
    );
    assert!(
        (flare_water - flare_stone).abs() < 0.001,
        "flying units ignore the floor ({flare_water} vs {flare_stone})"
    );
}

/// PAR-0140: a MOVE order issued through `SimCommand::UnitCommand` moves a
/// campaign player-team dagger to the world point (the core half of the
/// command-mode -> selection -> order chain).
#[test]
fn commanded_dagger_reaches_target() {
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::{BlockTable, TilePos};

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        waves: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;

    // Materialize a real player core so `checkGameState` keeps the session
    // playing while the order runs (`campaign_wave_timer_spawns_live_units`).
    let table: BlockTable = sim
        .ecs
        .0
        .get_resource::<BlockTable>()
        .expect("table")
        .clone();
    let core_block = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("core-shard")
        .expect("core-shard");
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(&mut sim.ecs.0, 10, TilePos::new(12, 32), 1, 0, 0, 0);
    sim.ecs.0.entity_mut(core).insert(crate::ecs::BuildingComp {
        pos: TilePos::new(12, 32),
        block: core_block,
        team: TeamId(1),
        rot: 0,
    });
    session.teams.register_core(core, TeamId(1), &session.rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    assert_eq!(
        crate::entities::comp::unit::lifecycle::ai_kind_of(&sim.ecs.0, dagger),
        Some(crate::ai::controller::AiKind::Command),
        "player-team campaign unit is commanded"
    );
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 260.0,
        y: 100.0,
        queue: false,
    })
    .expect("move order");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let pos = sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let dist = ((pos.x - 260.0).powi(2) + (pos.y - 100.0).powi(2)).sqrt();
    assert!(
        dist < 16.0,
        "commanded dagger reached the target ({dist} px)"
    );
}

/// H08 T6b: `setUnitCommand` carries the `(0, 0)` wire pair
/// (`command_emit.rs`); switching to the move command must clear the target,
/// not create a move order to world (0, 0) (`InputHandler.setUnitCommand`,
/// InputHandler.java:411-444).
#[test]
fn set_move_command_does_not_move_units() {
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::{BlockTable, TilePos};

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        waves: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    let table: BlockTable = sim
        .ecs
        .0
        .get_resource::<BlockTable>()
        .expect("table")
        .clone();
    let core_block = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("core-shard")
        .expect("core-shard");
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(&mut sim.ecs.0, 10, TilePos::new(12, 32), 1, 0, 0, 0);
    sim.ecs.0.entity_mut(core).insert(crate::ecs::BuildingComp {
        pos: TilePos::new(12, 32),
        block: core_block,
        team: crate::ecs::TeamId(1),
        rot: 0,
    });
    session
        .teams
        .register_core(core, crate::ecs::TeamId(1), &session.rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 0.0,
        y: 0.0,
        queue: false,
    })
    .expect("command switch applies");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let pos = sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let dist = ((pos.x - 100.0).powi(2) + (pos.y - 100.0).powi(2)).sqrt();
    assert!(
        dist < 1.0,
        "command switch did not move the unit ({dist} px)"
    );
}

/// G08 T15: a unit counts as having a command only with a live order target
/// (`CommandAI.hasCommand()` = `targetPos != null`), and
/// `set_command_mode_satisfied` flips the objective resource (G04 T4).
#[test]
fn any_unit_has_command_needs_target() {
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::{CampaignRuntime, ObjectiveInputState};
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        waves: false,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(
        !sim.any_unit_has_command(&[id]),
        "no order yet -> no command"
    );
    assert!(!sim.any_unit_has_command(&[]), "empty selection");
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 200.0,
        y: 100.0,
        queue: false,
    })
    .expect("move order");
    assert!(
        sim.any_unit_has_command(&[id]),
        "move order sets the target"
    );

    sim.set_command_mode_satisfied(true);
    let state = sim
        .ecs
        .0
        .get_resource::<ObjectiveInputState>()
        .expect("input state");
    assert!(state.command_mode_satisfied);
}
