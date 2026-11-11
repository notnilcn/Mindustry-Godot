// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The schematic info dialog controller.
//! Upstream: ui/dialogs/SchematicsDialog.java (SchematicInfoDialog, ~:774-834)

use crate::gdutil::variant::{parse_object, to_i64_lenient};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::campaign::util::{schematic_image_row, schematic_library};
use crate::ui::dialogs::editor::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable, styles};
use godot::classes::TextureRect;
use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::prelude::*;
use mind_core::ui::campaign::SchematicView;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "schematic_info" => Some(Box::new(SchematicInfoDialog::default())),
        _ => None,
    }
}

/// `SchematicInfoDialog.show`: the `[[Schematic] name]` title plus the body.
#[derive(Default)]
pub struct SchematicInfoDialog {
    /// The dialog body table (`_body`).
    body: Option<Gd<MindTable>>,
}

impl DialogController for SchematicInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@schematic");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        self.body = Some(dialog.content_table());
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl SchematicInfoDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let Some(mut body) = self.body.clone() else {
            return;
        };
        body.bind_mut().clear();
        let schematics = schematic_library(dialog);
        let index = dialog
            .context_value("index")
            .map(|value| to_i64_lenient(&value))
            .unwrap_or(-1);
        let Some(schematic) = find_schematic(&schematics, index) else {
            return;
        };
        // Java: `"[[" + Core.bundle.get("schematic") + "] " + name`. The title
        // label is a plain `Label`, so the escape renders as the literal
        // `[Schematic] Name` Mindustry shows.
        let title = format!("[{}] {}", kit::t("@schematic"), schematic.name);
        dialog.title_text = GString::from(title.as_str());
        dialog.title_color = styles::ACCENT;
        dialog.refresh_title();

        let mut info = factory::table();
        info.bind_mut().grow_horizontal();
        // `Core.bundle.format("schematic.info", w, h, tiles.size)`.
        let text = kit::tm_format(
            "@schematic.info",
            &[
                &schematic.width.to_string(),
                &schematic.height.to_string(),
                &schematic.tiles.len().to_string(),
            ],
        )
        .to_string();
        let label = factory::label(&text);
        info.bind_mut().add(&label).grow_x();
        info.bind_mut().row();

        // code-instantiated: the preview is bound to the selected schematic row.
        let mut holder = factory::space(0.0, 320.0);
        holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut image = crate::ui::kit::widgets::schematic_image::MindSchematicImage::new_alloc();
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        image.bind_mut().setup(schematic_image_row(&schematic));
        holder.add_child(&image);
        info.bind_mut().add(&holder).grow_x().pad(6.0);
        info.bind_mut().row();

        self.build_requirements(&mut info, &schematic);

        // The old value went through JSON (`serde_json` f32 text -> Godot f64);
        // reproduce that exact widening so the formatted text never shifts.
        let production = json_f32_as_f64(schematic.power_production) * 60.0;
        let consumption = json_f32_as_f64(schematic.power_consumption) * 60.0;
        if production != 0.0 || consumption != 0.0 {
            // code-instantiated: the schematic's power summary row.
            let mut row = factory::hbox(3);
            if production != 0.0 {
                row.add_child(&factory::glyph_label(
                    "power",
                    20,
                    Color::from_rgba(1.0, 0.827, 0.498, 1.0),
                ));
                let mut plus = factory::label(&format!("+{}", auto_fixed(production, 2)));
                plus.add_theme_color_override(
                    "default_color",
                    Color::from_rgba(1.0, 0.827, 0.498, 1.0),
                );
                row.add_child(&plus);
                if consumption != 0.0 {
                    let gap = factory::space(15.0, 0.0);
                    row.add_child(&gap);
                }
            }
            if consumption != 0.0 {
                row.add_child(&factory::glyph_label(
                    "power",
                    20,
                    Color::from_rgba(0.898, 0.329, 0.329, 1.0),
                ));
                let mut minus = factory::label(&format!("-{}", auto_fixed(consumption, 2)));
                minus.add_theme_color_override(
                    "default_color",
                    Color::from_rgba(0.898, 0.329, 0.329, 1.0),
                );
                row.add_child(&minus);
            }
            info.bind_mut().add(&row).pad(4.0);
            info.bind_mut().row();
        }

        let description = schematic.description.clone();
        if !description.is_empty() {
            let mut wrapped = factory::label(&markup(&format!("[lightgray]{description}")));
            wrapped.set_custom_minimum_size(Vector2::new(500.0, 0.0));
            wrapped.set_autowrap_mode(AutowrapMode::WORD_SMART);
            info.bind_mut().add(&wrapped).pad(8.0);
            info.bind_mut().row();
        }
        body.bind_mut().add(&info).grow_x();
        body.bind_mut().sort_now();
    }
}

impl SchematicInfoDialog {
    /// Requirement rows: 32 px icon + amount, four per row; when a player core
    /// exists and cannot afford the stack the label is `have/amount` in scarlet.
    fn build_requirements(&self, info: &mut Gd<MindTable>, schematic: &SchematicView) {
        if schematic.requirements.is_empty() {
            return;
        }
        let have = core_items();
        // code-instantiated: one requirement cell per schematic stack.
        let mut row = factory::hbox(4);
        let mut count = 0usize;
        for (item, amount) in &schematic.requirements {
            let item = item.as_str();
            let amount = i64::from(*amount);
            let mut icon = TextureRect::new_alloc();
            if let Some(texture) = kit::icon_texture(&format!("item-{item}-ui")) {
                icon.set_texture(&texture);
            }
            icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            row.add_child(&icon);
            let mut label = factory::label(&amount.to_string());
            let available = have
                .get(item)
                .map(|value| to_i64_lenient(&value))
                .unwrap_or(0);
            if !have.is_empty() && available < amount {
                label.set_text(&GString::from(
                    format!("[scarlet]{available}[]/{amount}").as_str(),
                ));
            }
            row.add_child(&label);
            count += 1;
            if count.is_multiple_of(4) {
                info.bind_mut().add(&row).pad(2.0);
                info.bind_mut().row();
                row = factory::hbox(4);
            }
        }
        if !count.is_multiple_of(4) {
            info.bind_mut().add(&row).pad(2.0);
            info.bind_mut().row();
        }
    }
}

/// `serde_json`'s f32 text parsed as f64: the exact widening the old JSON read
/// model produced (`to_gstring` -> Godot JSON -> `dict_f64_or`).
fn json_f32_as_f64(value: f32) -> f64 {
    serde_json::to_string(&value)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(f64::from(value))
}

/// The player core's item counts (`player.core().items`); empty in the menu.
fn core_items() -> VarDictionary {
    let Some(mut host) = kit::sim_host() else {
        return VarDictionary::new();
    };
    if !host.has_method("core_items_json") {
        return VarDictionary::new();
    }
    let value = host.call("core_items_json", &[]);
    parse_object(&value.to::<GString>().to_string())
}

/// Arc `Strings.autoFixed(value, max)`: truncate to `max` decimals, drop
/// trailing zeros (`Strings.java:1230`).
pub(in crate::ui::dialogs) fn auto_fixed(value: f64, max_decimals: usize) -> String {
    let mut text = format!("{value:.max_decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text.is_empty() {
        String::from("0")
    } else {
        text
    }
}

/// The view row for `index` (falls back to the first row).
fn find_schematic(schematics: &[SchematicView], index: i64) -> Option<SchematicView> {
    schematics
        .iter()
        .find(|row| i64::try_from(row.index).unwrap_or(-1) == index)
        .or_else(|| schematics.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `@schematic.info` picker falls back to the library's first row when
    /// the context index no longer exists.
    #[test]
    fn schematics_fall_back_to_the_first_row() {
        let first = SchematicView {
            index: 7,
            name: String::from("first"),
            description: String::new(),
            width: 3,
            height: 3,
            tiles: Vec::new(),
            has_core: false,
            labels: Vec::new(),
            file: None,
            mod_name: None,
            requirements: Vec::new(),
            power_consumption: 0.0,
            power_production: 0.0,
        };
        let schematics = vec![first];
        let picked = find_schematic(&schematics, 99);
        assert_eq!(picked.map(|row| row.index), Some(7));
        assert!(find_schematic(&[], 99).is_none());
    }
}
