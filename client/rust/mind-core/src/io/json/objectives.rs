// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapObjectives` JSON codec (plan 04 §3.6; ported from
//! `core/src/mindustry/game/MapObjectives.java`).
//!
//! This is the **persistence shape** of the objective executor: plan 12 owns
//! the runtime behavior (`update()`/`done()`, flags, logic code, markers UI).
//! Field names stay exactly upstream camelCase (OD9) and every struct is
//! `#[serde(default)]` so old/new JSON both load.
//!
//! Upstream fidelity notes:
//! - Serialization writes the **plain class name**
//!   (`"class": "DestroyUnits"`); reads accept both the plain name and the
//!   camelized legacy tag (`destroyUnits`), matching `registerObjective`.
//! - A `class` starting with a lowercase letter (pre-uppercase legacy saves)
//!   returns an **empty** executor — upstream drops all objectives to avoid
//!   deserializing classes that no longer exist.
//! - Unknown class tags warn + skip that objective (plan 04 deviation 6);
//!   upstream would throw away the whole array.
//! - `parents` are serialized as indices into the array (upstream resolves
//!   object references); the reader remaps them across skipped elements and
//!   drops out-of-range indices exactly like `JsonIO`'s second pass.

use std::sync::OnceLock;

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use super::content_serde::ColorHex;
use crate::content::Rgba;
use crate::io::{IoError, IoResult};

/// `Point2` wire shape (integer tile coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Point2 {
    /// X coordinate.
    #[serde(default)]
    pub x: i32,
    /// Y coordinate.
    #[serde(default)]
    pub y: i32,
}

/// `Vec2` wire shape (world coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec2 {
    /// X coordinate.
    #[serde(default)]
    pub x: f32,
    /// Y coordinate.
    #[serde(default)]
    pub y: f32,
}

/// `Layer.overlayUI` — default `ObjectiveMarker.drawLayer`.
pub const DRAW_LAYER_OVERLAY_UI: f32 = 120.0;

/// `WorldLabel.flagBackground | WorldLabel.flagOutline`.
pub const LABEL_FLAGS_DEFAULT: i8 = 3;

/// `Align.center`.
pub const ALIGN_CENTER: i32 = 1;

/// The packed `Point2.pack(-999, -999)` editor position default.
pub const fn default_editor_pos() -> i32 {
    (-999i32 << 16) | (-999i32 & 0xffff)
}

/// `Point2.pack(x, y)`.
pub const fn pack_point2(x: i32, y: i32) -> i32 {
    (x << 16) | (y & 0xffff)
}

/// Fields every `MapObjective` carries (`MapObjectives.MapObjective`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ObjectiveCommon {
    /// Hidden from the objective list UI.
    pub hidden: bool,
    /// Multi-line details shown on click.
    pub details: Option<String>,
    /// Logic code run on completion (`LExecutor.runLogicScript`).
    pub completion_logic_code: Option<String>,
    /// Rule flags added on completion.
    pub flags_added: Vec<String>,
    /// Rule flags removed on completion.
    pub flags_removed: Vec<String>,
    /// Markers attached to this objective.
    pub markers: Vec<ObjectiveMarker>,
    /// Indices of parent objectives in the container array.
    pub parents: Vec<usize>,
    /// Packed editor position (`Point2.pack(editorX, editorY)`).
    pub editor_pos: i32,
}

impl Default for ObjectiveCommon {
    fn default() -> Self {
        Self {
            hidden: false,
            details: None,
            completion_logic_code: None,
            flags_added: Vec::new(),
            flags_removed: Vec::new(),
            markers: Vec::new(),
            parents: Vec::new(),
            editor_pos: default_editor_pos(),
        }
    }
}

macro_rules! objective_struct {
    ($name:ident) => {
        objective_struct!($name,);
    };
    ($name:ident, $($field:ident : $ty:ty = $default:expr),* $(,)?) => {
        /// Objective variant fields (ported field set).
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name {
            /// Shared objective fields.
            pub common: ObjectiveCommon,
            $(
                #[doc = concat!("`", stringify!($field), "` field.")]
                pub $field: $ty,
            )*
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    common: ObjectiveCommon::default(),
                    $($field: $default,)*
                }
            }
        }
    };
}

objective_struct!(ResearchObjective, content: Option<String> = Some("copper".to_owned()));
objective_struct!(ProduceObjective, content: Option<String> = Some("copper".to_owned()));
objective_struct!(ItemObjective, item: Option<String> = Some("copper".to_owned()), amount: i32 = 1);
objective_struct!(CoreItemObjective, item: Option<String> = Some("copper".to_owned()), amount: i32 = 2);
objective_struct!(BuildCountObjective, block: Option<String> = Some("conveyor".to_owned()), count: i32 = 1);
objective_struct!(UnitCountObjective, unit: Option<String> = Some("dagger".to_owned()), count: i32 = 1);
objective_struct!(DestroyUnitsObjective, count: i32 = 1);
objective_struct!(
    TimerObjective,
    text: Option<String> = None,
    duration: f32 = 60.0 * 30.0,
    countup: f32 = 0.0
);
objective_struct!(
    DestroyBlockObjective,
    pos: Point2 = Point2 { x: 0, y: 0 },
    team: u8 = 2,
    block: Option<String> = Some("router".to_owned())
);
objective_struct!(
    DestroyBlocksObjective,
    positions: Vec<Point2> = Vec::new(),
    team: u8 = 2,
    block: Option<String> = Some("router".to_owned())
);
objective_struct!(CommandModeObjective);
objective_struct!(FlagObjective, flag: String = "flag".to_owned(), text: Option<String> = None);
objective_struct!(DestroyCoreObjective);

/// One in-map objective (`MapObjectives.MapObjective` subclasses).
#[derive(Debug, Clone, PartialEq)]
pub enum MapObjective {
    /// Research an unlockable.
    Research(ResearchObjective),
    /// Produce an unlockable.
    Produce(ProduceObjective),
    /// Store an item in the core.
    Item(ItemObjective),
    /// Transport an item into the core.
    CoreItem(CoreItemObjective),
    /// Place a block N times.
    BuildCount(BuildCountObjective),
    /// Produce N units.
    UnitCount(UnitCountObjective),
    /// Destroy N enemy units.
    DestroyUnits(DestroyUnitsObjective),
    /// Wait for a duration.
    Timer(TimerObjective),
    /// Destroy one block.
    DestroyBlock(DestroyBlockObjective),
    /// Destroy a set of blocks.
    DestroyBlocks(DestroyBlocksObjective),
    /// Command a unit.
    CommandMode(CommandModeObjective),
    /// Wait for a logic flag.
    Flag(FlagObjective),
    /// Destroy all enemy cores.
    DestroyCore(DestroyCoreObjective),
}

impl MapObjective {
    /// The shared objective fields.
    pub fn common(&self) -> &ObjectiveCommon {
        match self {
            MapObjective::Research(o) => &o.common,
            MapObjective::Produce(o) => &o.common,
            MapObjective::Item(o) => &o.common,
            MapObjective::CoreItem(o) => &o.common,
            MapObjective::BuildCount(o) => &o.common,
            MapObjective::UnitCount(o) => &o.common,
            MapObjective::DestroyUnits(o) => &o.common,
            MapObjective::Timer(o) => &o.common,
            MapObjective::DestroyBlock(o) => &o.common,
            MapObjective::DestroyBlocks(o) => &o.common,
            MapObjective::CommandMode(o) => &o.common,
            MapObjective::Flag(o) => &o.common,
            MapObjective::DestroyCore(o) => &o.common,
        }
    }

    /// The shared objective fields, mutably (parents remap).
    pub fn common_mut(&mut self) -> &mut ObjectiveCommon {
        match self {
            MapObjective::Research(o) => &mut o.common,
            MapObjective::Produce(o) => &mut o.common,
            MapObjective::Item(o) => &mut o.common,
            MapObjective::CoreItem(o) => &mut o.common,
            MapObjective::BuildCount(o) => &mut o.common,
            MapObjective::UnitCount(o) => &mut o.common,
            MapObjective::DestroyUnits(o) => &mut o.common,
            MapObjective::Timer(o) => &mut o.common,
            MapObjective::DestroyBlock(o) => &mut o.common,
            MapObjective::DestroyBlocks(o) => &mut o.common,
            MapObjective::CommandMode(o) => &mut o.common,
            MapObjective::Flag(o) => &mut o.common,
            MapObjective::DestroyCore(o) => &mut o.common,
        }
    }

    /// Plain class tag written upstream (`registerObjective` last tag wins).
    pub fn class_tag(&self) -> &'static str {
        match self {
            MapObjective::Research(_) => "Research",
            MapObjective::Produce(_) => "Produce",
            MapObjective::Item(_) => "Item",
            MapObjective::CoreItem(_) => "CoreItem",
            MapObjective::BuildCount(_) => "BuildCount",
            MapObjective::UnitCount(_) => "UnitCount",
            MapObjective::DestroyUnits(_) => "DestroyUnits",
            MapObjective::Timer(_) => "Timer",
            MapObjective::DestroyBlock(_) => "DestroyBlock",
            MapObjective::DestroyBlocks(_) => "DestroyBlocks",
            MapObjective::CommandMode(_) => "CommandMode",
            MapObjective::Flag(_) => "Flag",
            MapObjective::DestroyCore(_) => "DestroyCore",
        }
    }
}

impl Serialize for MapObjective {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("class", self.class_tag())?;
        serialize_common(&mut map, self.common())?;
        match self {
            MapObjective::Research(o) => {
                map.serialize_entry("content", &o.content)?;
            }
            MapObjective::Produce(o) => {
                map.serialize_entry("content", &o.content)?;
            }
            MapObjective::Item(o) => {
                map.serialize_entry("item", &o.item)?;
                map.serialize_entry("amount", &o.amount)?;
            }
            MapObjective::CoreItem(o) => {
                map.serialize_entry("item", &o.item)?;
                map.serialize_entry("amount", &o.amount)?;
            }
            MapObjective::BuildCount(o) => {
                map.serialize_entry("block", &o.block)?;
                map.serialize_entry("count", &o.count)?;
            }
            MapObjective::UnitCount(o) => {
                map.serialize_entry("unit", &o.unit)?;
                map.serialize_entry("count", &o.count)?;
            }
            MapObjective::DestroyUnits(o) => {
                map.serialize_entry("count", &o.count)?;
            }
            MapObjective::Timer(o) => {
                map.serialize_entry("text", &o.text)?;
                map.serialize_entry("duration", &o.duration)?;
                map.serialize_entry("countup", &o.countup)?;
            }
            MapObjective::DestroyBlock(o) => {
                map.serialize_entry("pos", &o.pos)?;
                map.serialize_entry("team", &o.team)?;
                map.serialize_entry("block", &o.block)?;
            }
            MapObjective::DestroyBlocks(o) => {
                map.serialize_entry("positions", &o.positions)?;
                map.serialize_entry("team", &o.team)?;
                map.serialize_entry("block", &o.block)?;
            }
            MapObjective::CommandMode(_) => {}
            MapObjective::Flag(o) => {
                map.serialize_entry("flag", &o.flag)?;
                map.serialize_entry("text", &o.text)?;
            }
            MapObjective::DestroyCore(_) => {}
        }
        map.serialize_entry("parents", &self.common().parents)?;
        map.serialize_entry("editorPos", &self.common().editor_pos)?;
        map.end()
    }
}

fn serialize_common<M: SerializeMap>(
    map: &mut M,
    common: &ObjectiveCommon,
) -> Result<(), M::Error> {
    map.serialize_entry("hidden", &common.hidden)?;
    map.serialize_entry("details", &common.details)?;
    map.serialize_entry("completionLogicCode", &common.completion_logic_code)?;
    map.serialize_entry("flagsAdded", &common.flags_added)?;
    map.serialize_entry("flagsRemoved", &common.flags_removed)?;
    map.serialize_entry("markers", &common.markers)?;
    Ok(())
}

/// A registered class tag parser (`JsonIO.classTag` + `readValue`).
pub type ObjectiveParser = fn(&Value) -> Option<MapObjective>;

/// Tag → deserializer registry (plan 04 §3.6).
#[derive(Clone, Default)]
pub struct ClassTagRegistry {
    tags: Vec<(&'static str, ObjectiveParser)>,
}

impl ClassTagRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one tag (append-only).
    pub fn register(&mut self, tag: &'static str, parser: ObjectiveParser) {
        self.tags.push((tag, parser));
    }

    /// Looks up a tag.
    pub fn get(&self, tag: &str) -> Option<ObjectiveParser> {
        self.tags
            .iter()
            .find_map(|(name, parser)| (*name == tag).then_some(*parser))
    }

    /// All registered tags in registration order.
    pub fn tag_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.tags.iter().map(|(tag, _)| *tag)
    }

    /// The upstream objective registrations: plain class name + camelized tag
    /// (`registerObjective` registers both; the plain tag is written).
    pub fn objectives() -> Self {
        let mut registry = Self::new();
        macro_rules! register {
            ($plain:literal, $camel:literal, $parser:path) => {
                registry.register($plain, $parser);
                registry.register($camel, $parser);
            };
        }
        register!("Research", "research", parse_research);
        register!("Produce", "produce", parse_produce);
        register!("Item", "item", parse_item);
        register!("CoreItem", "coreItem", parse_core_item);
        register!("BuildCount", "buildCount", parse_build_count);
        register!("UnitCount", "unitCount", parse_unit_count);
        register!("DestroyUnits", "destroyUnits", parse_destroy_units);
        register!("Timer", "timer", parse_timer);
        register!("DestroyBlock", "destroyBlock", parse_destroy_block);
        register!("DestroyBlocks", "destroyBlocks", parse_destroy_blocks);
        register!("CommandMode", "commandMode", parse_command_mode);
        register!("Flag", "flag", parse_flag);
        register!("DestroyCore", "destroyCore", parse_destroy_core);
        registry
    }
}

/// The process-wide objective tag registry.
pub fn class_tags() -> &'static ClassTagRegistry {
    static REGISTRY: OnceLock<ClassTagRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ClassTagRegistry::objectives)
}

/// The objective executor (`MapObjectives`): an ordered list with index-based
/// parent links.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapObjectives {
    /// All objectives; do not reorder after parents are resolved (plan 12).
    pub all: Vec<MapObjective>,
}

impl MapObjectives {
    /// Empty executor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of objectives.
    pub fn len(&self) -> usize {
        self.all.len()
    }

    /// Whether there are no objectives.
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// Objective at `index` (`get`).
    pub fn get(&self, index: usize) -> Option<&MapObjective> {
        self.all.get(index)
    }

    /// Iterates objectives in order.
    pub fn iter(&self) -> std::slice::Iter<'_, MapObjective> {
        self.all.iter()
    }

    /// Parses the upstream JSON array, applying the legacy lowercase rule and
    /// parent fix-up.
    pub fn from_value(value: &Value) -> Result<Self, IoError> {
        let Some(items) = value.as_array() else {
            return Err(IoError::corrupt("objectives must be a JSON array"));
        };
        // Upstream: any lowercase class tag means the whole array is legacy.
        for item in items {
            let class = item
                .get("class")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if class.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
                return Ok(Self::default());
            }
        }

        let registry = class_tags();
        let mut parsed: Vec<Option<MapObjective>> = Vec::with_capacity(items.len());
        for item in items {
            let tag = item
                .get("class")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match registry.get(tag) {
                Some(parser) => match parser(item) {
                    Some(mut objective) => {
                        *objective.common_mut() = common_from_value(item);
                        parsed.push(Some(objective));
                    }
                    None => {
                        log::warn!("skipping malformed objective class `{tag}`");
                        parsed.push(None);
                    }
                },
                None => {
                    log::warn!("skipping unknown objective class tag `{tag}`");
                    parsed.push(None);
                }
            }
        }

        // Index remap across skipped elements (deviation 6).
        let mut index_map: Vec<Option<usize>> = Vec::with_capacity(parsed.len());
        let mut next = 0usize;
        for entry in &parsed {
            if entry.is_some() {
                index_map.push(Some(next));
                next += 1;
            } else {
                index_map.push(None);
            }
        }

        let mut all = Vec::with_capacity(next);
        for mut objective in parsed.into_iter().flatten() {
            let remapped: Vec<usize> = objective
                .common()
                .parents
                .iter()
                .filter_map(|parent| index_map.get(*parent).copied().flatten())
                .collect();
            objective.common_mut().parents = remapped;
            all.push(objective);
        }
        Ok(Self { all })
    }

    /// Parses an upstream JSON string.
    pub fn from_json(string: &str) -> IoResult<Self> {
        let value: Value = serde_json::from_str(string)?;
        Self::from_value(&value)
    }

    /// Serializes to the upstream JSON string (`JsonIO.write`).
    pub fn to_json(&self) -> IoResult<String> {
        Ok(serde_json::to_string(self)?)
    }
}

impl Serialize for MapObjectives {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.all.iter())
    }
}

impl<'de> Deserialize<'de> for MapObjectives {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        MapObjectives::from_value(&value).map_err(serde::de::Error::custom)
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn i32_field(value: &Value, key: &str, default: i32) -> i32 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .map(|v| v as i32)
        .unwrap_or(default)
}

fn f32_field(value: &Value, key: &str, default: f32) -> f32 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn string_vec(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn usize_vec(value: &Value, key: &str) -> Vec<usize> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_u64)
                .map(|v| v as usize)
                .collect()
        })
        .unwrap_or_default()
}

fn point2(value: &Value) -> Point2 {
    Point2 {
        x: i32_field(value, "x", 0),
        y: i32_field(value, "y", 0),
    }
}

fn markers(value: &Value) -> Vec<ObjectiveMarker> {
    value
        .get("markers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match serde_json::from_value(item.clone()) {
                    Ok(marker) => Some(marker),
                    Err(err) => {
                        log::warn!("skipping unknown objective marker: {err}");
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn common_from_value(value: &Value) -> ObjectiveCommon {
    ObjectiveCommon {
        hidden: value
            .get("hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        details: string_field(value, "details"),
        completion_logic_code: string_field(value, "completionLogicCode"),
        flags_added: string_vec(value, "flagsAdded"),
        flags_removed: string_vec(value, "flagsRemoved"),
        markers: markers(value),
        parents: usize_vec(value, "parents"),
        editor_pos: i32_field(value, "editorPos", default_editor_pos()),
    }
}

fn parse_research(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::Research(ResearchObjective {
        content: Some(string_field(value, "content").unwrap_or_else(|| "copper".to_owned())),
        ..Default::default()
    }))
}

fn parse_produce(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::Produce(ProduceObjective {
        content: Some(string_field(value, "content").unwrap_or_else(|| "copper".to_owned())),
        ..Default::default()
    }))
}

fn parse_item(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::Item(ItemObjective {
        item: Some(string_field(value, "item").unwrap_or_else(|| "copper".to_owned())),
        amount: i32_field(value, "amount", 1),
        ..Default::default()
    }))
}

fn parse_core_item(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::CoreItem(CoreItemObjective {
        item: Some(string_field(value, "item").unwrap_or_else(|| "copper".to_owned())),
        amount: i32_field(value, "amount", 2),
        ..Default::default()
    }))
}

fn parse_build_count(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::BuildCount(BuildCountObjective {
        block: Some(string_field(value, "block").unwrap_or_else(|| "conveyor".to_owned())),
        count: i32_field(value, "count", 1),
        ..Default::default()
    }))
}

fn parse_unit_count(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::UnitCount(UnitCountObjective {
        unit: Some(string_field(value, "unit").unwrap_or_else(|| "dagger".to_owned())),
        count: i32_field(value, "count", 1),
        ..Default::default()
    }))
}

fn parse_destroy_units(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::DestroyUnits(DestroyUnitsObjective {
        count: i32_field(value, "count", 1),
        ..Default::default()
    }))
}

fn parse_timer(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::Timer(TimerObjective {
        text: string_field(value, "text"),
        duration: f32_field(value, "duration", 60.0 * 30.0),
        countup: f32_field(value, "countup", 0.0),
        ..Default::default()
    }))
}

fn parse_destroy_block(value: &Value) -> Option<MapObjective> {
    let team = i32_field(value, "team", 2) as u8;
    let block = string_field(value, "block").unwrap_or_else(|| "router".to_owned());
    Some(MapObjective::DestroyBlock(DestroyBlockObjective {
        pos: point2(value.get("pos").unwrap_or(&Value::Null)),
        team,
        block: Some(block),
        ..Default::default()
    }))
}

fn parse_destroy_blocks(value: &Value) -> Option<MapObjective> {
    let team = i32_field(value, "team", 2) as u8;
    let block = string_field(value, "block").unwrap_or_else(|| "router".to_owned());
    Some(MapObjective::DestroyBlocks(DestroyBlocksObjective {
        positions: value
            .get("positions")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(point2).collect())
            .unwrap_or_default(),
        team,
        block: Some(block),
        ..Default::default()
    }))
}

fn parse_command_mode(_value: &Value) -> Option<MapObjective> {
    Some(MapObjective::CommandMode(CommandModeObjective::default()))
}

fn parse_flag(value: &Value) -> Option<MapObjective> {
    Some(MapObjective::Flag(FlagObjective {
        flag: string_field(value, "flag").unwrap_or_else(|| "flag".to_owned()),
        text: string_field(value, "text"),
        ..Default::default()
    }))
}

fn parse_destroy_core(_value: &Value) -> Option<MapObjective> {
    Some(MapObjective::DestroyCore(DestroyCoreObjective::default()))
}

/// A marker attached to an objective (`ObjectiveMarker` subclasses).
///
/// Upstream's `@IndexBool` int fields are written as ints but read as either
/// ints or booleans (`updateField`); the custom deserializers below accept
/// both so old/new JSON both load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "class")]
pub enum ObjectiveMarker {
    /// `PointMarker` (plus the legacy `Minimap` alias).
    #[serde(
        rename = "Point",
        alias = "point",
        alias = "Minimap",
        alias = "minimap"
    )]
    Point(PointMarker),
    /// `ShapeTextMarker`.
    #[serde(rename = "ShapeText", alias = "shapeText")]
    ShapeText(ShapeTextMarker),
    /// `ShapeMarker`.
    #[serde(rename = "Shape", alias = "shape")]
    Shape(ShapeMarker),
    /// `TextMarker`.
    #[serde(rename = "Text", alias = "text")]
    Text(TextMarker),
    /// `LineMarker`.
    #[serde(rename = "Line", alias = "line")]
    Line(LineMarker),
    /// `TextureMarker`.
    #[serde(rename = "Texture", alias = "texture")]
    Texture(TextureMarker),
    /// `QuadMarker`.
    #[serde(rename = "Quad", alias = "quad")]
    Quad(QuadMarker),
    /// `LightMarker`.
    #[serde(rename = "Light", alias = "light")]
    Light(LightMarker),
}

/// `TextureHolder` value: string region, content name, or building position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextureHolder {
    /// Atlas region or asset name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string: Option<String>,
    /// Unlockable content name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Packed building position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub building: Option<i32>,
}

impl Default for TextureHolder {
    fn default() -> Self {
        Self {
            string: Some("white".to_owned()),
            content: None,
            building: None,
        }
    }
}

/// The shared `ObjectiveMarker` base fields (`ObjectiveMarker`).
///
/// Duplicated as concrete fields on each marker struct (serde's internally
/// tagged enums cannot use `flatten`), keeping the wire shape identical.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerCommon {
    /// Display in the world.
    pub world: i32,
    /// Display on the minimap (`-1` = false).
    pub minimap: i32,
    /// Emit light (`-1` = false).
    pub light: i32,
    /// Scale with zoom.
    pub autoscale: bool,
    /// Z-sorting layer.
    pub draw_layer: f32,
}

impl Default for MarkerCommon {
    fn default() -> Self {
        Self {
            world: 1,
            minimap: -1,
            light: -1,
            autoscale: false,
            draw_layer: DRAW_LAYER_OVERLAY_UI,
        }
    }
}

macro_rules! marker_struct {
    ($name:ident, $($field:ident : $ty:ty = $default:expr),* $(,)?) => {
        /// Objective marker struct (ported field set).
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        pub struct $name {
            /// Display in the world (`@IndexBool` int; reads bool too).
            #[serde(deserialize_with = "de_index_bool")]
            pub world: i32,
            /// Display on the minimap (`@IndexBool` int; reads bool too).
            #[serde(deserialize_with = "de_index_bool")]
            pub minimap: i32,
            /// Emit light (`@IndexBool` int; reads bool too).
            #[serde(deserialize_with = "de_index_bool")]
            pub light: i32,
            /// Scale with zoom.
            pub autoscale: bool,
            /// Z-sorting layer.
            pub draw_layer: f32,
            $(
                #[doc = concat!("`", stringify!($field), "` field.")]
                pub $field: $ty,
            )*
        }
        impl Default for $name {
            fn default() -> Self {
                let common = MarkerCommon::default();
                Self {
                    world: common.world,
                    minimap: common.minimap,
                    light: common.light,
                    autoscale: common.autoscale,
                    draw_layer: common.draw_layer,
                    $($field: $default,)*
                }
            }
        }
        impl $name {
            /// The shared marker fields.
            pub fn common(&self) -> MarkerCommon {
                MarkerCommon {
                    world: self.world,
                    minimap: self.minimap,
                    light: self.light,
                    autoscale: self.autoscale,
                    draw_layer: self.draw_layer,
                }
            }
        }
    };
}

marker_struct!(
    PointMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    radius: f32 = 5.0,
    stroke: f32 = 11.0,
    color: ColorHex = ColorHex(Rgba::from_rgba8888(0xf25555ff)),
);

marker_struct!(
    ShapeTextMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    text: String = String::new(),
    font_size: f32 = 1.0,
    text_height: f32 = 7.0,
    flags: i8 = LABEL_FLAGS_DEFAULT,
    text_align: i32 = ALIGN_CENTER,
    line_align: i32 = ALIGN_CENTER,
    radius: f32 = 6.0,
    rotation: f32 = 0.0,
    sides: i32 = 4,
    color: ColorHex = ColorHex(Rgba::from_rgba8888(0xffd37fff)),
);

marker_struct!(
    ShapeMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    radius: f32 = 8.0,
    rotation: f32 = 0.0,
    stroke: f32 = 1.0,
    start_angle: f32 = 0.0,
    end_angle: f32 = 360.0,
    fill: bool = false,
    outline: bool = true,
    sides: i32 = 4,
    color: ColorHex = ColorHex(Rgba::from_rgba8888(0xffd37fff)),
);

marker_struct!(
    TextMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    text: String = String::new(),
    font_size: f32 = 1.0,
    flags: i8 = LABEL_FLAGS_DEFAULT,
    text_align: i32 = ALIGN_CENTER,
    line_align: i32 = ALIGN_CENTER,
);

marker_struct!(
    LineMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    end_pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    stroke: f32 = 1.0,
    outline: bool = true,
    color1: ColorHex = ColorHex(Rgba::from_rgba8888(0xffd37fff)),
    color2: ColorHex = ColorHex(Rgba::from_rgba8888(0xffd37fff)),
);

marker_struct!(
    TextureMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    rotation: f32 = 0.0,
    width: f32 = 0.0,
    height: f32 = 0.0,
    texture: TextureHolder = TextureHolder::default(),
    color: ColorHex = ColorHex(Rgba::WHITE),
);

marker_struct!(
    QuadMarker,
    texture: TextureHolder = TextureHolder::default(),
    vertices: Vec<f32> = default_quad_vertices(),
    map_region: bool = true,
);

marker_struct!(
    LightMarker,
    pos: Vec2 = Vec2 { x: 0.0, y: 0.0 },
    radius: f32 = 5.0,
    color: ColorHex = ColorHex(Rgba::from_rgba8888(0xffd37fff)),
);

/// `Color.white.toFloatBits()` for the two white QuadMarker vertices.
fn default_quad_vertices() -> Vec<f32> {
    let mut vertices = vec![0.0f32; 24];
    for i in 0..4 {
        vertices[i * 6 + 2] = f32::from_bits(0xfeff_ffff);
        vertices[i * 6 + 5] = 0.0;
    }
    vertices
}

/// `@IndexBool` field read: ints pass through, booleans map to 1/-1
/// (`ObjectiveMarker.updateField`).
fn de_index_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Bool(true) => 1,
        Value::Bool(false) => -1,
        Value::Number(number) => number.as_i64().unwrap_or(1) as i32,
        _ => 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_legacy_lowercase_arrays() {
        assert!(MapObjectives::from_json("[]").unwrap().is_empty());
        // Legacy lowercase class tag → empty executor (upstream behavior).
        let legacy = r#"[{"class":"item","item":"copper","amount":5}]"#;
        assert!(MapObjectives::from_json(legacy).unwrap().is_empty());
    }

    #[test]
    fn objective_roundtrip_preserves_fields_and_parents() {
        let json = r#"[
            {"class":"Item","item":"copper","amount":5,"hidden":true,
             "details":"gather","flagsAdded":["a"],"flagsRemoved":["b"],
             "markers":[{"class":"Point","world":1,"minimap":-1,"light":-1,
                          "autoscale":false,"drawLayer":120.0,
                          "pos":{"x":10.5,"y":20.5},"radius":5.0,
                          "stroke":11.0,"color":"f25555ff"}],
             "parents":[],"editorPos":0},
            {"class":"DestroyUnits","count":3,"parents":[0],"editorPos":-1}
        ]"#;
        let objectives = MapObjectives::from_json(json).unwrap();
        assert_eq!(objectives.len(), 2);
        assert_eq!(objectives.all[1].common().parents, vec![0]);
        assert!(objectives.all[0].common().hidden);

        let out = objectives.to_json().unwrap();
        let reparsed = MapObjectives::from_json(&out).unwrap();
        assert_eq!(reparsed, objectives);
        assert!(out.contains("\"class\":\"Item\""));
        assert!(out.contains("\"class\":\"DestroyUnits\""));
        assert!(out.contains("\"parents\":[0]"));
        assert!(out.contains("\"editorPos\":0"));
    }

    #[test]
    fn unknown_class_is_skipped_and_parents_remapped() {
        let json = r#"[
            {"class":"Item","item":"copper","amount":1},
            {"class":"FutureThing","wat":true},
            {"class":"Flag","flag":"x","parents":[0,1,10]}
        ]"#;
        let objectives = MapObjectives::from_json(json).unwrap();
        assert_eq!(objectives.len(), 2);
        // original index 0 -> kept 0; original 1 (skipped) and 10 (oob) drop.
        assert_eq!(objectives.all[1].common().parents, vec![0]);
    }

    #[test]
    fn index_bool_fields_accept_bool_and_int() {
        let json = r#"[{"class":"Flag","flag":"x","markers":[
            {"class":"Text","text":"hi","world":true,"minimap":false,"light":-1,
             "autoscale":true,"drawLayer":120.0,"fontSize":1.0,"flags":3,
             "textAlign":1,"lineAlign":1,"pos":{"x":1,"y":2}}]}]"#;
        let objectives = MapObjectives::from_json(json).unwrap();
        let ObjectiveMarker::Text(marker) = &objectives.all[0].common().markers[0] else {
            panic!("expected text marker");
        };
        assert_eq!(marker.world, 1);
        assert_eq!(marker.minimap, -1);
        assert!(marker.autoscale);
        assert_eq!(marker.text, "hi");
    }

    #[test]
    fn missing_fields_use_upstream_defaults() {
        let objectives = MapObjectives::from_json(r#"[{"class":"Item"}]"#).unwrap();
        let MapObjective::Item(item) = &objectives.all[0] else {
            panic!("expected item objective");
        };
        assert_eq!(item.item.as_deref(), Some("copper"));
        assert_eq!(item.amount, 1);
        assert_eq!(
            objectives.all[0].common().editor_pos,
            pack_point2(-999, -999)
        );
    }

    #[test]
    fn class_tag_registry_lists_both_forms() {
        let registry = class_tags();
        assert!(registry.get("DestroyUnits").is_some());
        assert!(registry.get("destroyUnits").is_some());
        assert!(registry.get("nope").is_none());
        assert_eq!(registry.tag_names().count(), 26);
    }
}
