// SPDX-License-Identifier: GPL-3.0-only

//! Relay-side unit-command application (plan 15 M4 / plan 11 §3.8).
//!
//! Ports the apply half of `mindustry.gen.InputHandler.java:310-466` onto the
//! plan-11 [`CommandAiState`] runtime. The relay wire form ([`SimCommand`])
//! carries only unit ids, a command id and an `(x, y)` target, so the resolve
//! step maps it back to a [`CommandTarget`] (see [`resolve_wire_target`]);
//! [`pack_wire_target`] is the lossless encoder for that pair (plan-15 §3.3.1
//! target-kind gap: `EntitySeq` ids exceed an f32's 24-bit integer range).
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
    /// plus the wire-resolved target. `queue` appends the target to the unit's
    /// command queue instead of replacing it (`commandUnits(..., queueCommand)`;
    /// I01 T13). Any other id is `setUnitCommand`: switch the active command
    /// (no target is carried). Returns the number of units whose state changed.
    pub fn apply_unit_command(
        &mut self,
        ids: &[i32],
        command: u16,
        x: f32,
        y: f32,
        queue: bool,
    ) -> usize {
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
                    command_units_apply_with_move(&mut [state], move_command, target, queue);
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

/// Wire packing base for unit targets: `x = id & 0xFFFF`,
/// `y = WIRE_UNIT_BASE + (id >> 16)` (see [`pack_wire_target`]).
pub const WIRE_UNIT_BASE: f32 = 1_000_000.0;
/// Wire packing base for building targets:
/// `x = tile.x`, `y = WIRE_BUILDING_BASE + tile.y + 32768`.
pub const WIRE_BUILDING_BASE: f32 = 2_000_000.0;

/// Packs a [`CommandTarget`] into the `UnitCommand` wire `(x, y)` pair.
///
/// `EntitySeq` ids live at/above [`crate::sim::unit_runtime::UNIT_SEQ_BASE`]
/// (`1_000_000_000`) and do not fit an f32's 24-bit integer mantissa, so the
/// old `x = id as f32` encoding rounded them (`1_000_000_002 as f32 ==
/// 1_000_000_000`) and attack orders resolved the wrong entity (H03 blocker
/// #2). The id is split into two exactly-representable 16-bit halves instead.
/// Building targets pack the tile pair; [`CommandTarget::Position`] stays raw
/// world pixels.
///
/// The bases cannot collide with a real position: tiles are `i16`, so world
/// pixels satisfy `|v| <= 32768 * 8 = 262_144` (`Tile.worldx()`, Tile.java:
/// 166-171), far below [`WIRE_UNIT_BASE`]. Both packed `y` forms are integers
/// below `2^24` and the packed `x` is a 16-bit integer, so every half
/// round-trips exactly.
pub fn pack_wire_target(target: &CommandTarget) -> (f32, f32) {
    match target {
        CommandTarget::Position { x, y } => (*x, *y),
        CommandTarget::Unit(id) => {
            let id = *id as u32;
            ((id & 0xFFFF) as f32, WIRE_UNIT_BASE + (id >> 16) as f32)
        }
        CommandTarget::Building(pos) => (
            pos.x() as f32,
            WIRE_BUILDING_BASE + (pos.y() as i32 + 32768) as f32,
        ),
    }
}

/// Resolves the relay `(x, y)` pair back to a [`CommandTarget`] — the exact
/// inverse of [`pack_wire_target`].
pub fn resolve_wire_target(x: f32, y: f32) -> CommandTarget {
    if y >= WIRE_BUILDING_BASE {
        CommandTarget::Building(crate::world::TilePos::new(
            x as i16,
            ((y - WIRE_BUILDING_BASE) as i32 - 32768) as i16,
        ))
    } else if y >= WIRE_UNIT_BASE {
        let low = (x as i32 as u32) & 0xFFFF;
        let id = (((y - WIRE_UNIT_BASE) as u32) << 16) | low;
        CommandTarget::Unit(id as i32)
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
        let applied = runtime.apply_unit_command(&[id], 0, 12.0, 34.0, false);
        assert_eq!(applied, 1);
        assert_eq!(runtime.state(id).unwrap().target_pos, Some((12.0, 34.0)));
    }

    #[test]
    fn wire_target_resolution_matches_emitter() {
        assert_eq!(
            resolve_wire_target(50.0, 10.0),
            CommandTarget::Position { x: 50.0, y: 10.0 }
        );
        assert_eq!(
            resolve_wire_target(0.0, 0.0),
            CommandTarget::Position { x: 0.0, y: 0.0 }
        );
    }

    /// Regression for H03 blocker #2: the old `x = id as f32` wire rounded
    /// `EntitySeq` ids (`1_000_000_000` and `1_000_000_002` share one f32),
    /// so an attack order resolved the wrong entity.
    #[test]
    fn wire_target_round_trips_large_unit_ids() {
        for id in [1, 9, 1_000_000_001, 1_000_000_002, (1 << 24) + 1, i32::MAX] {
            let target = CommandTarget::Unit(id);
            let (x, y) = pack_wire_target(&target);
            assert_eq!(resolve_wire_target(x, y), target, "id {id}");
        }
        // The two ids that used to collapse into one f32 stay distinct.
        assert_eq!(
            1_000_000_002u32 as f32, 1_000_000_000u32 as f32,
            "f32 cannot separate these ids - the pre-fix wire bug"
        );
        let (ax, ay) = pack_wire_target(&CommandTarget::Unit(1_000_000_000));
        let (bx, by) = pack_wire_target(&CommandTarget::Unit(1_000_000_002));
        assert_ne!((ax, ay), (bx, by));
        assert_eq!(
            resolve_wire_target(ax, ay),
            CommandTarget::Unit(1_000_000_000)
        );
        assert_eq!(
            resolve_wire_target(bx, by),
            CommandTarget::Unit(1_000_000_002)
        );
    }

    #[test]
    fn wire_target_round_trips_building_tiles() {
        for pos in [
            crate::world::TilePos::new(0, 0),
            crate::world::TilePos::new(129, 55),
            crate::world::TilePos::new(-3, 32767),
        ] {
            let target = CommandTarget::Building(pos);
            let (x, y) = pack_wire_target(&target);
            assert_eq!(resolve_wire_target(x, y), target, "{pos:?}");
        }
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
        first.apply_unit_command(&[a], 0, 5.0, 6.0, false);
        first.apply_unit_command(&[b], 0, 9.0, 0.0, false);

        let mut second = UnitCommandRuntime::new();
        let a2 = second.spawn("dagger", 0).expect("dagger");
        let b2 = second.spawn("dagger", 1).expect("dagger");
        second.apply_unit_command(&[a2], 0, 5.0, 6.0, false);
        second.apply_unit_command(&[b2], 0, 9.0, 0.0, false);

        assert_eq!(first.checksum(), second.checksum());
    }
}
