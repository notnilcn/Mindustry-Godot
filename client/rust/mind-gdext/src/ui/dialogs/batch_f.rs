// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapProcessorsDialog.java,
//         core/src/mindustry/editor/MapLoadDialog.java,
//         core/src/mindustry/editor/SectorGenerateDialog.java,
//         core/src/mindustry/editor/BannedContentDialog.java,
//         core/src/mindustry/editor/data/MapAssetsDialog.java (+ `MapPatchesView`,
//         `MapContentView`, `MapBundlesView`, `MapImagesView`, `MapAudioView`,
//         `AssetView`), core/src/mindustry/ui/dialogs/SchematicsDialog.java
//         (`SchematicInfoDialog`, :774-834).

//! Editor data dialogs and the schematic info page.
//!
//! Each controller mirrors one `client/scenes/editor/**/*.gd` (or
//! `client/ui/dialogs/schematic_info_dialog.gd`) script: the map-processor list,
//! the map loader, the sector generator, the banned-content picker, the data
//! asset manager and the schematic info page. Behaviour (editor map state, the
//! data-asset owner, the banned-set model) stays in `MindEditor` /
//! `MindCampaign`; the controllers only read their JSON/array projections and
//! call the same dynamic method names the GDScript used.
//!
//! Prompts (`MindUi.show_text_input`/`show_confirm`) and the `icon_select`
//! signal are answered asynchronously; those handlers queue their work through
//! [`defer_action`] because a prompt is emitted from inside a bound `MindUi`
//! method and re-entering a bound GDExtension object panics (`PAR-0066`).

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    AcceptDialog, BoxContainer, Button, ButtonGroup, CenterContainer, ColorRect, Control,
    DisplayServer, FileAccess, FileDialog, GridContainer, HBoxContainer, Json, LineEdit,
    MarginContainer, Os, PanelContainer, RichTextLabel, StyleBoxFlat, Texture2D, TextureRect,
    VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};
use crate::ui::dialogs::editor::util_f::*;

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "schematic_info" => Some(Box::new(SchematicInfoDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// SchematicInfoDialog (`schematic_info`)
// ---------------------------------------------------------------------------

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
        let schematics = dialog.campaign_section("schematics");
        let index = dialog
            .context_value("index")
            .map(|value| variant_i64(&value))
            .unwrap_or(-1);
        let schematic = find_schematic(&schematics, index);
        if schematic.is_empty() {
            return;
        }
        // Java: `"[[" + Core.bundle.get("schematic") + "] " + name`. The title
        // label is a plain `Label`, so the escape renders as the literal
        // `[Schematic] Name` Mindustry shows.
        let title = format!(
            "[{}] {}",
            kit::t("@schematic"),
            dict_string(&schematic, "name")
        );
        dialog.title_text = GString::from(title.as_str());
        dialog.title_color = styles::ACCENT;
        dialog.refresh_title();

        let mut info = factory::table();
        info.bind_mut().grow_horizontal();
        // `Core.bundle.format("schematic.info", w, h, tiles.size)`.
        let tiles = schematic
            .get("tiles")
            .map(|value| {
                value
                    .try_to::<VarArray>()
                    .map(|array| array.len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let text = kit::tm_format(
            "@schematic.info",
            &[
                &dict_i64(&schematic, "width", 0).to_string(),
                &dict_i64(&schematic, "height", 0).to_string(),
                &tiles.to_string(),
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
        image.bind_mut().setup(schematic.clone());
        holder.add_child(&image);
        info.bind_mut().add(&holder).grow_x().pad(6.0);
        info.bind_mut().row();

        self.build_requirements(&mut info, &schematic);

        let production = dict_f64(&schematic, "power_production", 0.0) * 60.0;
        let consumption = dict_f64(&schematic, "power_consumption", 0.0) * 60.0;
        if production != 0.0 || consumption != 0.0 {
            // code-instantiated: the schematic's power summary row.
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 3);
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

        let description = dict_string(&schematic, "description");
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
    fn build_requirements(&self, info: &mut Gd<MindTable>, schematic: &VarDictionary) {
        let requirements = schematic
            .get("requirements")
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        if requirements.is_empty() {
            return;
        }
        let have = core_items();
        // code-instantiated: one requirement cell per schematic stack.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        let mut count = 0usize;
        for entry in requirements.iter_shared() {
            let Ok(entry) = entry.try_to::<VarArray>() else {
                continue;
            };
            let mut values = entry.iter_shared();
            let item = values
                .next()
                .map(|value| variant_string(&value))
                .unwrap_or_default();
            let amount = values.next().map(|value| variant_i64(&value)).unwrap_or(0);
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
                .get(item.as_str())
                .map(|value| variant_i64(&value))
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
                row = HBoxContainer::new_alloc();
                row.add_theme_constant_override("separation", 4);
            }
        }
        if !count.is_multiple_of(4) {
            info.bind_mut().add(&row).pad(2.0);
            info.bind_mut().row();
        }
    }
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
    parse_json_object(&value.to::<GString>().to_string())
}

/// Arc `Strings.autoFixed(value, max)`: truncate to `max` decimals, drop
/// trailing zeros (`Strings.java:1230`).
fn auto_fixed(value: f64, max_decimals: usize) -> String {
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
fn find_schematic(schematics: &VarArray, index: i64) -> VarDictionary {
    for row in schematics.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_i64(&row, "index", -1) == index {
            return row;
        }
    }
    schematics
        .iter_shared()
        .next()
        .and_then(|row| row.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PAR-0586: `find_schematic` walks a Godot `VarArray` of `VarDictionary`
    /// rows, so the fallback rule is exercised only in-engine.
    #[test]
    fn schematics_fall_back_to_the_first_row() {
        if !engine_available() {
            return;
        }
        let mut first = VarDictionary::new();
        first.set(&"index".to_variant(), &7i64.to_variant());
        let mut schematics = VarArray::new();
        schematics.push(&first.to_variant());
        let picked = find_schematic(&schematics, 99);
        assert_eq!(dict_i64(&picked, "index", -1), 7);
    }

}
