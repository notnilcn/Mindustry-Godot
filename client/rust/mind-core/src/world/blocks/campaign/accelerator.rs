// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Interplanetary Accelerator launch cost and launch timeline
//! (`Accelerator.AcceleratorBuild`, world/blocks/campaign/Accelerator.java).
//!
//! The per-tick state machine (`updateTile`, IO, item acceptance) lives in
//! [`crate::world::behavior::campaign`] and the launch step in
//! `Sim::accelerator_*`; this module owns the data both sides share:
//! the `consumeItems(coreNucleus.requirements)` cost (:76-85), the
//! `beginLaunch` burst schedule (:336-387) and the `updateLaunch` lightning
//! chance (:413-436). Render math lives in
//! [`crate::render::accelerator`].

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::Building;
use crate::world::block::BlockTable;

/// `AcceleratorBuild.beginLaunch`: the `coreLandDust` rings run on a 2-tick
/// stagger (`Accelerator.java:369-386`, `Time.run(i * 2f)`).
pub const DUST_RING_INTERVAL: f32 = 2.0;
/// `beginLaunch`: `for(int i = 0; i < 13; i++)`.
pub const DUST_RINGS: usize = 13;
/// `beginLaunch`: `float spacing = 12f`.
pub const DUST_RING_SPACING: f32 = 12.0;
/// `beginLaunch`: `Mathf.chance(0.7f - fi * 0.02f)`.
pub const DUST_CHANCE_BASE: f64 = 0.7;
/// `beginLaunch`: the per-ring chance falloff.
pub const DUST_CHANCE_STEP: f64 = 0.02;

/// `Accelerator.lightningLaunchChance` (`Accelerator.java:73`).
pub const LIGHTNING_LAUNCH_CHANCE: f64 = 0.8;
/// `Accelerator.launchDuration` (:51): the launch-cutscene half.
pub const LAUNCH_DURATION: f32 = 120.0;
/// `Accelerator.chargeDuration` (:52): the charging half.
pub const CHARGE_DURATION: f32 = 220.0;
/// `Accelerator.buildDuration` (:53): launch-core construction.
pub const BUILD_DURATION: f32 = 120.0;

/// The accelerator's launch cost: `consumeItems(launchBlock.requirements)`
/// plus `powerBufferRequirement` (`Accelerator.java:40,82-83`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AcceleratorLaunchCost {
    /// Item stacks the accelerator consumes (`coreNucleus.requirements`).
    pub items: Vec<(ItemId, i32)>,
    /// `powerBufferRequirement` drained by `power.graph.useBatteries`.
    pub power: f32,
}

impl AcceleratorLaunchCost {
    /// Sum of the item requirements (`itemCapacity` before the core is built,
    /// `Accelerator.java:80-85`).
    pub fn item_total(&self) -> i32 {
        self.items.iter().map(|(_, amount)| amount).sum()
    }
}

/// `Accelerator.init()`'s `consumeItems(coreNucleus.requirements)`: the item
/// stacks of the block's item consumer, in `Consumers` order.
pub fn launch_requirements(world: &World, entity: Entity) -> Vec<(ItemId, i32)> {
    let Some(building) = world.get::<Building>(entity) else {
        return Vec::new();
    };
    let Some(table) = world.get_resource::<BlockTable>() else {
        return Vec::new();
    };
    let Some(inst) = table.instance(building.block) else {
        return Vec::new();
    };
    let mut out: Vec<(ItemId, i32)> = Vec::new();
    for consumer in &inst.consumers.all {
        if let crate::world::consumers::ConsumeInstanceKind::Items(stacks) = &consumer.kind {
            for stack in stacks {
                match out.iter_mut().find(|(item, _)| *item == stack.item) {
                    Some((_, amount)) => *amount += stack.amount,
                    None => out.push((stack.item, stack.amount)),
                }
            }
        }
    }
    out
}

/// Full launch cost of `entity` (`Accelerator.java:40,76-85`).
pub fn launch_cost(world: &World, entity: Entity) -> AcceleratorLaunchCost {
    let power =
        crate::world::behavior::campaign::accelerator_def(world, entity).power_buffer_requirement;
    AcceleratorLaunchCost {
        items: launch_requirements(world, entity),
        power,
    }
}

/// The `beginLaunch`/`updateLaunch` timeline (`Accelerator.java:336-436`),
/// shared by the host pump and the render pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchSequence {
    /// `launchDuration` (:51).
    pub launch_duration: f32,
    /// `chargeDuration` (:52).
    pub charge_duration: f32,
    /// `buildDuration` (:53).
    pub build_duration: f32,
}

impl LaunchSequence {
    /// The vanilla timeline (:51-53).
    pub const fn vanilla() -> Self {
        Self {
            launch_duration: LAUNCH_DURATION,
            charge_duration: CHARGE_DURATION,
            build_duration: BUILD_DURATION,
        }
    }

    /// `launchDuration()` (`Accelerator.java:322-324`): `launch + charge`.
    pub fn total(&self) -> f32 {
        self.launch_duration + self.charge_duration
    }

    /// `Time.run(chargeDuration, ...)`: the launch burst/lightning tick.
    pub fn charge_tick(&self) -> f32 {
        self.charge_duration
    }

    /// `Time.runTask(launchDuration() - 6f, ...)`: the sector switch tick.
    pub fn switch_tick(&self) -> f32 {
        self.total() - 6.0
    }

    /// One `coreLandDust` ring of the 13-ring stagger (`Accelerator.java:
    /// 369-386`): `radius = block.size/2 + 1 + spacing * i`,
    /// `rays = ceil(radius * PI * 2 / 6)`.
    pub fn dust_ring(&self, size: i32, index: usize) -> DustRing {
        let radius = size as f32 / 2.0 + 1.0 + DUST_RING_SPACING * index as f32;
        DustRing {
            at: index as f32 * DUST_RING_INTERVAL,
            radius,
            rays: (radius * std::f32::consts::PI * 2.0 / 6.0).ceil() as i32,
            chance: (DUST_CHANCE_BASE - index as f64 * DUST_CHANCE_STEP).clamp(0.0, 1.0),
        }
    }

    /// `updateLaunch`'s per-tick lightning chance after the charge:
    /// `Mathf.chanceDelta(lightningLaunchChance * pow3In(chargeFout))`
    /// (`Accelerator.java:423-429`). `Time.delta` is 1 at the fixed step.
    pub fn lightning_chance(&self, phase: &crate::render::accelerator::LaunchPhase) -> f64 {
        if phase.raw_time <= self.charge_duration {
            return 0.0;
        }
        let fout = phase.charge_fout.max(0.0);
        LIGHTNING_LAUNCH_CHANCE * f64::from(fout * fout * fout)
    }
}

/// One scheduled `coreLandDust` ring (`Accelerator.java:369-386`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DustRing {
    /// `Time.run(i * 2f)` offset in ticks.
    pub at: f32,
    /// World-space radius around the accelerator.
    pub radius: f32,
    /// `ceil(radius * PI * 2 / 6)` dust positions.
    pub rays: i32,
    /// `chance(0.7 - i * 0.02)` per position.
    pub chance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::content::test_support::test_registry;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    /// A placed accelerator with its behavior state.
    fn accelerator_world() -> (
        EcsWorld,
        std::sync::Arc<crate::world::block::BlockInstance>,
        Entity,
        crate::content::ContentRegistry,
    ) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("interplanetary-accelerator")
            .expect("accelerator")
            .clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let e = inst.spawn(
            &mut world,
            0,
            TilePos::new(10, 10),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, e);
        world.insert_resource(table);
        (world, inst, e, content)
    }

    /// `Accelerator.init()` (`Accelerator.java:76-85`) + the vanilla
    /// `powerBufferRequirement` (Blocks.java:6876).
    #[test]
    fn launch_cost_matches_core_nucleus_and_the_power_buffer() {
        let (world, _inst, e, content) = accelerator_world();
        let cost = launch_cost(&world, e);
        assert_eq!(cost.item_total(), 25_000, "core-nucleus itemCapacity");
        assert_eq!(cost.power, 1_000_000.0);
        let mut items = cost.items;
        items.sort_by_key(|(item, _)| item.index());
        let mut expected = vec![
            (content.item_id("copper").expect("copper"), 8000),
            (content.item_id("lead").expect("lead"), 8000),
            (content.item_id("silicon").expect("silicon"), 5000),
            (content.item_id("thorium").expect("thorium"), 4000),
        ];
        expected.sort_by_key(|(item, _)| item.index());
        assert_eq!(items, expected);
    }

    /// `beginLaunch`/`Time.runTask` timing (`Accelerator.java:336-387,273`).
    #[test]
    fn launch_sequence_matches_the_java_timeline() {
        let sequence = LaunchSequence::vanilla();
        assert_eq!(sequence.total(), 340.0);
        assert_eq!(sequence.charge_tick(), 220.0);
        assert_eq!(sequence.switch_tick(), 334.0);

        let ring = sequence.dust_ring(7, 0);
        assert_eq!(ring.at, 0.0);
        assert_eq!(ring.radius, 4.5);
        assert_eq!(ring.rays, 5); // ceil(4.5 * 2PI / 6)
        assert!((ring.chance - 0.7).abs() < 1e-9);
        let last = sequence.dust_ring(7, DUST_RINGS - 1);
        assert_eq!(last.at, 24.0, "13th ring at i * 2 ticks");
        assert!((last.radius - (4.5 + 12.0 * 12.0)).abs() < 1e-6);
        assert!((last.chance - (0.7 - 12.0 * 0.02)).abs() < 1e-9);

        // `updateLaunch` only fires after the charge (`rawTime > charge`).
        let charge = crate::render::accelerator::LaunchPhase::from_land_time_in(
            (340.0 - 220.0) / 340.0,
            120.0,
            220.0,
        );
        assert_eq!(sequence.lightning_chance(&charge), 0.0);
        let launch = crate::render::accelerator::LaunchPhase::from_land_time_in(
            (340.0 - 280.0) / 340.0,
            120.0,
            220.0,
        );
        assert!(sequence.lightning_chance(&launch) > 0.0);
        assert!(sequence.lightning_chance(&launch) <= LIGHTNING_LAUNCH_CHANCE);
    }

    /// `AcceleratorBuild.version() = 1` and the `progress` field round-trip
    /// (`Accelerator.java:302-319`), matching the append-only building IO.
    #[test]
    fn accelerator_progress_io_roundtrip() {
        use crate::entities::comp::CampaignState;

        let (mut world, inst, e, _content) = accelerator_world();
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.progress = 0.625;
        }
        assert_eq!(inst.behavior.version(&world, e), 1);
        let mut buf = Vec::new();
        {
            let mut w: crate::world::behavior::BuildingWriter =
                crate::io::wire::WireWriter::new(&mut buf);
            inst.behavior.write(&world, e, &mut w);
        }
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            *state = CampaignState::default();
        }
        {
            let mut r: crate::world::behavior::BuildingReader =
                crate::io::wire::WireReader::new(&buf);
            inst.behavior.read(&mut world, e, &mut r, 1);
            assert_eq!(r.remaining(), 0, "revision 1 consumes progress");
        }
        assert_eq!(
            world.get::<CampaignState>(e).expect("state").progress,
            0.625
        );
        // Revision 0 has no `progress` field: the value stays at its default.
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            *state = CampaignState::default();
        }
        {
            let mut r: crate::world::behavior::BuildingReader =
                crate::io::wire::WireReader::new(&buf);
            inst.behavior.read(&mut world, e, &mut r, 0);
        }
        assert_eq!(world.get::<CampaignState>(e).expect("state").progress, 0.0);
    }

    /// A bare `Sim` placement sees the same cost (the block runtime's consumer
    /// lowering), and the launch cost is stable across reads.
    #[test]
    fn launch_cost_reads_the_live_block_table() {
        use crate::determinism::SimCommand;
        use crate::sim::Sim;

        let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
        let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
        sim.grid.fill(stone, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        let block = sim
            .world_block_id("interplanetary-accelerator")
            .expect("accelerator");
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("placed");
        let entity = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
        let cost = launch_cost(&sim.ecs.0, entity);
        assert_eq!(cost.item_total(), 25_000);
        assert_eq!(cost.power, 1_000_000.0);
    }
}
