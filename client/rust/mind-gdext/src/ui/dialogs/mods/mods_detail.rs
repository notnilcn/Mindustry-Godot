// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The mods dialog's details pane: details, the content grid and the enable/remove actions.
//! Upstream: ui/dialogs/ModsDialog.java

use super::mods_dialog::{ModsState, mods_refresh};
use super::mods_import::{
    mods_download_dependencies, mods_open_path, mods_open_url, mods_show_error,
};
use super::mods_list::{mods_details, mods_list, mods_rebuild_list, mods_state_details};
use super::util::{bt, defer_action, dict_bool, dict_string, mods_node, string_list, ui_call};
use crate::ui::kit;
use crate::ui::kit::widgets::factory;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Button, HBoxContainer, TextureRect, VBoxContainer};
use godot::prelude::*;
use std::{cell::RefCell, rc::Rc};

/// `showMod`: the expanded details block (upstream opens a BaseDialog).
pub(super) fn mods_add_details(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, mod_name: &str) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    let details = mods_details(mod_name);
    // code-instantiated: details are data-driven from `MindMods.details(name)`.
    let mut box_ = VBoxContainer::new_alloc();
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);

    let author = dict_string(&details, "author");
    if !author.is_empty() {
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@editor.author"),
            author
        )));
    }
    let version = dict_string(&details, "version");
    if !version.is_empty() {
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@mod.version"),
            version
        )));
    }
    let description = dict_string(&details, "description");
    if !description.is_empty() {
        let mut body_label = factory::label(&description);
        body_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        box_.add_child(&body_label);
    }
    let state_details = mods_state_details(&details);
    if !state_details.is_empty() {
        box_.add_child(&factory::label(&state_details));
    }

    // Dependency dialog (`@mod.dependencies.downloadall`) delegates to the browser.
    let missing = string_list(details.get("missing_dependencies").as_ref());
    if !missing.is_empty() {
        // code-instantiated: dependency import row only when deps are unmet.
        let mut dependency = factory::button(&bt("@mod.dependencies.downloadall"));
        {
            let missing = missing.clone();
            let callable = Callable::from_fn("mods_download_deps", move |_args: &[&Variant]| {
                let missing = missing.clone();
                defer_action("mods_download_deps_now", move || {
                    mods_download_dependencies(&missing);
                });
                Variant::nil()
            });
            dependency.connect("pressed", &callable);
        }
        box_.add_child(&dependency);
    }

    // code-instantiated: per-mod action row inside the details block.
    let mut actions = HBoxContainer::new_alloc();
    let mut folder = factory::button(&bt("@mods.openfolder"));
    {
        let path = dict_string(&details, "config_folder");
        let callable = Callable::from_fn("mods_details_folder", move |_args: &[&Variant]| {
            mods_open_path(&path);
            Variant::nil()
        });
        folder.connect("pressed", &callable);
    }
    actions.add_child(&folder);
    let repo = dict_string(&details, "repo");
    if !repo.is_empty() {
        let mut open_repo = factory::button(&bt("@mods.github.open"));
        let url = format!("https://github.com/{repo}");
        let callable = Callable::from_fn("mods_details_repo", move |_args: &[&Variant]| {
            mods_open_url(&url);
            Variant::nil()
        });
        open_repo.connect("pressed", &callable);
        actions.add_child(&open_repo);
    }
    box_.add_child(&actions);

    // `Seq<UnlockableContent> all = …` + `if(all.any()) … button("@mods.viewcontent")`
    // (`ModsDialog.java:455-481`): the per-mod content grid. Java opens a second
    // `BaseDialog`; the port expands the grid inside the details block (the same
    // inline pattern as the details themselves).
    let content = mods_content(mod_name);
    if !content.is_empty() {
        // code-instantiated: the view-content label is a bundle string.
        let mut view_content = factory::button(&bt("@mods.viewcontent"));
        view_content.set_custom_minimum_size(Vector2::new(300.0, 50.0));
        {
            let slot = state.clone();
            let name = mod_name.to_string();
            let callable = Callable::from_fn("mods_view_content", move |_args: &[&Variant]| {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    mods_toggle_content(&slot, &mut st, &name);
                }
                Variant::nil()
            });
            view_content.connect("pressed", &callable);
        }
        box_.add_child(&view_content);
        if st.open_content == mod_name {
            box_.add_child(&mods_content_grid(&content));
        }
    }

    {
        let mut bound = list.bind_mut();
        bound.add(&box_).grow_x().pad(6.0);
        bound.row();
    }
}

/// `MindMods.content_for(name)` rows (`{type, name, stem, path}`); empty when
/// the mod ships no content (Java's `all.any()` gate).
pub(super) fn mods_content(name: &str) -> Vec<VarDictionary> {
    let Some(mut mods) = mods_node() else {
        return Vec::new();
    };
    if !mods.has_method("content_for") {
        return Vec::new();
    }
    mods.call("content_for", &[GString::from(name).to_variant()])
        .try_to::<VarArray>()
        .map(|rows| {
            rows.iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `d.cont.pane(cs -> …)` (`ModsDialog.java:466-479`): 50 px icon buttons in
/// rows of 14 (Java: `min(width / Scl.scl(110), 14)`), each opening
/// `ui.content.show(c)`.
pub(super) fn mods_content_grid(content: &[VarDictionary]) -> Gd<VBoxContainer> {
    // code-instantiated: the grid is data-driven from the mod's content files.
    let mut grid = VBoxContainer::new_alloc();
    grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
    grid.add_theme_constant_override("separation", 4);
    for chunk in content.chunks(14) {
        let mut row = factory::hbox(4);
        for entry in chunk {
            row.add_child(&mods_content_icon(entry));
        }
        grid.add_child(&row);
    }
    grid
}

/// One content icon (`cs.button(new TextureRegionDrawable(c.uiIcon), Styles.flati,
/// iconMed, () -> ui.content.show(c))`, `ModsDialog.java:467-473`).
pub(super) fn mods_content_icon(entry: &VarDictionary) -> Gd<Button> {
    let content_name = dict_string(entry, "name");
    let content_type = dict_string(entry, "type");
    // code-instantiated: one icon button per content record.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flati");
    button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    let icon = format!("{content_type}-{content_name}-ui");
    if let Some(texture) = kit::icon_texture(&icon) {
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_mouse_filter(MouseFilter::IGNORE);
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        button.add_child(&image);
    } else {
        // No shipped icon: fall back to the content-type glyph.
        let glyph_name = match content_type.as_str() {
            "liquid" => "liquid",
            "unit" => "units",
            "block" => "blocks",
            _ => "book",
        };
        let mut glyph = factory::glyph_label(glyph_name, 26, Color::WHITE);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&glyph);
    }
    // `tooltip(c.localizedName)`; the port has no per-mod bundle key here, so
    // the file stem is the honest label.
    button.set_tooltip_text(&GString::from(dict_string(entry, "stem").as_str()));
    {
        let content_name = content_name.clone();
        let callable = Callable::from_fn("mods_content_info", move |_args: &[&Variant]| {
            let name = content_name.clone();
            defer_action("mods_content_info_now", move || {
                crate::ui::dialogs::batch_a::open_content_info(&name);
            });
            Variant::nil()
        });
        button.connect("pressed", &callable);
    }
    button
}

/// `_toggle_content(mod_name)`: expand/collapse one mod's content grid.
pub(super) fn mods_toggle_content(
    state: &Rc<RefCell<ModsState>>,
    st: &mut ModsState,
    mod_name: &str,
) {
    st.open_content = if st.open_content == mod_name {
        String::new()
    } else {
        mod_name.to_string()
    };
    let query = st
        .search
        .clone()
        .filter(|search| search.is_instance_valid())
        .map(|search| search.get_text().to_string())
        .unwrap_or_default();
    mods_rebuild_list(state, st, &query);
}

/// `_toggle_details(mod_name)`.
pub(super) fn mods_toggle_details(
    state: &Rc<RefCell<ModsState>>,
    st: &mut ModsState,
    mod_name: &str,
) {
    st.open_details = if st.open_details == mod_name {
        String::new()
    } else {
        mod_name.to_string()
    };
    let query = st
        .search
        .clone()
        .filter(|search| search.is_instance_valid())
        .map(|search| search.get_text().to_string())
        .unwrap_or_default();
    mods_rebuild_list(state, st, &query);
}

/// `mods.setEnabled(mod, !mod.enabled()); setup();`.
pub(super) fn mods_set_enabled(state: &Rc<RefCell<ModsState>>, mod_name: &str, enabled: bool) {
    if let Some(mut mods) = mods_node()
        && mods.has_method("set_enabled")
    {
        let _ = mods.call(
            "set_enabled",
            &[GString::from(mod_name).to_variant(), enabled.to_variant()],
        );
    }
    mods_refresh(state);
}

/// `@mods.restore` (`ModsDialog.java:56-59`): re-enable every failed mod.
pub(super) fn mods_restore_failed(state: &Rc<RefCell<ModsState>>) {
    let Some(mut mods) = mods_node() else {
        return;
    };
    if !mods.has_method("list") || !mods.has_method("set_enabled") {
        return;
    }
    let failed: Vec<String> = mods_list(&Some(mods.clone()))
        .iter()
        .filter(|entry| dict_bool(entry, "failed", false))
        .map(|entry| dict_string(entry, "name"))
        .collect();
    for name in failed {
        let _ = mods.call(
            "set_enabled",
            &[GString::from(name.as_str()).to_variant(), true.to_variant()],
        );
    }
    mods_refresh(state);
}

/// `ui.showConfirm("@confirm", "@mod.remove.confirm", …)`.
pub(super) fn mods_confirm_delete(state: &Rc<RefCell<ModsState>>, mod_name: &str) {
    state.borrow_mut().pending_delete = mod_name.to_string();
    if ui_call(
        "show_confirm",
        &[GString::from(bt("@mod.remove.confirm").as_str()).to_variant()],
    )
    .is_none()
    {
        mods_on_confirm(state, true);
    }
}

/// `_on_confirm(confirmed)`.
pub(super) fn mods_on_confirm(state: &Rc<RefCell<ModsState>>, confirmed: bool) {
    let mod_name = std::mem::take(&mut state.borrow_mut().pending_delete);
    if mod_name.is_empty() || !confirmed {
        return;
    }
    if let Some(mut mods) = mods_node()
        && mods.has_method("remove_mod")
    {
        let result = mods
            .call(
                "remove_mod",
                &[GString::from(mod_name.as_str()).to_variant()],
            )
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        if !dict_bool(&result, "ok", false) {
            let error = dict_string(&result, "error");
            mods_show_error(&if error.is_empty() {
                bt("@mod.delete.error")
            } else {
                error
            });
        }
    }
    state
        .borrow_mut()
        .updates
        .retain(|(name, _)| name != &mod_name);
}
