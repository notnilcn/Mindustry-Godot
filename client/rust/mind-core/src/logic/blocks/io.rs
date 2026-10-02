// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logic block read/write instruction dispatch (`LExecutor.ReadI`/`WriteI`) and
//! the `TypeIO` variable codec used by processor/memory persistence.
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java` (`ReadI`, `WriteI`),
//! `world/blocks/logic/{LogicBlock,MemoryBlock}.java` (`read`/`write`) and
//! `core/src/mindustry/io/TypeIO.java` (`writeObject`/`readObject`).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockKind, ContentRef};
use crate::entities::comp::{Building, TeamComp};
use crate::io::IoResult;
use crate::io::entity::{EntityReader, EntityWriter};
use crate::io::typeio::{self, EntityRef, TypeValue};
use crate::logic::executor::Executor;
use crate::logic::value::{LVar, LogicObject, VarRef};
use crate::world::TilePos;

use super::memory::MemoryBlockState;
use super::{LogicBlockState, block_kind_of, build_at, is_valid_building, privileged_of};

/// A logic cell value used by the read/write path.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    /// Numeric value (`LVar` non-object).
    Num(f64),
    /// Object value (`None` == Java `null`).
    Obj(Option<LogicObject>),
}

impl CellValue {
    /// Reads a variable cell.
    pub fn from_lvar(value: &LVar) -> Self {
        if value.is_obj {
            CellValue::Obj(value.obj.clone())
        } else {
            CellValue::Num(value.num)
        }
    }

    /// Writes this value into a destination cell (`LVar.set`).
    pub fn apply(&self, dst: &mut LVar) {
        match self {
            CellValue::Num(n) => dst.set_num(*n),
            CellValue::Obj(o) => dst.set_obj(o.clone()),
        }
    }

    /// `LVar.isobj`.
    pub fn is_obj(&self) -> bool {
        matches!(self, CellValue::Obj(_))
    }

    /// Numeric value (`LVar.num`).
    pub fn num(&self) -> f64 {
        match self {
            CellValue::Num(n) => *n,
            CellValue::Obj(Some(_)) => 1.0,
            CellValue::Obj(None) => 0.0,
        }
    }
}

/// `TypeIO.writeObject` for a logic object value.
pub fn write_object(w: &mut EntityWriter, world: &World, value: &LogicObject) -> IoResult<()> {
    match value {
        LogicObject::Str(s) => typeio::write_object(w, &TypeValue::Str(Some(s.clone()))),
        LogicObject::Content(c) => typeio::write_object(w, &TypeValue::Content(c.type_, c.id)),
        LogicObject::Team(team) => typeio::write_object(w, &TypeValue::Team(*team)),
        LogicObject::Building(e) => {
            let packed = world
                .get::<Building>(*e)
                .map(|b| b.tile.pack())
                .unwrap_or_else(|| e.index().index() as i32);
            typeio::write_object(w, &TypeValue::Building(EntityRef::Id(packed)))
        }
        LogicObject::Unit(e) => {
            typeio::write_object(w, &TypeValue::Unit(EntityRef::Id(e.index().index() as i32)))
        }
        LogicObject::Align(v) => typeio::write_object(w, &TypeValue::Double(*v as f64)),
        // Upstream `TypeIO.writeObject` rejects arbitrary enums; write null.
        LogicObject::Enum(_) | LogicObject::Query(_) => typeio::write_object(w, &TypeValue::Null),
    }
}

/// Writes one variable (`write.str(name)` + `TypeIO.writeObject(value)`).
pub fn write_variable(
    w: &mut EntityWriter,
    world: &World,
    name: &str,
    value: &CellValue,
) -> IoResult<()> {
    w.str(name)?;
    match value {
        CellValue::Num(n) => typeio::write_object(w, &TypeValue::Double(*n)),
        CellValue::Obj(None) => typeio::write_object(w, &TypeValue::Null),
        CellValue::Obj(Some(obj)) => write_object(w, world, obj),
    }
}

/// Converts a `TypeIO` object value to a logic object, resolving entity refs.
pub fn object_from_type(world: &World, value: TypeValue) -> Option<LogicObject> {
    match value {
        TypeValue::Null | TypeValue::Double(_) => None,
        TypeValue::Str(Some(s)) => Some(LogicObject::Str(s)),
        TypeValue::Str(None) => None,
        TypeValue::Content(type_, id) => Some(LogicObject::Content(ContentRef::new(type_, id))),
        TypeValue::Team(team) => Some(LogicObject::Team(team)),
        TypeValue::Building(EntityRef::Id(packed)) => {
            let pos = TilePos::from_pack(packed);
            build_at(world, pos.x() as i32, pos.y() as i32).map(LogicObject::Building)
        }
        TypeValue::Unit(EntityRef::Id(id)) => {
            Entity::from_raw_u32(id as u32).map(LogicObject::Unit)
        }
        _ => None,
    }
}

/// Reads a logic object value back (`TypeIO.readObjectBoxed`).
pub fn read_object(r: &mut EntityReader, world: &World) -> IoResult<Option<LogicObject>> {
    let value = typeio::read_object(r)?;
    Ok(object_from_type(world, value))
}

/// Reads one variable (`read.str()` + `TypeIO.readObjectBoxed`).
pub fn read_variable(r: &mut EntityReader, world: &World) -> IoResult<(String, CellValue)> {
    let name = r.str()?;
    // Peek the tag so the `doubleType` fast path avoids boxing (upstream).
    let tag = r.ub()?;
    let cell = if tag == typeio::tags::DOUBLE {
        CellValue::Num(r.d()?)
    } else if tag == typeio::tags::NULL {
        CellValue::Obj(None)
    } else {
        match typeio::read_object_tag(r, true, None, false, true, tag)? {
            TypeValue::Double(n) => CellValue::Num(n),
            TypeValue::Str(Some(s)) => CellValue::Obj(Some(LogicObject::Str(s))),
            TypeValue::Str(None) | TypeValue::Null => CellValue::Obj(None),
            TypeValue::Content(type_, id) => {
                CellValue::Obj(Some(LogicObject::Content(ContentRef::new(type_, id))))
            }
            TypeValue::Team(team) => CellValue::Obj(Some(LogicObject::Team(team))),
            TypeValue::Building(EntityRef::Id(packed)) => {
                let pos = TilePos::from_pack(packed);
                CellValue::Obj(
                    build_at(world, pos.x() as i32, pos.y() as i32).map(LogicObject::Building),
                )
            }
            TypeValue::Unit(EntityRef::Id(id)) => {
                CellValue::Obj(Entity::from_raw_u32(id as u32).map(LogicObject::Unit))
            }
            _ => CellValue::Obj(None),
        }
    };
    Ok((name, cell))
}

/// Team of a building (`Building.team`).
fn team_of(world: &World, e: Entity) -> Option<u8> {
    world.get::<TeamComp>(e).map(|t| t.team)
}

/// `LReadable.readable(exec)` for processor-adjacent blocks.
pub fn readable(world: &World, exec: &Executor, e: Entity) -> bool {
    is_valid_building(world, e)
        && (exec.privileged || (team_of(world, e) == Some(exec.team) && !privileged_of(world, e)))
}

/// Applies a cell value to the caller's output variable.
fn apply_output(exec: &mut Executor, output: VarRef, cell: CellValue) {
    if exec.arena.get(output.id()).constant {
        return;
    }
    cell.apply(exec.arena.get_mut(output.id()));
}

/// `ReadI` dispatch: `LReadable` target, string char or query fallback.
pub fn read_target(
    world: &mut World,
    exec: &mut Executor,
    target: Option<&LogicObject>,
    address: VarRef,
    output: VarRef,
) {
    match target {
        Some(LogicObject::Building(e)) => {
            let e = *e;
            let position = exec.arena.get(address.id()).clone();
            let cell = read_building(world, exec, e, &position);
            apply_output(exec, output, cell);
        }
        Some(LogicObject::Str(s)) => {
            let index = exec.arena.get(address.id()).numi();
            let value = if index < 0 || index as usize >= s.chars().count() {
                f64::NAN
            } else {
                s.chars().nth(index as usize).map(u32::from).unwrap_or(0) as f64
            };
            apply_output(exec, output, CellValue::Num(value));
        }
        Some(LogicObject::Query(_)) => {
            // `@queries` arena lives in the executor; the query type is deferred.
            apply_output(exec, output, CellValue::Obj(None));
        }
        _ => apply_output(exec, output, CellValue::Obj(None)),
    }
}

/// Reads one cell from a building (`LogicBuild.read`/`MemoryBuild.read`).
fn read_building(world: &World, exec: &Executor, e: Entity, position: &LVar) -> CellValue {
    if !readable(world, exec, e) {
        return CellValue::Obj(None);
    }
    match block_kind_of(world, e) {
        Some(BlockKind::MemoryBlock) => {
            let Some(state) = world.get::<MemoryBlockState>(e) else {
                return CellValue::Obj(None);
            };
            super::memory::read_slot(state, position.numi())
        }
        Some(BlockKind::LogicBlock) => {
            let Some(state) = world.get::<LogicBlockState>(e) else {
                return CellValue::Obj(None);
            };
            if position.is_obj {
                if let Some(LogicObject::Str(name)) = &position.obj {
                    return match state.executor.optional_var(name) {
                        Some(id) => CellValue::from_lvar(state.executor.arena.get(id)),
                        None => {
                            CellValue::Obj(state.optional_link(name).map(LogicObject::Building))
                        }
                    };
                }
                CellValue::Obj(None)
            } else {
                let index = position.numi();
                let linked = if index >= 0 {
                    state.executor.links.get(index as usize).copied()
                } else {
                    None
                };
                CellValue::Obj(linked.map(LogicObject::Building))
            }
        }
        _ => CellValue::Obj(None),
    }
}

/// `WriteI` dispatch: `LWritable` target.
pub fn write_target(
    world: &mut World,
    exec: &mut Executor,
    target: Option<&LogicObject>,
    address: VarRef,
    input: VarRef,
) {
    let Some(LogicObject::Building(e)) = target else {
        return;
    };
    let e = *e;
    if !readable(world, exec, e) {
        return;
    }
    let position = exec.arena.get(address.id()).clone();
    let value = CellValue::from_lvar(exec.arena.get(input.id()));
    match block_kind_of(world, e) {
        Some(BlockKind::MemoryBlock) => {
            if let Some(mut state) = world.get_mut::<MemoryBlockState>(e) {
                super::memory::write_slot(&mut state, position.numi(), &value);
            }
        }
        Some(BlockKind::LogicBlock) => {
            if position.is_obj
                && let Some(LogicObject::Str(name)) = &position.obj
                && let Some(mut state) = world.get_mut::<LogicBlockState>(e)
                && let Some(id) = state.executor.optional_var(name)
                && !state.executor.arena.get(id).constant
            {
                let cell = value.clone();
                cell.apply(state.executor.arena.get_mut(id));
            }
        }
        _ => {}
    }
}
