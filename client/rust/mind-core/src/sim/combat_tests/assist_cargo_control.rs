// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Assist, cargo-dispatch, assembler, turret-proxy and control-block regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

/// H01 T12: a team-1 `poly` with the `assist` command escorts the local
/// builder player and copies their build plan (`BuilderAI.java:57-100,143-186`).
/// Before T12 `onlyAssist` returned early for every tick and the poly idled.
#[test]
fn assist_poly_follows_building_player() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
    use crate::world::ConstructState;

    let mut sim = live_sim(64);
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
    // A core funds the build (`shouldSkip`) and the campaign unit cap
    // (`unitCap 0 + core +8`).
    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.item_id("copper").expect("copper")
    };
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(core) {
        items.add(copper, 500, 4000);
    }

    let player = sim
        .spawn_player_unit("alpha", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("player alpha");
    let poly = sim
        .spawn_live_unit("poly", 1, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("poly");
    let assist = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .unit_command_by_name("assist")
            .expect("assist")
            .id
    };
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(poly) {
        slot.kind = AiKind::Command;
        slot.command = Some(assist);
    }

    // Phase 1: the player is idle -> `moveTo(assistFollowing, hit + hit/2 + 60)`.
    let start = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let ppos = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    let apos = *sim.ecs.0.get::<Pos>(player).expect("player pos");
    let dst = ((ppos.x - apos.x).powi(2) + (ppos.y - apos.y).powi(2)).sqrt();
    eprintln!("assist escort: poly {ppos:?} player {apos:?} dst {dst:.1} start {start:?}");
    assert!(
        ppos.x != start.x || ppos.y != start.y,
        "the assist poly left its idle spot"
    );
    // `assistFollowing.type.hitSize + unit.type.hitSize/2 + 60` (+ the `moving`
    // tolerance of 5): alpha hit 8, poly hit 9.
    assert!(
        dst <= 8.0 + 9.0 / 2.0 + 65.0,
        "the poly escorts the player ({dst})"
    );

    // Phase 2: queue the player's wall plan one tile east; the alpha starts the
    // construct (`updateBuildLogic`), then the cost is frozen so the plan stays
    // open for the whole acceptance window.
    let wall = BlockId::new(block(&sim, "copper-wall"));
    let plan = UnitBuildPlan::place(41, 40, 0, wall);
    sim.ecs
        .0
        .get_mut::<BuilderPlans>(player)
        .expect("player plans")
        .plans
        .push_back(plan);
    let mut construct = None;
    for _ in 0..30 {
        sim.tick().expect("tick");
        if let Some(entity) = sim.grid.tiles.get(41, 40).build
            && sim.ecs.0.get::<ConstructState>(entity).is_some()
        {
            construct = Some(entity);
            break;
        }
    }
    let construct = construct.expect("the alpha started the construct");
    sim.ecs
        .0
        .get_mut::<ConstructState>(construct)
        .expect("construct state")
        .build_cost = 1_000_000.0;
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let player_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(player)
        .and_then(|plans| plans.plans.front().cloned());
    let poly_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(poly)
        .and_then(|plans| plans.plans.front().cloned());
    // The per-unit plan progress mirrors diverge as each builder works; the
    // plan identity (tile/block/rotation/mode) is what is copied.
    let plan_id = |plan: Option<UnitBuildPlan>| {
        plan.map(|plan| (plan.x, plan.y, plan.block, plan.rotation, plan.breaking))
    };
    eprintln!("assist copy: player {player_plan:?} poly {poly_plan:?}");
    assert!(
        player_plan.is_some(),
        "the player is still actively building"
    );
    assert_eq!(
        plan_id(poly_plan),
        plan_id(player_plan),
        "the poly mimics the player's front plan"
    );
    // The poly reached the construct's range (buildRange - hitSize*2 = 202).
    let plan_tile = (41.0 * 8.0, 40.0 * 8.0);
    let ppos = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    let plan_dst = ((ppos.x - plan_tile.0).powi(2) + (ppos.y - plan_tile.1).powi(2)).sqrt();
    assert!(
        plan_dst <= 202.0 + 12.0,
        "the poly moved into build range of the plan ({plan_dst})"
    );
}

/// H01 T12: an idle `rebuild` poly (the poly default command, `onlyAssist`
/// false) picks an actively-building unit within `buildRadius` and copies its
/// plan (`BuilderAI.java:149-169`).
#[test]
fn idle_rebuild_poly_helps_builder() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::builder::{BuilderAiState, DEFAULT_REBUILD_PERIOD};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
    use crate::world::ConstructState;

    let mut sim = live_sim(64);
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
    // A core funds the build (`shouldSkip`) and the campaign unit cap
    // (`unitCap 0 + core +8`).
    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.item_id("copper").expect("copper")
    };
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(core) {
        items.add(copper, 500, 4000);
    }

    let player = sim
        .spawn_player_unit("alpha", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("player alpha");
    let helper = sim
        .spawn_live_unit("poly", 1, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("helper poly");
    // `slot.command = None` -> the poly's `defaultCommand = rebuild`.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(helper) {
        slot.kind = AiKind::Command;
    }

    let wall = BlockId::new(block(&sim, "copper-wall"));
    let plan = UnitBuildPlan::place(41, 40, 0, wall);
    sim.ecs
        .0
        .get_mut::<BuilderPlans>(player)
        .expect("player plans")
        .plans
        .push_back(plan);
    let mut construct = None;
    for _ in 0..30 {
        sim.tick().expect("tick");
        if let Some(entity) = sim.grid.tiles.get(41, 40).build
            && sim.ecs.0.get::<ConstructState>(entity).is_some()
        {
            construct = Some(entity);
            break;
        }
    }
    let construct = construct.expect("the alpha started the construct");
    sim.ecs
        .0
        .get_mut::<ConstructState>(construct)
        .expect("construct state")
        .build_cost = 1_000_000.0;

    let start = *sim.ecs.0.get::<Pos>(helper).expect("helper pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }

    let state = sim
        .ecs
        .0
        .get::<BuilderAiState>(helper)
        .copied()
        .expect("builder state");
    let player_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(player)
        .and_then(|plans| plans.plans.front().cloned());
    let helper_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(helper)
        .and_then(|plans| plans.plans.front().cloned());
    let hpos = *sim.ecs.0.get::<Pos>(helper).expect("helper pos");
    let plan_tile = (41.0 * 8.0, 40.0 * 8.0);
    let plan_dst = ((hpos.x - plan_tile.0).powi(2) + (hpos.y - plan_tile.1).powi(2)).sqrt();
    eprintln!(
        "rebuild help: following {:?} plan_dst {plan_dst:.1} start {start:?} state \
         {state:?}",
        state.following
    );
    assert_eq!(
        state.following,
        Some(player),
        "the idle rebuild poly follows the building player"
    );
    // The per-unit plan progress mirrors diverge as each builder works; the
    // plan identity (tile/block/rotation/mode) is what is copied.
    let plan_id = |plan: Option<UnitBuildPlan>| {
        plan.map(|plan| (plan.x, plan.y, plan.block, plan.rotation, plan.breaking))
    };
    assert_eq!(
        plan_id(helper_plan),
        plan_id(player_plan),
        "the helper poly copies the player's front plan"
    );
    assert!(
        plan_dst <= 202.0 + 12.0,
        "the helper moved into build range of the plan ({plan_dst})"
    );
    assert_eq!(
        state.rebuild_period, DEFAULT_REBUILD_PERIOD,
        "no buildAi rule -> default rebuild period"
    );
}

/// H01 T13: a `manifold` (CargoAI) is dispatched to `AiKind::Cargo` instead of
/// the generic flying branch. Without a tether the controller early-returns
/// (`CargoAI.java:28`), so the unit stays put; the generic FlyingAI would have
/// closed on the enemy core.
#[test]
fn manifold_dispatches_cargo_ai() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(64);
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
    // A team-1 core satisfies the campaign unit cap (`unitCap 0 + core +8`).
    let _core = place(&mut sim, 5, 5, "core-shard", 1);
    let _enemy_core = place(&mut sim, 55, 55, "core-shard", 2);
    let manifold = sim
        .spawn_live_unit("manifold", 1, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("manifold");
    let start = *sim.ecs.0.get::<Pos>(manifold).expect("manifold pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let kind = sim
        .ecs
        .0
        .get::<ControllerSlot>(manifold)
        .map(|slot| slot.kind);
    assert_eq!(kind, Some(AiKind::Cargo), "the manifold keeps CargoAI");
    let pos = *sim.ecs.0.get::<Pos>(manifold).expect("manifold alive");
    assert_eq!(
        (pos.x, pos.y),
        (start.x, start.y),
        "no tether -> the cargo unit never paths to the enemy core"
    );
}

/// H01 T13: an `assembly-drone` (AssemblerAI) is dispatched and reaches the
/// spot the assembler arranges (`AssemblerAI.java:12-24`).
#[test]
fn assembly_drone_dispatches() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::assembler::AssemblerAi;

    let mut sim = live_sim(64);
    let _enemy_core = place(&mut sim, 55, 55, "core-shard", 2);
    let drone = sim
        .spawn_live_unit("assembly-drone", 1, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("assembly-drone");
    assert_eq!(
        sim.ecs.0.get::<ControllerSlot>(drone).map(|slot| slot.kind),
        Some(AiKind::Assembler),
        "the drone's content controller is AssemblerAI"
    );
    // The assembler block arranges the drone around its spawn perimeter
    // (`UnitAssembler.java:509-513`); the port writes the same state directly
    // until the block half lands.
    sim.ecs.0.entity_mut(drone).insert(AssemblerAi {
        unit: Some(drone),
        target_x: 26.0 * 8.0,
        target_y: 26.0 * 8.0,
        target_angle: 90.0,
    });
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(drone).expect("drone alive");
    let dst = ((pos.x - 26.0 * 8.0).powi(2) + (pos.y - 26.0 * 8.0).powi(2)).sqrt();
    let rotation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(drone)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    eprintln!(
        "drone at ({}, {}) rotation {rotation} dst {dst}",
        pos.x, pos.y
    );
    assert!(dst <= 10.0, "the drone reached its arranged spot ({dst})");
    assert!(
        (rotation - 90.0).abs() < 0.001 || (rotation - 450.0).abs() < 0.001,
        "the drone faces the arranged angle ({rotation})"
    );
}

/// I01 T12: every turret carries the detached `block` unit proxy
/// (`TurretBuild.unit = UnitTypes.block.create(team)`, `Turret.java:297`),
/// positioned on the turret each tick (`unit.tile(this)`, `Turret.java:535-537`)
/// and dropped with the building. The proxy is never in the live unit list, so
/// it is never saved.
#[test]
fn turret_has_block_unit() {
    use crate::ecs::EntitySeq;
    use crate::entities::comp::unit::comp::{BlockUnitComp, UnitTypeComp};
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    let unit = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .block_unit
        .expect("block unit proxy");
    // `UnitTypes.block`.
    let runtime = sim.unit_runtime().expect("runtime");
    let type_id = sim
        .ecs
        .0
        .get::<UnitTypeComp>(unit)
        .expect("proxy type")
        .type_id;
    assert_eq!(
        runtime.content.unit(type_id).expect("block def").name,
        "block"
    );
    // `unit.tile(this)`: the proxy sits on the turret's tile.
    let block_tile = sim
        .ecs
        .0
        .get::<Building>(duo)
        .expect("building")
        .tile
        .pack();
    assert_eq!(
        sim.ecs
            .0
            .get::<BlockUnitComp>(unit)
            .expect("block unit comp")
            .tile,
        block_tile
    );
    // Team + position follow the turret.
    let (bx, by) = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|pos| (pos.x, pos.y))
        .expect("turret pos");
    let (ux, uy) = sim
        .ecs
        .0
        .get::<Pos>(unit)
        .map(|pos| (pos.x, pos.y))
        .expect("proxy pos");
    assert_eq!((ux, uy), (bx, by), "proxy sits on the turret");
    assert_eq!(sim.ecs.0.get::<TeamComp>(unit).expect("proxy team").team, 1);
    assert!(
        !runtime.units.contains(&unit),
        "the proxy is detached (never saved / never unit-updated)"
    );
    let _ = sim.ecs.0.get::<EntitySeq>(unit).expect("proxy seq");

    // Removing the turret removes the proxy.
    sim.command(SimCommand::Break {
        x: 20,
        y: 20,
        player: None,
    })
    .expect("break");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(unit).is_err(),
        "proxy despawned with the turret"
    );
}

/// I01 T12 (`Turret.java:571-600`): a possessed turret aims at the player's
/// cursor and fires on the shoot button (`controlled()` -> `targetPos.set(
/// unit.aimX(), unit.aimY()); canShoot = unit.isShooting()`).
#[test]
fn controlled_turret_shoots_at_aim() {
    use crate::ecs::EntitySeq;
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim.set_player_team(1);
    let unit = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .block_unit
        .expect("block unit proxy");
    let seq = sim.ecs.0.get::<EntitySeq>(unit).expect("proxy seq").0 as i32;
    assert!(sim.possess_unit(seq), "player possesses the turret proxy");
    assert_eq!(sim.player_unit(), Some(unit));

    // Aim east of the turret (its default rotation is 90/south) and hold fire.
    let (bx, by) = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|pos| (pos.x, pos.y))
        .expect("turret pos");
    let before = sim.unit_runtime().expect("runtime").bullets_created;
    for _ in 0..120 {
        sim.set_player_control(0.0, 0.0, true, bx + 120.0, by, false, false);
        sim.tick().expect("tick");
    }
    let after = sim.unit_runtime().expect("runtime").bullets_created;
    assert!(
        after > before,
        "the controlled turret fired ({before} -> {after})"
    );
    let rotation = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .rotation;
    assert!(
        (rotation.rem_euclid(360.0)).abs() < 5.0
            || (rotation.rem_euclid(360.0) - 360.0).abs() < 5.0,
        "the turret turned toward the cursor ({rotation})"
    );

    // Releasing the control stops the player-driven fire (the turret drops the
    // `Player` marker).
    sim.clear_unit();
    assert!(sim.player_unit().is_none());
}

/// I01 T12 (`CoreBlock.java:619-637`): Ctrl-click on the own core clears the
/// player's unit and respawns it at that core (`player.clearUnit();
/// player.deathTimer = deathDelay + 1; requestSpawn(player)`).
#[test]
fn core_control_select_respawns() {
    let mut sim = live_sim(48);
    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let alpha = sim
        .spawn_player_unit("alpha", 1, 200.0, 200.0, 0.0)
        .expect("alpha");
    assert_eq!(sim.player_unit(), Some(alpha));

    sim.command(SimCommand::BuildingControlSelect { x: 10, y: 10 })
        .expect("control select");
    sim.tick().expect("tick");
    let player = sim.player_unit().expect("respawned player");
    assert_ne!(player, alpha, "the old unit was released");
    let (cx, cy) = sim
        .ecs
        .0
        .get::<Pos>(core)
        .map(|pos| (pos.x, pos.y))
        .expect("core pos");
    let (px, py) = sim
        .ecs
        .0
        .get::<Pos>(player)
        .map(|pos| (pos.x, pos.y))
        .expect("player pos");
    assert!(
        (px - cx).abs() < 1.0 && (py - cy).abs() < 1.0,
        "respawned at the selected core ({px}, {py}) vs ({cx}, {cy})"
    );
}

/// J03 T2: a freshly placed idle duo draws its head facing up.
///
/// `BaseTurretBuild.rotation = 90` (`BaseTurret.java:47`) and the barrel angle
/// is `drawrot() = rotation - 90 = 0` (`Turret.java:327-329`); with no ammo the
/// turret never turns (`Turret.updateTile` gates targeting on `hasAmmo`,
/// `Turret.java:571-605`). Pins the sim half of the drawn angle: a no-ammo duo
/// keeps `rotation = 90` (drawrot 0) across 60 ticks.
#[test]
fn placed_duo_idle_head_rotation_is_up() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    assert_eq!(
        sim.ecs
            .0
            .get::<TurretState>(duo)
            .expect("turret state")
            .rotation,
        90.0,
        "BaseTurretBuild starts at rotation 90"
    );
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    assert_eq!(state.total_ammo, 0, "no ammo");
    assert!(state.target.is_none(), "no target");
    assert_eq!(state.rotation, 90.0, "idle turret stays at 90");
    // The renderer draws at `rotation - 90` (blocks.rs / render/turret.rs).
    let drawrot = state.rotation - 90.0;
    assert!(
        drawrot.abs() < 1e-6,
        "idle head drawrot is 0 (up): {drawrot}"
    );
}

/// L06 T1: `UnitAssemblerBuild.wasOccupied = checkSolid(getUnitSpawn(), false)`
/// (`UnitAssembler.java:515,679-685`) with the vanilla `areaSize = 13`
/// (`Blocks.java:6566`): a solid tile or a grounded unit on the spawn point
/// occupies a ground output; a flyer or a `spawnedByCore` unit does not.
#[test]
fn assembler_output_area_occupancy() {
    use crate::world::blocks::units::UnitAssembler;
    let mut sim = live_sim(64);
    let assembler = place(&mut sim, 20, 20, "tank-assembler", 1);
    sim.tick().expect("tick");
    let occupied = |sim: &Sim| {
        sim.ecs
            .0
            .get::<UnitAssembler>(assembler)
            .expect("assembler")
            .was_occupied
    };
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitAssembler>(assembler)
            .map(|a| a.area_size),
        Some(13)
    );
    assert!(!occupied(&sim), "free output area");
    // Spawn point: rotation 0, `x + tilesize * (13 + 5) / 2` = tile (29, 20).
    let (sx, sy) = (29.0 * 8.0, 20.0 * 8.0);
    let flare = sim.spawn_live_unit("flare", 1, sx, sy, 0.0).expect("flare");
    sim.tick().expect("tick");
    assert!(!occupied(&sim), "a flyer does not block a ground output");
    sim.ecs.0.despawn(flare);
    let dagger = sim
        .spawn_live_unit("dagger", 2, sx, sy, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(dagger)
    {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    sim.tick().expect("tick");
    assert!(occupied(&sim), "a grounded unit on the spawn point");
    if let Some(mut core) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::unit::comp::UnitCore>(dagger)
    {
        core.spawned_by_core = true;
    }
    sim.tick().expect("tick");
    assert!(!occupied(&sim), "spawnedByCore units are ignored");
    sim.ecs.0.despawn(dagger);
    let _wall = place(&mut sim, 29, 20, "copper-wall", 1);
    sim.tick().expect("tick");
    assert!(occupied(&sim), "a solid tile under a ground output");
}

/// L06 T1: `UnitAssemblerBuild.spawned()` (`UnitAssembler.java:556-574`): the
/// product takes `commandPos` (`CommandAI.commandPosition`) and goes into the
/// same-team payload block under the spawn point when it accepts it
/// (`unit.buildOn()`); otherwise it is added to the world.
#[test]
fn assembled_unit_takes_command_pos_and_payload_handoff() {
    use crate::world::blocks::payloads::PayloadHolder;
    use crate::world::blocks::units::behavior::deliver_assembled_unit;
    let mut sim = live_sim(64);
    // Team 1 is the player's (`rules.defaultTeam`), so products get `CommandAI`
    // (`UnitType.controller`, UnitType.java:283).
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let conveyor = place(&mut sim, 10, 10, "payload-conveyor", 1);
    let def = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .cloned()
        .expect("dagger");
    let in_runtime = |sim: &Sim, unit: Entity| {
        sim.unit_runtime()
            .is_some_and(|runtime| runtime.units.contains(&unit))
    };
    // Payload conveyor at tile (10, 10) (size 3, centred) under the spawn.
    let (unit, handed) = deliver_assembled_unit(
        &mut sim.ecs.0,
        &def,
        1,
        80.0,
        80.0,
        0.0,
        Some((400.0, 240.0)),
    );
    assert!(handed, "handed to the payload conveyor");
    assert!(!in_runtime(&sim, unit), "a payload unit is not added");
    let held = sim
        .ecs
        .0
        .get::<PayloadHolder>(conveyor)
        .and_then(|holder| holder.payload);
    assert_eq!(held.and_then(|p| p.entity), Some(unit), "conveyor holds it");
    // Open ground: `unit.add()`.
    let (free, handed) = deliver_assembled_unit(
        &mut sim.ecs.0,
        &def,
        1,
        300.0,
        300.0,
        0.0,
        Some((400.0, 240.0)),
    );
    assert!(!handed);
    assert!(in_runtime(&sim, free), "added to the world");
    let slot = sim
        .ecs
        .0
        .get::<crate::ai::controller::ControllerSlot>(free)
        .cloned()
        .expect("controller");
    if slot.kind == crate::ai::controller::AiKind::Command {
        assert_eq!(
            slot.target,
            Some(crate::world::TilePos::new(50, 30)),
            "commandPos -> move target"
        );
    } else {
        panic!(
            "a player-team product should be commandable: {:?}",
            slot.kind
        );
    }
    // An enemy-team payload block never takes it.
    let enemy = place(&mut sim, 40, 40, "payload-conveyor", 2);
    let (_, handed) = deliver_assembled_unit(&mut sim.ecs.0, &def, 1, 320.0, 320.0, 0.0, None);
    assert!(!handed, "team check");
    assert!(
        sim.ecs
            .0
            .get::<PayloadHolder>(enemy)
            .and_then(|holder| holder.payload)
            .is_none()
    );
}

/// L06 T2: `RouterBuild implements ControlBlock` (`Router.java:30-53`): an own
/// size-1 router yields a lazily created, stable `block` proxy
/// (`unit()`, `:37-44`); the distributor (size 2) cannot be controlled
/// (`canControl() = size == 1`); turrets keep theirs (`playerControllable`).
#[test]
fn router_is_a_control_block() {
    let mut sim = live_sim(48);
    sim.set_player_team(1);
    let router = place(&mut sim, 10, 10, "router", 1);
    let _distributor = place(&mut sim, 20, 20, "distributor", 1);
    let _duo = place(&mut sim, 30, 30, "duo", 1);
    let _enemy = place(&mut sim, 40, 40, "router", 2);
    let pos = |x: i16, y: i16| crate::world::TilePos::new(x, y);
    let first = sim
        .control_block_unit_id(pos(10, 10))
        .expect("router proxy");
    assert_eq!(
        sim.control_block_unit_id(pos(10, 10)),
        Some(first),
        "stable"
    );
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::router::RouterBuild>(router)
        .and_then(|state| state.block_unit)
        .expect("proxy stored");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::BlockUnitComp>(proxy)
            .is_some()
    );
    assert!(
        !sim.unit_runtime().expect("runtime").units.contains(&proxy),
        "never added to the unit pass"
    );
    assert_eq!(sim.control_block_unit_id(pos(20, 20)), None, "distributor");
    assert!(sim.control_block_unit_id(pos(30, 30)).is_some(), "duo");
    assert_eq!(sim.control_block_unit_id(pos(40, 40)), None, "enemy team");
}

/// L06 T2: a possessed router (`Router.java:102-117`) routes only toward the
/// aimed quadrant and only while shooting; unpossessed it round-robins.
#[test]
fn possessed_router_routes_toward_aim_while_shooting() {
    use crate::ai::types::player::PlayerInput;
    use crate::world::blocks::distribution::transfer;
    let mut sim = live_sim(48);
    sim.set_player_team(1);
    let router = place(&mut sim, 10, 10, "router", 1);
    let east = place(&mut sim, 11, 10, "container", 1);
    let north = place(&mut sim, 9, 11, "container", 1);
    let copper = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .item_id("copper")
        .expect("copper");
    let proxy_seq = sim
        .control_block_unit_id(crate::world::TilePos::new(10, 10))
        .expect("proxy");
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::router::RouterBuild>(router)
        .and_then(|state| state.block_unit)
        .expect("proxy");
    // `InputHandler.unitControl` on the proxy (the Ctrl+click path).
    assert!(sim.possess_unit(proxy_seq), "possess the router proxy");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::Player>(proxy)
            .is_some()
    );
    let set_input = |sim: &mut Sim, shooting: bool| {
        let mut input = sim
            .ecs
            .0
            .remove_resource::<PlayerInput>()
            .unwrap_or_default();
        // Aim at tile (14, 10): due east of the router.
        input.aim_x = 14.0 * 8.0;
        input.aim_y = 10.0 * 8.0;
        input.shooting = shooting;
        sim.ecs.0.insert_resource(input);
    };
    let stored = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::ItemModule>(e)
            .map_or(0, |items| items.get(copper))
    };
    set_input(&mut sim, false);
    transfer::dispatch_handle_item(&mut sim.ecs.0, router, router, copper);
    for _ in 0..30 {
        set_input(&mut sim, false);
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored(&sim, east) + stored(&sim, north),
        0,
        "holds while not shooting"
    );
    assert_eq!(stored(&sim, router), 1);
    for _ in 0..30 {
        set_input(&mut sim, true);
        sim.tick().expect("tick");
    }
    assert_eq!(stored(&sim, east), 1, "aimed side");
    assert_eq!(stored(&sim, north), 0, "never the other side");
}
