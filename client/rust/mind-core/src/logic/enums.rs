// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Option enums for logic statements/instructions.
//!
//! Ported from `core/src/mindustry/logic/{FetchType,QueryType,QueryShape,RadarSort,
//! RadarTarget,LLocate,TileLayer,MessageType,CutsceneAction,LMarkerControl,
//! LCategory,LUnitControl}.java` and `LogicDisplay.GraphicsType`, plus the
//! logic-relevant subset of `world/meta/BlockFlag.java`.
//!
//! Text IO serializes the Java enum `name()`; ordinals are frozen and append-only.

use crate::content::ContentType;

/// `LVar` field encode/decode used by the statement text codec.
#[allow(clippy::result_unit_err)]
pub trait MlogField: Clone {
    /// Field kind for the plan-14 editor metadata.
    const KIND: FieldKind;
    /// Writes the token(s) appended after the separating space.
    fn write(&self, out: &mut String);
    /// Parses a token exactly like Java `Enum.valueOf`/`String.valueOf`.
    fn parse(token: &str) -> Result<Self, ()>;
}

/// Statement field type tag (`LogicStatementProcessor` supports strings/enums/primitives).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// `String`.
    Str,
    /// `int`.
    Int,
    /// `boolean`.
    Bool,
    /// Java enum.
    Enum,
}

impl MlogField for String {
    const KIND: FieldKind = FieldKind::Str;
    fn write(&self, out: &mut String) {
        out.push_str(self);
    }
    fn parse(token: &str) -> Result<Self, ()> {
        Ok(token.to_owned())
    }
}

impl MlogField for bool {
    const KIND: FieldKind = FieldKind::Bool;
    fn write(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
    fn parse(token: &str) -> Result<Self, ()> {
        // Java `Boolean.valueOf`.
        Ok(token == "true")
    }
}

impl MlogField for i32 {
    const KIND: FieldKind = FieldKind::Int;
    fn write(&self, out: &mut String) {
        use std::fmt::Write as _;
        let _ = write!(out, "{self}");
    }
    fn parse(token: &str) -> Result<Self, ()> {
        token.parse::<i32>().map_err(|_| ())
    }
}

impl MlogField for ContentType {
    const KIND: FieldKind = FieldKind::Enum;
    fn write(&self, out: &mut String) {
        out.push_str(self.name());
    }
    fn parse(token: &str) -> Result<Self, ()> {
        ContentType::ALL
            .iter()
            .copied()
            .find(|c| c.name() == token)
            .ok_or(())
    }
}

/// Defines a Java-enum port with explicit `name()` strings (text-IO ABI).
macro_rules! logic_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $java:literal),* $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $(
                #[allow(missing_docs)]
                $variant,
            )*
        }

        impl $name {
            /// All variants in declaration order (`values()`).
            pub const ALL: &'static [$name] = &[$( $name::$variant ),*];

            /// Java enum `name()`.
            pub const fn name(self) -> &'static str {
                match self { $( $name::$variant => $java ),* }
            }

            /// Parses a Java enum name.
            pub fn from_name(s: &str) -> Option<Self> {
                match s { $( $java => Some($name::$variant), )* _ => None }
            }

            /// Ordinal (`ordinal()`).
            pub const fn ordinal(self) -> usize {
                self as usize
            }
        }

        impl MlogField for $name {
            const KIND: FieldKind = FieldKind::Enum;
            fn write(&self, out: &mut String) {
                out.push_str(self.name());
            }
            fn parse(token: &str) -> Result<Self, ()> {
                Self::from_name(token).ok_or(())
            }
        }
    };
}

pub(crate) use logic_enum;

logic_enum!(
    /// `LogicDisplay.GraphicsType`.
    GraphicsType {
        Clear => "clear",
        Color => "color",
        Col => "col",
        Stroke => "stroke",
        Line => "line",
        Rect => "rect",
        LineRect => "lineRect",
        Poly => "poly",
        LinePoly => "linePoly",
        Triangle => "triangle",
        Image => "image",
        Print => "print",
        Translate => "translate",
        Scale => "scale",
        Rotate => "rotate",
        Reset => "reset"
    }
);

logic_enum!(
    /// `FetchType`.
    FetchType {
        Unit => "unit",
        UnitCount => "unitCount",
        Player => "player",
        PlayerCount => "playerCount",
        Core => "core",
        CoreCount => "coreCount",
        Build => "build",
        BuildCount => "buildCount"
    }
);

logic_enum!(
    /// `QueryType`.
    QueryType { Unit => "unit", Building => "building", Bullet => "bullet" }
);

impl QueryType {
    /// `QueryType.queryable` (bullets are bugged upstream and excluded).
    pub const QUERYABLE: &'static [QueryType] = &[QueryType::Unit, QueryType::Building];
}

logic_enum!(
    /// `QueryShape`.
    QueryShape { Circle => "circle", Rect => "rect" }
);

logic_enum!(
    /// `RadarSort`.
    RadarSort {
        Distance => "distance",
        Health => "health",
        Shield => "shield",
        Armor => "armor",
        MaxHealth => "maxHealth"
    }
);

logic_enum!(
    /// `RadarTarget`.
    RadarTarget {
        Any => "any",
        Enemy => "enemy",
        Ally => "ally",
        Player => "player",
        Attacker => "attacker",
        Flying => "flying",
        Boss => "boss",
        Ground => "ground"
    }
);

logic_enum!(
    /// `LLocate`.
    LLocate {
        Ore => "ore",
        Building => "building",
        Spawn => "spawn",
        Damaged => "damaged"
    }
);

logic_enum!(
    /// `TileLayer`.
    TileLayer {
        Floor => "floor",
        Ore => "ore",
        Block => "block",
        Building => "building"
    }
);

impl TileLayer {
    /// `TileLayer.settable`.
    pub const SETTABLE: &'static [TileLayer] =
        &[TileLayer::Floor, TileLayer::Ore, TileLayer::Block];
}

logic_enum!(
    /// `MessageType`.
    MessageType {
        Notify => "notify",
        Announce => "announce",
        Toast => "toast",
        Mission => "mission"
    }
);

logic_enum!(
    /// `CutsceneAction`.
    CutsceneAction {
        Active => "active",
        Pan => "pan",
        Zoom => "zoom",
        Stop => "stop",
        Shake => "shake",
        GetHud => "getHud",
        SetHud => "setHud"
    }
);

logic_enum!(
    /// `LMarkerControl` (parameter names are metadata for plan 14).
    LMarkerControl {
        Remove => "remove",
        World => "world",
        Minimap => "minimap",
        Light => "light",
        Autoscale => "autoscale",
        Pos => "pos",
        EndPos => "endPos",
        DrawLayer => "drawLayer",
        Color => "color",
        Radius => "radius",
        Stroke => "stroke",
        Outline => "outline",
        Rotation => "rotation",
        Shape => "shape",
        Arc => "arc",
        FlushText => "flushText",
        FontSize => "fontSize",
        TextHeight => "textHeight",
        TextAlign => "textAlign",
        LineAlign => "lineAlign",
        LabelFlags => "labelFlags",
        Texture => "texture",
        TextureSize => "textureSize",
        Posi => "posi",
        Uvi => "uvi",
        Colori => "colori"
    }
);

logic_enum!(
    /// `LUnitControl`.
    LUnitControl {
        Idle => "idle",
        Stop => "stop",
        Move => "move",
        Approach => "approach",
        Pathfind => "pathfind",
        AutoPathfind => "autoPathfind",
        Boost => "boost",
        Target => "target",
        Targetp => "targetp",
        ItemDrop => "itemDrop",
        ItemTake => "itemTake",
        PayDrop => "payDrop",
        PayTake => "payTake",
        PayEnter => "payEnter",
        Mine => "mine",
        Flag => "flag",
        Build => "build",
        Deconstruct => "deconstruct",
        GetBlock => "getBlock",
        Within => "within",
        Unbind => "unbind"
    }
);

logic_enum!(
    /// Logic-relevant `world/meta/BlockFlag` subset (`allLogic`).
    BlockFlag {
        Core => "core",
        Storage => "storage",
        Generator => "generator",
        Turret => "turret",
        Factory => "factory",
        Repair => "repair",
        Battery => "battery",
        Reactor => "reactor",
        Drill => "drill",
        Shield => "shield"
    }
);

impl BlockFlag {
    /// `BlockFlag.allLogic`.
    pub const ALL_LOGIC: &'static [BlockFlag] = BlockFlag::ALL;
}

/// `LCategory` (colors/icons live in plan 14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LCategory {
    /// `unknown`.
    Unknown,
    /// `io`.
    Io,
    /// `block`.
    Block,
    /// `operation`.
    Operation,
    /// `control`.
    Control,
    /// `unit`.
    Unit,
    /// `world`.
    World,
}

impl LCategory {
    /// All categories in declaration order.
    pub const ALL: &'static [LCategory] = &[
        LCategory::Unknown,
        LCategory::Io,
        LCategory::Block,
        LCategory::Operation,
        LCategory::Control,
        LCategory::Unit,
        LCategory::World,
    ];

    /// Bundle name (`lcategory.<name>`).
    pub const fn name(self) -> &'static str {
        match self {
            LCategory::Unknown => "unknown",
            LCategory::Io => "io",
            LCategory::Block => "block",
            LCategory::Operation => "operation",
            LCategory::Control => "control",
            LCategory::Unit => "unit",
            LCategory::World => "world",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_names_roundtrip() {
        for &v in GraphicsType::ALL {
            assert_eq!(GraphicsType::from_name(v.name()), Some(v));
        }
        assert_eq!(
            GraphicsType::from_name("lineRect"),
            Some(GraphicsType::LineRect)
        );
        assert_eq!(GraphicsType::from_name("nope"), None);
    }

    #[test]
    fn ordinals_are_declaration_order() {
        assert_eq!(GraphicsType::Clear.ordinal(), 0);
        assert_eq!(GraphicsType::Reset.ordinal(), 15);
        assert_eq!(LLocate::Ore.ordinal(), 0);
        assert_eq!(TileLayer::Block.name(), "block");
    }
}
