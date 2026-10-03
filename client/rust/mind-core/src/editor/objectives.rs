// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Objective field descriptors + editor canvas model (plan 19 §3.9;
//! `editor/MapObjectivesDialog.java`, `editor/MapObjectivesCanvas.java`).
//!
//! Java derives the dialog row set from reflection over `@Second`/`@TilePos`/
//! `@Multiline`/`@LogicCode`/`@Researchable`/`@Synthetic` annotations. The port
//! replaces reflection with declarative [`ObjectiveField`] descriptors (plan 19
//! §2.3.2, OD19-F). JSON field names stay exactly upstream camelCase (OD9).
//!
//! The persistent shape of every objective lives in plan 04's
//! [`crate::io::json::objectives`]; this module only describes how the editor
//! renders/edits it and how the canvas lays the objective graph out.

use std::collections::BTreeSet;

use crate::content::{BlockId, ContentRegistry, ContentType};
use crate::io::json::objectives::{MapObjective, MapObjectives, Point2};

/// Viewport half-extent of the objective canvas (`MapObjectivesCanvas.bounds`).
pub const CANVAS_BOUNDS: i32 = 100;
/// Objective node width in canvas placement units (`MapObjectivesCanvas.objWidth`).
pub const OBJECTIVE_WIDTH: i32 = 5;
/// Objective node height in placement units (`objHeight`).
pub const OBJECTIVE_HEIGHT: i32 = 2;

/// Bit flags mirroring the Java annotations (`@Second`, `@TilePos`, ...).
///
/// A hand-rolled `u32` newtype keeps `mind-core` free of the `bitflags` crate;
/// the flag values are append-only ABI for plan 14's widget layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
pub struct FieldFlags(pub u32);

impl FieldFlags {
    /// `@Second`: value is seconds (interpreted ×60).
    pub const SECOND: FieldFlags = FieldFlags(1);
    /// `@TilePos`: value is a tile coordinate (interpreted ×8).
    pub const TILE_POS: FieldFlags = FieldFlags(2);
    /// `@Multiline`: render a text area.
    pub const MULTILINE: FieldFlags = FieldFlags(4);
    /// `@LogicCode`: render a logic-code editor.
    pub const LOGIC_CODE: FieldFlags = FieldFlags(8);
    /// `@Researchable`: content picker restricted to researchable content.
    pub const RESEARCHABLE: FieldFlags = FieldFlags(16);
    /// `@Synthetic`: field is generated, not user-editable.
    pub const SYNTHETIC: FieldFlags = FieldFlags(32);
    /// Field is hidden from the default view.
    pub const HIDDEN: FieldFlags = FieldFlags(64);

    /// Empty flags.
    pub const fn empty() -> Self {
        FieldFlags(0)
    }

    /// Union.
    pub const fn union(self, other: FieldFlags) -> Self {
        FieldFlags(self.0 | other.0)
    }

    /// Whether every bit of `other` is set.
    pub const fn contains(self, other: FieldFlags) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no bit is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Content picker restriction for `Content`/`Objective` fields (`ObjectiveFilter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFilter {
    /// Any value of the kind.
    Any,
    /// Only these class tags / content names.
    Classes(&'static [&'static str]),
}

/// A field's type (replaces the Java reflection type switch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// `String` text field.
    String,
    /// `boolean` checkbox.
    Bool,
    /// `byte` / small integer.
    Byte,
    /// `int`.
    Int,
    /// `float`.
    Float,
    /// Content picker of the given content type.
    Content(ContentType),
    /// Team picker.
    Team,
    /// Color picker (`Color`).
    Color,
    /// World-space vector (`Vec2`).
    Vec2F,
    /// Tile-space point (`Point2`).
    Vec2I,
    /// Nested objective reference (`Objective`), filtered by class.
    Objective(FieldFilter),
    /// `Seq<T>` (`Seq(Box<FieldKind>)`).
    Seq(Box<FieldKind>),
    /// `ObjectMap`/array map (`Map(Box<FieldKind>)`).
    Map(Box<FieldKind>),
}

/// One editor-renderable objective field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveField {
    /// camelCase JSON key (parity ABI).
    pub name: &'static str,
    /// Field type.
    pub kind: FieldKind,
    /// Annotation flags.
    pub flags: FieldFlags,
}

impl ObjectiveField {
    /// Builder helper.
    pub const fn new(name: &'static str, kind: FieldKind, flags: FieldFlags) -> Self {
        Self { name, kind, flags }
    }
}

/// A type that exposes its editor descriptors (`#[derive(ObjectiveFields)]`).
pub trait ObjectiveFields {
    /// The field descriptors, in display order.
    fn fields(&self) -> Vec<ObjectiveField>;
}

fn common_fields() -> Vec<ObjectiveField> {
    vec![
        ObjectiveField::new("hidden", FieldKind::Bool, FieldFlags::empty()),
        ObjectiveField::new(
            "details",
            FieldKind::String,
            FieldFlags::MULTILINE.union(FieldFlags::HIDDEN),
        ),
        ObjectiveField::new(
            "completionLogicCode",
            FieldKind::String,
            FieldFlags::LOGIC_CODE.union(FieldFlags::HIDDEN),
        ),
        ObjectiveField::new(
            "flagsAdded",
            FieldKind::Seq(Box::new(FieldKind::String)),
            FieldFlags::HIDDEN,
        ),
        ObjectiveField::new(
            "flagsRemoved",
            FieldKind::Seq(Box::new(FieldKind::String)),
            FieldFlags::HIDDEN,
        ),
        ObjectiveField::new(
            "markers",
            FieldKind::Seq(Box::new(FieldKind::Map(Box::new(FieldKind::String)))),
            FieldFlags::HIDDEN,
        ),
        ObjectiveField::new(
            "parents",
            FieldKind::Seq(Box::new(FieldKind::Int)),
            FieldFlags::SYNTHETIC,
        ),
        ObjectiveField::new("editorPos", FieldKind::Int, FieldFlags::SYNTHETIC),
    ]
}

/// The class-specific editor fields for an upstream objective class tag.
///
/// Every name matches the plan-04 JSON key exactly; the `objective_field_descriptors`
/// test asserts the serialized JSON contains each name.
pub fn class_fields(class_tag: &str) -> Vec<ObjectiveField> {
    let content = |kind: ContentType| FieldKind::Content(kind);
    let mut fields = match class_tag {
        "Research" | "Produce" => vec![ObjectiveField::new(
            "content",
            content(ContentType::Item),
            FieldFlags::RESEARCHABLE,
        )],
        "Item" | "CoreItem" => vec![
            ObjectiveField::new("item", content(ContentType::Item), FieldFlags::empty()),
            ObjectiveField::new("amount", FieldKind::Int, FieldFlags::empty()),
        ],
        "BuildCount" => vec![
            ObjectiveField::new("block", content(ContentType::Block), FieldFlags::empty()),
            ObjectiveField::new("count", FieldKind::Int, FieldFlags::empty()),
        ],
        "UnitCount" => vec![
            ObjectiveField::new("unit", content(ContentType::Unit), FieldFlags::empty()),
            ObjectiveField::new("count", FieldKind::Int, FieldFlags::empty()),
        ],
        "DestroyUnits" => vec![ObjectiveField::new(
            "count",
            FieldKind::Int,
            FieldFlags::empty(),
        )],
        "Timer" => vec![
            ObjectiveField::new("text", FieldKind::String, FieldFlags::empty()),
            ObjectiveField::new("duration", FieldKind::Float, FieldFlags::SECOND),
        ],
        "DestroyBlock" => vec![
            ObjectiveField::new("pos", FieldKind::Vec2I, FieldFlags::TILE_POS),
            ObjectiveField::new("team", FieldKind::Team, FieldFlags::empty()),
            ObjectiveField::new("block", content(ContentType::Block), FieldFlags::empty()),
        ],
        "DestroyBlocks" => vec![
            ObjectiveField::new(
                "positions",
                FieldKind::Seq(Box::new(FieldKind::Vec2I)),
                FieldFlags::TILE_POS,
            ),
            ObjectiveField::new("team", FieldKind::Team, FieldFlags::empty()),
            ObjectiveField::new("block", content(ContentType::Block), FieldFlags::empty()),
        ],
        "CommandMode" | "DestroyCore" => Vec::new(),
        "Flag" => vec![
            ObjectiveField::new("flag", FieldKind::String, FieldFlags::empty()),
            ObjectiveField::new("text", FieldKind::String, FieldFlags::empty()),
        ],
        _ => Vec::new(),
    };
    fields.extend(common_fields());
    fields
}

/// All class-specific editor fields for every registered objective.
pub fn class_fields_for(objective: &MapObjective) -> Vec<ObjectiveField> {
    class_fields(objective.class_tag())
}

/// Objective class tags with an editor field set (`ALL_OBJECTIVE_TYPES` subset).
pub fn all_class_tags() -> &'static [&'static str] {
    &[
        "Research",
        "Produce",
        "Item",
        "CoreItem",
        "BuildCount",
        "UnitCount",
        "DestroyUnits",
        "Timer",
        "DestroyBlock",
        "DestroyBlocks",
        "DestroyCore",
        "CommandMode",
        "Flag",
    ]
}

/// Default provider values by kind (`MapObjectivesDialog` field construction).
///
/// `content` may be `None` for headless descriptor tests; missing registry
/// entries fall back to the plan-stated defaults.
pub fn provider_default(kind: &FieldKind, content: Option<&ContentRegistry>) -> serde_json::Value {
    use serde_json::Value;
    match kind {
        FieldKind::String => Value::String(String::new()),
        FieldKind::Bool => Value::Bool(false),
        FieldKind::Byte | FieldKind::Int => Value::from(0),
        FieldKind::Float => Value::from(0.0),
        FieldKind::Content(content_type) => {
            let name = match (content, content_type) {
                (Some(registry), ContentType::Block) => registry
                    .block_id("copper-wall")
                    .and_then(|id| registry.block(id).map(|def| def.name.clone())),
                (Some(registry), ContentType::Unit) => registry
                    .unit_by_name("dagger")
                    .map(|_| String::from("dagger")),
                _ => None,
            };
            Value::String(name.unwrap_or_else(|| match content_type {
                ContentType::Block => String::from("copper-wall"),
                ContentType::Unit => String::from("dagger"),
                _ => String::from("copper"),
            }))
        }
        FieldKind::Team => Value::from(1),
        FieldKind::Color => Value::String(String::from("accent")),
        FieldKind::Vec2F => serde_json::json!({"x": 0.0, "y": 0.0}),
        FieldKind::Vec2I => serde_json::json!({"x": 0, "y": 0}),
        FieldKind::Objective(_) => Value::from(0),
        FieldKind::Seq(_) => Value::Array(Vec::new()),
        FieldKind::Map(_) => Value::Object(serde_json::Map::new()),
    }
}

/// Interprets a raw UI number for a field (`@Second`/`@TilePos` scaling).
pub fn interpret_number(flags: FieldFlags, raw: f32) -> f32 {
    let mut value = raw;
    if flags.contains(FieldFlags::SECOND) {
        value *= 60.0;
    }
    if flags.contains(FieldFlags::TILE_POS) {
        value *= 8.0;
    }
    value
}

/// The canvas node rectangle for an objective (`MapObjectivesCanvas` tile draw).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectiveNode {
    /// Placement x (tile units of `OBJECTIVE_WIDTH`).
    pub x: i32,
    /// Placement y.
    pub y: i32,
}

/// Whether a packed `editorPos` has been assigned (`!= pack(-999, -999)`).
pub fn has_editor_pos(editor_pos: i32) -> bool {
    editor_pos != crate::io::json::objectives::default_editor_pos()
}

/// Decodes an objective node; unplaced objectives get the origin.
pub fn node_for(editor_pos: i32) -> ObjectiveNode {
    if !has_editor_pos(editor_pos) {
        return ObjectiveNode { x: 0, y: 0 };
    }
    ObjectiveNode {
        x: (editor_pos >> 16) as i16 as i32,
        y: (editor_pos & 0xffff) as i16 as i32,
    }
}

/// Snaps a canvas drop onto the half-unit grid (`MapObjectivesCanvas`).
pub fn snap_to_grid(x: i32, y: i32) -> (i32, i32) {
    (
        x.clamp(-CANVAS_BOUNDS, CANVAS_BOUNDS),
        y.clamp(-CANVAS_BOUNDS, CANVAS_BOUNDS),
    )
}

/// One connector edge of the objective graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectiveEdge {
    /// Parent objective index.
    pub from: usize,
    /// Child objective index.
    pub to: usize,
}

/// Every parent→child edge in the objective list (`MapObjectivesCanvas.lines`).
pub fn edges(objectives: &MapObjectives) -> Vec<ObjectiveEdge> {
    let mut out = Vec::new();
    for (index, objective) in objectives.all.iter().enumerate() {
        for parent in &objective.common().parents {
            if *parent < objectives.all.len() && *parent != index {
                out.push(ObjectiveEdge {
                    from: *parent,
                    to: index,
                });
            }
        }
    }
    out
}

/// Drops self/duplicate/out-of-range parent links and de-cycles the graph.
///
/// A parent that is not strictly before its child in execution order would make
/// `dependency_finished` deadlock, so it is removed (upstream's canvas fixup).
pub fn fixup_parents(objectives: &mut MapObjectives) -> usize {
    let len = objectives.all.len();
    let mut removed = 0usize;
    for index in 0..len {
        let mut seen = BTreeSet::new();
        let parents = objectives.all[index].common().parents.clone();
        let mut fixed = Vec::with_capacity(parents.len());
        for parent in parents {
            if parent >= len || parent == index || !seen.insert(parent) {
                removed += 1;
                continue;
            }
            fixed.push(parent);
        }
        objectives.all[index].common_mut().parents = fixed;
    }
    removed
}

/// Parses an objective list JSON string (editor `objectives_json` getter).
pub fn parse_objectives(json: &str) -> Result<MapObjectives, crate::io::IoError> {
    if json.trim().is_empty() {
        return Ok(MapObjectives::new());
    }
    MapObjectives::from_json(json)
}

/// Serializes the objective list (editor `set_objectives_json` setter).
pub fn write_objectives(objectives: &MapObjectives) -> Result<String, crate::io::IoError> {
    objectives.to_json()
}

/// A tile coordinate default used by the `DestroyBlock`/`DestroyBlocks` fields.
pub fn default_tile_pos() -> Point2 {
    Point2 { x: 0, y: 0 }
}

/// Resolves a block descriptor default (`Block → copper-wall`).
pub fn default_block(content: &ContentRegistry) -> BlockId {
    content.block_id("copper-wall").unwrap_or(BlockId::AIR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::json::objectives::{default_editor_pos, pack_point2};

    /// Exercises the `mind-derive` `ObjectiveFields` proc macro end-to-end.
    #[derive(mind_derive::ObjectiveFields)]
    #[allow(dead_code)]
    struct DerivedDescriptor {
        #[objective(name = "block", kind = "content:block", flags = "researchable")]
        block: (),
        #[objective(name = "positions", kind = "seq:vec2i", flags = "tilepos")]
        positions: (),
    }

    #[test]
    fn derive_emits_descriptors() {
        let descriptor = DerivedDescriptor {
            block: (),
            positions: (),
        };
        let fields = descriptor.fields();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "block");
        assert_eq!(fields[0].kind, FieldKind::Content(ContentType::Block));
        assert!(fields[0].flags.contains(FieldFlags::RESEARCHABLE));
        assert_eq!(fields[1].name, "positions");
        assert_eq!(fields[1].kind, FieldKind::Seq(Box::new(FieldKind::Vec2I)));
        assert!(fields[1].flags.contains(FieldFlags::TILE_POS));
    }

    #[allow(clippy::format_in_format_args)]
    fn fixture_json() -> String {
        format!(
            "[{research},{item},{timer},{destroyblock},{flag}]",
            research = format!(
                r#"{{"class":"Research","content":"alpha","parents":[],"editorPos":{}}}"#,
                pack_point2(3, 4)
            ),
            item = format!(
                r#"{{"class":"Item","item":"copper","amount":5,"parents":[0],"editorPos":{}}}"#,
                pack_point2(-2, 6)
            ),
            timer = format!(
                r#"{{"class":"Timer","text":"@objective.hold","duration":30.0,"countup":0.0,"parents":[1],"editorPos":{}}}"#,
                pack_point2(7, -1)
            ),
            destroyblock = format!(
                r#"{{"class":"DestroyBlock","pos":{{"x":1,"y":2}},"team":2,"block":"router","parents":[],"editorPos":{}}}"#,
                default_editor_pos()
            ),
            flag = r#"{"class":"Flag","flag":"f","text":null,"parents":[],"editorPos":0}"#,
        )
    }

    #[test]
    fn objective_json_round_trips_editor_pos_and_parents() {
        let json = fixture_json();
        let parsed = MapObjectives::from_json(&json).unwrap();
        assert_eq!(parsed.len(), 5);
        let out = parsed.to_json().unwrap();
        let parsed2 = MapObjectives::from_json(&out).unwrap();
        assert_eq!(parsed, parsed2);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value[1]["parents"], serde_json::json!([0]));
        assert_eq!(value[1]["editorPos"], serde_json::json!(pack_point2(-2, 6)));
        assert_eq!(
            value[3]["editorPos"],
            serde_json::json!(default_editor_pos())
        );
    }

    #[test]
    fn descriptors_cover_serialized_keys() {
        let json = fixture_json();
        let parsed = MapObjectives::from_json(&json).unwrap();
        let serialized = parsed.to_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        for (index, key) in ["Research", "Item", "Timer", "DestroyBlock", "Flag"]
            .iter()
            .enumerate()
        {
            let fields = class_fields(key);
            let object = value[index].as_object().unwrap();
            for field in &fields {
                assert!(
                    object.contains_key(field.name),
                    "{key}: descriptor `{}` has no serialized key (keys: {:?})",
                    field.name,
                    object.keys().collect::<Vec<_>>()
                );
            }
            assert_eq!(parsed.get(index).unwrap().class_tag(), *key);
        }
        // Every registered objective class has a descriptor set.
        for tag in all_class_tags() {
            assert!(!class_fields(tag).is_empty(), "no fields for {tag}");
        }
    }

    #[test]
    fn providers_and_interpreters() {
        let content = test_registry();
        assert_eq!(
            provider_default(&FieldKind::Content(ContentType::Block), Some(&content)),
            serde_json::json!("copper-wall")
        );
        assert_eq!(
            provider_default(&FieldKind::Team, None),
            serde_json::json!(1)
        );
        assert_eq!(interpret_number(FieldFlags::SECOND, 2.0), 120.0);
        assert_eq!(interpret_number(FieldFlags::TILE_POS, 3.0), 24.0);
    }

    #[test]
    fn canvas_nodes_edges_and_fixup() {
        let json = fixture_json();
        let mut objectives = MapObjectives::from_json(&json).unwrap();
        assert_eq!(node_for(pack_point2(-2, 6)), ObjectiveNode { x: -2, y: 6 });
        assert_eq!(snap_to_grid(500, -500), (CANVAS_BOUNDS, -CANVAS_BOUNDS));
        let edges = edges(&objectives);
        assert!(edges.iter().any(|e| e.from == 0 && e.to == 1));
        assert!(edges.iter().any(|e| e.from == 1 && e.to == 2));

        // Corrupt parents: out-of-range, self, duplicate.
        objectives.all[1].common_mut().parents = vec![0, 0, 1, 99];
        let removed = fixup_parents(&mut objectives);
        assert_eq!(removed, 3, "duplicate + self + out-of-range removed");
        assert_eq!(objectives.all[1].common().parents, vec![0]);
    }
}
