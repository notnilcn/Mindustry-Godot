// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logic-block integration (plan 13 M3+).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/*.java`. The concrete
//! `BuildingBehavior` implementations live in the submodules; this module owns
//! the shared state types, the plan-12 rules seam and the tile→building link
//! resolvers used by the VM's read/write instructions.

pub mod io;
pub mod logic_block;
pub mod memory;
pub mod switch;

pub use logic_block::{LogicBlockBehavior, LogicBlockState};
pub use memory::{MemSlot, MemoryBehavior, MemoryBlockState};
pub use switch::SwitchBehavior;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use indexmap::IndexMap;

use crate::content::BlockKind;
use crate::ecs::TeamId;
use crate::entities::comp::Building;
use crate::logic::executor::Executor;
use crate::logic::value::{LogicObject, VarRef};
use crate::world::TileBuilds;
use crate::world::block::BlockTable;
use crate::world::block_kind_data::{BlockKindData, LogicBlockDef, MemoryDef, SwitchDef};

/// `LogicBlock.LogicLink`.
#[derive(Clone, Debug, PartialEq)]
pub struct LogicLink {
    /// Valid (resolvable) link.
    pub valid: bool,
    /// Link x tile.
    pub x: i32,
    /// Link y tile.
    pub y: i32,
    /// Link name (`cell1`, `switch1`, …).
    pub name: String,
    /// Last resolved building.
    pub last_build: Option<Entity>,
    /// Variable bound to this link.
    pub logic_var: Option<VarRef>,
}

impl LogicLink {
    /// Creates an unresolved link.
    pub fn new(name: impl Into<String>, x: i32, y: i32) -> Self {
        Self {
            valid: false,
            x,
            y,
            name: name.into(),
            last_build: None,
            logic_var: None,
        }
    }

    /// `LogicLink.copy()`.
    pub fn copy(&self) -> Self {
        Self {
            valid: self.valid,
            x: self.x,
            y: self.y,
            name: self.name.clone(),
            last_build: None,
            logic_var: None,
        }
    }

    /// `LogicLink.trySet(exec, value)`: binds/clears the link's logic variable.
    pub fn try_set(&mut self, exec: &mut Executor, value: Option<Entity>) {
        let obj = value.map(LogicObject::Building);
        let id = match self.logic_var {
            Some(var) => var,
            None => {
                self.logic_var = exec.optional_var(&self.name).map(VarRef::Local);
                match self.logic_var {
                    Some(var) => var,
                    None => return,
                }
            }
        };
        exec.arena.get_mut(id.id()).set_link(obj);
    }
}

/// World-processor privilege/rules access seam (plan 12 reconciliation, R1).
///
/// Today this reads upstream defaults; plan 12 replaces the impl with direct
/// `Rules` access without changing the VM API.
pub trait LogicRulesApi {
    /// `state.rules.editor`.
    fn editor(&self) -> bool;
    /// `state.rules.allowEditWorldProcessors`.
    fn allow_edit_world_processors(&self) -> bool;
    /// `state.rules.disableWorldProcessors`.
    fn disable_world_processors(&self) -> bool;
    /// `state.rules.worldProcessorPlayerLink`.
    fn world_processor_player_link(&self) -> bool {
        true
    }
    /// `state.rules.defaultTeam`.
    fn default_team(&self) -> TeamId {
        TeamId::SHARDED
    }
    /// Whether this is a playtesting map (editor/test world).
    fn playtesting_map(&self) -> bool {
        false
    }
}

/// A rules source with upstream defaults; plan 12 supplies the real impl.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultLogicRules;

impl LogicRulesApi for DefaultLogicRules {
    fn editor(&self) -> bool {
        false
    }
    fn allow_edit_world_processors(&self) -> bool {
        false
    }
    fn disable_world_processors(&self) -> bool {
        false
    }
}

/// Rules seam resource (plan 12 reconciliation); optional.
#[derive(Resource)]
pub struct LogicRulesRes(pub Box<dyn LogicRulesApi + Send + Sync>);

/// `LogicBuild.accessible()`.
pub fn accessible(privileged: bool, rules: &dyn LogicRulesApi) -> bool {
    !privileged || rules.editor() || rules.playtesting_map() || rules.allow_edit_world_processors()
}

/// `LogicTimeouts` (`LExecutor.unitTimeouts`), cleared on reset.
#[derive(Clone, Debug, Default, Component)]
pub struct LogicTimeouts {
    /// Unit entity index → last timeout sim time.
    pub timeouts: IndexMap<u32, f64>,
}

impl LogicTimeouts {
    /// Refreshes a unit's timeout to `now` (`updateTimeout`).
    pub fn update_timeout(&mut self, unit: u32, now: f64) {
        self.timeouts.insert(unit, now);
    }

    /// Whether `delay` sim-time has elapsed since the unit's last timeout.
    pub fn timeout_done(&self, unit: u32, delay: f64, now: f64) -> bool {
        match self.timeouts.get(&unit) {
            Some(last) => now - last >= delay,
            None => true,
        }
    }

    /// Clears all timeouts (`ResetEvent`).
    pub fn clear(&mut self) {
        self.timeouts.clear();
    }
}

/// `LogicDisplay.displays` arena.
#[derive(Clone, Debug, Default, Component)]
pub struct LogicDisplays {
    /// Display slots (`None` = free).
    pub slots: Vec<Option<Entity>>,
}

impl LogicDisplays {
    /// Adds a display, reusing the first free slot.
    pub fn add(&mut self, entity: Entity) -> usize {
        if let Some(index) = self.slots.iter().position(Option::is_none) {
            self.slots[index] = Some(entity);
            index
        } else {
            self.slots.push(Some(entity));
            self.slots.len() - 1
        }
    }

    /// Removes a display slot.
    pub fn remove(&mut self, index: usize) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = None;
        }
    }

    /// Display entity at a slot (`commandImage` lookup).
    pub fn get(&self, index: usize) -> Option<Entity> {
        self.slots.get(index).copied().flatten()
    }

    /// Clears all slots (`ResetEvent`).
    pub fn clear(&mut self) {
        self.slots.clear();
    }
}

/// Building entity occupying tile `(x, y)` (`World.build`).
pub fn build_at(world: &World, x: i32, y: i32) -> Option<Entity> {
    world.get_resource::<TileBuilds>()?.get(x, y)
}

/// Content kind of a building entity.
pub fn block_kind_of(world: &World, e: Entity) -> Option<BlockKind> {
    let block = world.get::<Building>(e)?.block;
    let table = world.get_resource::<BlockTable>()?;
    Some(table.get(block)?.def.kind)
}

/// Content name of a building's block.
pub fn block_name_of(world: &World, e: Entity) -> Option<String> {
    let block = world.get::<Building>(e)?.block;
    let table = world.get_resource::<BlockTable>()?;
    Some(table.get(block)?.def.name.clone())
}

/// Block size in tiles of a building.
pub fn block_size_of(world: &World, e: Entity) -> Option<i32> {
    let block = world.get::<Building>(e)?.block;
    let table = world.get_resource::<BlockTable>()?;
    Some(table.get(block)?.def.size)
}

/// `Block.privileged` for a logic building.
pub fn privileged_of(world: &World, e: Entity) -> bool {
    let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
        return false;
    };
    let Some(table) = world.get_resource::<BlockTable>() else {
        return false;
    };
    let Some(inst) = table.get(block) else {
        return false;
    };
    match &inst.kind_data {
        BlockKindData::Logic(def) => def.privileged,
        BlockKindData::Memory(def) => def.privileged,
        BlockKindData::Switch(def) => def.privileged,
        _ => false,
    }
}

/// `LogicBlock` family knobs for a building.
pub fn logic_def_of(world: &World, e: Entity) -> Option<LogicBlockDef> {
    let block = world.get::<Building>(e)?.block;
    match &world.get_resource::<BlockTable>()?.get(block)?.kind_data {
        BlockKindData::Logic(def) => Some(def.clone()),
        _ => None,
    }
}

/// `MemoryBlock` family knobs for a building.
pub fn memory_def_of(world: &World, e: Entity) -> Option<MemoryDef> {
    let block = world.get::<Building>(e)?.block;
    match &world.get_resource::<BlockTable>()?.get(block)?.kind_data {
        BlockKindData::Memory(def) => Some(def.clone()),
        _ => None,
    }
}

/// `SwitchBlock` family knobs for a building.
pub fn switch_def_of(world: &World, e: Entity) -> Option<SwitchDef> {
    let block = world.get::<Building>(e)?.block;
    match &world.get_resource::<BlockTable>()?.get(block)?.kind_data {
        BlockKindData::Switch(def) => Some(def.clone()),
        _ => None,
    }
}

/// `Building.isValid()` (not dead).
pub fn is_valid_building(world: &World, e: Entity) -> bool {
    world
        .get::<crate::entities::comp::Health>(e)
        .is_some_and(|h| h.health > 0.0)
}

/// `LogicBlock.getLinkName(block)`: link base name for a block name.
pub fn link_name_for(block_name: &str) -> String {
    if block_name.contains('-') {
        let split: Vec<&str> = block_name.split('-').collect();
        if split.len() >= 2 {
            let last = split[split.len() - 1];
            if last == "large" || last.parse::<f32>().is_ok() {
                return split[split.len() - 2].to_owned();
            }
            return last.to_owned();
        }
    }
    block_name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_names_match_upstream() {
        assert_eq!(link_name_for("memory-cell"), "cell");
        assert_eq!(link_name_for("memory-bank"), "bank");
        assert_eq!(link_name_for("switch"), "switch");
        assert_eq!(link_name_for("world-switch"), "switch");
        assert_eq!(link_name_for("logic-processor"), "processor");
        assert_eq!(link_name_for("large-logic-display"), "display");
        assert_eq!(link_name_for("large-canvas"), "canvas");
        assert_eq!(link_name_for("reinforced-message"), "message");
        assert_eq!(link_name_for("copper-wall-large"), "wall");
    }

    #[test]
    fn display_arena_reuses_slots() {
        let mut displays = LogicDisplays::default();
        let entity = Entity::from_raw_u32(1).expect("valid test entity index");
        assert_eq!(displays.add(entity), 0);
        assert_eq!(displays.add(entity), 1);
        displays.remove(0);
        assert_eq!(displays.add(entity), 0);
    }

    #[test]
    fn timeouts_track_delay() {
        let mut timeouts = LogicTimeouts::default();
        assert!(timeouts.timeout_done(7, 90.0, 100.0));
        timeouts.update_timeout(7, 100.0);
        assert!(!timeouts.timeout_done(7, 90.0, 100.0));
        assert!(timeouts.timeout_done(7, 90.0, 190.0));
        timeouts.clear();
        assert!(timeouts.timeout_done(7, 90.0, 100.0));
    }

    #[test]
    fn compress_codec_roundtrip() {
        let links = vec![
            LogicLink::new("cell1", 3, 4),
            LogicLink::new("switch1", 2, 5),
        ];
        let bytes = logic_block::compress("set x 1\n", &links);
        assert!(!bytes.is_empty());
        // zlib header (0x78) is the Java `DeflaterOutputStream` default.
        assert_eq!(bytes[0], 0x78);
    }

    #[test]
    fn find_link_name_increments() {
        let links = vec![LogicLink::new("cell1", 0, 0), LogicLink::new("cell3", 0, 0)];
        assert_eq!(logic_block::find_link_name(&links, "cell"), "cell2");
        let empty: Vec<LogicLink> = Vec::new();
        assert_eq!(logic_block::find_link_name(&empty, "cell"), "cell1");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::io::CellValue;
    use super::*;
    use crate::io::wire::{WireReader, WireWriter};
    use crate::logic::value::LogicObject;
    use crate::world::ConfigValue;
    use crate::world::building_io::BuildingCodec;
    use crate::world::harness::BuildHarness;

    fn processor_rig() -> (BuildHarness, Entity, Entity) {
        let mut harness = BuildHarness::new(16, 16, 1);
        let processor = harness
            .content()
            .block_id("micro-processor")
            .expect("micro-processor");
        let memory = harness
            .content()
            .block_id("memory-cell")
            .expect("memory-cell");
        assert!(harness.place(4, 4, processor, 0, true));
        assert!(harness.place(5, 4, memory, 0, true));
        let pe = harness.build_at(4, 4).expect("processor entity");
        let me = harness.build_at(5, 4).expect("memory entity");
        (harness, pe, me)
    }

    #[test]
    fn logic_link_write_read() {
        let (mut harness, pe, me) = processor_rig();
        let code = "write 7 cell1 0\nwrite 8 cell1 1\nread r cell1 0\n";
        assert!(harness.configure(
            4,
            4,
            ConfigValue::Bytes(logic_block::compress(code, &[]).into())
        ));
        assert!(harness.configure(4, 4, ConfigValue::Point2(5, 4)));
        for _ in 0..4 {
            harness.tick();
        }

        let state = harness.world.get::<LogicBlockState>(pe).expect("state");
        assert_eq!(state.code, code);
        assert_eq!(state.links.len(), 1);
        assert_eq!(state.links[0].name, "cell1");
        assert_eq!(state.executor.links, vec![me]);

        let links_id = state.executor.arena.get_id("@links").expect("@links");
        assert_eq!(state.executor.arena.get(links_id).num, 1.0);

        let r = state
            .executor
            .optional_var("r")
            .map(|id| state.executor.arena.get(id).num());
        assert_eq!(r, Some(7.0));

        let memory = harness
            .world
            .get::<MemoryBlockState>(me)
            .expect("memory state");
        assert_eq!(memory.read(0), CellValue::Num(7.0));
        assert_eq!(memory.read(1), CellValue::Num(8.0));
    }

    #[test]
    fn logic_build_revision5_roundtrip() {
        let (mut harness, pe, _me) = processor_rig();
        let code = "set a 3\nop add b a 4\nwait 1.5\n";
        assert!(harness.configure(
            4,
            4,
            ConfigValue::Bytes(logic_block::compress(code, &[]).into())
        ));
        for _ in 0..2 {
            harness.tick();
        }

        let mut buf = Vec::new();
        {
            let mut writer = WireWriter::new(&mut buf);
            BuildingCodec::write(&harness.world, pe, &mut writer, false).expect("write");
        }

        let processor = harness
            .content()
            .block_id("micro-processor")
            .expect("micro-processor");
        assert!(harness.place(8, 8, processor, 0, true));
        let pe2 = harness.build_at(8, 8).expect("processor 2");
        let mut reader = WireReader::new(&buf);
        // `fog=false` writes the version-3 base layout (no visible-flags field).
        BuildingCodec::read(&mut harness.world, pe2, &mut reader, 3).expect("read");

        let state = harness.world.get::<LogicBlockState>(pe2).expect("state");
        assert_eq!(state.code, code);
        let a = state
            .executor
            .optional_var("a")
            .map(|id| state.executor.arena.get(id).num());
        assert_eq!(a, Some(3.0));
        assert_eq!(state.ipt, 2);
    }

    #[test]
    fn memory_revision_roundtrip() {
        let mut harness = BuildHarness::new(16, 16, 1);
        let memory = harness
            .content()
            .block_id("memory-cell")
            .expect("memory-cell");
        assert!(harness.place(3, 3, memory, 0, true));
        let me = harness.build_at(3, 3).expect("memory");
        {
            let mut state = harness
                .world
                .get_mut::<MemoryBlockState>(me)
                .expect("memory state");
            state.write(0, &CellValue::Num(42.0));
            state.write(1, &CellValue::Obj(Some(LogicObject::Str("hi".into()))));
        }

        let inst = harness
            .world
            .get_resource::<BlockTable>()
            .expect("table")
            .get_named("memory-cell")
            .expect("memory instance")
            .clone();
        let mut buf = Vec::new();
        {
            let mut writer = WireWriter::new(&mut buf);
            inst.behavior.write(&harness.world, me, &mut writer);
        }

        assert!(harness.place(4, 4, memory, 0, true));
        let me2 = harness.build_at(4, 4).expect("memory 2");
        let mut reader = WireReader::new(&buf);
        inst.behavior.read(&mut harness.world, me2, &mut reader, 1);

        let state = harness
            .world
            .get::<MemoryBlockState>(me2)
            .expect("memory state 2");
        assert_eq!(state.read(0), CellValue::Num(42.0));
        assert_eq!(
            state.read(1),
            CellValue::Obj(Some(LogicObject::Str("hi".into())))
        );
        // Out of range reads null.
        assert_eq!(state.read(9999), CellValue::Obj(None));
    }

    #[test]
    fn link_names_increment() {
        let mut harness = BuildHarness::new(16, 16, 1);
        let processor = harness
            .content()
            .block_id("micro-processor")
            .expect("micro-processor");
        let memory = harness
            .content()
            .block_id("memory-cell")
            .expect("memory-cell");
        assert!(harness.place(4, 4, processor, 0, true));
        assert!(harness.place(5, 4, memory, 0, true));
        assert!(harness.place(4, 5, memory, 0, true));
        let code = "write 1 cell1 0\nwrite 2 cell2 0\n";
        assert!(harness.configure(
            4,
            4,
            ConfigValue::Bytes(logic_block::compress(code, &[]).into())
        ));
        assert!(harness.configure(4, 4, ConfigValue::Point2(5, 4)));
        assert!(harness.configure(4, 4, ConfigValue::Point2(4, 5)));
        let pe = harness.build_at(4, 4).expect("processor");
        let state = harness.world.get::<LogicBlockState>(pe).expect("state");
        let mut names: Vec<String> = state.links.iter().map(|l| l.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["cell1".to_string(), "cell2".to_string()]);
    }
}
