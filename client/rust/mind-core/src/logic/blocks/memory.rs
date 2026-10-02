// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MemoryBlock`/`MemoryBuild` (plan 13 M3).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/MemoryBlock.java`. Slot
//! storage mirrors the upstream sentinel model: a cell is either a number or an
//! object value; `read` out of range returns null and `write` out of range is
//! ignored.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::io::entity::{EntityReader, EntityWriter};
use crate::io::typeio::{self, TypeValue};
use crate::logic::value::LogicObject;
use crate::world::behavior::BuildingBehavior;

use super::io::CellValue;
use super::memory_def_of;

/// One memory cell (`sentinel` marks a numeric slot upstream).
#[derive(Clone, Debug, PartialEq)]
pub enum MemSlot {
    /// Numeric slot (`numberMemory`).
    Num(f64),
    /// Object slot (`objectMemory`; `None` == `null`).
    Obj(Option<LogicObject>),
}

/// Runtime memory slots for one memory building.
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct MemoryBlockState {
    /// Cells in address order.
    pub slots: Vec<MemSlot>,
}

impl MemoryBlockState {
    /// Creates `capacity` numeric-zero cells (`Arrays.fill(objectMemory, sentinel)`).
    pub fn new(capacity: i32) -> Self {
        let capacity = capacity.max(0) as usize;
        Self {
            slots: vec![MemSlot::Num(0.0); capacity],
        }
    }

    /// `MemoryBuild.read`: out-of-range -> null object.
    pub fn read(&self, address: i32) -> CellValue {
        read_slot(self, address)
    }

    /// `MemoryBuild.write`: out-of-range ignored.
    pub fn write(&mut self, address: i32, value: &CellValue) {
        write_slot(self, address, value);
    }

    /// Capacity (`MemoryBlock.memoryCapacity`).
    pub fn capacity(&self) -> i32 {
        self.slots.len() as i32
    }
}

/// `MemoryBuild.read(position, output)` slot semantics.
pub fn read_slot(state: &MemoryBlockState, address: i32) -> CellValue {
    if address < 0 || address as usize >= state.slots.len() {
        return CellValue::Obj(None);
    }
    match &state.slots[address as usize] {
        MemSlot::Num(n) => CellValue::Num(*n),
        MemSlot::Obj(o) => CellValue::Obj(o.clone()),
    }
}

/// `MemoryBuild.write(position, value)` slot semantics.
pub fn write_slot(state: &mut MemoryBlockState, address: i32, value: &CellValue) {
    if address < 0 || address as usize >= state.slots.len() {
        return;
    }
    state.slots[address as usize] = match value {
        CellValue::Num(n) => MemSlot::Num(*n),
        CellValue::Obj(o) => MemSlot::Obj(o.clone()),
    };
}

/// `MemoryBlock.MemoryBuild` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct MemoryBehavior;

impl BuildingBehavior for MemoryBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let capacity = memory_def_of(world, e).map(|def| def.capacity).unwrap_or(0);
        world.entity_mut(e).insert(MemoryBlockState::new(capacity));
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        let Some(state) = world.get::<MemoryBlockState>(e) else {
            return;
        };
        w.i(state.slots.len() as i32);
        for slot in &state.slots {
            let result = match slot {
                MemSlot::Num(n) => typeio::write_object(w, &TypeValue::Double(*n)),
                MemSlot::Obj(Some(object)) => super::io::write_object(w, world, object),
                MemSlot::Obj(None) => typeio::write_object(w, &TypeValue::Null),
            };
            let _ = result;
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, revision: u8) {
        let Ok(amount) = r.i() else {
            return;
        };
        let amount = amount.max(0);

        // Decode into locals first so entity-ref resolution can borrow `&World`
        // before the mutable `MemoryBlockState` borrow below.
        let mut decoded: Vec<MemSlot> = Vec::with_capacity(amount as usize);
        if revision == 0 {
            for _ in 0..amount {
                let Ok(val) = r.d() else { break };
                decoded.push(MemSlot::Num(val));
            }
        } else {
            for _ in 0..amount {
                let Ok(raw_tag) = r.b() else { break };
                let tag = raw_tag as u8;
                if tag == typeio::tags::DOUBLE {
                    let Ok(value) = r.d() else { break };
                    decoded.push(MemSlot::Num(value));
                } else {
                    let Ok(value) = typeio::read_object_tag(r, true, None, false, true, tag) else {
                        break;
                    };
                    decoded.push(MemSlot::Obj(super::io::object_from_type(world, value)));
                }
            }
        }

        let Some(mut state) = world.get_mut::<MemoryBlockState>(e) else {
            return;
        };
        for (i, slot) in decoded.into_iter().enumerate() {
            if i < state.slots.len() {
                state.slots[i] = slot;
            }
        }
    }
}
