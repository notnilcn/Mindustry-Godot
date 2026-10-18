// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindLogic` — the plan-13 mlog host surface (plan 13 §3.12/§7c, M8).
//!
//! This is a thin Godot shell over the Godot-free `mind_core::logic` module: it
//! exposes the statement metadata that the plan-14 logic editor consumes, plus
//! text parse/compile/standalone-run probes for the MCP oracle. It contains no
//! VM logic of its own.
//!
//! The simulator-integrated `logic_place`/`logic_set_code`/`logic_get_state`
//! probes live on `MindSimHost`; they await the plan-05/07 live logic-building
//! integration (the P0 `Sim` uses `BuildingComp`, not plan-07's logic
//! behaviors). This node is declared in `res://scenes/game.tscn` by the
//! orchestrator (HLP §6.6); it is not a project autoload.

use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Node};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::logic::assembler::Assembler;
use mind_core::logic::enums::FieldKind;
use mind_core::logic::script::run_standalone;
use mind_core::logic::statement::{all_statements, default_statement, statement_fields};
use mind_core::logic::textio;

/// `MindLogic` — statement metadata + assemble/parse/run facade.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindLogic {
    base: Base<Node>,
}

#[godot_api]
impl INode for MindLogic {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

/// Builds a `{GString: Variant}` dictionary.
fn dict(pairs: Vec<(&str, Variant)>) -> Dictionary<GString, Variant> {
    let mut out = Dictionary::<GString, Variant>::new();
    for (key, value) in pairs {
        out.set(&GString::from(key), &value);
    }
    out
}

fn field_dict(
    name: &str,
    kind: FieldKind,
    options: &[&str],
    enum_class: &str,
) -> Dictionary<GString, Variant> {
    let mut list = PackedStringArray::new();
    for option in options {
        list.push(&GString::from(*option));
    }
    dict(vec![
        ("name", GString::from(name).to_variant()),
        (
            "kind",
            GString::from(match kind {
                FieldKind::Str => "str",
                FieldKind::Int => "int",
                FieldKind::Bool => "bool",
                FieldKind::Enum => "enum",
            })
            .to_variant(),
        ),
        ("options", list.to_variant()),
        ("enum_class", GString::from(enum_class).to_variant()),
    ])
}

#[godot_api]
impl MindLogic {
    /// Rebuilds the node's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        log::info!(
            "MindLogic ready ({} registered statements)",
            all_statements().len()
        );
    }

    /// Number of registered statements (53 in vanilla).
    #[func]
    pub fn statement_count(&self) -> i64 {
        all_statements().len() as i64
    }

    /// Registered statement names in registry order (plan-14 add dialog).
    #[func]
    pub fn statement_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for meta in all_statements() {
            out.push(&GString::from(meta.registered_name));
        }
        out
    }

    /// Field descriptors `[{name, kind}]` for `name`, in serialization order.
    #[func]
    pub fn statement_fields(&self, name: GString) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        let Some(fields) = statement_fields(&name.to_string()) else {
            return out;
        };
        for field in fields {
            out.push(
                &field_dict(field.name, field.kind, field.options, field.enum_class).to_variant(),
            );
        }
        out
    }

    /// Full metadata for `name` (`{}` when unknown).
    #[func]
    pub fn statement_meta(&self, name: GString) -> Dictionary<GString, Variant> {
        let requested = name.to_string();
        let Some(meta) = all_statements()
            .into_iter()
            .find(|meta| meta.registered_name == requested)
        else {
            return Dictionary::<GString, Variant>::new();
        };
        let mut fields = Array::<Variant>::new();
        for field in &meta.fields {
            fields.push(
                &field_dict(field.name, field.kind, field.options, field.enum_class).to_variant(),
            );
        }
        dict(vec![
            (
                "registered_name",
                GString::from(meta.registered_name).to_variant(),
            ),
            (
                "bundle",
                GString::from(
                    mind_core::logic::statement::bundle_key(meta.registered_name).as_str(),
                )
                .to_variant(),
            ),
            (
                "rust_variant",
                GString::from(meta.rust_variant).to_variant(),
            ),
            ("category", GString::from(meta.category.name()).to_variant()),
            ("privileged", meta.privileged.to_variant()),
            ("use_wrapping", meta.use_wrapping.to_variant()),
            ("hidden", meta.hidden.to_variant()),
            ("fields", fields.to_variant()),
        ])
    }

    /// Concise hand-off summary for plan 14 (counts + category names).
    #[func]
    pub fn metadata_handoff(&self) -> Dictionary<GString, Variant> {
        let metas = all_statements();
        let mut categories: Vec<&'static str> = Vec::new();
        for meta in &metas {
            if !categories.contains(&meta.category.name()) {
                categories.push(meta.category.name());
            }
        }
        let mut category_array = PackedStringArray::new();
        for category in categories {
            category_array.push(&GString::from(category));
        }
        dict(vec![
            ("statement_count", (metas.len() as i64).to_variant()),
            (
                "privileged_count",
                (metas.iter().filter(|m| m.privileged).count() as i64).to_variant(),
            ),
            (
                "hidden_count",
                (metas.iter().filter(|m| m.hidden).count() as i64).to_variant(),
            ),
            (
                "max_instructions",
                (textio::MAX_INSTRUCTIONS as i64).to_variant(),
            ),
            ("categories", category_array.to_variant()),
        ])
    }

    /// Normalized program text for `source` (`{ok, normalized, statements}` or
    /// `{ok:false, error}`).
    #[func]
    pub fn parse(&self, source: GString, privileged: bool) -> Dictionary<GString, Variant> {
        match Assembler::read(&source.to_string(), privileged) {
            Ok(statements) => dict(vec![
                ("ok", true.to_variant()),
                ("statements", (statements.len() as i64).to_variant()),
                (
                    "normalized",
                    GString::from(Assembler::write(&statements).as_str()).to_variant(),
                ),
            ]),
            Err(error) => dict(vec![
                ("ok", false.to_variant()),
                (
                    "error",
                    GString::from(error.to_string().as_str()).to_variant(),
                ),
            ]),
        }
    }

    /// Assembles `source` (`{ok, instructions, vars}` or an error).
    #[func]
    pub fn compile(&self, source: GString, privileged: bool) -> Dictionary<GString, Variant> {
        match Assembler::assemble(&source.to_string(), privileged) {
            Ok(asm) => {
                let mut vars = PackedStringArray::new();
                for cell in &asm.arena.cells {
                    if !cell.constant && !cell.name.starts_with('@') {
                        vars.push(&GString::from(cell.name.as_str()));
                    }
                }
                dict(vec![
                    ("ok", true.to_variant()),
                    ("instructions", (asm.instructions.len() as i64).to_variant()),
                    ("vars", vars.to_variant()),
                ])
            }
            Err(error) => dict(vec![
                ("ok", false.to_variant()),
                (
                    "error",
                    GString::from(error.to_string().as_str()).to_variant(),
                ),
            ]),
        }
    }

    /// Runs `source` on a temporary executor for `ticks` at `ipt` and dumps the
    /// non-constant variables (`{ok, vars, text}` or an error).
    #[func]
    pub fn run(
        &self,
        source: GString,
        ticks: i64,
        ipt: i64,
        privileged: bool,
    ) -> Dictionary<GString, Variant> {
        let ticks = ticks.clamp(0, 1_000_000) as u64;
        let ipt = ipt.clamp(1, 1000) as i32;
        match run_standalone(&source.to_string(), privileged, ticks, ipt) {
            Some(exec) => {
                let mut vars = Dictionary::<GString, Variant>::new();
                for cell in &exec.arena.cells {
                    if cell.constant || cell.name.starts_with('@') {
                        continue;
                    }
                    let value: Variant = if cell.is_obj {
                        match &cell.obj {
                            None => Variant::nil(),
                            Some(value) => GString::from(value.display().as_str()).to_variant(),
                        }
                    } else {
                        cell.num.to_variant()
                    };
                    vars.set(&GString::from(cell.name.as_str()), &value);
                }
                dict(vec![
                    ("ok", true.to_variant()),
                    (
                        "text",
                        GString::from(exec.text_buffer.as_str()).to_variant(),
                    ),
                    ("vars", vars.to_variant()),
                ])
            }
            None => dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("assemble failed").to_variant()),
            ]),
        }
    }

    /// Parses `source` into editor rows
    /// (`[{name, category, fields: [String]}]`; `[]` on a parse error).
    ///
    /// The inverse is [`Self::write_statements`] (`LogicDialog.show` loads the
    /// canvas from the statement list; `LCanvas.save` writes it back).
    #[func]
    pub fn parse_statements(&self, source: GString, privileged: bool) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        let Ok(statements) = Assembler::read(&source.to_string(), privileged) else {
            return out;
        };
        for statement in statements {
            out.push(&statement_row(&statement).to_variant());
        }
        out
    }

    /// Serializes editor rows back to program text (one statement per line).
    ///
    /// Rows come from [`Self::parse_statements`] or [`Self::default_statement`];
    /// a row whose name is unknown is skipped, mirroring `LAssembler`'s
    /// malformed-input handling.
    #[func]
    pub fn write_statements(&self, statements: Array<Variant>) -> GString {
        let mut out = String::new();
        for value in statements.iter_shared() {
            let Some(dict) = value.try_to::<Dictionary<GString, Variant>>().ok() else {
                continue;
            };
            if let Some(line) = row_to_line(&dict) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        GString::from(out.as_str())
    }

    /// A default instance of `name` as an editor row (`{}` when unknown).
    #[func]
    pub fn default_statement_row(&self, name: GString) -> Dictionary<GString, Variant> {
        match default_statement(&name.to_string()) {
            Some(statement) => statement_row(&statement),
            None => Dictionary::<GString, Variant>::new(),
        }
    }

    /// `LStatement.sanitize`: makes user text-field input safe for the parser
    /// (`LStatement.java:94-150`).
    #[func]
    pub fn sanitize(&self, value: GString) -> GString {
        GString::from(mind_core::logic::statement::Statement::sanitize(&value.to_string()).as_str())
    }

    /// The editor row items for a statement's current values
    /// (`LStatement.build(Table)`): `[{kind, index, label, label_before, wide}]`.
    #[func]
    pub fn editor_layout(&self, name: GString, values: PackedStringArray) -> Array<Variant> {
        let values: Vec<String> = values
            .as_slice()
            .iter()
            .map(|value| value.to_string())
            .collect();
        let mut out = Array::<Variant>::new();
        for item in mind_core::logic::statement::editor_layout(&name.to_string(), &values) {
            out.push(
                &dict(vec![
                    (
                        "kind",
                        GString::from(match item.kind {
                            mind_core::logic::statement::EditorKind::Text => "text",
                            mind_core::logic::statement::EditorKind::Field => "field",
                            mind_core::logic::statement::EditorKind::Select => "select",
                            mind_core::logic::statement::EditorKind::Color => "color",
                            mind_core::logic::statement::EditorKind::Align => "align",
                            mind_core::logic::statement::EditorKind::Char => "char",
                            mind_core::logic::statement::EditorKind::Jump => "jump",
                        })
                        .to_variant(),
                    ),
                    ("index", (item.index as i64).to_variant()),
                    ("label", GString::from(item.label).to_variant()),
                    ("label_before", item.label_before.to_variant()),
                    ("wide", item.wide.to_variant()),
                ])
                .to_variant(),
            );
        }
        out
    }

    /// `showSelect` options for a select bound to field `index`
    /// (`[{name, label, key}]`; `key` is the bundle key or empty).
    #[func]
    pub fn editor_options(&self, name: GString, index: i64) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        if index < 0 {
            return out;
        }
        for option in mind_core::logic::statement::editor_options(&name.to_string(), index as usize)
        {
            out.push(
                &dict(vec![
                    ("name", GString::from(option.name.as_str()).to_variant()),
                    ("label", GString::from(option.label.as_str()).to_variant()),
                    ("key", GString::from(option.key.as_str()).to_variant()),
                ])
                .to_variant(),
            );
        }
        out
    }

    /// The value fixups a discriminator change applies (`DrawStatement`'s
    /// `showSelect` callback), returned as the new field list.
    #[func]
    pub fn apply_type_change(&self, name: GString, values: PackedStringArray) -> PackedStringArray {
        let mut values: Vec<String> = values
            .as_slice()
            .iter()
            .map(|value| value.to_string())
            .collect();
        mind_core::logic::statement::editor_type_change(&name.to_string(), &mut values);
        let mut out = PackedStringArray::new();
        for value in &values {
            out.push(&GString::from(value.as_str()));
        }
        out
    }

    /// Build-time field fixups (`SetRuleStatement.rebuild`'s `p1` default),
    /// returned as the (possibly changed) field list.
    #[func]
    pub fn editor_normalize(&self, name: GString, values: PackedStringArray) -> PackedStringArray {
        let mut values: Vec<String> = values
            .as_slice()
            .iter()
            .map(|value| value.to_string())
            .collect();
        mind_core::logic::statement::editor_normalize(&name.to_string(), &mut values);
        let mut out = PackedStringArray::new();
        for value in &values {
            out.push(&GString::from(value.as_str()));
        }
        out
    }

    /// `LogicFx` entries for `EffectsDialog` (`EffectsDialog.java`): the effect
    /// name plus the parameter flags the statement's field row consumes.
    #[func]
    pub fn effect_entries(&self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for entry in mind_core::logic::fx::all() {
            out.push(
                &dict(vec![
                    ("name", GString::from(entry.name).to_variant()),
                    ("bounds", (entry.bounds as f64).to_variant()),
                    ("size", entry.size.to_variant()),
                    ("rotate", entry.rotate.to_variant()),
                    ("color", entry.color.to_variant()),
                    ("data", GString::from(entry.data.unwrap_or("")).to_variant()),
                ])
                .to_variant(),
            );
        }
        out
    }

    /// `Layer` fields for `SetMarkerStatement`'s draw-layer list
    /// (`LStatements.java:2415-2428`): `[{name, value}]` in reflection order.
    #[func]
    pub fn marker_layers(&self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for (name, value) in mind_core::logic::statement::MARKER_LAYERS {
            out.push(
                &dict(vec![
                    ("name", GString::from(*name).to_variant()),
                    ("value", (*value as f64).to_variant()),
                ])
                .to_variant(),
            );
        }
        out
    }

    /// `LAccess.settable` names (`SetPropStatement`'s sensors tab).
    #[func]
    pub fn settable_accesses(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for access in mind_core::logic::access::LAccess::SETTABLE {
            out.push(&GString::from(access.name()));
        }
        out
    }

    /// `LAccess.senseable`/`senseablePrivileged` names (`SensorStatement`'s
    /// sensors tab; the world-processor flag picks the privileged table).
    #[func]
    pub fn senseable_accesses(&self, privileged: bool) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        let entries = if privileged {
            mind_core::logic::access::LAccess::senseable_privileged()
        } else {
            mind_core::logic::access::LAccess::senseable()
        };
        for access in entries {
            out.push(&GString::from(access.name()));
        }
        out
    }

    /// The built-in global entries (`GlobalVarsDialog`: `@logic.globals`).
    ///
    /// Rows are `{name, description}`; the description falls back to the
    /// `lglobal.<name>` bundle key (`GlobalVarsDialog.java:33-36`).
    #[func]
    pub fn globals(&self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        let Some(globals) = mind_core::logic::globals::GlobalVars::global_entries() else {
            return out;
        };
        for entry in globals {
            out.push(
                &dict(vec![
                    ("name", GString::from(entry.as_str()).to_variant()),
                    (
                        "description",
                        GString::from(format!("lglobal.{}", entry).as_str()).to_variant(),
                    ),
                ])
                .to_variant(),
            );
        }
        out
    }

    /// A minimal valid starter program (`set`/`op`/`jump`), for the editor.
    #[func]
    pub fn default_program(&self) -> GString {
        let statements = [
            default_statement("set"),
            default_statement("op"),
            default_statement("jump"),
        ];
        let mut out = String::new();
        for statement in statements.into_iter().flatten() {
            statement.write(&mut out);
            out.push('\n');
        }
        GString::from(out.as_str())
    }
}

/// One editor row: `{name, category, fields: [String]}`.
fn statement_row(
    statement: &mind_core::logic::statement::Statement,
) -> Dictionary<GString, Variant> {
    let mut fields = Array::<Variant>::new();
    for value in statement.field_values() {
        fields.push(&GString::from(value.as_str()).to_variant());
    }
    dict(vec![
        (
            "name",
            GString::from(statement.registered_name()).to_variant(),
        ),
        (
            "bundle",
            GString::from(
                mind_core::logic::statement::bundle_key(statement.registered_name()).as_str(),
            )
            .to_variant(),
        ),
        (
            "category",
            GString::from(statement.category().name()).to_variant(),
        ),
        ("fields", fields.to_variant()),
    ])
}

/// Rebuilds a statement line from an editor row (`None` when unknown).
fn row_to_line(row: &Dictionary<GString, Variant>) -> Option<String> {
    let name = row
        .get(&GString::from("name"))
        .and_then(|value| value.try_to::<GString>().ok())?
        .to_string();
    let mut tokens: Vec<String> = vec![name];
    if let Some(fields) = row
        .get(&GString::from("fields"))
        .and_then(|value| value.try_to::<Array<Variant>>().ok())
    {
        for value in fields.iter_shared() {
            if let Ok(text) = value.try_to::<GString>() {
                tokens.push(text.to_string());
            }
        }
    }
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let statement = mind_core::logic::statement::read_statement(&refs)?;
    let mut out = String::new();
    statement.write(&mut out);
    Some(out)
}
