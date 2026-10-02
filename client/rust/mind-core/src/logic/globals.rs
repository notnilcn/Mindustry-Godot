// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Global logic variables and constants.
//!
//! Ported from `core/src/mindustry/logic/GlobalVars.java`. This milestone
//! registers the state-independent constants (math, team control ids, `LAccess`
//! names, align names, named colors); the content/`@sfx-*`/time-client constants
//! and the per-tick `update()` land with the M7 content integration.
//!
//! `logicids.dat` is intentionally absent (HLP §9): `lookup` returns null,
//! `lookup_logic_id` returns `-1`, and no `@<type>Count` constants exist.

use indexmap::{IndexMap, IndexSet};

use crate::content::{ContentRef, ContentType};
use crate::logic::access::LAccess;
use crate::logic::value::{LVar, LogicObject, VarArena};
use crate::math::ArcRand;

/// `GlobalVars.ctrlProcessor`.
pub const CTRL_PROCESSOR: i32 = 1;
/// `GlobalVars.ctrlPlayer`.
pub const CTRL_PLAYER: i32 = 2;
/// `GlobalVars.ctrlCommand`.
pub const CTRL_COMMAND: i32 = 3;

/// `GlobalVars.lookableContent`.
pub const LOOKABLE_CONTENT: [ContentType; 5] = [
    ContentType::Block,
    ContentType::Unit,
    ContentType::Item,
    ContentType::Liquid,
    ContentType::Team,
];

/// `GlobalVars.writableLookableContent`.
pub const WRITABLE_LOOKABLE_CONTENT: [ContentType; 4] = [
    ContentType::Block,
    ContentType::Unit,
    ContentType::Item,
    ContentType::Liquid,
];

/// A documented global variable entry (`GlobalVars.VarEntry`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarEntry {
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Icon name.
    pub icon: String,
    /// Privileged.
    pub privileged: bool,
}

/// `GlobalVars` — logic constant/variable table.
#[derive(Clone, Debug)]
pub struct GlobalVars {
    /// Constant cells by name (lookup only).
    pub cells: IndexMap<String, LVar>,
    /// Privileged names.
    pub privileged_names: IndexSet<String>,
    /// Documentation entries (plan 14).
    pub entries: Vec<VarEntry>,
    /// Global random stream (`GlobalVars.rand`).
    pub rand: ArcRand,
}

impl Default for GlobalVars {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalVars {
    /// Creates and initializes the state-independent constants.
    pub fn new() -> Self {
        let mut globals = Self {
            cells: IndexMap::new(),
            privileged_names: IndexSet::new(),
            entries: Vec::new(),
            rand: ArcRand::new(0),
        };
        globals.init_static();
        globals
    }

    fn put_entry(&mut self, name: &str, value: Option<LogicObject>, privileged: bool) {
        let mut var = LVar::new(name);
        var.constant = true;
        match value {
            Some(obj) => {
                var.is_obj = true;
                var.obj = Some(obj);
            }
            None => {
                var.is_obj = true;
                var.obj = None;
            }
        }
        self.cells.insert(name.to_owned(), var);
        if privileged {
            self.privileged_names.insert(name.to_owned());
        }
        self.entries.push(VarEntry {
            name: name.to_owned(),
            description: String::new(),
            icon: String::new(),
            privileged,
        });
    }

    fn put_num(&mut self, name: &str, value: f64, privileged: bool) {
        let mut var = LVar::num_const(name, value);
        var.constant = true;
        self.cells.insert(name.to_owned(), var);
        if privileged {
            self.privileged_names.insert(name.to_owned());
        }
        self.entries.push(VarEntry {
            name: name.to_owned(),
            description: String::new(),
            icon: String::new(),
            privileged,
        });
    }

    /// State-independent constants (`GlobalVars.init` minus content/audio).
    fn init_static(&mut self) {
        self.put_entry("the end", None, true);
        self.put_num("false", 0.0, false);
        self.put_num("true", 1.0, false);
        self.put_entry("null", None, true);

        // math
        self.put_num("@pi", std::f64::consts::PI, false);
        self.put_num("π", std::f64::consts::PI, false);
        self.put_num("@e", std::f64::consts::E, false);
        self.put_num("@degToRad", std::f64::consts::PI / 180.0, false);
        self.put_num("@radToDeg", 180.0 / std::f64::consts::PI, false);

        // control ids
        self.put_num("@ctrlProcessor", CTRL_PROCESSOR as f64, false);
        self.put_num("@ctrlPlayer", CTRL_PLAYER as f64, false);
        self.put_num("@ctrlCommand", CTRL_COMMAND as f64, false);

        // sensor constants
        for sensor in LAccess::ALL {
            let name = format!("@{}", sensor.name());
            let mut var = LVar::new(&name);
            var.constant = true;
            var.is_obj = true;
            var.obj = Some(LogicObject::Enum(sensor.name()));
            self.cells.insert(name, var);
        }

        // align constants
        for name in [
            "center",
            "top",
            "bottom",
            "left",
            "right",
            "topLeft",
            "topRight",
            "bottomLeft",
            "bottomRight",
        ] {
            let value = crate::logic::statement::name_to_align(name).unwrap_or(0);
            let key = format!("@{name}");
            // Upstream `GlobalVars.init`: `put("@" + name, align)` where `align`
            // is an `Integer`; `DrawI` reads it with `p1.numi()`.
            let var = LVar::num_const(key, value as f64);
            self.cells.insert(var.name.clone(), var);
        }

        // named colors: @color<Name> where Name = capitalize(lowercased name)
        for (name, color) in NAMED_COLORS {
            let capitalized = capitalize(name);
            let key = format!("@color{capitalized}");
            self.put_num(&key, rgba_to_double_bits(*color), false);
        }
    }

    /// `GlobalVars.get(name, privileged)`.
    pub fn get(&self, name: &str, privileged: bool) -> Option<LVar> {
        if !privileged && self.privileged_names.contains(name) {
            return self.cells.get("null").cloned();
        }
        self.cells.get(name).cloned()
    }

    /// `GlobalVars.waitVar` (`@wait`).
    pub fn wait_var(&self) -> Option<&LVar> {
        self.cells.get("@wait")
    }

    /// `GlobalVars.lookupContent`: teams by index (0..255), otherwise `None`.
    pub fn lookup_content(&self, type_: ContentType, id: i32) -> Option<LogicObject> {
        if type_ == ContentType::Team {
            if (0..256).contains(&id) {
                return Some(LogicObject::Team(id as u8));
            }
            return None;
        }
        None
    }

    /// `GlobalVars.lookupLogicId`: always `-1` without `logicids.dat`.
    pub fn lookup_logic_id(&self, _content: ContentRef) -> i32 {
        -1
    }

    /// `GlobalVars.remove(name)` (data-patch reset).
    pub fn remove(&mut self, name: &str) {
        self.cells.shift_remove(name);
    }
}

/// Capitalizes the first character (`Strings.capitalize` for ASCII names).
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Red/green/blue/alpha bytes (`Color` 0–255 components).
pub type Rgba8 = (u8, u8, u8, u8);

/// `Color.rgba8888(r,g,b,a)` → `Double.longBitsToDouble(rgba8888 & 0xffffffffL)`.
pub fn rgba_to_double_bits(color: Rgba8) -> f64 {
    let (r, g, b, a) = color;
    let packed = ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32);
    f64::from_bits(packed as u64)
}

/// `Color.toDoubleBits(int r,g,b,a)`.
pub fn rgba_u8_to_double_bits(r: i32, g: i32, b: i32, a: i32) -> f64 {
    rgba_to_double_bits((r as u8, g as u8, b as u8, a as u8))
}

/// Arc named colors (lowercase keys; `Colors.reset()`), `(name, (r,g,b,a))`.
#[rustfmt::skip]
pub const NAMED_COLORS: &[(&str, Rgba8)] = &[
    ("clear", (0, 0, 0, 0)),
    ("black", (0, 0, 0, 255)),
    ("white", (255, 255, 255, 255)),
    ("lightgray", (204, 204, 204, 255)),
    ("gray", (128, 128, 128, 255)),
    ("darkgray", (64, 64, 64, 255)),
    ("blue", (64, 64, 230, 255)),
    ("navy", (0, 0, 128, 255)),
    ("royal", (64, 64, 230, 255)),
    ("slate", (112, 128, 144, 255)),
    ("sky", (135, 206, 235, 255)),
    ("cyan", (0, 255, 255, 255)),
    ("teal", (0, 128, 128, 255)),
    ("green", (56, 214, 103, 255)),
    ("acid", (127, 255, 0, 255)),
    ("lime", (0, 255, 0, 255)),
    ("forest", (34, 139, 34, 255)),
    ("olive", (128, 128, 0, 255)),
    ("yellow", (255, 255, 0, 255)),
    ("gold", (255, 215, 0, 255)),
    ("goldenrod", (218, 165, 32, 255)),
    ("orange", (255, 165, 0, 255)),
    ("brown", (165, 42, 42, 255)),
    ("tan", (210, 180, 140, 255)),
    ("brick", (178, 34, 34, 255)),
    ("red", (229, 84, 84, 255)),
    ("scarlet", (255, 36, 0, 255)),
    ("crimson", (220, 20, 60, 255)),
    ("coral", (255, 127, 80, 255)),
    ("salmon", (250, 128, 114, 255)),
    ("pink", (255, 192, 203, 255)),
    ("magenta", (255, 0, 255, 255)),
    ("purple", (128, 0, 128, 255)),
    ("violet", (238, 130, 238, 255)),
    ("maroon", (128, 0, 0, 255)),
];

/// `Colors.get(name)` for the lowercase named colors.
pub fn named_color(name: &str) -> Option<Rgba8> {
    NAMED_COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
}

/// Folds the constants into an arena (assemble-time copy); returns the cell id.
pub fn insert_into(arena: &mut VarArena, var: &LVar) -> crate::logic::value::VarId {
    let id = arena.put_var(&var.name);
    let cell = arena.get_mut(id);
    cell.is_obj = var.is_obj;
    cell.obj = var.obj.clone();
    cell.num = var.num;
    cell.constant = true;
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_constants_resolve() {
        let g = GlobalVars::new();
        assert_eq!(g.get("null", false).unwrap().obj, None);
        assert!(g.get("null", false).unwrap().is_obj);
        assert_eq!(g.get("true", false).unwrap().num, 1.0);
        assert_eq!(g.get("@pi", false).unwrap().num, std::f64::consts::PI);
        assert!(g.get("@ctrlProcessor", false).is_some());
        assert!(g.get("the end", false).is_some());
        // privileged constants fall back to null for non-privileged executors
        assert_eq!(g.get("the end", false).unwrap().obj, None);
    }

    #[test]
    fn named_color_red_matches_arc() {
        let red = named_color("red").unwrap();
        assert_eq!(red, (229, 84, 84, 255));
        assert_eq!(rgba_to_double_bits(red), f64::from_bits(0xe5_54_54_ff_u64));
    }

    #[test]
    fn lookup_is_absent_file_behavior() {
        let g = GlobalVars::new();
        assert_eq!(g.lookup_logic_id(ContentRef::new(ContentType::Item, 0)), -1);
        assert_eq!(g.lookup_content(ContentType::Item, 0), None);
        assert_eq!(
            g.lookup_content(ContentType::Team, 3),
            Some(LogicObject::Team(3))
        );
    }
}
