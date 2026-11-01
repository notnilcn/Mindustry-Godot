// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/world/blocks/units/UnitFactory.java:292-340
//         (`buildConfiguration` command row) and
//         core/src/mindustry/world/blocks/units/Reconstructor.java:215-240
//         (J05 T2).
//
// The unit-command icon row Java builds under a factory's plan grid (40 px
// `clearNoneTogglei` cells, `columns = clamp(units.size, 2, selectionColumns)`
// and a 4 px `Pal.gray` separator) or as the reconstructor's whole panel
// (5 columns of 44 px cells over `Styles.black6`).
//
// A click configures the command (`configure(item)`); the checked cell follows
// `command == item || (command == null && unit.defaultCommand == item)`
// (`UnitFactory.java:314`). Java's click listener never clears the config (the
// group's min-check-0 uncheck is re-checked by the next `update`); a
// reconstructor click additionally closes the panel (`deselect()`, `:230`),
// which the fragment applies from `close_select`.

//! `MindItemCommandRow` — the unit-command toggle row.
//!
//! Options come from the `config_spec` `commands` object; every cell is a
//! glyph toggle with the localized command name as its tooltip, and only the
//! checked transition reports through [`MindItemCommandRow::picked`].

use godot::builtin::VarDictionary;
use godot::classes::control::{CursorShape, MouseFilter};
use godot::classes::{
    Button, ButtonGroup, ColorRect, Control, IPanelContainer, Label, PanelContainer, StyleBox,
    StyleBoxEmpty, StyleBoxFlat,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::item_selection::{
    clear_none_toggle_style, dict_array, dict_bool, dict_f32, dict_i32, dict_string, localized,
    option_dict,
};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{MindTable, styles};

/// `Pal.gray` (`Pal.java:82`) — the factory's separator strip (`Color8(0x45,
/// 0x45, 0x45)`).
const SEPARATOR_COLOR: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);

/// `commands.image(Tex.whiteui, Pal.gray).height(4f)` (`UnitFactory.java:307`).
const SEPARATOR_HEIGHT: f32 = 4.0;

/// Java `Icon` field names -> `MindIcons` keys where the port's alias table has
/// no entry (`UnitCommand.getIcon()` returns the `Icon` drawable).
const ICON_ALIASES: &[(&str, &str)] = &[
    ("downOpen", "down-open"),
    ("upOpen", "up-open"),
    ("rightOpen", "right-open"),
    ("leftOpen", "left-open"),
];

/// `MindItemCommandRow` — the `clearNoneTogglei` command cell grid.
#[derive(GodotClass)]
#[class(base=PanelContainer, init)]
pub struct MindItemCommandRow {
    base: Base<PanelContainer>,
    /// Whether the fragment closes the panel after a pick
    /// (`Reconstructor.java:230` `deselect()`).
    close_select: bool,
}

#[godot_api]
impl IPanelContainer for MindItemCommandRow {}

#[godot_api]
impl MindItemCommandRow {
    /// Emitted with the command name on a checked click.
    #[signal]
    fn picked(command: GString);

    /// Whether the fragment closes the panel after a pick
    /// (`Reconstructor.java:230` `deselect()`).
    #[func]
    pub fn close_select(&self) -> bool {
        self.close_select
    }

    /// Builds the row from the `config_spec` `commands` object.
    #[func]
    pub fn build(&mut self, spec: VarDictionary) {
        self.close_select = dict_bool(&spec, "close_select", false);
        if dict_bool(&spec, "background", true) {
            let mut style = StyleBoxFlat::new_gd();
            style.set_bg_color(styles::BLACK_6);
            style.set_content_margin_all(0.0);
            self.base_mut()
                .add_theme_stylebox_override("panel", &style.upcast::<StyleBox>());
        } else {
            self.base_mut().add_theme_stylebox_override(
                "panel",
                &StyleBoxEmpty::new_gd().upcast::<StyleBox>(),
            );
        }
        // code-instantiated: one toggle per command from the `config_spec` JSON
        // (runtime content; the count is driven by the produced unit type).
        let mut grid = MindTable::new_alloc();
        self.base_mut().add_child(&grid);
        let columns = dict_i32(&spec, "columns", 5).max(1);
        let cell_size = dict_f32(&spec, "size", 40.0);
        let mut group = ButtonGroup::new_gd();
        group.set_allow_unpress(true);
        if dict_bool(&spec, "separator", false) {
            let mut bar = ColorRect::new_alloc();
            bar.set_color(SEPARATOR_COLOR);
            bar.set_custom_minimum_size(Vector2::new(0.0, SEPARATOR_HEIGHT));
            bar.set_mouse_filter(MouseFilter::IGNORE);
            let mut table = grid.bind_mut();
            table.add(&bar).grow_x().colspan(columns);
            table.row();
        }
        let options = dict_array(&spec, "options");
        let mut index = 0;
        for option in options.iter_shared() {
            let Some(option) = option_dict(&option) else {
                continue;
            };
            let button = self.make_button(&option, cell_size, &group);
            grid.bind_mut().add(&button).size(cell_size);
            index += 1;
            if index % columns == 0 {
                grid.bind_mut().row();
            }
        }
        // `if(list.size < columns)`: empty 40 px cells pad the row
        // (`UnitFactory.java:322-326`).
        if options.len() < columns as usize {
            for _ in 0..(columns as usize - options.len()) {
                let mut filler = Control::new_alloc();
                filler.set_mouse_filter(MouseFilter::IGNORE);
                grid.bind_mut().add(&filler).size(cell_size);
            }
        }
        // `MindTable` layout needs an explicit pass after a runtime rebuild
        // (`mind_table.gd sort_now`), repeated once deferred like the dialogs
        // do.
        grid.bind_mut().sort_now();
        self.base_mut().call_deferred("resort", &[]);
    }

    /// The deferred `sort_now` that used to be
    /// `grid.call_deferred("sort_now")`.
    #[func]
    fn resort(&mut self) {
        let children = self.base().get_children();
        for child in children.iter_shared() {
            if let Ok(mut table) = child.try_cast::<MindTable>() {
                table.bind_mut().sort_now();
            }
        }
    }

    /// `UnitFactory.java:311-315`: a size-box `clearNoneTogglei` toggle with the
    /// command's `Icon` glyph and the localized command name as its tooltip.
    fn make_button(
        &self,
        option: &VarDictionary,
        cell_size: f32,
        group: &Gd<ButtonGroup>,
    ) -> Gd<Button> {
        let command_name = dict_string(option, "name", "");
        let mut icon_name = dict_string(option, "icon", "");
        if let Some((_, alias)) = ICON_ALIASES.iter().find(|(key, _)| *key == icon_name) {
            icon_name = (*alias).to_string();
        }
        let mut button = factory::glyph_button(&icon_name, "", "clearNoneTogglei");
        button.set_name(format!("command-{command_name}").as_str());
        button.set_toggle_mode(true);
        button.set_button_group(group);
        button.set_custom_minimum_size(Vector2::new(cell_size, cell_size));
        button.set_default_cursor_shape(CursorShape::POINTING_HAND);
        button.set_tooltip_text(&localized(&dict_string(option, "label", ""), &command_name));
        clear_none_toggle_style(&mut button);
        let children = button.get_children();
        for child in children.iter_shared() {
            if let Ok(mut label) = child.try_cast::<Label>() {
                // Java draws the `Icon` drawable at the button's image size
                // (`imageSize = size`), so the glyph fills the cell.
                label.add_theme_font_size_override("font_size", cell_size as i32);
            }
        }
        if dict_bool(option, "checked", false) {
            button.set_pressed_no_signal(true);
        }
        // Java's click listener always configures the command (an uncheck is
        // re-checked by the next `update`), so only the checked transition
        // reports.
        let target = self.to_gd();
        let callable = Callable::from_fn("item_command_pick", move |args: &[&Variant]| {
            let on = args
                .first()
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            if !on {
                return;
            }
            let mut node = target.clone().upcast::<Node>();
            node.emit_signal(
                "picked",
                &[GString::from(command_name.as_str()).to_variant()],
            );
        });
        button.connect("toggled", &callable);
        button
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_aliases_resolve_java_field_names() {
        let alias = |name: &str| {
            ICON_ALIASES
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, alias)| *alias)
        };
        assert_eq!(alias("downOpen"), Some("down-open"));
        assert_eq!(alias("upOpen"), Some("up-open"));
        assert_eq!(alias("rightOpen"), Some("right-open"));
        assert_eq!(alias("leftOpen"), Some("left-open"));
        // A name outside the table is used verbatim.
        assert_eq!(alias("move"), None);
    }

    #[test]
    fn separator_matches_pal_gray() {
        // `Pal.gray` is #454545 (`Pal.java:82`).
        assert_eq!(SEPARATOR_COLOR.r8(), 0x45);
        assert_eq!(SEPARATOR_HEIGHT, 4.0);
    }
}
