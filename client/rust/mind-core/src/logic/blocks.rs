// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logic-block integration seam (M3+).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/LogicBlock.java`
//! (`LogicBuild`). The concrete `BuildingBehavior` wiring, link fixpoint,
//! config/IO and revision-5 codec need plan 07's `BuildingBehavior` + ECS; this
//! module fixes the state shape, the privilege surface and the two plan-12
//! reconciliation traits so M3 can land without API churn.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use indexmap::IndexMap;

use crate::ecs::TeamId;
use crate::logic::executor::Executor;

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
    pub logic_var: Option<crate::logic::value::VarRef>,
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
}

/// Runtime state of one logic processor (`LogicBuild` minus behavior plumbing).
#[derive(Component, Debug)]
pub struct LogicBlockState {
    /// Program source.
    pub code: String,
    /// Per-processor VM.
    pub executor: Executor,
    /// Instruction accumulation (`LogicBuild.accumulator`).
    pub accumulator: f32,
    /// Links.
    pub links: Vec<LogicLink>,
    /// Link name → index cache.
    pub link_map: Option<IndexMap<String, usize>>,
    /// Duplicate-link check has run.
    pub checked_duplicates: bool,
    /// Instructions per tick (block kind data).
    pub ipt: i32,
    /// World-processor tag (max 32 chars).
    pub tag: Option<String>,
    /// World-processor icon tag.
    pub icon_tag: char,
    /// `@links` variable.
    pub links_var: Option<crate::logic::value::VarRef>,
    /// Deferred variable/wait application after a save load.
    pub load_pending: bool,
}

impl LogicBlockState {
    /// Creates a processor state with an empty program.
    pub fn new(ipt: i32) -> Self {
        Self {
            code: String::new(),
            executor: Executor::new(),
            accumulator: 0.0,
            links: Vec::new(),
            link_map: None,
            checked_duplicates: false,
            ipt,
            tag: None,
            icon_tag: '\0',
            links_var: None,
            load_pending: false,
        }
    }
}

/// World-processor privilege/rules access seam (plan 12 reconciliation, R1).
///
/// Today this reads the fields plan 12 M0–M2 landed; the remaining fields return
/// their upstream defaults and are replaced with direct `Rules` access at plan-12
/// M3+ without changing the VM API.
pub trait LogicRulesApi {
    /// `state.rules.editor`.
    fn editor(&self) -> bool;
    /// `state.rules.allowEditWorldProcessors`.
    fn allow_edit_world_processors(&self) -> bool;
    /// `state.rules.disableWorldProcessors`.
    fn disable_world_processors(&self) -> bool;
    /// `state.rules.worldProcessorPlayerLink`.
    fn world_processor_player_link(&self) -> bool;
    /// `state.rules.logicUnitControl`.
    fn logic_unit_control(&self) -> bool;
    /// `state.rules.logicUnitBuild`.
    fn logic_unit_build(&self) -> bool;
    /// `state.rules.logicUnitDeconstruct`.
    fn logic_unit_deconstruct(&self) -> bool;
    /// `state.rules.allowLogicData`.
    fn allow_logic_data(&self) -> bool;
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
    fn world_processor_player_link(&self) -> bool {
        true
    }
    fn logic_unit_control(&self) -> bool {
        false
    }
    fn logic_unit_build(&self) -> bool {
        false
    }
    fn logic_unit_deconstruct(&self) -> bool {
        false
    }
    fn allow_logic_data(&self) -> bool {
        false
    }
}

/// `LogicBuild.accessible()`.
pub fn accessible(privileged: bool, rules: &dyn LogicRulesApi) -> bool {
    !privileged || rules.editor() || rules.playtesting_map() || rules.allow_edit_world_processors()
}

/// `LogicTimeouts` (`LExecutor.unitTimeouts`), cleared on reset.
#[derive(Clone, Debug, Default)]
pub struct LogicTimeouts {
    /// Unit entity → last timeout sim time.
    pub timeouts: IndexMap<u32, f64>,
}

/// `LogicDisplay.displays` arena.
#[derive(Clone, Debug, Default)]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_surface_defaults() {
        let rules = DefaultLogicRules;
        assert!(accessible(false, &rules));
        assert!(!accessible(true, &rules));
        assert!(rules.world_processor_player_link());
        assert!(!rules.logic_unit_control());
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
}
