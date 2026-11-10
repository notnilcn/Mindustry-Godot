// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map locales dialog controller: shell, locale table and property cards.
//! Upstream: editor/MapLocalesDialog.java

mod model;
mod ui;

use godot::classes::HBoxContainer;
use godot::classes::LineEdit;
use godot::classes::MarginContainer;
use godot::classes::PanelContainer;
use godot::classes::VBoxContainer;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::SizeFlags;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::MindTable;
use crate::ui::kit::widgets::factory;
use mind_core::maps::locales::ensure_locale;

use super::util::*;

use self::model::{
    LOCALE_APPLY_ALL_META, LOCALE_APPLY_NODE, LOCALE_CARD_WIDTH, LOCALE_COLLAPSED_META,
    LOCALE_FILTER_OVERLAY, LOCALE_GRAY, LOCALE_PENDING_DELETE_META, LOCALE_PENDING_SAVE_META,
    LOCALE_REQUEST_MAIN, LOCALE_REQUEST_TABLES, LOCALE_RESIZE_COLUMNS_META, LOCALE_SAVED_META,
    LOCALE_SEARCH_META, LOCALE_SEARCH_VALUE_META, LOCALE_SELECTED_META, LOCALE_SHOW_CORRECT_META,
    LOCALE_SHOW_MISSING_META, LOCALE_SHOW_SAME_META, LOCALE_SUB_OVERLAY, locale_add_property,
    locale_apply, locale_default_locale, locale_display_name, locale_main_columns,
    locale_mark_dirty, locale_matches_search, locale_parse_or_default, locale_property_status,
    locale_selected, locale_set_saved_state, locale_set_state, locale_show_flags, locale_state,
    locale_status_visible, locale_sync_apply, locale_viewport_width,
};
use self::ui::{
    locale_ask_confirm, locale_bind_confirm, locale_bind_resize, locale_build_card,
    locale_glyph_toggle, locale_main_refresh, locale_rule, locale_show_add_locale,
    locale_show_edit_menu, locale_show_filter, locale_show_locale_edit,
};

// -- MapLocalesDialog -------------------------------------------------------

/// `@editor.locales` (`MapLocalesDialog.java:55-164`): the map's per-locale
/// property bundles, their status-coloured cards, the locale sidebar and the
/// property add panel.
#[derive(Default)]
pub struct MapLocalesDialog {
    /// Locale sidebar list (`langs`).
    langs: Option<Gd<MindTable>>,
    /// Property card pane content (`main`'s pane).
    main: Option<Gd<MindTable>>,
    /// The main search field (re-synced after the filter dialog).
    search: Option<Gd<LineEdit>>,
}

impl DialogController for MapLocalesDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.locales");
        dialog.should_pause = false;
        dialog.clear_buttons();
        // Java `BaseDialog` fills the parent (`setFillParent(true)`); the three
        // columns need the whole viewport.
        dialog.fill_panel();
        // `buttons.table(t -> { back; apply; edit })` + the `?` info button.
        dialog.add_default_close_button();
        let node = dialog.to_gd();
        let apply = kit::t("@editor.apply").to_string();
        let apply_target = node.clone();
        let callable = Callable::from_fn("locales_apply", move |_args: &[&Variant]| {
            locale_apply(&apply_target);
        });
        let mut button = dialog.add_button(&apply, Some(callable), "ok", 210.0);
        button.set_name(LOCALE_APPLY_NODE);
        let edit = kit::t("@edit").to_string();
        let edit_target = node.clone();
        let callable = Callable::from_fn("locales_edit", move |_args: &[&Variant]| {
            locale_show_edit_menu(&edit_target);
        });
        dialog.add_button(&edit, Some(callable), "edit", 210.0);
        let callable = Callable::from_fn("locales_info", |_args: &[&Variant]| {
            ui_show_info(&kit::t("@locales.info"));
        });
        dialog.add_button("?", Some(callable), "", 60.0);
        locale_bind_confirm(&node);
        locale_bind_resize(dialog);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = MindDialog::take_request(dialog);
        match request.as_str() {
            LOCALE_REQUEST_MAIN => self.build_main(dialog),
            LOCALE_REQUEST_TABLES => self.build_tables(dialog),
            // `shown(this::setup)`.
            _ => self.setup(dialog),
        }
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        close_overlay(dialog, LOCALE_SUB_OVERLAY);
        close_overlay(dialog, LOCALE_FILTER_OVERLAY);
        let node = dialog.to_gd();
        if meta_bool(&node, LOCALE_SAVED_META) {
            return;
        }
        // `Dialog.closeOnBack` (`MapLocalesDialog.java:66-85`): hiding with
        // unsaved edits offers `@editor.savechanges`; confirming writes the
        // `locales` tag (`state.mapLocales` has no editor-facade seam).
        let Some(ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("show_confirm") {
            return;
        }
        set_meta_text(&mut node.clone(), LOCALE_PENDING_DELETE_META, "");
        set_meta_bool(&mut node.clone(), LOCALE_PENDING_SAVE_META, true);
        ui_show_confirm(&kit::t("@editor.savechanges").to_string());
    }
}

impl MapLocalesDialog {
    /// `shown(this::setup)` + `show(locales)`: read the map's `locales` tag and
    /// rebuild from scratch. The filter/search/apply-to-all state is kept
    /// across opens like the reused Java dialog instance; only the bundles,
    /// `lastSaved` and `saved` reset.
    fn setup(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        let node = dialog.to_gd();
        let loaded = locale_parse_or_default(
            &editor_json("locales_json", &[])
                .unwrap_or_else(|| GString::from("{}"))
                .to_string(),
        );
        locale_set_state(&node, &loaded);
        locale_set_saved_state(&node, &loaded);
        let mut meta = node.clone();
        set_meta_bool(&mut meta, LOCALE_SAVED_META, true);
        if !meta.has_meta(LOCALE_SELECTED_META) || meta_text(&meta, LOCALE_SELECTED_META).is_empty()
        {
            set_meta_text(&mut meta, LOCALE_SELECTED_META, &locale_default_locale());
        }
        // `applytoall`/`collapsed`/`searchString`/`searchByValue`/`show*` are
        // field initializers of the long-lived Java dialog instance.
        let defaults = [
            (LOCALE_APPLY_ALL_META, true),
            (LOCALE_COLLAPSED_META, false),
            (LOCALE_SEARCH_VALUE_META, false),
            (LOCALE_SHOW_CORRECT_META, true),
            (LOCALE_SHOW_MISSING_META, true),
            (LOCALE_SHOW_SAME_META, true),
        ];
        for (key, value) in defaults {
            if !meta.has_meta(key) {
                set_meta_bool(&mut meta, key, value);
            }
        }
        if !meta.has_meta(LOCALE_SEARCH_META) {
            set_meta_text(&mut meta, LOCALE_SEARCH_META, "");
        }
        dialog.clear_content();
        self.build_shell(dialog);
        self.build_tables(dialog);
    }

    /// `setup`'s fixed frame: the locale sidebar, the search/check/card column
    /// and the add-property panel.
    fn build_shell(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        let mut root = dialog.content_table();
        let mut row = HBoxContainer::new_alloc();
        row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.set_v_size_flags(SizeFlags::EXPAND_FILL);
        row.add_theme_constant_override("separation", 10);
        root.bind_mut().add(&row).grow_x().grow_y();

        // `langs = new Table(Tex.button)`: locale rows + the @add button.
        let mut sidebar = PanelContainer::new_alloc();
        sidebar.add_theme_stylebox_override("panel", &button_nine_patch("button"));
        sidebar.set_custom_minimum_size(Vector2::new(324.0, 0.0));
        sidebar.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut sidebar_column = factory::vbox(6);
        sidebar.add_child(&sidebar_column);
        let mut langs_scroll = MindScroll::new_alloc();
        langs_scroll.bind_mut().set_scrolling_disabled(true, false);
        let mut langs_scroll_control = as_control(&langs_scroll);
        langs_scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut langs = langs_scroll.bind_mut().content();
        langs.bind_mut().grow_horizontal();
        self.langs = Some(langs.clone());
        sidebar_column.add_child(&langs_scroll_control);
        let add_text = kit::t("@add").to_string();
        let add_target = node.clone();
        let callable = Callable::from_fn("locales_add_locale", move |_args: &[&Variant]| {
            locale_show_add_locale(&add_target);
        });
        let mut add = factory::icon_button("add", &add_text, "defaultt", AlignmentMode::CENTER);
        add.set_custom_minimum_size(Vector2::new(250.0, 56.0));
        add.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        add.connect("pressed", &callable);
        sidebar_column.add_child(&add);
        row.add_child(&sidebar);

        // Middle column: the search row, `@locales.applytoall` and the cards.
        let mut middle = VBoxContainer::new_alloc();
        middle.set_h_size_flags(SizeFlags::EXPAND_FILL);
        middle.set_v_size_flags(SizeFlags::EXPAND_FILL);
        middle.add_theme_constant_override("separation", 6);
        row.add_child(&middle);

        let collapsed = meta_bool(&node, LOCALE_COLLAPSED_META);
        let (mut collapse, collapse_glyph) = locale_glyph_toggle(
            if collapsed { "up-open" } else { "down-open" },
            "emptyTogglei",
            35.0,
        );
        collapse.set_pressed_no_signal(collapsed);
        let mut search_row = HBoxContainer::new_alloc();
        search_row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        search_row.add_theme_constant_override("separation", 6);
        let collapse_for_state = collapse.clone();
        let collapse_target = node.clone();
        let callable = Callable::from_fn("locales_collapse_all", move |_args: &[&Variant]| {
            let collapsed = collapse_for_state.is_pressed();
            set_meta_bool(
                &mut collapse_target.clone(),
                LOCALE_COLLAPSED_META,
                collapsed,
            );
            let mut glyph = collapse_glyph.clone();
            glyph.set_text(&kit::glyph(if collapsed { "up-open" } else { "down-open" }));
            MindDialog::request_refresh(&collapse_target, LOCALE_REQUEST_MAIN);
        });
        collapse.connect("pressed", &callable);
        search_row.add_child(&collapse);
        let filter_target = node.clone();
        let callable = Callable::from_fn("locales_filter", move |_args: &[&Variant]| {
            locale_show_filter(&filter_target, locale_main_refresh(&filter_target));
        });
        let mut filter = factory::glyph_button("filter", "", "emptyi");
        filter.set_custom_minimum_size(Vector2::new(35.0, 35.0));
        filter.connect("pressed", &callable);
        search_row.add_child(&filter);
        let by_value = meta_bool(&node, LOCALE_SEARCH_VALUE_META);
        let placeholder = if by_value {
            kit::t("@locales.searchvalue")
        } else {
            kit::t("@locales.searchname")
        };
        let mut search = factory::field(&placeholder.to_string());
        search.set_max_length(64);
        search.set_custom_minimum_size(Vector2::new(250.0, 0.0));
        search.set_text(&GString::from(
            meta_text(&node, LOCALE_SEARCH_META).as_str(),
        ));
        let search_target = node.clone();
        let callable = Callable::from_fn("locales_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| value.to::<GString>().to_string())
                .unwrap_or_default();
            set_meta_text(&mut search_target.clone(), LOCALE_SEARCH_META, &text);
            MindDialog::request_refresh(&search_target, LOCALE_REQUEST_MAIN);
        });
        search.connect("text_changed", &callable);
        self.search = Some(search.clone());
        search_row.add_child(&search);
        let clear_target = node.clone();
        let clear_field = search.clone();
        let callable = Callable::from_fn("locales_search_clear", move |_args: &[&Variant]| {
            set_meta_text(&mut clear_target.clone(), LOCALE_SEARCH_META, "");
            let mut field = clear_field.clone();
            field.set_text(&GString::from(""));
            MindDialog::request_refresh(&clear_target, LOCALE_REQUEST_MAIN);
        });
        let mut clear = factory::glyph_button("cancel", "", "emptyi");
        clear.set_custom_minimum_size(Vector2::new(35.0, 35.0));
        clear.connect("pressed", &callable);
        search_row.add_child(&clear);
        middle.add_child(&as_control(&search_row));

        let apply_all = meta_bool(&node, LOCALE_APPLY_ALL_META);
        let check_target = node.clone();
        let listener = Callable::from_fn("locales_apply_all", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| value.to::<bool>())
                .unwrap_or(false);
            set_meta_bool(&mut check_target.clone(), LOCALE_APPLY_ALL_META, pressed);
        });
        let check = mind_check(
            &kit::t("@locales.applytoall").to_string(),
            apply_all,
            listener,
        );
        let mut check_wrap = MarginContainer::new_alloc();
        for (side, value) in [("margin_left", 10), ("margin_right", 10)] {
            check_wrap.add_theme_constant_override(side, value);
        }
        check_wrap.add_child(&as_control(&check));
        middle.add_child(&as_control(&check_wrap));

        middle.add_child(&locale_rule(LOCALE_GRAY));
        let mut main_scroll = MindScroll::new_alloc();
        main_scroll.bind_mut().set_scrolling_disabled(true, false);
        let mut main_scroll_control = as_control(&main_scroll);
        main_scroll_control.set_v_size_flags(SizeFlags::EXPAND_FILL);
        main_scroll_control.set_custom_minimum_size(Vector2::new(0.0, 360.0));
        let mut main = main_scroll.bind_mut().content();
        main.bind_mut().grow_horizontal();
        main.bind_mut().set_table_top(true);
        self.main = Some(main.clone());
        middle.add_child(&main_scroll_control);
        middle.add_child(&locale_rule(LOCALE_GRAY));

        // Property addition panel (`cont.table(Tex.button, t -> ...).right()`).
        let mut panel = PanelContainer::new_alloc();
        panel.add_theme_stylebox_override("panel", &button_nine_patch("button"));
        panel.set_custom_minimum_size(Vector2::new(380.0, 0.0));
        panel.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
        let mut column = factory::vbox(6);
        panel.add_child(&column);
        let mut name_field = factory::field(&kit::t("@locales.property.name").to_string());
        name_field.set_max_length(64);
        name_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
        column.add_child(&name_field);
        let mut value_area = text_area("");
        value_area.set_placeholder(&kit::t("@locales.property.text"));
        value_area.set_custom_minimum_size(Vector2::new(0.0, 140.0));
        value_area.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let area_for_limit = value_area.clone();
        let callable = Callable::from_fn("locales_value_limit", move |_args: &[&Variant]| {
            let text = area_for_limit.get_text().to_string();
            let truncated = truncate_chars(&text, 1000);
            if truncated != text {
                let mut area = area_for_limit.clone();
                area.set_text(&GString::from(truncated.as_str()));
            }
        });
        value_area.connect("text_changed", &callable);
        column.add_child(&value_area);
        let add_target = node.clone();
        let add_name = name_field.clone();
        let add_value = value_area.clone();
        let callable = Callable::from_fn("locales_add_property", move |_args: &[&Variant]| {
            let key = add_name.get_text().to_string();
            let value = add_value.get_text().to_string();
            let mut state = locale_state(&add_target);
            let selected = locale_selected(&add_target);
            let apply_all = meta_bool(&add_target, LOCALE_APPLY_ALL_META);
            locale_add_property(&mut state, &selected, &key, &value, apply_all);
            locale_set_state(&add_target, &state);
            locale_mark_dirty(&add_target);
            MindDialog::request_refresh(&add_target, LOCALE_REQUEST_MAIN);
        });
        let mut add_property =
            factory::icon_button("add", &add_text, "defaultt", AlignmentMode::CENTER);
        add_property.set_custom_minimum_size(Vector2::new(0.0, 50.0));
        add_property.set_h_size_flags(SizeFlags::EXPAND_FILL);
        add_property.connect("pressed", &callable);
        column.add_child(&add_property);
        row.add_child(&panel);
    }

    /// `buildTables` (`MapLocalesDialog.java:166-173`): keep the selected
    /// locale present, then rebuild both panes.
    fn build_tables(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        let mut state = locale_state(&node);
        let selected = locale_selected(&node);
        if ensure_locale(&mut state, &selected) {
            locale_set_state(&node, &state);
        }
        self.build_locales(dialog);
        self.build_main(dialog);
        locale_sync_apply(&node, meta_bool(&node, LOCALE_SAVED_META));
    }

    /// `buildLocalesTable` (`MapLocalesDialog.java:175-201`): one flat-toggle
    /// row per available locale present in the map, with edit and delete.
    fn build_locales(&mut self, dialog: &mut MindDialog) {
        let Some(mut langs) = self.langs.clone() else {
            return;
        };
        langs.bind_mut().clear();
        let node = dialog.to_gd();
        let state = locale_state(&node);
        let selected = locale_selected(&node);
        for code in available_locales() {
            if !state.0.contains_key(&code) {
                continue;
            }
            let display = locale_display_name(&code);
            let mut toggle = factory::button(&display);
            toggle.set_theme_type_variation("flatTogglet");
            toggle.set_toggle_mode(true);
            toggle.set_pressed_no_signal(code == selected);
            toggle.set_custom_minimum_size(Vector2::new(200.0, 50.0));
            let target = node.clone();
            let name = code.clone();
            let callable = Callable::from_fn("locales_select", move |_args: &[&Variant]| {
                if name == locale_selected(&target) {
                    return;
                }
                set_meta_text(&mut target.clone(), LOCALE_SELECTED_META, &name);
                MindDialog::request_refresh(&target, LOCALE_REQUEST_TABLES);
            });
            toggle.connect("pressed", &callable);
            langs.bind_mut().add(&toggle).width(200.0).min_height(50.0);
            let mut edit = factory::glyph_button("edit", "", "flati");
            edit.set_custom_minimum_size(Vector2::new(50.0, 50.0));
            let target = node.clone();
            let name = code.clone();
            let callable = Callable::from_fn("locales_locale_edit", move |_args: &[&Variant]| {
                locale_show_locale_edit(&target, &name);
            });
            edit.connect("pressed", &callable);
            langs.bind_mut().add(&edit).size(50.0);
            let mut remove = factory::glyph_button("trash", "", "flati");
            remove.set_custom_minimum_size(Vector2::new(50.0, 50.0));
            let target = node.clone();
            let name = code.clone();
            let callable =
                Callable::from_fn("locales_locale_delete", move |_args: &[&Variant]| {
                    set_meta_text(&mut target.clone(), LOCALE_PENDING_DELETE_META, &name);
                    locale_ask_confirm(&format!(
                        "{}\n{}",
                        kit::t("@confirm"),
                        kit::t("@locales.deletelocale")
                    ));
                });
            remove.connect("pressed", &callable);
            langs.bind_mut().add(&remove).size(50.0);
            langs.bind_mut().row();
        }
    }

    /// `buildMain` (`MapLocalesDialog.java:203-314`): the sorted, filtered,
    /// status-coloured property cards.
    fn build_main(&mut self, dialog: &mut MindDialog) {
        let Some(mut main) = self.main.clone() else {
            return;
        };
        main.bind_mut().clear();
        let node = dialog.to_gd();
        let state = locale_state(&node);
        let selected = locale_selected(&node);
        let props = state.0.get(&selected).cloned().unwrap_or_default();
        let columns = locale_main_columns(locale_viewport_width(&node));
        set_meta_int(
            &mut node.clone(),
            LOCALE_RESIZE_COLUMNS_META,
            columns as i64,
        );
        let mut keys: Vec<String> = props.keys().cloned().collect();
        keys.sort();
        let search = meta_text(&node, LOCALE_SEARCH_META);
        let by_value = meta_bool(&node, LOCALE_SEARCH_VALUE_META);
        let (show_correct, show_missing, show_same) = locale_show_flags(&node);
        let mut shown = 0usize;
        for key in keys {
            let value = props.get(&key).cloned().unwrap_or_default();
            let status = locale_property_status(&state, &key, Some(&value), &selected, false);
            if !locale_status_visible(status, show_correct, show_missing, show_same) {
                continue;
            }
            let candidate = if by_value {
                value.as_str()
            } else {
                key.as_str()
            };
            if !locale_matches_search(&search, candidate) {
                continue;
            }
            let card = locale_build_card(&node, &key, &value, status);
            {
                let mut bound = main.bind_mut();
                let cell = bound.add(&card).width(LOCALE_CARD_WIDTH).pad(5.0);
                let _ = cell.top();
            }
            shown += 1;
            if shown.is_multiple_of(columns) {
                main.bind_mut().row();
            }
        }
        if shown == 0 {
            let label = factory::label(&kit::t("@empty").to_string());
            let mut bound = main.bind_mut();
            bound.add(&label).colspan(columns as i32).pad(20.0);
            bound.row();
        }
        self.sync_search(&node);
        locale_sync_apply(&node, meta_bool(&node, LOCALE_SAVED_META));
    }

    /// Keeps the search field and its message in sync with the shared
    /// `searchString`/`searchByValue` state (the Java field's per-frame
    /// `update` callbacks).
    fn sync_search(&mut self, node: &Gd<MindDialog>) {
        let Some(mut field) = self.search.clone() else {
            return;
        };
        let search = meta_text(node, LOCALE_SEARCH_META);
        if field.get_text().to_string() != search {
            field.set_text(&GString::from(search.as_str()));
        }
        let by_value = meta_bool(node, LOCALE_SEARCH_VALUE_META);
        let placeholder = if by_value {
            kit::t("@locales.searchvalue")
        } else {
            kit::t("@locales.searchname")
        };
        if field.get_placeholder() != placeholder {
            field.set_placeholder(&placeholder);
        }
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_locales" => Some(Box::new(MapLocalesDialog::default())),
        _ => None,
    }
}
