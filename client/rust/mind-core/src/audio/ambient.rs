// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building ambient loop sources.
//!
//! `BuildingComp.add` registers an `AmbientSource` when
//! `block.ambientSound != Sounds.none` (`BuildingComp.java:131-133`);
//! `getAmbientSound`/`getAmbientVolume` resolve
//! `block.ambientSound`/`block.ambientSoundVolume * ambientVolume()`
//! (`BuildingComp.java:2296-2303`, `:2022`). Class defaults come from the
//! block constructors, `Blocks.java` overrides win. The 20 Hz poll +
//! aggregation live in [`super::loops::AmbientPoller`].

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use super::ids::SoundId;
use super::loops::{AmbientEntry, AmbientSnapshot};
use super::math::MIN_LOOP_VOLUME;
use crate::content::ContentRegistry;
use crate::content::registries::blocks::{BlockDef, BlockKind};
use crate::ecs::EntitySeq;
use crate::entities::comp::Building;
use crate::world::blocks::defense::shields::ForceProjectorState;
use crate::world::blocks::distribution::conveyor::ConveyorBuild;
use crate::world::blocks::power::behavior::ImpactReactorState;
use crate::world::blocks::power::generator::GeneratorState;
use crate::world::modules::ItemModule;

/// `Blocks.java` per-block ambient overrides `(name, sound, volume)`; the
/// volume-only rows (`overdrive-*`, `repair-*`) spell the class sound too.
const BLOCK_AMBIENT: &[(&str, SoundId, f32)] = &[
    ("silicon-smelter", SoundId::LOOP_SMELTER, 0.07),
    ("silicon-crucible", SoundId::LOOP_SMELTER, 0.07),
    ("kiln", SoundId::LOOP_SMELTER, 0.07),
    ("phase-weaver", SoundId::LOOP_TECH, 0.02),
    ("pyratite-mixer", SoundId::LOOP_MACHINE_SPIN, 0.1),
    ("blast-mixer", SoundId::LOOP_MACHINE_SPIN, 0.12),
    ("pulverizer", SoundId::LOOP_GRIND, 0.025),
    ("silicon-arc-furnace", SoundId::LOOP_SMELTER, 0.12),
    ("electrolyzer", SoundId::LOOP_ELECTRIC_HUM, 0.08),
    ("atmospheric-concentrator", SoundId::LOOP_EXTRACT, 0.06),
    ("oxidation-chamber", SoundId::LOOP_EXTRACT, 0.08),
    ("electric-heater", SoundId::LOOP_HUM, 0.03),
    ("slag-heater", SoundId::LOOP_HUM, 0.03),
    ("phase-heater", SoundId::LOOP_HUM, 0.03),
    ("carbide-crucible", SoundId::LOOP_SMELTER, 0.09),
    ("surge-crucible", SoundId::LOOP_SMELTER, 0.9),
    ("cyanogen-synthesizer", SoundId::LOOP_EXTRACT, 0.08),
    ("phase-synthesizer", SoundId::LOOP_TECH, 0.04),
    ("overdrive-projector", SoundId::LOOP_CIRCUIT, 0.08),
    ("overdrive-dome", SoundId::LOOP_CIRCUIT, 0.12),
    ("combustion-generator", SoundId::LOOP_SMELTER, 0.03),
    ("thermal-generator", SoundId::LOOP_HUM, 0.06),
    ("steam-generator", SoundId::LOOP_SMELTER, 0.06),
    ("differential-generator", SoundId::LOOP_DIFFERENTIAL, 0.12),
    ("thorium-reactor", SoundId::LOOP_THORIUM_REACTOR, 0.11),
    ("impact-reactor", SoundId::LOOP_PULSE, 0.08),
    ("turbine-condenser", SoundId::LOOP_HUM, 0.06),
    ("chemical-combustion-chamber", SoundId::LOOP_SMELTER, 0.06),
    ("pyrolysis-generator", SoundId::LOOP_SMELTER, 0.06),
    ("flux-reactor", SoundId::LOOP_FLUX, 0.15),
    ("neoplasia-reactor", SoundId::LOOP_BIO, 0.2),
    ("cultivator", SoundId::LOOP_CULTIVATOR, 0.075),
    ("vent-condenser", SoundId::LOOP_HUM, 0.06),
    ("cliff-crusher", SoundId::LOOP_DRILL, 0.04),
    ("large-cliff-crusher", SoundId::LOOP_DRILL, 0.08),
    ("repair-point", SoundId::BEAM_HEAL, 0.9),
    ("repair-turret", SoundId::BEAM_HEAL, 1.25),
];

/// `Block.ambientSound`/`ambientSoundVolume` for a block (`None` when the
/// block has no ambient sound, including `heat-source`'s explicit
/// `Sounds.none`, `Blocks.java:6802`).
pub fn block_ambient(def: &BlockDef) -> Option<(SoundId, f32)> {
    if def.name == "heat-source" {
        return None;
    }
    if let Some((_, sound, volume)) = BLOCK_AMBIENT.iter().find(|(name, _, _)| *name == def.name) {
        return Some((*sound, *volume));
    }
    // Class defaults (constructor assignments). `Block.ambientSoundVolume`
    // defaults to 0.05 (`Block.java:358`) where a constructor does not set it.
    match def.kind {
        BlockKind::Conveyor | BlockKind::ArmoredConveyor => Some((SoundId::LOOP_CONVEYOR, 0.0022)),
        BlockKind::StackConveyor => Some((SoundId::LOOP_CONVEYOR, 0.004)),
        BlockKind::Drill => Some((SoundId::LOOP_DRILL, 0.019)),
        BlockKind::BurstDrill => Some((SoundId::DRILL_CHARGE, 0.18)),
        BlockKind::BeamDrill => Some((SoundId::LOOP_MINE_BEAM, 0.05)),
        BlockKind::Fracker => Some((SoundId::LOOP_DRILL, 0.03)),
        BlockKind::GenericCrafter
        | BlockKind::AttributeCrafter
        | BlockKind::HeatCrafter
        | BlockKind::HeatProducer => Some((SoundId::LOOP_MACHINE, 0.03)),
        BlockKind::RepairTurret => Some((SoundId::BEAM_HEAL, 1.0)),
        BlockKind::ForceProjector => Some((SoundId::LOOP_SHIELD, 0.1)),
        BlockKind::Reconstructor => Some((SoundId::LOOP_UNIT_BUILDING, 0.05)),
        BlockKind::UnitFactory => Some((SoundId::LOOP_UNIT_BUILDING, 0.09)),
        BlockKind::UnitAssembler => Some((SoundId::LOOP_UNIT_BUILDING, 0.13)),
        BlockKind::UnitCargoLoader => Some((SoundId::LOOP_UNIT_BUILDING, 0.05)),
        BlockKind::OverdriveProjector => Some((SoundId::LOOP_CIRCUIT, 0.13)),
        BlockKind::RegenProjector => Some((SoundId::LOOP_REGEN, 0.45)),
        // `Separator` overrides `shouldAmbientSound` but never assigns
        // `ambientSound`, so it is silent upstream too.
        _ => None,
    }
}

/// `BuildingComp.shouldAmbientSound` per class (`:1217` + overrides).
fn should_ambient_sound(
    world: &World,
    entity: Entity,
    def: &BlockDef,
    building: &Building,
) -> bool {
    match def.kind {
        BlockKind::GenericCrafter
        | BlockKind::AttributeCrafter
        | BlockKind::HeatCrafter
        | BlockKind::HeatProducer => building.efficiency > 0.0,
        BlockKind::Separator => building.efficiency > 0.0,
        BlockKind::Conveyor | BlockKind::ArmoredConveyor => world
            .get::<ConveyorBuild>(entity)
            .map_or(building.should_consume, |conveyor| {
                conveyor.clog_heat <= 0.5
            }),
        BlockKind::StackConveyor => false,
        BlockKind::Drill | BlockKind::BurstDrill => {
            building.efficiency > 0.01
                && world
                    .get::<ItemModule>(entity)
                    .is_none_or(|items| items.total() < def.item_capacity)
        }
        // `RepairTurret.shouldAmbientSound` needs a target field the ECS does
        // not store (deviation): gate on efficiency only.
        BlockKind::RepairTurret => building.efficiency > 0.0,
        BlockKind::ForceProjector => world
            .get::<ForceProjectorState>(entity)
            .is_none_or(|state| !state.broken && state.real_radius() > 1.0),
        // `BuildingComp.shouldAmbientSound() = shouldConsume()`.
        _ => building.should_consume,
    }
}

/// `BuildingComp.ambientVolume` per class (`:2022` + overrides).
fn ambient_volume_scale(world: &World, entity: Entity, def: &BlockDef, building: &Building) -> f32 {
    match def.kind {
        // `Drill.ambientVolume = efficiency * size^2 / 4` (`Drill.java:257`);
        // `BurstDrill`'s `progress^4` factor is deferred (Erekir only).
        BlockKind::Drill | BlockKind::BurstDrill => {
            building.efficiency * (def.size as f32).powi(2) / 4.0
        }
        // `PowerGenerator.ambientVolume = clamp(productionEfficiency)`.
        BlockKind::ConsumeGenerator
        | BlockKind::ThermalGenerator
        | BlockKind::NuclearReactor
        | BlockKind::SolarGenerator
        | BlockKind::VariableReactor
        | BlockKind::HeaterGenerator => world
            .get::<GeneratorState>(entity)
            .map_or(building.efficiency, |state| {
                state.production_efficiency.clamp(0.0, 1.0)
            }),
        // `ImpactReactor.ambientVolume = warmup`.
        BlockKind::ImpactReactor => world
            .get::<ImpactReactorState>(entity)
            .map_or(building.efficiency, |state| state.warmup),
        _ => building.efficiency,
    }
}

/// Fills `out` with one entry per live building whose ambient sound should
/// play now (deterministic `(EntitySeq, index)` order).
pub fn collect_ambient(world: &World, content: &ContentRegistry, out: &mut AmbientSnapshot) {
    out.entries.clear();
    let mut buildings: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter(|entity| entity.get::<Building>().is_some())
        .map(|entity| {
            (
                entity.get::<EntitySeq>().map_or(u64::MAX, |seq| seq.0),
                entity.id(),
            )
        })
        .collect();
    buildings.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));

    for (_, entity) in buildings {
        let Some(building) = world.get::<Building>(entity) else {
            continue;
        };
        let Some(def) = content.block(building.block) else {
            continue;
        };
        let Some((sound, base_volume)) = block_ambient(def) else {
            continue;
        };
        if !should_ambient_sound(world, entity, def, building) {
            continue;
        }
        let volume = base_volume * ambient_volume_scale(world, entity, def, building);
        if volume <= MIN_LOOP_VOLUME {
            continue;
        }
        out.entries.push(AmbientEntry {
            sound,
            x: f32::from(building.tile.x()) * 8.0 + def.offset,
            y: f32::from(building.tile.y()) * 8.0 + def.offset,
            volume,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_ambient_table() {
        let content = crate::world::harness::BuildHarness::load_content();
        let def = |name: &str| {
            content
                .block_id(name)
                .and_then(|id| content.block(id))
                .unwrap_or_else(|| panic!("block `{name}`"))
        };
        assert_eq!(
            block_ambient(def("mechanical-drill")),
            Some((SoundId::LOOP_DRILL, 0.019))
        );
        assert_eq!(
            block_ambient(def("conveyor")),
            Some((SoundId::LOOP_CONVEYOR, 0.0022))
        );
        assert_eq!(
            block_ambient(def("silicon-smelter")),
            Some((SoundId::LOOP_SMELTER, 0.07))
        );
        // Plain `GenericCrafter` class default.
        assert_eq!(
            block_ambient(def("graphite-press")),
            Some((SoundId::LOOP_MACHINE, 0.03))
        );
        // `HeatProducer` with the Blocks.java volume override.
        assert_eq!(
            block_ambient(def("electric-heater")),
            Some((SoundId::LOOP_HUM, 0.03))
        );
        assert_eq!(
            block_ambient(def("overdrive-projector")),
            Some((SoundId::LOOP_CIRCUIT, 0.08))
        );
        assert_eq!(block_ambient(def("copper-wall")), None);
        // `StackConveyor` class default.
        assert_eq!(
            block_ambient(def("plastanium-conveyor")),
            Some((SoundId::LOOP_CONVEYOR, 0.004))
        );
        // Explicit `Sounds.none`.
        assert_eq!(block_ambient(def("heat-source")), None);
    }

    fn ambient_sim() -> crate::sim::Sim {
        let mut sim = crate::sim::Sim::new(
            3,
            32,
            32,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        sim.install_block_runtime().expect("runtime");
        sim
    }

    fn place(sim: &mut crate::sim::Sim, name: &str, x: i16, y: i16) -> Entity {
        let block = sim.world_block_id(name).expect("block");
        sim.command(crate::determinism::SimCommand::Place {
            x,
            y,
            block,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("place");
        sim.grid
            .entity_at(crate::world::TilePos::new(x, y))
            .expect("entity")
    }

    fn collect(sim: &crate::sim::Sim) -> AmbientSnapshot {
        let mut out = AmbientSnapshot::default();
        let content = sim.block_runtime().expect("runtime").content();
        collect_ambient(&sim.ecs.0, content, &mut out);
        out
    }

    #[test]
    fn drill_volume_formula() {
        let mut sim = ambient_sim();
        let drill = place(&mut sim, "mechanical-drill", 5, 5);
        sim.ecs
            .0
            .get_mut::<Building>(drill)
            .expect("building")
            .efficiency = 1.0;
        // `efficiency * size^2 / 4` with size 2 -> 1.0.
        let out = collect(&sim);
        let entry = out
            .entries
            .iter()
            .find(|entry| entry.sound == SoundId::LOOP_DRILL)
            .expect("drill entry");
        assert!(
            (entry.volume - 0.019).abs() < 1.0e-6,
            "volume {}",
            entry.volume
        );

        // efficiency 0 -> no entry (`efficiency > 0.01`).
        sim.ecs
            .0
            .get_mut::<Building>(drill)
            .expect("building")
            .efficiency = 0.0;
        assert!(collect(&sim).entries.is_empty());

        // Full item module -> no entry.
        sim.ecs
            .0
            .get_mut::<Building>(drill)
            .expect("building")
            .efficiency = 1.0;
        let cap = {
            let content = sim.block_runtime().expect("runtime").content();
            content
                .block_id("mechanical-drill")
                .and_then(|id| content.block(id))
                .expect("mechanical-drill")
                .item_capacity
        };
        let copper = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .item_id("copper")
            .expect("copper");
        sim.ecs
            .0
            .get_mut::<ItemModule>(drill)
            .expect("module")
            .add(copper, cap, cap);
        assert!(collect(&sim).entries.is_empty(), "full drill is silent");
    }

    #[test]
    fn conveyor_clog_gate() {
        let mut sim = ambient_sim();
        let conveyor = place(&mut sim, "conveyor", 5, 5);
        sim.ecs
            .0
            .get_mut::<Building>(conveyor)
            .expect("building")
            .efficiency = 1.0;
        sim.ecs
            .0
            .get_mut::<ConveyorBuild>(conveyor)
            .expect("conveyor")
            .clog_heat = 0.2;
        let out = collect(&sim);
        let entry = out
            .entries
            .iter()
            .find(|entry| entry.sound == SoundId::LOOP_CONVEYOR)
            .expect("conveyor entry");
        assert!(
            (entry.volume - 0.0022).abs() < 1.0e-6,
            "volume {}",
            entry.volume
        );

        sim.ecs
            .0
            .get_mut::<ConveyorBuild>(conveyor)
            .expect("conveyor")
            .clog_heat = 0.6;
        assert!(
            collect(&sim).entries.is_empty(),
            "clogged conveyor is silent"
        );
    }
}
