// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Free helpers shared by the palette and objective APIs.
//! Upstream: none (port-only; tag/row helpers)

use super::*;

/// GDScript tag for one objective field kind (`FieldKind`).
pub(super) fn objective_kind_tag(kind: &FieldKind) -> String {
    match kind {
        FieldKind::String => "string".to_owned(),
        FieldKind::Bool => "bool".to_owned(),
        FieldKind::Byte => "byte".to_owned(),
        FieldKind::Int => "int".to_owned(),
        FieldKind::Float => "float".to_owned(),
        FieldKind::Content(content_type) => format!("content:{}", content_type.name()),
        FieldKind::Team => "team".to_owned(),
        FieldKind::Color => "color".to_owned(),
        FieldKind::Vec2F => "vec2f".to_owned(),
        FieldKind::Vec2I => "vec2i".to_owned(),
        FieldKind::Objective(_) => "objective".to_owned(),
        FieldKind::Seq(inner) => format!("seq:{}", objective_kind_tag(inner)),
        FieldKind::Map(inner) => format!("map:{}", objective_kind_tag(inner)),
        FieldKind::IndexBool => "indexbool".to_owned(),
        FieldKind::LabelFlag => "labelflag".to_owned(),
        FieldKind::Alignment => "alignment".to_owned(),
        FieldKind::TextureHolder => "texture".to_owned(),
        FieldKind::Vertices => "verts".to_owned(),
        FieldKind::Marker => "marker".to_owned(),
    }
}

/// GDScript tags for one objective field's annotation flags (`FieldFlags`).
pub(super) fn objective_flag_tags(flags: FieldFlags) -> Vec<&'static str> {
    let mut tags = Vec::new();
    for (flag, tag) in [
        (FieldFlags::SECOND, "second"),
        (FieldFlags::TILE_POS, "tilepos"),
        (FieldFlags::MULTILINE, "multiline"),
        (FieldFlags::LOGIC_CODE, "logiccode"),
        (FieldFlags::RESEARCHABLE, "researchable"),
        (FieldFlags::SYNTHETIC, "synthetic"),
        (FieldFlags::HIDDEN, "hidden"),
        (FieldFlags::UNORDERED, "unordered"),
        (FieldFlags::ALIGN_NO_VER, "align-nover"),
        (FieldFlags::ALIGN_NO_HOR, "align-nohor"),
    ] {
        if flags.contains(flag) {
            tags.push(tag);
        }
    }
    tags
}

/// Builds one palette/`block_info` row.
pub(super) fn palette_row(def: &BlockDef) -> VarDictionary {
    let mut row = VarDictionary::new();
    let key = |name: &str| GString::from(name);
    row.set(&key("name"), &GString::from(def.name.as_str()).to_variant());
    row.set(&key("id"), &(def.id.raw() as i64).to_variant());
    row.set(&key("size"), &(def.size as i64).to_variant());
    row.set(&key("is_floor"), &block_info::is_floor(def).to_variant());
    row.set(
        &key("is_overlay"),
        &mind_core::editor::palette::is_overlay_floor(def).to_variant(),
    );
    row.set(&key("is_multiblock"), &(def.size > 1).to_variant());
    row.set(
        &key("is_core"),
        &(def.kind == BlockKind::CoreBlock).to_variant(),
    );
    row.set(&key("synthetic"), &block_info::synthetic(def).to_variant());
    row.set(&key("configurable"), &def.configurable.to_variant());
    row.set(&key("save_config"), &def.save_config.to_variant());
    row.set(
        &key("build_visibility"),
        &GString::from(def.build_visibility.name()).to_variant(),
    );
    row.set(
        &key("ui_icon"),
        &GString::from(format!("ui/block-{}-ui", def.name).as_str()).to_variant(),
    );
    row
}
