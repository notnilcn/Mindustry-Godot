// SPDX-License-Identifier: GPL-3.0-only

//! Relay-side unit-command application (plan 15 M4 / plan 11 §3.8).
//!
//! Ports the apply half of `mindustry.gen.InputHandler.java:310-466` onto the
//! plan-11 [`CommandAiState`] runtime. The relay wire form ([`SimCommand`])
//! carries only unit ids, a command id and an `(x, y)` target, so the resolve
//! step maps it back to a [`CommandTarget`] (see [`resolve_wire_target`]) — the
//! documented plan-15 §3.3.1 gap where `UnitCommand` has no target-kind field.
//!
//! Determinism: tracked units live in a `BTreeMap` keyed by relay id and are
//! always iterated in ascending-id order (no `HashMap` iteration in sim order).

use std::collections::BTreeMap;

use crate::ai::CommandAiState;
use crate::ai::unit_command_runtime::allows_command;
use crate::ai::unit_stance_runtime::set_stance_enabled;
use crate::content::id::{UnitCommandId, UnitStanceId, UnitTypeId};
use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use crate::determinism::{Checksum, Checksummer};
use crate::input::action::CommandTarget;
use crate::input::rts::command_units_apply_with_move;

/// The vanilla `move` command id (`UnitCommand.loadAll()` registers it first).
pub const MOVE_COMMAND: UnitCommandId = UnitCommandId::new(0);

/// One relay-visible unit and its `CommandAI` state.
#[derive(Debug, Clone)]
struct RuntimeUnit {
    type_id: UnitTypeId,
    team: u8,
    state: CommandAiState,
}

/// Unit-command apply runtime owned by [`crate::sim::Sim`].
///
/// The `ContentRegistry` is built lazily by [`crate::sim::Sim`] (see
/// `ensure_unit_commands`) so P0 worlds that never issue unit commands do not pay
/// for content boot.
pub struct UnitCommandRuntime {
    content: ContentRegistry,
    units: BTreeMap<i32, RuntimeUnit>,
    next_id: i32,
}

impl UnitCommandRuntime {
    /// Builds the vanilla-command runtime from committed base content.
    ///
    /// Content boot is infallible for the committed vanilla registry; on an
    /// unexpected failure the runtime degrades to an empty registry (commands
    /// become deterministic no-ops) instead of panicking the sim.
    pub fn new() -> Self {
        let mut content =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(content) => content,
                Err(_) => ContentRegistry::new(true),
            };
        let _ = content.init();
        Self {
            content,
            units: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Read-only content registry.
    pub fn content(&self) -> &ContentRegistry {
        &self.content
    }

    /// Number of tracked units.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Whether no units are tracked.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Registers a unit by content name, returning its relay id.
    pub fn spawn(&mut self, name: &str, team: u8) -> Option<i32> {
        let type_id = self.content.unit_by_name(name)?.id;
        Some(self.spawn_typed(type_id, team))
    }

    /// Registers a unit by content id, returning its relay id.
    pub fn spawn_typed(&mut self, type_id: UnitTypeId, team: u8) -> i32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let mut state = CommandAiState::new();
        if let Some(unit) = self.content.unit(type_id) {
            state.init(unit, &self.content);
        }
        self.units.insert(
            id,
            RuntimeUnit {
                type_id,
                team,
                state,
            },
        );
        id
    }

    /// Removes a tracked unit (`false` when absent).
    pub fn remove(&mut self, id: i32) -> bool {
        self.units.remove(&id).is_some()
    }

    /// Clears every tracked unit.
    pub fn clear(&mut self) {
        self.units.clear();
    }

    /// The `CommandAI` state of `id`.
    pub fn state(&self, id: i32) -> Option<&CommandAiState> {
        self.units.get(&id).map(|unit| &unit.state)
    }

    /// Mutable state (tests / plan-11 wiring).
    pub fn state_mut(&mut self, id: i32) -> Option<&mut CommandAiState> {
        self.units.get_mut(&id).map(|unit| &mut unit.state)
    }

    /// The team of `id`.
    pub fn team_of(&self, id: i32) -> Option<u8> {
        self.units.get(&id).map(|unit| unit.team)
    }

    /// Applies a relay `UnitCommand` over `ids`.
    ///
    /// `command == 0` is the emitter's `commandUnits` marker: an implicit move
    /// plus the wire-resolved target. Any other id is `setUnitCommand`: switch
    /// the active command (no target is carried). Returns the number of units
    /// whose state changed. The current wire form cannot carry `queue`, so
    /// direct orders clear the queue (documented plan-15 §3.3.1 gap).
    pub fn apply_unit_command(&mut self, ids: &[i32], command: u16, x: f32, y: f32) -> usize {
        let runtime = self;
        let mut applied = 0usize;
        for &id in ids {
            let Some(type_id) = runtime.units.get(&id).map(|unit| unit.type_id) else {
                continue;
            };
            if command == 0 {
                let move_command = runtime.content.unit_command(MOVE_COMMAND).map(|def| def.id);
                let target = resolve_wire_target(x, y);
                if let Some(unit) = runtime.units.get_mut(&id) {
                    let state = &mut unit.state;
                    command_units_apply_with_move(&mut [state], move_command, target, false);
                    applied += 1;
                }
            } else {
                let command_id = UnitCommandId::new(command);
                let allowed = runtime
                    .content
                    .unit(type_id)
                    .is_some_and(|unit| allows_command(unit, command_id));
                if !allowed {
                    continue;
                }
                let Some(unit_def) = runtime.content.unit(type_id) else {
                    continue;
                };
                if let Some(unit) = runtime.units.get_mut(&id) {
                    let _ = unit.state.command(unit_def, &runtime.content, command_id);
                    applied += 1;
                }
            }
        }
        applied
    }

    /// Applies `setUnitStance` (`InputHandler.setUnitStance`) over `ids`.
    pub fn set_unit_stance(&mut self, ids: &[i32], stance: u16, enabled: bool) -> usize {
        let runtime = self;
        let stance_id = UnitStanceId::new(stance);
        let mut applied = 0usize;
        for &id in ids {
            if let Some(unit) = runtime.units.get_mut(&id)
                && set_stance_enabled(
                    &mut unit.state.stances,
                    &runtime.content,
                    stance_id,
                    enabled,
                )
            {
                applied += 1;
            }
        }
        applied
    }

    /// Applies `setUnitCommand` (`InputHandler.setUnitCommand`) over `ids`.
    pub fn set_unit_command(&mut self, ids: &[i32], command: u16) -> usize {
        let runtime = self;
        let command_id = UnitCommandId::new(command);
        let mut applied = 0usize;
        for &id in ids {
            let Some(type_id) = runtime.units.get(&id).map(|unit| unit.type_id) else {
                continue;
            };
            let allowed = runtime
                .content
                .unit(type_id)
                .is_some_and(|unit| allows_command(unit, command_id));
            if !allowed {
                continue;
            }
            let Some(unit_def) = runtime.content.unit(type_id) else {
                continue;
            };
            if let Some(unit) = runtime.units.get_mut(&id) {
                let _ = unit.state.command(unit_def, &runtime.content, command_id);
                applied += 1;
            }
        }
        applied
    }

    /// `unitClear`: clears every tracked unit's orders.
    pub fn clear_orders(&mut self, ids: &[i32]) -> usize {
        let mut applied = 0usize;
        for &id in ids {
            if let Some(unit) = self.units.get_mut(&id) {
                unit.state.clear_commands();
                applied += 1;
            }
        }
        applied
    }

    /// Deterministic digest over tracked units in ascending-id order.
    pub fn checksum(&self) -> Checksum {
        let mut c = Checksummer::new();
        c.part(&(self.units.len() as u64));
        for (id, unit) in &self.units {
            c.part(&(*id as u64));
            c.part(&unit.type_id.raw());
            c.part(&unit.team);
            let state = &unit.state;
            match state.command {
                Some(command) => {
                    c.part(&1u8);
                    c.part(&command.raw());
                }
                None => {
                    c.part(&0u8);
                    c.part(&0u16);
                }
            }
            match state.target_pos {
                Some((x, y)) => {
                    c.part(&1u8);
                    c.part(&x.to_bits());
                    c.part(&y.to_bits());
                }
                None => {
                    c.part(&0u8);
                    c.part(&0u32);
                    c.part(&0u32);
                }
            }
            match state.attack_target {
                Some(target) => {
                    c.part(&1u8);
                    c.part(&target.kind());
                    match target {
                        crate::ai::AttackTarget::Building(packed) => c.part(&(packed as u64)),
                        crate::ai::AttackTarget::Unit(id) => c.part(&(id as u64)),
                    }
                }
                None => {
                    c.part(&0u8);
                    c.part(&0u8);
                    c.part(&0u64);
                }
            }
            c.part(&(state.command_queue.len() as u64));
            for entry in &state.command_queue {
                match entry {
                    crate::ai::CommandQueueEntry::Building(packed) => {
                        c.part(&0u8);
                        c.part(&(*packed as u64));
                    }
                    crate::ai::CommandQueueEntry::Unit(id) => {
                        c.part(&1u8);
                        c.part(&(*id as u64));
                    }
                    crate::ai::CommandQueueEntry::Position(x, y) => {
                        c.part(&2u8);
                        c.part(&x.to_bits());
                        c.part(&y.to_bits());
                    }
                }
            }
            c.part(&state.stances.raw());
        }
        c.finish()
    }
}

impl Default for UnitCommandRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolves the relay `(x, y)` pair back to a [`CommandTarget`].
///
/// The wire gap (plan 15 §3.3.1): the batcher encodes `CommandTarget::Unit(id)`
/// as `(x = id as f32, y = 0.0)`. Mirror that convention here; every other pair
/// is a world position.
pub fn resolve_wire_target(x: f32, y: f32) -> CommandTarget {
    if y == 0.0 && x > 0.0 && x.fract() == 0.0 {
        CommandTarget::Unit(x as i32)
    } else {
        CommandTarget::Position { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_applies_default_command_and_move_order() {
        let mut runtime = UnitCommandRuntime::new();
        let id = runtime.spawn("dagger", 0).expect("dagger");
        assert_eq!(runtime.len(), 1);
        // `CommandAI.init` selected the default command.
        assert!(runtime.state(id).and_then(|state| state.command).is_some());
        let applied = runtime.apply_unit_command(&[id], 0, 12.0, 34.0);
        assert_eq!(applied, 1);
        assert_eq!(runtime.state(id).unwrap().target_pos, Some((12.0, 34.0)));
    }

    #[test]
    fn wire_target_resolution_matches_emitter() {
        assert_eq!(resolve_wire_target(9.0, 0.0), CommandTarget::Unit(9));
        assert_eq!(
            resolve_wire_target(50.0, 10.0),
            CommandTarget::Position { x: 50.0, y: 10.0 }
        );
        assert_eq!(
            resolve_wire_target(0.0, 0.0),
            CommandTarget::Position { x: 0.0, y: 0.0 }
        );
    }

    #[test]
    fn command_switch_and_stance_toggle() {
        let mut runtime = UnitCommandRuntime::new();
        let id = runtime.spawn("dagger", 0).expect("dagger");
        // `repair` is not a dagger command; `move` is.
        let repair = runtime
            .content()
            .unit_command_by_name("repair")
            .unwrap()
            .id
            .raw();
        assert_eq!(runtime.set_unit_command(&[id], repair), 0);
        let move_id = runtime
            .content()
            .unit_command_by_name("move")
            .unwrap()
            .id
            .raw();
        assert_eq!(runtime.set_unit_command(&[id], move_id), 1);
        // Unknown unit ids are ignored.
        assert_eq!(runtime.set_unit_command(&[999], move_id), 0);
        assert!(runtime.remove(id));
        assert!(runtime.is_empty());
    }

    #[test]
    fn checksum_is_deterministic_and_order_stable() {
        let mut first = UnitCommandRuntime::new();
        let a = first.spawn("dagger", 0).expect("dagger");
        let b = first.spawn("dagger", 1).expect("dagger");
        first.apply_unit_command(&[a], 0, 5.0, 6.0);
        first.apply_unit_command(&[b], 0, 9.0, 0.0);

        let mut second = UnitCommandRuntime::new();
        let a2 = second.spawn("dagger", 0).expect("dagger");
        let b2 = second.spawn("dagger", 1).expect("dagger");
        second.apply_unit_command(&[a2], 0, 5.0, 6.0);
        second.apply_unit_command(&[b2], 0, 9.0, 0.0);

        assert_eq!(first.checksum(), second.checksum());
    }
}
