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
        log::info!(
            "MindLogic ready ({} registered statements)",
            all_statements().len()
        );
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

fn field_dict(name: &str, kind: FieldKind) -> Dictionary<GString, Variant> {
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
    ])
}

#[godot_api]
impl MindLogic {
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
            out.push(&field_dict(field.name, field.kind).to_variant());
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
            fields.push(&field_dict(field.name, field.kind).to_variant());
        }
        dict(vec![
            (
                "registered_name",
                GString::from(meta.registered_name).to_variant(),
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
