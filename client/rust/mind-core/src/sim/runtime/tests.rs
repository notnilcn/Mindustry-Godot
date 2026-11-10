// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Runtime tests: content config lowering, core item counting, map
//! materialization, item traits, consumer damage and the dynamic-consume bridge.
//!
//! Upstream: none (port-only tests).

use super::*;
use crate::content::{
    BlockId, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use crate::ecs::BuildingComp;
use crate::game::play::PlaySession;
use crate::game::rules::Rules;
use crate::game::runtime::sync_session_with_sim;
use crate::sim::Sim;
#[cfg(feature = "msav-import")]
use crate::sim::test_support::tick_n;
use crate::world::TilePos;
use crate::world::building_io::DecodedBase;
use crate::world::limits::BuildRules;
use crate::world::modules::ItemModule;
use bevy_ecs::world::World;
fn registry() -> ContentRegistry {
    create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content boot")
}

/// `Trigger.blastGenerator` counter (`TriggerRegistry` listeners are plain
/// fn pointers, so the GAPS #58 drain test counts through a module static).
static BLAST_GENERATOR_FIRES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_blast_generator(_world: &mut World) {
    BLAST_GENERATOR_FIRES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// T4: the reconstructor's command panel submits `("content", "<command>")`;
/// `resolve_config` must lower the name to the typed `UnitCommand` content
/// value (`Reconstructor.config(UnitCommand.class, ...)`).
#[test]
fn content_config_resolves_unit_commands() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let value = sim
        .block_runtime()
        .expect("runtime")
        .resolve_config(&crate::determinism::ConfigValue::Content(
            "rebuild".to_owned(),
        ))
        .expect("resolve");
    match value {
        crate::world::config::ConfigValue::Content(
            crate::content::ContentType::UnitCommand,
            id,
        ) => assert_eq!(
            sim.block_runtime()
                .expect("runtime")
                .content()
                .get_by_name(crate::content::ContentType::UnitCommand, "rebuild")
                .map(|reference| reference.id),
            Some(id)
        ),
        other => panic!("expected a unit command, got {other:?}"),
    }
}

/// `CoreBlock.handleItem` bumps `stats.coreItemCount` for default-team
/// intakes (belt/dump transfers), while `handleStack` (the player offload
/// path) does not.
#[test]
fn core_item_count_counts_transfers_not_the_player_offload() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::blocks::distribution::transfer::{
        dispatch_handle_item, dispatch_handle_stack,
    };

    let content = registry();
    let core = content.block_id("core-shard").expect("core-shard");
    let copper = content.item_id("copper").expect("copper");
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .default_team = 1;
    let pos = TilePos::new(5, 6);
    assert!(
        sim.apply(crate::command::Command::Place {
            x: 5,
            y: 6,
            block: core,
        })
        .is_ok()
    );
    let core_entity = sim.grid.entity_at(pos).expect("core entity");
    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    sim.ecs.0.insert_resource(CampaignRuntime::new(
        PlaySession::new(Rules {
            default_team: 1,
            ..Rules::default()
        }),
        campaign,
    ));

    dispatch_handle_item(&mut sim.ecs.0, core_entity, core_entity, copper);
    let after_item = sim.ecs.0.get_resource::<CampaignRuntime>().map(|runtime| {
        runtime
            .session
            .stats
            .core_item_count
            .get("copper")
            .copied()
            .unwrap_or(0)
    });
    assert_eq!(after_item, Some(1), "belt/dump intake counts");
    dispatch_handle_stack(&mut sim.ecs.0, core_entity, copper, 2);
    let after_stack = sim.ecs.0.get_resource::<CampaignRuntime>().map(|runtime| {
        runtime
            .session
            .stats
            .core_item_count
            .get("copper")
            .copied()
            .unwrap_or(0)
    });
    assert_eq!(after_stack, Some(1), "offload stack path does not count");
}

/// A loaded map carries tile blocks with no ECS entities; materializing the
/// reader's decoded payload queue must link a live building and make the
/// session sync see the map's default-team core (EV-0037).
#[test]
fn materialize_links_loaded_map_buildings() {
    let content = registry();
    let core = content.block_id("core-shard").expect("core-shard");
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let pos = TilePos::new(5, 6);
    // `MapIO`/`Context` writes the block without the entity.
    sim.grid.set_block(pos, core, 1, 0).expect("tile block");
    assert_eq!(sim.group_counts().get("build"), Some(&0));

    let pending = [(
        sim.grid.index(pos).expect("index"),
        DecodedBase {
            team: 1,
            rotation: 0,
            health: 250.0,
            ..DecodedBase::default()
        },
    )];
    assert_eq!(sim.materialize_map_buildings(&pending), 1);
    assert_eq!(sim.group_counts().get("build"), Some(&1));
    let entity = sim.grid.entity_at(pos).expect("entity linked");
    let comp = sim.ecs.0.get::<BuildingComp>(entity).expect("BuildingComp");
    assert_eq!(comp.team.0, 1);
    assert_eq!(comp.block, core);
    assert_eq!(comp.pos, pos);

    let mut session = PlaySession::new(Rules {
        waves: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    let sync = sync_session_with_sim(&mut session, &mut sim.ecs.0, &content, 0, false);
    assert_eq!(sync.player_cores, 1);
    assert_eq!(session.player_core_count(), 1);
    // Placements carry `session.default_team` (`Team.sharded`), not the
    // `BuildRules` default (derelict).
    assert_eq!(
        sim.ecs
            .0
            .get_resource::<BuildRules>()
            .map(|rules| rules.default_team),
        Some(1),
        "build rules follow the session default team"
    );
}

/// A materialized drill must get its grid-derived `dominantItem` back, or a
/// loaded save's drill stays inert (`Drill.countOre`; EV-0065).
#[test]
fn materialize_refreshes_drill_ore() {
    use crate::entities::comp::building::DrillState;
    use crate::world::TilePos;

    let content = registry();
    let drill = content.block_id("mechanical-drill").expect("drill");
    let ore = content.block_id("ore-copper").expect("ore-copper");
    let copper = content.item_id("copper").expect("copper");
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let pos = TilePos::new(5, 5);
    // The reader writes the block and tile data without the ECS entity.
    sim.grid.set_block(pos, drill, 1, 0).expect("tile block");
    let index = sim.grid.index(pos).expect("index");
    sim.grid.tiles.geti_mut(index).overlay = ore;
    let extracted = sim.materialize_map_buildings(&[(
        index,
        DecodedBase {
            team: 1,
            rotation: 0,
            health: 80.0,
            ..DecodedBase::default()
        },
    )]);
    assert_eq!(extracted, 1);
    let entity = sim.grid.entity_at(pos).expect("entity linked");
    let state = sim.ecs.0.get::<DrillState>(entity).expect("DrillState");
    assert_eq!(state.dominant_item, Some(copper));
    assert_eq!(state.dominant_items, 1, "one ore tile under the 2x2 drill");
}

/// The evaluator's EV-0047 repro at the core level: load the vendored
/// Ground Zero map, materialize its buildings, place a mechanical drill on
/// map ore (stored in `tile.overlay`) beside a core and tick. The drill
/// must deliver copper into the core's team-shared inventory. Skips without
/// the vendored asset (`msav-import` feature only).
#[cfg(feature = "msav-import")]
#[test]
fn ground_zero_drill_feeds_the_materialized_core() {
    use crate::editor::context::EditorContext;
    use crate::io::save::{SaveIo, SaveReadState};
    use std::path::Path;

    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = registry();
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let (result, pending) = {
        let mut context = EditorContext::new(&mut grid, &content);
        let (result, pending) = {
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut content),
                ..SaveReadState::default()
            };
            let result = SaveIo::load_bytes(&bytes, &mut state);
            (result, std::mem::take(&mut context.pending_buildings))
        };
        (result, pending)
    };
    result.expect("map imports");
    grid.end_map_load(&content);

    let ore = content.block_id("ore-copper").expect("ore-copper");
    let core_block = content.block_id("core-shard").expect("core");
    let drill = content.block_id("mechanical-drill").expect("drill");
    let width = grid.tiles.width;
    let height = grid.tiles.height;
    let air = BlockId::AIR;
    // 2x2 ore footprint with a clear 6x4 rectangle around it for the core.
    let mut spot = None;
    'outer: for y in 2..(height - 5) {
        for x in 2..(width - 7) {
            let mut ok = true;
            for dx in 0..6 {
                for dy in 0..4 {
                    let in_ore = (0..2).contains(&dx) && (0..2).contains(&dy);
                    let t = grid.tile(x + dx, y + dy);
                    if t.block != air {
                        ok = false;
                    }
                    if in_ore && t.overlay != ore {
                        ok = false;
                    }
                    if !in_ore && t.overlay != air && dx > 1 {
                        ok = false;
                    }
                }
            }
            if ok {
                spot = Some((x, y + 1));
                break 'outer;
            }
        }
    }
    let Some((x, y)) = spot else {
        panic!("no ore patch with clear core space");
    };

    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    sim.install_block_runtime().expect("runtime");
    assert!(sim.materialize_map_buildings(&pending) > 0);
    // Team 1: stage the core through the API form (`player: None`) so the
    // player core-placement gate (`CoreBlock.canPlaceOn`, H02 T4) does not
    // reject the fixture; a bare `Command::Place` would carry team 0
    // (derelict) and derelict buildings no longer update (H09 T4).
    sim.command(crate::determinism::SimCommand::Place {
        x: (x + 3) as i16,
        y: y as i16,
        block: core_block.raw(),
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("core command");
    sim.place_for_team(x as i16, y as i16, drill, 0, 1);
    let core_entity = sim
        .grid
        .entity_at(TilePos::new((x + 3) as i16, y as i16))
        .expect("core entity");
    let drill_entity = sim
        .grid
        .entity_at(TilePos::new(x as i16, y as i16))
        .expect("drill entity");
    let copper = content.item_id("copper").expect("copper");
    // `CoreBlock.CoreBuild` stores into the team-shared inventory
    // (`CoreBlock.java:764-780`), not the core entity's own `ItemModule`.
    // Let the drill warm up and mine (mechanical drill + 4-ore footprint is
    // roughly one copper per 650 ticks), then read the team pool.
    tick_n(&mut sim, 1800);
    let stored =
        crate::world::blocks::storage::team_items::core_item_count(&sim.ecs.0, core_entity, copper);
    assert!(
        stored >= 1,
        "core received no copper from the map-ore drill; drill entity {drill_entity:?}"
    );
}

/// Without the live runtime the payload queue is inert (P0 spine unchanged).
#[test]
fn materialize_is_a_noop_without_the_runtime() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    assert_eq!(
        sim.materialize_map_buildings(&[(0, DecodedBase::default())]),
        0
    );
}

/// GAPS #58: `install_block_runtime_from` installs the [`ItemTraits`] table
/// the trait filters (`ConsumeItemFlammable`/`Radioactive`/`Explosive`/
/// `Charged`) read; without it every trait-filtered item is rejected.
#[test]
fn live_runtime_installs_item_traits() {
    let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::world::consumers::ItemTraits>()
            .is_none()
    );
    sim.install_block_runtime().expect("runtime");
    let traits = sim
        .ecs
        .0
        .get_resource::<crate::world::consumers::ItemTraits>()
        .expect("item traits installed");
    let content = sim.block_runtime().expect("runtime").content();
    for item in content.items() {
        assert_eq!(traits.flammability_of(item.id), item.flammability);
        assert_eq!(traits.radioactivity_of(item.id), item.radioactivity);
        assert_eq!(traits.explosiveness_of(item.id), item.explosiveness);
        assert_eq!(traits.charge_of(item.id), item.charge);
    }
    // Vanilla sanity: `blast-compound` is explosive, `coal` flammable.
    let blast = content.item_id("blast-compound").expect("blast-compound");
    let coal = content.item_id("coal").expect("coal");
    assert!(traits.explosiveness_of(blast) > 0.0);
    assert!(traits.flammability_of(coal) > 0.0);
}

/// GAPS #58: the building pass queues `ConsumeItemExplode` damage in
/// [`PendingConsumerDamage`]; `Sim::tick` drains it through
/// `Building.damage` (health + `Trigger.blastGenerator`).
#[test]
fn consumer_damage_queue_is_drained_by_tick() {
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
    assert!(sim.place_for_team(6, 6, wall, 0, 1), "wall placed");
    let entity = sim.grid.entity_at(TilePos::new(6, 6)).expect("wall entity");
    let before = sim
        .ecs
        .0
        .get::<crate::entities::comp::Health>(entity)
        .expect("health")
        .health;
    // `TriggerRegistry` listeners are plain `fn(&mut World)` pointers, so a
    // module static counts the `Trigger.blastGenerator` fire.
    BLAST_GENERATOR_FIRES.store(0, std::sync::atomic::Ordering::SeqCst);
    sim.triggers
        .on(super::super::Trigger::BlastGenerator, count_blast_generator);
    sim.ecs
        .0
        .insert_resource(crate::world::consumers::PendingConsumerDamage(vec![(
            entity, 25.0,
        )]));
    sim.tick().expect("tick");
    let after = sim
        .ecs
        .0
        .get::<crate::entities::comp::Health>(entity)
        .expect("health")
        .health;
    assert!(
        (before - after - 25.0).abs() < 1e-3,
        "queued ConsumeItemExplode damage applied ({before} -> {after})"
    );
    assert!(
        !sim.ecs
            .0
            .contains_resource::<crate::world::consumers::PendingConsumerDamage>(),
        "queue drained"
    );
    assert!(
        BLAST_GENERATOR_FIRES.load(std::sync::atomic::Ordering::SeqCst) >= 1,
        "Trigger.blastGenerator fired"
    );
}

/// GAPS #58: the [`SimDynamicConsume`] bridge resolves the Java closures —
/// a `UnitFactory`'s selected plan requirements (`ConsumeItemDynamic`).
#[test]
fn dynamic_consume_bridge_reads_factory_plan_items() {
    use crate::world::consumers::{ConsumeInstance, ConsumeInstanceKind, runtime_efficiency};

    let mut sim = Sim::new(1, 24, 24, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let content = sim.block_runtime().expect("runtime").content().clone();
    let factory_block = content
        .blocks()
        .iter()
        .find(|def| !def.unit_plans.is_empty())
        .map(|def| def.id)
        .expect("a UnitFactory block");
    assert!(
        sim.place_for_team(10, 10, factory_block, 0, 1),
        "factory placed"
    );
    let entity = sim
        .grid
        .entity_at(TilePos::new(10, 10))
        .expect("factory entity");
    let plan_index = sim
        .ecs
        .0
        .get::<crate::world::blocks::units::UnitFactoryBuild>(entity)
        .map(|state| state.current_plan)
        .unwrap_or(-1);
    assert!(plan_index >= 0, "created() selects the first plan");
    let plan = &content.block(factory_block).expect("def").unit_plans[plan_index as usize];
    {
        let mut items = sim.ecs.0.get_mut::<ItemModule>(entity).expect("items");
        for stack in &plan.requirements {
            let held = items.get(stack.item);
            items.remove(stack.item, held);
            items.add(stack.item, stack.amount, 1_000_000);
        }
    }
    let consumer = ConsumeInstance::new(ConsumeInstanceKind::ItemDynamic);
    let bridge = SimDynamicConsume { content: &content };
    assert_eq!(
        runtime_efficiency(&sim.ecs.0, entity, &consumer, 1.0, Some(&bridge)),
        1.0,
        "plan requirements held -> efficiency 1"
    );
    {
        let stack = plan.requirements.first().expect("plan requirements");
        let mut items = sim.ecs.0.get_mut::<ItemModule>(entity).expect("items");
        items.remove(stack.item, 1);
    }
    assert_eq!(
        runtime_efficiency(&sim.ecs.0, entity, &consumer, 1.0, Some(&bridge)),
        0.0,
        "missing requirement -> efficiency 0"
    );
    // Without the bridge the dynamic consumer is inert (`ConsumeItemDynamic`
    // has no static stacks), which is why the live pass must supply it.
    assert_eq!(
        runtime_efficiency(&sim.ecs.0, entity, &consumer, 1.0, None),
        0.0
    );
}
