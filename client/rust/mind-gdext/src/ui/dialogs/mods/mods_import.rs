// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The mods dialog's import flows, update actions and error/guide dialogs.
//! Upstream: ui/dialogs/ModsDialog.java

use super::mod_browser::BROWSER_INSTALL_META;
use super::mods_dialog::{ModsState, mods_refresh};
use super::mods_list::{mods_details, mods_errors};
use super::util::{
    bt, defer_action, dialog_node, dict_bool, dict_string, mods_node, open_dialog,
    refresh_dialog_deferred, ui_call, variant_bool,
};
use godot::classes::{Control, Json, Node};
use godot::prelude::*;
use std::{cell::RefCell, rc::Rc};

/// `_import_file`: hide the chooser and open the native `.zip`/`.jar` picker.
pub(super) fn mods_import_file(state: &Rc<RefCell<ModsState>>) {
    mods_hide_import_dialog(state);
    let start = mods_node()
        .filter(|node| node.has_method("mods_dir"))
        .map(|mut node| node.call("mods_dir", &[]).to::<GString>().to_string())
        .unwrap_or_else(|| String::from("user://mods"));
    let mut filters = PackedStringArray::new();
    filters.push("*.zip");
    filters.push("*.jar");
    let slot = state.clone();
    let callable = Callable::from_fn("mods_files_chosen", move |args: &[&Variant]| {
        let status = args
            .first()
            .map(|value| variant_bool(value))
            .unwrap_or(false);
        if !status {
            return Variant::nil();
        }
        let paths = match args.get(1) {
            Some(value) => value.to::<PackedStringArray>(),
            None => PackedStringArray::new(),
        };
        let names: Vec<String> = paths
            .as_slice()
            .iter()
            .map(|path| path.to_string())
            .collect();
        let slot = slot.clone();
        defer_action("mods_import_paths", move || {
            for path in names {
                mods_import_path(&slot, &path);
            }
        });
        Variant::nil()
    });
    let title = bt("@mod.import.file");
    let shown = crate::platform::dialogs::show_file_dialog(
        &title, &start, "", false, false, true, false, &filters, &callable,
    );
    if !shown {
        log::warn!("[mods] native file dialog unavailable; cannot import a file");
    }
}

/// `_import_path(path)`.
pub(super) fn mods_import_path(state: &Rc<RefCell<ModsState>>, path: &str) {
    let Some(mut mods) = mods_node() else {
        return;
    };
    if !mods.has_method("import_mod") {
        return;
    }
    let result = mods
        .call("import_mod", &[GString::from(path).to_variant()])
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
    mods_refresh(state);
}

/// `@mod.import.github`: `ui.showTextInput("@mod.import.github", "", 64,
/// Core.settings.getString("lastmod", ""), …)`.
pub(super) fn mods_import_github(state: &Rc<RefCell<ModsState>>) {
    mods_hide_import_dialog(state);
    let last = ui_call(
        "settings_get_text",
        &[
            GString::from("lastmod").to_variant(),
            GString::from("").to_variant(),
        ],
    )
    .map(|value| value.to::<GString>().to_string())
    .unwrap_or_default();
    state.borrow_mut().pending_github = true;
    let _ = ui_call(
        "show_text_input",
        &[
            GString::from(bt("@mod.import.github").as_str()).to_variant(),
            GString::from("").to_variant(),
            64i64.to_variant(),
            GString::from(last.as_str()).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `_on_text_input(text)`: empty text = cancelled.
pub(super) fn mods_on_text_input(state: &Rc<RefCell<ModsState>>, text: &str) {
    {
        let mut st = state.borrow_mut();
        if !st.pending_github {
            return;
        }
        st.pending_github = false;
    }
    if text.trim().is_empty() {
        return;
    }
    let raw = text.to_string();
    defer_action("mods_github_import", move || mods_github_import(&raw));
}

/// Text cleanup + `lastmod` + `githubImportMod` (`:176-191`).
pub(super) fn mods_github_import(raw: &str) {
    let mut text = raw.trim().replace(' ', "");
    if let Some(rest) = text.strip_prefix("https://github.com/") {
        text = rest.to_string();
    }
    if text.is_empty() {
        return;
    }
    let _ = ui_call(
        "settings_put_text",
        &[
            GString::from("lastmod").to_variant(),
            GString::from(text.as_str()).to_variant(),
        ],
    );
    if let Some(browser) = dialog_node("mod_browser") {
        install_repos_on(&browser, &[text]);
    } else {
        let mut context = VarDictionary::new();
        context.set("repo", &GString::from(text.as_str()).to_variant());
        open_dialog(
            "mod_browser",
            &Json::stringify(&context.to_variant()).to_string(),
        );
    }
}

/// `githubImportMod(mod.getRepo(), …)` via the browser.
pub(super) fn mods_update_mod(mod_name: &str) {
    let repo = dict_string(&mods_details(mod_name), "repo");
    if repo.is_empty() {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("repo", &GString::from(repo.as_str()).to_variant());
    open_dialog(
        "mod_browser",
        &Json::stringify(&context.to_variant()).to_string(),
    );
}

/// `_update_all()`: reinstall every mod with a published update.
pub(super) fn mods_update_all(state: &ModsState) {
    let mut repos: Vec<String> = Vec::new();
    for (name, _) in &state.updates {
        let repo = dict_string(&mods_details(name), "repo");
        if !repo.is_empty() {
            repos.push(repo);
        }
    }
    if let Some(browser) = dialog_node("mod_browser") {
        install_repos_on(&browser, &repos);
    } else {
        open_dialog("mod_browser", "{}");
    }
}

/// `_download_dependencies(dependencies)`: browser.show() then install.
pub(super) fn mods_download_dependencies(dependencies: &[String]) {
    open_dialog("mod_browser", "{}");
    if let Some(browser) = dialog_node("mod_browser") {
        install_repos_on(&browser, dependencies);
    }
}

/// `install_repos(items)` on the browser node (metadata handoff; see the
/// module docs).
pub(super) fn install_repos_on(browser: &Gd<Node>, items: &[String]) {
    if items.is_empty() {
        return;
    }
    let mut array = VarArray::new();
    for item in items {
        array.push(&GString::from(item.as_str()).to_variant());
    }
    let mut browser = browser.clone();
    browser.set_meta(BROWSER_INSTALL_META, &array.to_variant());
    refresh_dialog_deferred(&browser);
}

/// `Core.app.openFolder(modDirectory.absolutePath())`.
pub(super) fn mods_open_folder() {
    let path = mods_node()
        .filter(|node| node.has_method("mods_dir"))
        .map(|mut node| node.call("mods_dir", &[]).to::<GString>().to_string())
        .unwrap_or_default();
    mods_open_path(&path);
}

/// `OS.shell_open(path)` through the platform seam.
pub(super) fn mods_open_path(path: &str) {
    if path.is_empty() {
        return;
    }
    let _ = crate::platform::uri::open_uri(path);
}

/// `Core.app.openURI(url)` through the platform seam.
pub(super) fn mods_open_url(url: &str) {
    if url.is_empty() {
        return;
    }
    let _ = crate::platform::uri::open_uri(url);
}

/// `_show_errors()`: the `name: details` summary prompt.
pub(super) fn mods_show_errors() {
    let mut lines: Vec<String> = Vec::new();
    for entry in mods_errors(&mods_node()) {
        lines.push(format!(
            "{}: {}",
            dict_string(&entry, "name"),
            dict_string(&entry, "details")
        ));
    }
    mods_show_error(&lines.join("\n"));
}

/// Deferred: callers include `MindUi` signal handlers, where calling back into
/// the `&mut`-bound `MindUi` would panic.
pub(super) fn mods_show_error(text: &str) {
    let text = text.to_string();
    defer_action("mods_show_error_now", move || {
        mods_show_error_now(&text);
    });
}

/// `_show_error_now(text)`: `ui.showText("@mods", text)` else `showInfo`.
pub(super) fn mods_show_error_now(text: &str) {
    if text.is_empty() {
        return;
    }
    if ui_call(
        "show_text",
        &[
            GString::from(bt("@mods").as_str()).to_variant(),
            GString::from(text).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

/// `_show_import_dialog()`.
pub(super) fn mods_show_import_dialog() {
    let Some(node) = dialog_node("mods") else {
        return;
    };
    let Some(overlay) = node.get_node_or_null("ImportDialog") else {
        return;
    };
    let Ok(mut overlay) = overlay.try_cast::<Control>() else {
        return;
    };
    overlay.set_visible(true);
    overlay.move_to_front();
}

/// `_hide_import_dialog()`.
pub(super) fn mods_hide_import_dialog(state: &Rc<RefCell<ModsState>>) {
    if let Some(mut overlay) = state.borrow().import_overlay.clone()
        && overlay.is_instance_valid()
    {
        overlay.set_visible(false);
    }
}
