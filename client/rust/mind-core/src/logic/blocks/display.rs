// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicDisplay`/`TileableLogicDisplay` (plan 13 M4).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/{LogicDisplay,
//! TileableLogicDisplay}.java`. The command queue, `operations` counter and the
//! linked-display arena are sim state; `FrameBuffer`/`processCommands` is plan
//! 16. Tileable multi-tile linking is represented by the root/index fields here
//! and completed with plan 16's render path.

use std::collections::{BTreeSet, VecDeque};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::{Building, TeamComp};
use crate::io::entity::{EntityReader, EntityWriter};
use crate::logic::executor::draw::white_float_bits;
use crate::world::behavior::BuildingBehavior;

/// Arc `Geometry.d8` (x, y offsets in bit order): `(1,0), (1,1), (0,1),
/// (-1,1), (-1,0), (-1,-1), (0,-1), (1,-1)`.
pub const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Arc `Geometry.d8edge` (diagonal neighbours).
pub const D8_EDGE: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

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
    /// `TileableLogicDisplayBuild.tilesWidth`.
    pub tiles_width: i32,
    /// `TileableLogicDisplayBuild.tilesHeight`.
    pub tiles_height: i32,
    /// `TileableLogicDisplayBuild.originX` (bottom-left tile).
    pub origin_x: i32,
    /// `TileableLogicDisplayBuild.originY` (bottom-left tile).
    pub origin_y: i32,
    /// `TileableLogicDisplayBuild.bits` (8-neighbour same-block mask).
    pub bits: u8,
    /// `TileableLogicDisplayBuild.needsUpdate`.
    pub needs_update: bool,
    /// `TileableLogicDisplayBuild.rectangular`.
    pub rectangular: bool,
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
            tiles_width: 1,
            tiles_height: 1,
            origin_x: 0,
            origin_y: 0,
            bits: 0,
            needs_update: false,
            rectangular: false,
        }
    }
}

impl LogicDisplayState {
    /// `LogicDisplayBuild.sense` size/buffer/operations values.
    pub fn buffer_size(&self) -> i32 {
        self.commands.len() as i32
    }
}

/// `TileableLogicDisplay.linkDisplays(start)`: floods same-block proximity,
/// picks the bottom-left root and assigns every member's root/geometry.
pub fn link_displays(world: &mut World, start: Entity) {
    let mut queue: Vec<Entity> = vec![start];
    let mut members: Vec<Entity> = vec![start];
    let mut processed: BTreeSet<u32> = BTreeSet::new();
    let mut root: Option<Entity> = None;
    let mut top_x = 0i32;
    let mut top_y = 0i32;
    let mut bot_x = 0i32;
    let mut bot_y = 0i32;
    while let Some(next) = queue.pop() {
        processed.insert(next.index().index());
        let Some(building) = world.get::<Building>(next) else {
            continue;
        };
        let (tx, ty) = (i32::from(building.tile.x()), i32::from(building.tile.y()));
        let block = building.block;
        let proximity: Vec<Entity> = building.proximity.iter().copied().collect();
        match root {
            None => {
                root = Some(next);
                top_x = tx;
                top_y = ty;
                bot_x = tx;
                bot_y = ty;
            }
            Some(current) => {
                let root_tile = world
                    .get::<Building>(current)
                    .map(|b| (i32::from(b.tile.x()), i32::from(b.tile.y())))
                    .unwrap_or((tx, ty));
                // `next.tile.x < root.tile.x || next.tile.y < root.tile.y`.
                if tx < root_tile.0 || ty < root_tile.1 {
                    root = Some(next);
                }
            }
        }
        top_x = top_x.max(tx);
        top_y = top_y.max(ty);
        bot_x = bot_x.min(tx);
        bot_y = bot_y.min(ty);
        for prox in proximity {
            let same_block = world.get::<Building>(prox).map(|b| b.block) == Some(block);
            if same_block
                && world.get::<LogicDisplayState>(prox).is_some()
                && processed.insert(prox.index().index())
            {
                queue.push(prox);
                members.push(prox);
            }
        }
    }
    let Some(root) = root else {
        return;
    };
    let width = top_x - bot_x + 1;
    let height = top_y - bot_y + 1;
    let rectangular = width * height == members.len() as i32;
    for member in members {
        if let Some(mut state) = world.get_mut::<LogicDisplayState>(member) {
            state.needs_update = false;
            state.rectangular = rectangular;
            state.root_display = Some(root);
            state.tiles_width = width;
            state.tiles_height = height;
            state.origin_x = bot_x;
            state.origin_y = bot_y;
        }
    }
}

/// `TileableLogicDisplayBuild.onProximityUpdate`: the 8-neighbour same-block
/// mask (`TileBitmask` input).
pub fn update_tileable_bits(world: &mut World, e: Entity) {
    let Some(building) = world.get::<Building>(e) else {
        return;
    };
    let (x, y) = (i32::from(building.tile.x()), i32::from(building.tile.y()));
    let block = building.block;
    let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
    let mut bits = 0u8;
    for (i, (dx, dy)) in D8.iter().enumerate() {
        let Some(other) = super::build_at(world, x + dx, y + dy) else {
            continue;
        };
        if world.get::<Building>(other).map(|b| b.block) == Some(block)
            && world.get::<TeamComp>(other).map(|t| t.team) == Some(team)
        {
            bits |= 1 << i;
        }
    }
    if let Some(mut state) = world.get_mut::<LogicDisplayState>(e) {
        state.bits = bits;
    }
}

/// `TileableLogicDisplayBuild.updateOthers()`: the four diagonal neighbours
/// recompute their bits.
fn update_tileable_others(world: &mut World, e: Entity) {
    let Some(building) = world.get::<Building>(e) else {
        return;
    };
    let (x, y) = (i32::from(building.tile.x()), i32::from(building.tile.y()));
    let block = building.block;
    for (dx, dy) in D8_EDGE {
        let Some(other) = super::build_at(world, x + dx, y + dy) else {
            continue;
        };
        if world.get::<Building>(other).map(|b| b.block) == Some(block)
            && world.get::<LogicDisplayState>(other).is_some()
        {
            update_tileable_bits(world, other);
        }
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

/// `TileableLogicDisplay.TileableLogicDisplayBuild` behavior: relinks the
/// display group when the proximity grid changes.
#[derive(Debug, Default, Clone, Copy)]
pub struct TileableDisplayBehavior;

impl BuildingBehavior for TileableDisplayBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        DisplayBehavior.create_state(world, e);
    }

    fn on_removed(&self, world: &mut World, e: Entity) {
        DisplayBehavior.on_removed(world, e);
    }

    fn version(&self, world: &World, e: Entity) -> u8 {
        DisplayBehavior.version(world, e)
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        DisplayBehavior.write(world, e, w);
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, revision: u8) {
        DisplayBehavior.read(world, e, r, revision);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let needs_update = world
            .get::<LogicDisplayState>(e)
            .is_some_and(|state| state.needs_update);
        if needs_update {
            link_displays(world, e);
        }
    }

    fn on_proximity_added(&self, world: &mut World, e: Entity) {
        // `needsUpdate = true; updateOthers();` (`TileableLogicDisplay.java:275-281`).
        if let Some(mut state) = world.get_mut::<LogicDisplayState>(e) {
            state.needs_update = true;
        }
        update_tileable_others(world, e);
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        // `bits = 0; for(i = 0; i < 8; i++){ ... }` (`TileableLogicDisplay.java:151-162`).
        update_tileable_bits(world, e);
    }

    fn on_proximity_removed(&self, world: &mut World, e: Entity) {
        // `for(var other : proximity){ if(other instanceof Tileable && ...)
        // other.needsUpdate = true; } updateOthers();` (`:284-297`).
        let proximity: Vec<Entity> = world
            .get::<Building>(e)
            .map(|building| building.proximity.iter().copied().collect())
            .unwrap_or_default();
        let mut processed: BTreeSet<u32> = BTreeSet::new();
        for other in proximity {
            if world.get::<LogicDisplayState>(other).is_some()
                && processed.insert(other.index().index())
                && let Some(mut state) = world.get_mut::<LogicDisplayState>(other)
            {
                state.needs_update = true;
            }
        }
        update_tileable_others(world, e);
    }
}
