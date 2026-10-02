// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicDisplay`/`TileableLogicDisplay` (plan 13 M4).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/{LogicDisplay,
//! TileableLogicDisplay}.java`. The command queue, `operations` counter and the
//! linked-display arena are sim state; `FrameBuffer`/`processCommands` is plan
//! 16. Tileable multi-tile linking is represented by the root/index fields here
//! and completed with plan 16's render path.

use std::collections::VecDeque;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::io::entity::{EntityReader, EntityWriter};
use crate::logic::executor::draw::white_float_bits;
use crate::world::behavior::BuildingBehavior;

/// Runtime state of one display (`LogicDisplay.LogicDisplayBuild` minus GPU).
#[derive(Component, Debug, Clone, PartialEq)]
pub struct LogicDisplayState {
    /// Root display (self for non-tileable; bottom-left for tileable).
    pub root_display: Option<Entity>,
    /// Packed `Color.toFloatBits` (`LogicDisplayBuild.color`).
    pub color: f32,
    /// Stroke width (`LogicDisplayBuild.stroke`).
    pub stroke: f32,
    /// Packed `DisplayCmd` queue (cap `MAX_DISPLAY_BUFFER`).
    pub commands: VecDeque<u64>,
    /// Saved transform matrix (`Mat.val`, 4x4).
    pub transform: Option<[f32; 16]>,
    /// `LogicDisplayBuild.operations`.
    pub operations: u64,
    /// Arena index (`LogicDisplayBuild.index`).
    pub index: i32,
    /// `LogicDisplayBuild.processing`.
    pub processing: bool,
    /// Display size in pixels.
    pub display_size: i32,
}

impl Default for LogicDisplayState {
    fn default() -> Self {
        Self {
            root_display: None,
            color: white_float_bits(),
            stroke: 1.0,
            commands: VecDeque::new(),
            transform: None,
            operations: 0,
            index: -1,
            processing: false,
            display_size: 64,
        }
    }
}

impl LogicDisplayState {
    /// `LogicDisplayBuild.sense` size/buffer/operations values.
    pub fn buffer_size(&self) -> i32 {
        self.commands.len() as i32
    }
}

/// `LogicDisplay.LogicDisplayBuild` behavior (also used for tileable displays).
#[derive(Debug, Default, Clone, Copy)]
pub struct DisplayBehavior;

impl BuildingBehavior for DisplayBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let display_size = super::display_def_of(world, e)
            .map(|def| def.display_size)
            .or_else(|| super::tileable_def_of(world, e).map(|def| def.display_size))
            .unwrap_or(64);
        world.init_resource::<super::LogicDisplays>();
        let index = {
            let mut displays = world.resource_mut::<super::LogicDisplays>();
            displays.add(e) as i32
        };
        let state = LogicDisplayState {
            root_display: Some(e),
            display_size,
            index,
            ..Default::default()
        };
        world.entity_mut(e).insert(state);
    }

    fn on_removed(&self, world: &mut World, e: Entity) {
        let index = world
            .get::<LogicDisplayState>(e)
            .map(|state| state.index)
            .unwrap_or(-1);
        if index >= 0
            && let Some(mut displays) = world.get_resource_mut::<super::LogicDisplays>()
        {
            displays.remove(index as usize);
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        let Some(state) = world.get::<LogicDisplayState>(e) else {
            return;
        };
        match &state.transform {
            Some(matrix) => {
                w.bool(true);
                for value in matrix {
                    w.f(*value);
                }
            }
            None => w.bool(false),
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, revision: u8) {
        if revision < 1 {
            return;
        }
        let Ok(has_transform) = r.bool() else {
            return;
        };
        let transform = if has_transform {
            let mut matrix = [0.0f32; 16];
            for slot in &mut matrix {
                let Ok(value) = r.f() else { return };
                *slot = value;
            }
            Some(matrix)
        } else {
            None
        };
        if let Some(mut state) = world.get_mut::<LogicDisplayState>(e) {
            state.transform = transform;
        }
    }
}
