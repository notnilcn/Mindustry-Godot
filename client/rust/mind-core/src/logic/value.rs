// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Value & variable model.
//!
//! Ported from `core/src/mindustry/logic/LVar.java`. A [`LVar`] is a named cell
//! that is either numeric or object-valued (`is_obj`); the object slot is
//! `Option`, where `None` is Java's `null` object. This mirrors upstream exactly
//! so the ported `LogicTests` assertions translate 1:1.

use bevy_ecs::entity::Entity;
use indexmap::IndexMap;

use crate::content::ContentRef;

/// Index into an [`Executor`](super::executor::Executor)'s variable vector
/// (plan 13 §3.2 `VarId`).
pub type VarId = u32;

/// Local variable reference. Global (mutable `@time`… ) references are resolved
/// to locals at load time in this milestone; the enum keeps room for the plan
/// §3.10 global arena.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VarRef {
    /// Index into the executor's local variable vector.
    Local(VarId),
}

impl VarRef {
    /// The underlying arena id.
    pub const fn id(self) -> VarId {
        match self {
            VarRef::Local(id) => id,
        }
    }
}

/// Object-valued logic value.
///
/// Java `Object` values are represented structurally instead of by pointer
/// identity (`Structs.eq` semantics).
#[derive(Clone, Debug, PartialEq)]
pub enum LogicObject {
    /// A building (`LogicBuild` or any `Building`).
    Building(Entity),
    /// A unit entity.
    Unit(Entity),
    /// A content record (item/liquid/block/unit/weather/status/…).
    Content(ContentRef),
    /// A team index (`Team.all`).
    Team(u8),
    /// A Java `String` object value (print, flags, channels, …).
    Str(String),
    /// A Java enum object value (e.g. `LUnitControl.idle`).
    Enum(&'static str),
    /// `@topLeft`/align constants from `LStatement.nameToAlign`.
    Align(i32),
    /// A `@queries` result arena index.
    Query(u32),
}

impl LogicObject {
    /// Object `toString()` equivalent, used by `print`/`format`.
    pub fn display(&self) -> String {
        match self {
            LogicObject::Building(_) => "[building]".into(),
            LogicObject::Unit(_) => "[unit]".into(),
            LogicObject::Content(_) => "[content]".into(),
            LogicObject::Team(id) => id.to_string(),
            LogicObject::Str(s) => s.clone(),
            LogicObject::Enum(name) => (*name).into(),
            LogicObject::Align(value) => value.to_string(),
            LogicObject::Query(_) => "[object]".into(),
        }
    }
}

/// A named logic variable. Ported from `LVar`.
#[derive(Clone, Debug, PartialEq)]
pub struct LVar {
    /// Variable name (`LVar.name`).
    pub name: String,
    /// Index in the owning arena, `-1` while assembling.
    pub id: i32,
    /// Object-valued flag (`LVar.isobj`).
    pub is_obj: bool,
    /// Read-only constant flag (`LVar.constant`).
    pub constant: bool,
    /// Object value (`LVar.objval`); `None` == Java `null`.
    pub obj: Option<LogicObject>,
    /// Numeric value (`LVar.numval`).
    pub num: f64,
    /// Sim ms of the last `sync` (`LVar.syncTime`).
    pub synced_at: f64,
}

impl LVar {
    /// Creates a null-object variable (`new LVar(name)` → `isobj` false until set).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: -1,
            is_obj: false,
            constant: false,
            obj: None,
            num: 0.0,
            synced_at: 0.0,
        }
    }

    /// A null-object variable (the default state produced by `putVar`).
    pub fn null_obj(name: impl Into<String>) -> Self {
        let mut var = Self::new(name);
        var.is_obj = true;
        var.obj = None;
        var
    }

    /// A numeric constant.
    pub fn num_const(name: impl Into<String>, value: f64) -> Self {
        let mut var = Self::new(name);
        var.constant = true;
        var.num = value;
        var
    }

    /// An object constant.
    pub fn obj_const(name: impl Into<String>, value: Option<LogicObject>) -> Self {
        let mut var = Self::new(name);
        var.is_obj = true;
        var.obj = value;
        var.constant = true;
        var
    }

    /// `LVar.building()`.
    pub fn building(&self) -> Option<Entity> {
        match self.obj {
            Some(LogicObject::Building(entity)) if self.is_obj => Some(entity),
            _ => None,
        }
    }

    /// `LVar.obj()`.
    pub fn value_obj(&self) -> Option<&LogicObject> {
        if self.is_obj { self.obj.as_ref() } else { None }
    }

    /// `LVar.team()`.
    pub fn team(&self) -> Option<u8> {
        if self.is_obj {
            match &self.obj {
                Some(LogicObject::Team(team)) => Some(*team),
                _ => None,
            }
        } else {
            let t = self.num;
            if !(0.0..256.0).contains(&t) || crate::logic::value::invalid(t) {
                None
            } else {
                Some(t as u8)
            }
        }
    }

    /// `LVar.bool()`.
    pub fn as_bool(&self) -> bool {
        if self.is_obj {
            self.obj.is_some()
        } else {
            self.num.abs() >= 0.00001
        }
    }

    /// `LVar.num()`.
    pub fn num(&self) -> f64 {
        if self.is_obj {
            if self.obj.is_some() { 1.0 } else { 0.0 }
        } else if invalid(self.num) {
            0.0
        } else {
            self.num
        }
    }

    /// `LVar.numOrNan()`.
    pub fn num_or_nan(&self) -> f64 {
        if self.is_obj {
            if self.obj.is_some() { 1.0 } else { f64::NAN }
        } else if invalid(self.num) {
            0.0
        } else {
            self.num
        }
    }

    /// `LVar.numf()`.
    pub fn numf(&self) -> f32 {
        self.num() as f32
    }

    /// `LVar.numfWorld()`.
    pub fn numf_world(&self) -> f32 {
        unconv(self.numf())
    }

    /// `LVar.numfOrNan()`.
    pub fn numf_or_nan(&self) -> f32 {
        self.num_or_nan() as f32
    }

    /// `LVar.numi()` (truncates toward zero).
    pub fn numi(&self) -> i32 {
        self.num() as i32
    }

    /// `LVar.setbool`.
    pub fn set_bool(&mut self, value: bool) {
        self.set_num(if value { 1.0 } else { 0.0 });
    }

    /// `LVar.setnum`.
    pub fn set_num(&mut self, value: f64) {
        if self.constant {
            return;
        }
        if invalid(value) {
            self.obj = None;
            self.is_obj = true;
        } else {
            self.num = value;
            self.obj = None;
            self.is_obj = false;
        }
    }

    /// `LVar.setobj`.
    pub fn set_obj(&mut self, value: Option<LogicObject>) {
        if self.constant {
            return;
        }
        self.obj = value;
        self.is_obj = true;
    }

    /// `LVar.setconst`.
    pub fn set_const(&mut self, value: Option<LogicObject>) {
        self.obj = value;
        self.is_obj = true;
    }

    /// `LVar.setlink`: `Some` marks a linked-block constant, `None` demotes.
    pub fn set_link(&mut self, value: Option<LogicObject>) {
        self.is_obj = true;
        if value.is_none() {
            self.obj = None;
            self.constant = false;
        } else {
            self.obj = value;
            self.constant = true;
        }
    }

    /// `LVar.set(LVar)`.
    pub fn set_from(&mut self, other: &LVar) {
        self.is_obj = other.is_obj;
        if self.is_obj {
            self.obj = other.obj.clone();
        } else {
            self.num = if invalid(other.num) { 0.0 } else { other.num };
        }
    }

    /// The numeric constant form used by `op`/`set` for hidden literals.
    pub fn from_num_value(value: f64) -> Self {
        Self::num_const(format!("___{value}"), value)
    }
}

/// `LVar.invalid`.
pub fn invalid(d: f64) -> bool {
    d.is_nan() || d.is_infinite()
}

/// `World.conv` — tile coordinate to world coordinate.
pub fn conv(v: f32) -> f32 {
    v * 8.0
}

/// `World.unconv` — world coordinate to tile coordinate.
pub fn unconv(v: f32) -> f32 {
    v / 8.0
}

/// `Structs.eq` for two optional object values.
pub fn structs_eq(a: Option<&LogicObject>, b: Option<&LogicObject>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `LVar` arena: ordered name → cell table (insertion order is deterministic).
#[derive(Clone, Debug, Default)]
pub struct VarArena {
    /// Cells in insertion order.
    pub cells: Vec<LVar>,
    /// Name → id index (lookup only, never iterated on sim paths).
    pub by_name: IndexMap<String, VarId>,
}

impl VarArena {
    /// Empty arena.
    pub fn new() -> Self {
        Self::default()
    }

    /// `putVar`: returns the existing variable or inserts a null-object one.
    pub fn put_var(&mut self, name: &str) -> VarId {
        if let Some(&id) = self.by_name.get(name) {
            return id;
        }
        let id = self.cells.len() as VarId;
        let mut var = LVar::null_obj(name);
        var.id = id as i32;
        self.by_name.insert(name.to_owned(), id);
        self.cells.push(var);
        id
    }

    /// Inserts a constant numeric value by name (`putConst(name, value)`).
    pub fn put_num_const(&mut self, name: &str, value: f64) -> VarId {
        let id = self.put_var(name);
        let cell = &mut self.cells[id as usize];
        cell.is_obj = false;
        cell.num = value;
        cell.obj = None;
        cell.constant = true;
        id
    }

    /// Inserts a constant object value by name.
    pub fn put_obj_const(&mut self, name: &str, value: Option<LogicObject>) -> VarId {
        let id = self.put_var(name);
        let cell = &mut self.cells[id as usize];
        cell.is_obj = true;
        cell.obj = value;
        cell.constant = true;
        id
    }

    /// Inserts a constant string value by name.
    pub fn put_str_const(&mut self, name: &str, value: String) -> VarId {
        self.put_obj_const(name, Some(LogicObject::Str(value)))
    }

    /// Cell by id.
    pub fn get(&self, id: VarId) -> &LVar {
        &self.cells[id as usize]
    }

    /// Mutable cell by id.
    pub fn get_mut(&mut self, id: VarId) -> &mut LVar {
        &mut self.cells[id as usize]
    }

    /// Name lookup.
    pub fn get_id(&self, name: &str) -> Option<VarId> {
        self.by_name.get(name).copied()
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether the arena is empty.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_of_null_object_is_zero_and_nan_variant_is_nan() {
        let v = LVar::null_obj("x");
        assert_eq!(v.num(), 0.0);
        assert!(v.num_or_nan().is_nan());
        assert!(!v.as_bool());
    }

    #[test]
    fn invalid_numbers_normalize_to_null_object() {
        let mut v = LVar::new("x");
        v.set_num(f64::NAN);
        assert!(v.is_obj && v.obj.is_none());
        assert_eq!(v.num(), 0.0);
        v.set_num(2.0);
        assert!(!v.is_obj);
        assert_eq!(v.num(), 2.0);
    }

    #[test]
    fn constants_ignore_assignment() {
        let mut v = LVar::num_const("c", 3.0);
        v.set_num(9.0);
        assert_eq!(v.num(), 3.0);
    }

    #[test]
    fn set_from_preserves_object_flag() {
        let other = LVar::obj_const("s", Some(LogicObject::Str("hi".into())));
        let mut v = LVar::new("v");
        v.set_from(&other);
        assert!(v.is_obj);
        assert_eq!(v.obj, Some(LogicObject::Str("hi".into())));
    }
}
