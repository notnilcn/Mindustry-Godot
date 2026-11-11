// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared `sim` test fixtures: every sim test builds its world through these
//! helpers instead of its own copy. Bodies are the ones the per-module
//! fixtures used (`combat_tests`, `production_tests`, `logic_campaign_tests`)
//! before W2 adopted them.
//! Upstream: none (port-only test support).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::Mut;

use crate::content::{BlockId, ContentRegistry, ItemId, LiquidId};
use crate::determinism::{ConfigValue, SimCommand};
use crate::entities::comp::Health;
use crate::game::rules::Rules;
use crate::game::runtime::CampaignRuntime;
use crate::world::TilePos;
use crate::world::modules::{ItemModule, LiquidModule};

use super::Sim;

/// Installs the block + unit runtimes on an existing sim, in that order (the
/// live-sim shape the host installs). `unit_runtime::tests::install` (and its
/// `unit_payload_tests` copy) built the same pair.
pub(crate) fn install_live_runtimes(sim: &mut Sim) {
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
}

/// AIR floor/overlay, seed `seed`, block + unit runtime installed.
/// `live_sim_seeded(1, n)` == `combat_tests::live_sim(n)` (`combat_tests.rs:17`).
pub(crate) fn live_sim_seeded(seed: u64, size: i32) -> Sim {
    let mut sim = Sim::new(seed, size, size, BlockId::AIR, BlockId::AIR);
    install_live_runtimes(&mut sim);
    sim
}

/// The combat fixture: `live_sim_seeded(1, n)`.
pub(crate) fn live_sim(size: i32) -> Sim {
    live_sim_seeded(1, size)
}

/// `production_tests::runtime_sim` (:445) / `logic_campaign_tests::wide_sim`
/// body, parameterised: seed 3, stone fill, block runtime only.
pub(crate) fn stone_sim(size: i32) -> Sim {
    let mut sim = Sim::new(3, size, size, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim
}

/// The production fixture: `stone_sim(32)`.
pub(crate) fn runtime_sim() -> Sim {
    stone_sim(32)
}

/// `production_tests::runtime_sim_with_content` (:157): `stone_sim(32)` over
/// a caller-patched content registry.
pub(crate) fn runtime_sim_with_content(content: ContentRegistry) -> Sim {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime_from(crate::sim::runtime::SimRuntimeConfig::default(), content)
        .expect("runtime");
    sim
}

/// `for _ in 0..n { sim.tick().expect("tick"); }` (101 inline copies in
/// `combat_tests`, 117 in `production_tests`).
pub(crate) fn tick_n(sim: &mut Sim, n: usize) {
    for _ in 0..n {
        sim.tick().expect("tick");
    }
}

/// Block id via the **unit runtime's** content (`combat_tests::block`, :24).
/// NOT `Sim::world_block_id`, which takes `&mut self` and lazily installs
/// `WorldApplyRuntime` (that changes `world_apply_checksum`).
pub(crate) fn unit_content_block(sim: &Sim, name: &str) -> u16 {
    sim.unit_runtime()
        .expect("runtime")
        .content
        .block_id(name)
        .expect("block")
        .get()
}

/// `combat_tests::place` (:33): SimCommand::Place, rotation 0, player None;
/// returns `tiles.get(x,y).build`.
pub(crate) fn place_at(sim: &mut Sim, x: i16, y: i16, name: &str, team: u8) -> Entity {
    let block = unit_content_block(sim, name);
    sim.command(SimCommand::Place {
        x,
        y,
        block,
        rotation: 0,
        team,
        player: None,
    })
    .expect("place");
    sim.grid
        .tiles
        .get(x as i32, y as i32)
        .build
        .expect("placed building")
}

/// `production_tests::place` (:16): team 1, argument order (name, x, y);
/// returns via `grid.entity_at`.
pub(crate) fn place_team1(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
    let id = sim.world_block_id(name).expect("block");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    sim.grid.entity_at(TilePos::new(x, y)).expect("entity")
}

/// `production_tests::item` (:30): item id through the live runtime table.
pub(crate) fn item(sim: &Sim, name: &str) -> ItemId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .item_id(name)
        .expect("item")
}

/// `production_tests::liquid` (:424): liquid id through the live runtime
/// table.
pub(crate) fn liquid(sim: &Sim, name: &str) -> LiquidId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .liquid_id(name)
        .expect("liquid")
}

/// `production_tests::configure` (:507): SimCommand::Configure.
pub(crate) fn configure(sim: &mut Sim, x: i16, y: i16, value: ConfigValue) {
    sim.command(SimCommand::Configure { x, y, value })
        .expect("configure");
}

/// `combat_tests::health` (:63): live building/unit health.
pub(crate) fn health(sim: &Sim, e: Entity) -> Option<f32> {
    sim.ecs.0.get::<Health>(e).map(|h| h.health)
}

/// `production_tests::stored_item` (:512): inventory count of `item`.
pub(crate) fn stored_item(sim: &Sim, e: Entity, item: ItemId) -> i32 {
    sim.ecs
        .0
        .get::<ItemModule>(e)
        .map(|module| module.get(item))
        .unwrap_or(0)
}

/// `production_tests::stored_liquid` (:437): liquid module content of `liquid`.
pub(crate) fn stored_liquid(sim: &Sim, e: Entity, liquid: LiquidId) -> f32 {
    sim.ecs
        .0
        .get::<LiquidModule>(e)
        .map(|m| m.get(liquid))
        .unwrap_or(0.0)
}

/// `logic_campaign_tests::feed_turret` (:199): feeds turret ammo through the
/// real `acceptItem`/`handleItem` dispatch (the item module does not hold
/// turret ammo).
pub(crate) fn feed_turret(sim: &mut Sim, turret: Entity, item_name: &str, amount: usize) {
    use crate::world::blocks::distribution::transfer;
    let ammo = item(sim, item_name);
    let world = &mut sim.ecs.0;
    for _ in 0..amount {
        assert!(
            transfer::dispatch_accept_item(world, turret, turret, ammo),
            "{item_name} accepted"
        );
        transfer::dispatch_handle_item(world, turret, turret, ammo);
    }
}

/// `logic_campaign_tests::runtime` (:93): the live campaign runtime.
pub(crate) fn campaign_runtime(sim: &Sim) -> &CampaignRuntime {
    sim.ecs
        .0
        .get_resource::<CampaignRuntime>()
        .expect("campaign runtime")
}

/// Mutable [`campaign_runtime`].
pub(crate) fn campaign_runtime_mut(sim: &mut Sim) -> Mut<'_, CampaignRuntime> {
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign runtime")
}

// ---- production fixtures used by the logic/campaign suites ----

/// Installs a campaign session (the live rules/markers/wave source) on `sim`.
pub(crate) fn install_campaign_session(sim: &mut Sim, rules: Rules) {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::universe::Campaign;
    let content = crate::world::harness::BuildHarness::load_content();
    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
}

/// Live-client logic resources (what `gdext` `install_sim_seams` inserts).
pub(crate) fn install_logic_world(sim: &mut Sim) {
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState {
            headless: false,
            ..crate::logic::world::LogicWorldState::new()
        });
    sim.ecs
        .0
        .insert_resource(crate::logic::world::MessageState::default());
    let content = crate::world::harness::BuildHarness::load_content();
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicContentIndex::from_content(
            &content,
        ));
}

/// Places a configurable world processor running `code`.
pub(crate) fn place_world_processor(sim: &mut Sim, x: i16, y: i16, code: &str) {
    let id = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let bytes = crate::logic::blocks::logic_block::compress(code, &[]);
    configure(sim, x, y, ConfigValue::Bytes(bytes));
}

/// The campaign rules the logic suites run with (`Rules.default_team = 1`,
/// world-processor editing allowed).
pub(crate) fn test_rules() -> Rules {
    Rules {
        default_team: 1,
        allow_edit_world_processors: true,
        ..Default::default()
    }
}

// ---- logic/campaign fixture shapes ----

/// Content/team logic constants, installed before the processors are decoded
/// (`GlobalVars.install_world`, H06 T20).
fn install_globals(sim: &mut Sim) {
    let content = crate::world::harness::BuildHarness::load_content();
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
}

/// `runtime_sim` + unit runtime + logic globals + campaign session + live
/// logic resources (what the host installs before a sector starts).
pub(crate) fn finish_sim(mut sim: Sim, rules: Rules) -> Sim {
    sim.install_unit_runtime().expect("unit runtime");
    install_globals(&mut sim);
    install_campaign_session(&mut sim, rules);
    install_logic_world(&mut sim);
    sim
}

/// The logic suite fixture: `finish_sim(runtime_sim(), test_rules())`.
pub(crate) fn campaign_sim() -> Sim {
    finish_sim(runtime_sim(), test_rules())
}

/// A flat map of `size` x `size` (Ground Zero's 256x256 map area needs a
/// winner that cannot disable the limit).
pub(crate) fn wide_sim(size: i32) -> Sim {
    stone_sim(size)
}

/// Asserts the per-tick logic event drain left `LogicWorldState.events` empty.
pub(crate) fn assert_events_drained(sim: &Sim) {
    let state = sim
        .ecs
        .0
        .get_resource::<crate::logic::world::LogicWorldState>()
        .expect("logic world");
    assert!(
        state.events.is_empty(),
        "logic events are drained each tick"
    );
}
