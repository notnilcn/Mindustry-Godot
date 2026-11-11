// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The icon (and sector marker) select dialog controller.
//! Upstream: ui/dialogs/IconSelectDialog.java (with PlanetDialog.java:1223-1283
//! for the sector marker mode)

use godot::classes::Button;
use godot::classes::ColorRect;
use godot::classes::Control;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use crate::ui::dialogs::editor::util::*;

use super::file_chooser::{emit_dialog_signal, hide_node};

// ---------------------------------------------------------------------------
// IconSelectDialog
// ---------------------------------------------------------------------------

/// `Vars.accessibleIcons` — order and Java field names are the stored ABI
/// (`SectorInfo.icon` keeps the field name; the display maps through
/// `MindIcons`).
const ACCESSIBLE_ICONS: [&str; 31] = [
    "effect",
    "power",
    "logic",
    "units",
    "liquid",
    "production",
    "defense",
    "turret",
    "distribution",
    "crafting",
    "settings",
    "cancel",
    "zoom",
    "ok",
    "star",
    "home",
    "pencil",
    "up",
    "down",
    "left",
    "right",
    "hammer",
    "warning",
    "tree",
    "admin",
    "map",
    "modePvp",
    "terrain",
    "modeSurvival",
    "commandRally",
    "commandAttack",
];

/// `Vars.defaultContentIcons`.
const CONTENT_TYPES: [&str; 5] = ["item", "liquid", "block", "unit", "status"];

/// `min(20, Core.graphics.getWidth() / Scl.scl(52))` (1152 px viewport -> 20).
const ICON_COLUMNS: usize = 20;

/// `@icon_select` (`client/ui/dialogs/icon_select_dialog.gd`,
/// `IconSelectDialog.java`): the vanilla `accessibleIcons` grid plus content
/// icons, and the sector marker mode (`PlanetDialog.java:1223-1283`).
pub struct IconSelectDialog {
    mode: String,
    checked_icon: String,
    checked_content: String,
    grid: Option<Gd<MindTable>>,
    /// `database_json` entries grouped by content type (content order per type).
    rows: [Vec<VarDictionary>; 5],
}

impl Default for IconSelectDialog {
    fn default() -> Self {
        Self {
            mode: "icon".to_string(),
            checked_icon: String::new(),
            checked_content: String::new(),
            grid: None,
            rows: Default::default(),
        }
    }
}

impl DialogController for IconSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.add_default_close_button();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.mode = {
            let mode = dialog.context_string("mode");
            if mode.is_empty() {
                "icon".to_string()
            } else {
                mode.to_string()
            }
        };
        self.checked_icon = dialog.context_string("icon").to_string();
        self.checked_content = dialog.context_string("content_icon").to_string();
        self.load_rows(dialog);
        self.rebuild(dialog);
    }
}

impl IconSelectDialog {
    /// `MindCampaign.database_json` entries grouped by type: hidden content is
    /// already dropped by the model; locked entries are filtered at build time
    /// like `u.unlocked()` (`PlanetDialog.java:1263`).
    fn load_rows(&mut self, dialog: &mut MindDialog) {
        self.rows = Default::default();
        let view = dialog.campaign_json("database_json", &[]);
        for category in array_of(&view, "categories").iter_shared() {
            let Ok(category) = category.try_to::<VarDictionary>() else {
                continue;
            };
            for tag in array_of(&category, "tags").iter_shared() {
                let Ok(tag) = tag.try_to::<VarDictionary>() else {
                    continue;
                };
                for entry in array_of(&tag, "entries").iter_shared() {
                    let Ok(entry) = entry.try_to::<VarDictionary>() else {
                        continue;
                    };
                    let type_name = dict_string(&entry, "content_type");
                    let Some(index) = CONTENT_TYPES.iter().position(|kind| *kind == type_name)
                    else {
                        continue;
                    };
                    self.rows[index].push(entry);
                }
            }
        }
    }

    /// `IconSelectDialog` grid: 48 px buttons, `cols = min(20, width/52)`, the
    /// none button, the accessible icons, then each `defaultContentIcons` type
    /// behind a 3 px accent rule. Java renders it in a `cont.pane`, so it
    /// scrolls.
    fn rebuild(&mut self, dialog: &mut MindDialog) {
        if self.grid.is_none() {
            let mut table = dialog.content_table();
            // code-instantiated: the icon grid is a scrolling pane sized by
            // content rows (`IconSelectDialog.java:26-31` `cont.pane`).
            let mut scroll = MindScroll::new_alloc();
            scroll.set_custom_minimum_size(Vector2::new(1000.0, 420.0));
            scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
            let mut grid = factory::table();
            grid.bind_mut().grow_horizontal();
            let grid_control: Gd<Control> = grid.clone().upcast();
            scroll.add_child(&grid_control);
            table.bind_mut().add(&scroll).grow_x().grow_y().pad(4.0);
            table.bind_mut().row();
            self.grid = Some(grid);
        }
        let Some(mut grid) = self.grid.clone() else {
            return;
        };
        grid.bind_mut().clear();
        let sector_mode = self.mode == "sector";
        let none_checked = self.checked_icon.is_empty() && self.checked_content.is_empty();
        let target = dialog.to_gd();

        let none_control: Gd<Control> = factory::glyph_label("none", 28, Color::WHITE).upcast();
        let none_button = icon_toggle(&none_control, none_checked, {
            let target = target.clone();
            Callable::from_fn("icon_select_none", move |_args: &[&Variant]| {
                if sector_mode {
                    icon_select_choose_sector(&target, true, "", "");
                } else {
                    icon_select_choose_icon(&target, false, "");
                }
            })
        });
        grid.bind_mut().add(&none_button).size(48.0);

        let mut index = 1usize;
        for icon_name in ACCESSIBLE_ICONS {
            let control: Gd<Control> = factory::glyph_label(icon_name, 28, Color::WHITE).upcast();
            let checked = icon_name == self.checked_icon;
            let button = icon_toggle(&control, checked, {
                let target = target.clone();
                Callable::from_fn("icon_select_icon", move |_args: &[&Variant]| {
                    icon_select_choose_icon(&target, sector_mode, icon_name);
                })
            });
            grid.bind_mut().add(&button).size(48.0);
            index += 1;
            if index.is_multiple_of(ICON_COLUMNS) {
                grid.bind_mut().row();
            }
        }

        for (type_index, type_name) in CONTENT_TYPES.iter().enumerate() {
            grid.bind_mut().row();
            let mut rule = ColorRect::new_alloc();
            rule.set_color(styles::ACCENT);
            rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
            rule.set_mouse_filter(MouseFilter::IGNORE);
            grid.bind_mut()
                .add(&rule)
                .grow_x()
                .colspan(ICON_COLUMNS as i32);
            grid.bind_mut().row();
            index = 0;
            for entry in self.rows[type_index].clone() {
                if !dict_bool(&entry, "unlocked", false) {
                    continue;
                }
                let entry_name = dict_string(&entry, "name");
                let checked = entry_name == self.checked_content;
                let button = icon_content_button(&target, &entry, type_name, &entry_name, checked);
                grid.bind_mut().add(&button).size(48.0);
                index += 1;
                if index.is_multiple_of(ICON_COLUMNS) {
                    grid.bind_mut().row();
                }
            }
        }
    }
}

/// One 48 px `squareTogglei` cell around the given content control
/// (`PlanetDialog.java:1236-1256`).
fn icon_toggle(content: &Gd<Control>, checked: bool, on_pressed: Callable) -> Gd<Button> {
    // code-instantiated: one data-driven toggle per accessible icon.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("squareTogglei");
    button.set_toggle_mode(true);
    button.set_pressed(checked);
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    let mut content = content.clone();
    content.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    content.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&content);
    button.connect("pressed", &on_pressed);
    button
}

/// One content entry: `uiIcon` via the `<type>-<name>-ui` / `-full` region
/// chain (`UnlockableContent.uiIcon`; `PlanetDialog.java:1265-1275`).
fn icon_content_button(
    target: &Gd<MindDialog>,
    entry: &VarDictionary,
    type_name: &str,
    entry_name: &str,
    checked: bool,
) -> Gd<Button> {
    let ui_region = format!("{type_name}-{entry_name}-ui");
    let full_region = format!("{type_name}-{entry_name}-full");
    // code-instantiated: one data-driven toggle per unlocked content entry.
    let mut button = factory::image_button_first(
        &[ui_region.as_str(), full_region.as_str(), entry_name],
        "squareTogglei",
    );
    button.set_toggle_mode(true);
    button.set_pressed(checked);
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    button.set_expand_icon(true);
    button.add_theme_constant_override("icon_max_width", 32);
    button.set_tooltip_text(&kit::t(&format!("@{type_name}.{entry_name}.name")).to_string());
    let _ = entry;
    let target = target.clone();
    let name = entry_name.to_string();
    let callable = Callable::from_fn("icon_select_content", move |_args: &[&Variant]| {
        // `_content_button` always routes through `_choose_sector`; outside
        // sector mode that falls back to `_choose_icon("")`.
        icon_select_choose_sector(&target, false, "", &name);
    });
    button.connect("pressed", &callable);
    button
}

/// Vanilla mode (schematics tags): the picked accessible icon name.
fn icon_select_choose_icon(target: &Gd<MindDialog>, sector_mode: bool, icon_name: &str) {
    if sector_mode {
        icon_select_choose_sector(target, true, icon_name, "");
        return;
    }
    if !target.is_instance_valid() {
        return;
    }
    emit_dialog_signal(
        target,
        "icon_selected",
        &[GString::from(icon_name).to_variant()],
    );
    hide_node(target);
}

/// Sector mode: the picked `Iconc` field name or content name; the unused half
/// is empty (`PlanetDialog.java:1238-1281` nulls the other).
fn icon_select_choose_sector(
    target: &Gd<MindDialog>,
    sector_mode: bool,
    icon_name: &str,
    content_name: &str,
) {
    if !sector_mode {
        if icon_name.is_empty() {
            icon_select_choose_icon(target, false, icon_name);
        }
        return;
    }
    if !target.is_instance_valid() {
        return;
    }
    emit_dialog_signal(
        target,
        "sector_icon_selected",
        &[
            GString::from(icon_name).to_variant(),
            GString::from(content_name).to_variant(),
        ],
    );
}

/// A read-model array field (`[]` when absent or another shape).
fn array_of(dict: &VarDictionary, key: &str) -> VarArray {
    dict.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "icon_select" => Some(Box::new(IconSelectDialog::default())),
        _ => None,
    }
}
