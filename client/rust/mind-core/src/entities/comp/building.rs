// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `Building` entity state (`entities/comp/BuildingComp.java`) and the
//! per-family state components (`world/blocks/**/\*Build` inner classes).
//!
//! Ported from `core/src/mindustry/entities/comp/BuildingComp.java` fields and
//! the per-build-class state fields of the family blocks. Java's per-block inner
//! `Building` subclasses become ECS components selected by
//! [`crate::world::block::BlockInstance::spawn`] (plan 07 deviation §2.4.1).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use mind_macros::SimComponent;
use smallvec::SmallVec;

use crate::content::{BlockId, ItemId};
use crate::world::TilePos;

/// Placed-block runtime state (the `Building` component).
///
/// `team`/`health` live on [`crate::entities::comp::TeamComp`]/
/// [`crate::entities::comp::Health`] (plan 07 §3.4 R16). This component is the
/// `Groups.build` membership key, exactly like the marker it replaces.
#[derive(Debug, Clone, Component, SimComponent)]
#[sim(component, base)]
pub struct Building {
    /// Center tile.
    pub tile: TilePos,
    /// Block content id.
    pub block: BlockId,
    /// Rotation in 90-degree steps.
    pub rotation: u8,
    /// Whether the block is enabled (`enabled`).
    pub enabled: bool,
    /// Last entity that disabled this building.
    pub last_disabler: Option<Entity>,
    /// Efficiency this frame (`efficiency`).
    pub efficiency: f32,
    /// Optional-consumer efficiency (`optionalEfficiency`).
    pub optional_efficiency: f32,
    /// Efficiency before optional subtraction (`potentialEfficiency`).
    pub potential_efficiency: f32,
    /// Whether the power consumer should run (`shouldConsumePower`).
    pub should_consume_power: bool,
    /// Overdrive time scale (`timeScale`).
    pub time_scale: f32,
    /// Remaining time-scale duration in ticks (`timeScaleDuration`).
    pub time_scale_duration: f32,
    /// Linked buildings in `Edges::edges(size)` order (`proximity`).
    pub proximity: SmallVec<[Entity; 6]>,
    /// Dump cursor (`cdump`).
    pub cdump: u8,
    /// Dump accumulator (`dumpAccum`).
    pub dump_accum: f32,
    /// Last player to access the building.
    pub last_accessed: Option<String>,
    /// Fog/visibility revision flags (`visibleFlags`).
    pub visible_flags: u64,
    /// Previous visibility (`wasVisible`).
    pub was_visible: bool,
    /// Whether the building was damaged last tick (`wasDamaged`).
    pub was_damaged: bool,
    /// Whether the entity is asleep (`sleeping`).
    pub sleeping: bool,
    /// Accumulated sleep time (`sleepTime`).
    pub sleep_time: f32,
    /// Whether `create()` completed (`initialized`).
    pub initialized: bool,
    /// Heal suppression countdown (`healSuppressionTime`).
    pub heal_suppression_time: f32,
    /// Last heal timestamp (`lastHealTime`).
    pub last_heal_time: f32,
    /// Last damage timestamp (`lastDamageTime`).
    pub last_damage_time: f32,
}

impl Building {
    /// Creates a building component for a placed block.
    pub fn new(tile: TilePos, block: BlockId, rotation: u8) -> Self {
        Self {
            tile,
            block,
            rotation,
            enabled: true,
            last_disabler: None,
            efficiency: 1.0,
            optional_efficiency: 1.0,
            potential_efficiency: 1.0,
            should_consume_power: true,
            time_scale: 1.0,
            time_scale_duration: 0.0,
            proximity: SmallVec::new(),
            cdump: 0,
            dump_accum: 0.0,
            last_accessed: None,
            visible_flags: 0,
            was_visible: false,
            was_damaged: false,
            sleeping: false,
            sleep_time: 0.0,
            initialized: true,
            heal_suppression_time: 0.0,
            last_heal_time: 0.0,
            last_damage_time: 0.0,
        }
    }
}

/// Per-building interval timers (`Interval(block.timers)`).
#[derive(Debug, Clone, Default, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct Timers(pub SmallVec<[f32; 4]>);

/// Current and last rotation of the building (helper read accessors).
impl Timers {
    /// Creates `len` zeroed timers.
    pub fn with_len(len: usize) -> Self {
        Self(smallvec::smallvec![0.0; len])
    }
}

/// `GenericCrafter`/`HeatCrafter` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct CrafterState {
    /// Craft progress (`progress`).
    pub progress: f32,
    /// Warmup (`warmup`).
    pub warmup: f32,
    /// Total progress accumulator (`totalProgress`).
    pub total_progress: f32,
    /// Output liquid accumulator (`outputAccumulator`).
    pub output_accumulator: SmallVec<[f32; 2]>,
    /// Deterministic RNG state used by `Separator` weighted output.
    pub rng: u64,
}

/// `Drill`/`BurstDrill` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct DrillState {
    /// Drill progress (`progress`).
    pub progress: f32,
    /// Warmup (`warmup`).
    pub warmup: f32,
    /// Time drilled (`timeDrilled`).
    pub time_drilled: f32,
    /// Last drill speed (`lastDrillSpeed`).
    pub last_drill_speed: f32,
    /// Dominant item being drilled.
    pub dominant_item: Option<ItemId>,
    /// Number of dominant tiles (`dominantItems`).
    pub dominant_items: u16,
}

/// `BeamDrill` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct BeamDrillState {
    /// Drill progress (`progress`).
    pub progress: f32,
    /// Warmup (`warmup`).
    pub warmup: f32,
    /// Last item count (`lastItemCount`).
    pub last_item_count: i32,
}

/// `Wall` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct WallState {
    /// Autotile bitmask (`autotile`).
    pub autotile_bits: u8,
    /// Hit flash timer (`hit`).
    pub hit: f32,
}

/// `Door`/`AutoDoor` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct DoorState {
    /// Whether the door is open (`open`).
    pub open: bool,
    /// Connected doors (`DoorBuild.chained`); unused by `AutoDoor`.
    pub chained: SmallVec<[Entity; 6]>,
}

/// `Radar` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct RadarState {
    /// Discovery progress (`progress`).
    pub progress: f32,
}

/// Generic sandbox source/sink state (`ItemSource`/`LiquidSource`).
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct SandboxState {
    /// Emission accumulator.
    pub accumulator: f32,
    /// Configured item (item source).
    pub item: Option<ItemId>,
}

/// `Pump`/`SolidPump` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct PumpState {
    /// Warmup (`warmup`).
    pub warmup: f32,
}

/// `Accelerator`/`LandingPad`/`LaunchPad` build state.
#[derive(Debug, Clone, Default, PartialEq, Component)]
pub struct CampaignState {
    /// Accelerator heat `0..1`.
    pub heat: f32,
    /// Landing-pad cooldown.
    pub cooldown: f32,
    /// Launch-pad accumulated launch time.
    pub launch_time: f32,
    /// Configured item (landing pad).
    pub item: Option<ItemId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_defaults_are_java_initial_values() {
        let building = Building::new(TilePos::new(3, 4), BlockId::STONE_WALL, 0);
        assert!(building.enabled);
        assert_eq!(building.efficiency, 1.0);
        assert_eq!(building.time_scale, 1.0);
        assert!(building.initialized);
        assert!(building.proximity.is_empty());
    }
}
