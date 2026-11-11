// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The mod browser dialog controller. Filled by S10 (docs/PLANS).
//! Upstream: ui/dialogs/ModBrowserDialog.java

use super::mods_dialog::MODS_UPDATES_META;
use super::mods_import::mods_open_url;
use super::mods_list::mods_details;
use super::util::{
    bt, defer_action, dialog_node, dict_bool, dict_i64, dict_string, fmt_key, mods_node,
    refresh_dialog_deferred, string_list, ui_call, variant_i64, variant_string,
};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{MindLabel, MindTable};
use godot::classes::control::SizeFlags;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, Control, DirAccess, FileAccess, HBoxContainer, HttpRequest, Json, LineEdit,
    ProjectSettings, Timer, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use std::{cell::RefCell, cmp::Reverse, rc::Rc};

/// The `ModBrowserDialog` controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "mod_browser" => Some(Box::new(ModBrowserDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ModBrowserDialog (`mod_browser`, `ModBrowserDialog.java`)
// ---------------------------------------------------------------------------

/// Ported from `Vars.modJsonURLs` (tried in order).
const MOD_JSON_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/Anuken/MindustryMods/master/mods.json",
    "https://cdn.jsdelivr.net/gh/anuken/mindustrymods/mods.json",
];
/// Ported from `Vars.ghApi`.
const GH_API: &str = "https://api.github.com";
/// TEMP download directory for zipballs (deleted after import).
const CACHE_PATH: &str = "user://modcache";
/// `HTTPRequest.RESULT_SUCCESS`.
const HTTP_RESULT_SUCCESS: i64 = 0;
/// The mods dialog's `install_repos(items)` replacement (metadata handoff).
pub(super) const BROWSER_INSTALL_META: &str = "_mind_browser_install_repos";
/// Guard so the `mod_import_progress` binding runs once per dialog node.
const BROWSER_SIGNALS_META: &str = "_mind_browser_signals_bound";

/// The browser's HTTP slots (one `HTTPRequest` per concurrent step).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HttpSlot {
    Listings,
    Api,
    Release,
    Zip,
}

/// `ModBrowserDialog` view state (`_listings`/`_installed_repos`/`_queue`/…).
struct BrowserState {
    search: Option<Gd<LineEdit>>,
    repo_field: Option<Gd<LineEdit>>,
    sort_button: Option<Gd<Button>>,
    status: Option<Gd<MindLabel>>,
    list: Option<Gd<MindTable>>,
    listings: Vec<VarDictionary>,
    installed_repos: Vec<(String, String)>,
    /// `_order_date` (sort by date first).
    order_date: bool,
    open_repo: String,
    /// Batch reinstall queue (`_queue`).
    queue: Vec<String>,
    /// Name-only installs deferred until the listings load.
    pending_install: Vec<String>,
    pending_repo: String,
    pending_release: String,
    pending_url_index: usize,
    downloading: bool,
    fetching: bool,
    game_build: i64,
    game_revision: i64,
    list_http: Option<Gd<HttpRequest>>,
    api_http: Option<Gd<HttpRequest>>,
    release_http: Option<Gd<HttpRequest>>,
    zip_http: Option<Gd<HttpRequest>>,
    /// `_process` replacement (streams the zipball percent).
    progress_timer: Option<Gd<Timer>>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            search: None,
            repo_field: None,
            sort_button: None,
            status: None,
            list: None,
            listings: Vec::new(),
            installed_repos: Vec::new(),
            order_date: true,
            open_repo: String::new(),
            queue: Vec::new(),
            pending_install: Vec::new(),
            pending_repo: String::new(),
            pending_release: String::new(),
            pending_url_index: 0,
            downloading: false,
            fetching: false,
            game_build: 0,
            game_revision: 0,
            list_http: None,
            api_http: None,
            release_http: None,
            zip_http: None,
            progress_timer: None,
        }
    }
}

/// `@mods.browser` — the GitHub mod browser (`ModBrowserDialog.java`).
#[derive(Default)]
pub struct ModBrowserDialog {
    state: Rc<RefCell<BrowserState>>,
}

impl DialogController for ModBrowserDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@mods.browser");
        dialog.should_pause = false;
        // `makeButtonOverlay()` (`ModBrowserDialog.java:61`).
        dialog.apply_button_overlay(true);
        browser_add_http(dialog, &self.state);
        browser_build(dialog, &self.state);
        dialog.add_default_close_button();
        browser_connect(dialog, &self.state);
        let mut st = self.state.borrow_mut();
        st.game_build = i64::from(mods_i32("game_build"));
        st.game_revision = i64::from(mods_i32("game_revision"));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // A batch handed over by the mods dialog (`install_repos`).
        let pending = browser_take_install_meta(&dialog.to_gd());
        {
            let mut st = self.state.borrow_mut();
            if !pending.is_empty() {
                browser_install_repos(&self.state, &mut st, &pending);
            }
            // Lazy fetch on first open (upstream `shown(this::rebuildBrowser)`).
            browser_fetch_listings(&self.state, &mut st);
        }
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let repo = dialog.context_string("repo").to_string();
        if repo.is_empty() {
            return;
        }
        let mut st = self.state.borrow_mut();
        if let Some(mut field) = st.repo_field.clone()
            && field.is_instance_valid()
        {
            field.set_text(&GString::from(repo.as_str()));
        }
        browser_install_repo(&mut st, &repo);
    }
}

/// `MindMods` integer method (`game_build`/`game_revision`).
fn mods_i32(method: &str) -> i32 {
    let Some(mut mods) = mods_node() else {
        return 0;
    };
    if !mods.has_method(method) {
        return 0;
    }
    mods.call(method, &[]).to::<i32>()
}

/// `_http_headers`: GitHub rejects API/raw requests without a `User-Agent`.
fn http_headers() -> PackedStringArray {
    let mut headers = PackedStringArray::new();
    headers.push("User-Agent: Mindustry-Godot");
    headers.push("Accept: application/vnd.github+json");
    headers
}

/// `_add_http()`: one `HTTPRequest` per concurrent step (list/api/release/zip).
fn browser_add_http(dialog: &mut MindDialog, state: &Rc<RefCell<BrowserState>>) {
    // `ready()` runs again after an extension reload; drop the previous nodes.
    for name in [
        "BrowserListHttp",
        "BrowserApiHttp",
        "BrowserReleaseHttp",
        "BrowserZipHttp",
        "BrowserProgressTimer",
    ] {
        if let Some(mut existing) = dialog.base().get_node_or_null(name) {
            existing.queue_free();
        }
    }
    let list_http = browser_make_http("BrowserListHttp", state, HttpSlot::Listings);
    // code-instantiated: the listings request node has no static scene.
    dialog.base_mut().add_child(&list_http);
    let api_http = browser_make_http("BrowserApiHttp", state, HttpSlot::Api);
    // code-instantiated: the repository request node has no static scene.
    dialog.base_mut().add_child(&api_http);
    let release_http = browser_make_http("BrowserReleaseHttp", state, HttpSlot::Release);
    // code-instantiated: the release request node has no static scene.
    dialog.base_mut().add_child(&release_http);
    let zip_http = browser_make_http("BrowserZipHttp", state, HttpSlot::Zip);
    // code-instantiated: the zipball request node has no static scene.
    dialog.base_mut().add_child(&zip_http);

    // `_process` replacement: poll the zipball byte counters while downloading.
    // code-instantiated: a `DialogController` has no per-frame hook.
    let mut timer = Timer::new_alloc();
    timer.set_name("BrowserProgressTimer");
    timer.set_wait_time(0.1);
    timer.set_one_shot(false);
    timer.set_autostart(false);
    {
        let slot = state.clone();
        let tick = Callable::from_fn("browser_progress", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_progress_tick(&mut st);
            }
            Variant::nil()
        });
        timer.connect("timeout", &tick);
    }
    dialog.base_mut().add_child(&timer);

    let mut st = state.borrow_mut();
    st.list_http = Some(list_http);
    st.api_http = Some(api_http);
    st.release_http = Some(release_http);
    st.zip_http = Some(zip_http);
    st.progress_timer = Some(timer);
}

/// Builds one `HTTPRequest` with its `request_completed` handler.
fn browser_make_http(
    name: &str,
    state: &Rc<RefCell<BrowserState>>,
    slot: HttpSlot,
) -> Gd<HttpRequest> {
    // code-instantiated: one HTTPRequest per concurrent HTTP step.
    let mut http = HttpRequest::new_alloc();
    http.set_name(name);
    let state = state.clone();
    let callable = Callable::from_fn("browser_request_completed", move |args: &[&Variant]| {
        let result = args.first().map(|value| variant_i64(value)).unwrap_or(-1);
        let code = args.get(1).map(|value| variant_i64(value)).unwrap_or(0);
        let body = match args.get(3) {
            Some(value) => value.to::<PackedByteArray>(),
            None => PackedByteArray::new(),
        };
        if let Ok(mut st) = state.try_borrow_mut() {
            match slot {
                HttpSlot::Listings => {
                    browser_on_list_completed(&state, &mut st, result, code, &body)
                }
                HttpSlot::Api => browser_on_api_completed(&mut st, result, code, &body),
                HttpSlot::Release => browser_on_release_completed(&mut st, result, code, &body),
                HttpSlot::Zip => browser_on_zip_completed(&state, &mut st, result, code, &body),
            }
        }
        Variant::nil()
    });
    http.connect("request_completed", &callable);
    http
}

/// `_build()`.
fn browser_build(dialog: &mut MindDialog, state: &Rc<RefCell<BrowserState>>) {
    let mut root = dialog.content_table();
    {
        let mut control: Gd<Control> = root.clone().upcast();
        control.add_theme_constant_override("separation", 6);
    }

    // Search + sort row (`ModBrowserDialog` search field / order toggle).
    // code-instantiated: the search row filters the fetched listings.
    let mut search_row = HBoxContainer::new_alloc();
    // code-instantiated: the listing filter field.
    let mut search = factory::field(&bt("@search"));
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_search", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_rebuild(&slot, &mut st);
            }
            Variant::nil()
        });
        search.connect("text_changed", &callable);
    }
    search_row.add_child(&search);
    // code-instantiated: the date/stars order toggle.
    let mut sort_button = factory::image_button("list", "emptyi");
    sort_button.set_tooltip_text(&bt("@mods.browser.sortdate"));
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_sort", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_toggle_order(&slot, &mut st);
            }
            Variant::nil()
        });
        sort_button.connect("pressed", &callable);
    }
    search_row.add_child(&sort_button);
    {
        let mut bound = root.bind_mut();
        bound.add(&search_row).grow_x().pad(4.0);
        bound.row();
    }

    // Direct GitHub repo import fallback (`@mod.import.github`).
    // code-instantiated: the direct `owner/repo` install row.
    let mut repo_row = HBoxContainer::new_alloc();
    // code-instantiated: the `owner/repo` field.
    let mut repo_field = factory::field(&bt("@mods.github.open"));
    repo_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_repo_submit", move |args: &[&Variant]| {
            let repo = args
                .first()
                .map(|value| variant_string(value))
                .unwrap_or_default();
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_install_repo(&mut st, &repo);
            }
            Variant::nil()
        });
        repo_field.connect("text_submitted", &callable);
    }
    repo_row.add_child(&repo_field);
    // code-instantiated: the direct install button.
    let mut install = factory::button(&bt("@mods.browser.add"));
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_repo_add", move |_args: &[&Variant]| {
            let repo = slot
                .try_borrow()
                .ok()
                .and_then(|st| st.repo_field.clone())
                .filter(|field| field.is_instance_valid())
                .map(|field| field.get_text().to_string())
                .unwrap_or_default();
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_install_repo(&mut st, &repo);
            }
            Variant::nil()
        });
        install.connect("pressed", &callable);
    }
    repo_row.add_child(&install);
    {
        let mut bound = root.bind_mut();
        bound.add(&repo_row).grow_x().pad(4.0);
        bound.row();
    }

    // code-instantiated: the transient status line (`@loading`/errors/percent).
    let mut status = factory::label("");
    status.set_visible(false);
    {
        let mut bound = root.bind_mut();
        bound.add(&status).grow_x().pad(2.0);
        bound.row();
    }

    // code-instantiated: the listing rows are data-driven from `mods.json`.
    let mut list = factory::table();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    list.set_v_size_flags(SizeFlags::EXPAND_FILL);
    {
        let mut bound = root.bind_mut();
        bound.add(&list).grow().pad(4.0);
    }

    let mut st = state.borrow_mut();
    st.search = Some(search);
    st.repo_field = Some(repo_field);
    st.sort_button = Some(sort_button);
    st.status = Some(status);
    st.list = Some(list);
}

/// `_connect_mods()`: the import-progress percent on the status line.
fn browser_connect(dialog: &mut MindDialog, state: &Rc<RefCell<BrowserState>>) {
    let mut node = dialog.to_gd();
    if node.has_meta(BROWSER_SIGNALS_META) {
        return;
    }
    if let Some(mut mods) = mods_node()
        && mods.has_signal("mod_import_progress")
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_import_progress", move |args: &[&Variant]| {
            let ratio = args
                .get(1)
                .and_then(|value| value.try_to::<f32>().ok())
                .unwrap_or(0.0);
            let slot = slot.clone();
            defer_action("browser_import_progress_now", move || {
                if let Ok(st) = slot.try_borrow() {
                    browser_on_import_progress(&st, ratio);
                }
            });
            Variant::nil()
        });
        mods.connect("mod_import_progress", &callable);
    }
    node.set_meta(BROWSER_SIGNALS_META, &true.to_variant());
}

/// The `install_repos(items)` payload a mods-dialog handoff left behind.
fn browser_take_install_meta(node: &Gd<MindDialog>) -> Vec<String> {
    if !node.has_meta(BROWSER_INSTALL_META) {
        return Vec::new();
    }
    let mut node = node.clone();
    let value = node.get_meta(BROWSER_INSTALL_META);
    node.remove_meta(BROWSER_INSTALL_META);
    string_list(Some(&value))
}

/// `_fetch_listings()`.
fn browser_fetch_listings(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState) {
    if !st.listings.is_empty() {
        browser_rebuild(state, st);
        return;
    }
    if st.fetching {
        return;
    }
    st.fetching = true;
    browser_set_status(st, &bt("@loading"));
    browser_fetch_url_index(state, st, 0);
}

/// `_fetch_url_index(index)`.
fn browser_fetch_url_index(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState, index: usize) {
    if index >= MOD_JSON_URLS.len() {
        st.fetching = false;
        browser_set_status(st, &fmt_key("@connectfail", &["mods.json"]));
        browser_rebuild(state, st);
        return;
    }
    st.pending_url_index = index;
    let url = MOD_JSON_URLS[index];
    if !matches!(
        browser_request(st, HttpSlot::Listings, url),
        Some(godot::global::Error::OK)
    ) {
        browser_fetch_url_index(state, st, index + 1);
    }
}

/// `_on_list_completed`.
fn browser_on_list_completed(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    result: i64,
    code: i64,
    body: &PackedByteArray,
) {
    if result == HTTP_RESULT_SUCCESS && code == 200 {
        let parsed = Json::parse_string(&body.get_string_from_utf8());
        if let Ok(raw) = parsed.try_to::<VarArray>() {
            browser_parse_listings(state, st, &raw);
            return;
        }
    }
    let next = st.pending_url_index + 1;
    browser_fetch_url_index(state, st, next);
}

/// Normalizes each raw listing through `mind-core`'s `ModListing` (camelCase +
/// defaults) so the dialog never re-implements the listing model.
fn browser_parse_listings(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    raw: &VarArray,
) {
    st.fetching = false;
    st.listings.clear();
    let mut mods = mods_node();
    for entry in raw.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let mut listing = entry.clone();
        if let Some(mods) = mods.as_mut()
            && mods.has_method("parse_listing")
        {
            let parsed = mods
                .call(
                    "parse_listing",
                    &[Json::stringify(&entry.to_variant()).to_variant()],
                )
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            if dict_bool(&parsed, "ok", false)
                && let Some(value) = parsed.get("listing")
                && let Ok(row) = value.try_to::<VarDictionary>()
            {
                listing = row;
            }
        }
        st.listings.push(listing);
    }
    if let Some(mut status) = st.status.clone()
        && status.is_instance_valid()
    {
        status.set_visible(false);
    }
    browser_refresh_installed(st);
    browser_rebuild(state, st);
    // A queued dependency install waits for the listings to resolve names.
    if !st.pending_install.is_empty() {
        let pending = std::mem::take(&mut st.pending_install);
        browser_install_repos(state, st, &pending);
    }
}

/// `_refresh_installed()`.
fn browser_refresh_installed(st: &mut BrowserState) {
    st.installed_repos.clear();
    if let Some(mut mods) = mods_node()
        && mods.has_method("list")
        && mods.has_method("details")
    {
        let value = mods.call("list", &[]);
        if let Ok(rows) = value.try_to::<VarArray>() {
            for entry in rows.iter_shared() {
                let Ok(entry) = entry.try_to::<VarDictionary>() else {
                    continue;
                };
                let mod_name = dict_string(&entry, "name");
                let details = mods
                    .call("details", &[GString::from(mod_name.as_str()).to_variant()])
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let repo = dict_string(&details, "repo");
                if !repo.is_empty() {
                    st.installed_repos.push((repo, mod_name));
                }
            }
        }
    }
    browser_publish_updates(st);
}

/// Pushes `internal name -> remote version` to the mods dialog (`withUpdates`).
fn browser_publish_updates(st: &mut BrowserState) {
    let Some(mods_dialog) = dialog_node("mods") else {
        return;
    };
    let mut updates = VarDictionary::new();
    for listing in st.listings.clone() {
        let repo = dict_string(&listing, "repo");
        if repo.is_empty() {
            continue;
        }
        let Some(mod_name) = st
            .installed_repos
            .iter()
            .find(|(installed, _)| *installed == repo)
            .map(|(_, name)| name.clone())
        else {
            continue;
        };
        let details = mods_details(&mod_name);
        let latest = browser_latest_version(st, &listing);
        if !latest.is_empty() && browser_version_newer(&latest, &dict_string(&details, "version")) {
            updates.set(mod_name.as_str(), &latest.to_variant());
        }
    }
    let mut mods_dialog = mods_dialog;
    mods_dialog.set_meta(MODS_UPDATES_META, &updates.to_variant());
    refresh_dialog_deferred(&mods_dialog);
}

/// Version that would be installed: the release matching this build, else the
/// listing's latest (`refreshModUpdates` uses the matching release's version).
fn browser_latest_version(st: &BrowserState, listing: &VarDictionary) -> String {
    if let Some(release) = browser_matching_release(st, listing) {
        let version = dict_string(&release, "version");
        if !version.is_empty() {
            return version;
        }
    }
    dict_string(listing, "version")
}

/// Numeric comparison of the dot-separated version prefix (`checkNewerSemver`
/// intent); `"1.2.1"` is newer than `"1.2"`, an equal prefix is not newer.
fn browser_version_newer(newer: &str, older: &str) -> bool {
    if newer == older {
        return false;
    }
    let left = browser_version_parts(newer);
    let right = browser_version_parts(older);
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// `_version_parts(version)`.
fn browser_version_parts(version: &str) -> Vec<i64> {
    let mut parts = Vec::new();
    for chunk in version.trim().split('.') {
        if chunk.is_empty() {
            continue;
        }
        let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
        parts.push(digits.parse().unwrap_or(0));
    }
    parts
}

/// `_rebuild()`.
fn browser_rebuild(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    if !list.is_instance_valid() {
        return;
    }
    list.bind_mut().clear();
    if st.listings.is_empty() {
        let mut bound = list.bind_mut();
        bound.add(&factory::label(&bt("@mods.none"))).pad(8.0);
        return;
    }
    let query = st
        .search
        .clone()
        .filter(|search| search.is_instance_valid())
        .map(|search| search.get_text().to_string().to_lowercase())
        .unwrap_or_default();
    let mut visible: Vec<VarDictionary> = Vec::new();
    for listing in &st.listings {
        if query.is_empty()
            || dict_string(listing, "name").to_lowercase().contains(&query)
            || dict_string(listing, "repo").to_lowercase().contains(&query)
        {
            visible.push(listing.clone());
        }
    }
    if st.order_date {
        visible.sort_by_key(|listing| Reverse(dict_string(listing, "last_updated")));
    } else {
        visible.sort_by_key(|listing| Reverse(dict_i64(listing, "stars", 0)));
    }
    if visible.is_empty() {
        let mut bound = list.bind_mut();
        bound.add(&factory::label(&bt("@none.found"))).pad(8.0);
        return;
    }
    for listing in visible {
        browser_add_listing_row(state, st, &listing);
    }
}

/// `_add_listing_row(listing)`.
fn browser_add_listing_row(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    listing: &VarDictionary,
) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    let repo = dict_string(listing, "repo");
    let name = dict_string(listing, "name");
    let mod_name = if name.is_empty() { repo.clone() } else { name };
    // code-instantiated: browser rows are data-driven from the fetched listings.
    let mut row = HBoxContainer::new_alloc();
    row.set_h_size_flags(SizeFlags::EXPAND_FILL);

    // code-instantiated: the info card button (name/version/stars block).
    let mut info = Button::new_alloc();
    info.set_theme_type_variation("flatBordert");
    info.set_text_alignment(HorizontalAlignment::LEFT);
    info.set_h_size_flags(SizeFlags::EXPAND_FILL);
    info.set_text(&format!(
        "{}\n{} {}  \u{2605} {}",
        mod_name,
        bt("@mods.browser.latest"),
        dict_string(listing, "version"),
        dict_i64(listing, "stars", 0),
    ));
    {
        let slot = state.clone();
        let repo = repo.clone();
        let callable = Callable::from_fn("browser_toggle_repo", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                st.open_repo = if st.open_repo == repo {
                    String::new()
                } else {
                    repo.clone()
                };
                browser_rebuild(&slot, &mut st);
            }
            Variant::nil()
        });
        info.connect("pressed", &callable);
    }
    row.add_child(&info);

    let installed = st
        .installed_repos
        .iter()
        .any(|(installed, _)| *installed == repo);
    // code-instantiated: the per-listing add/reinstall button.
    let mut add = factory::button(&if installed {
        bt("@mods.browser.reinstall")
    } else {
        bt("@mods.browser.add")
    });
    {
        let slot = state.clone();
        let repo = repo.clone();
        let callable = Callable::from_fn("browser_install_row", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let repo = repo.clone();
            defer_action("browser_install_row_now", move || {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    browser_install_repo(&mut st, &repo);
                }
            });
            Variant::nil()
        });
        add.connect("pressed", &callable);
    }
    row.add_child(&add);

    {
        let mut bound = list.bind_mut();
        bound.add(&row).grow_x().pad(3.0);
        bound.row();
    }
    if st.open_repo == repo {
        browser_add_listing_details(state, st, listing);
    }
}

/// `_add_listing_details(listing)`.
fn browser_add_listing_details(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    listing: &VarDictionary,
) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    // code-instantiated: details come from the fetched listing (GitHub model).
    let mut box_ = VBoxContainer::new_alloc();
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let author = dict_string(listing, "author");
    if !author.is_empty() {
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@editor.author"),
            author
        )));
    }
    let description = dict_string(listing, "description");
    if !description.is_empty() {
        let mut description_label = factory::label(&description);
        description_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        box_.add_child(&description_label);
    }
    let min_version = {
        let value = dict_string(listing, "min_game_version");
        if value.is_empty() {
            dict_string(listing, "minGameVersion")
        } else {
            value
        }
    };
    if !min_version.is_empty() {
        box_.add_child(&factory::label(&fmt_key(
            "@mod.requiresversion",
            &[min_version.as_str()],
        )));
    }
    let tags = string_list(listing.get("tags").as_ref());
    if !tags.is_empty() {
        box_.add_child(&factory::label(&tags.join(", ")));
    }

    if let Some(release) = browser_matching_release(st, listing) {
        let version = {
            let value = dict_string(&release, "version");
            if value.is_empty() {
                dict_string(&release, "id")
            } else {
                value
            }
        };
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@mods.browser.releases"),
            version
        )));
    }
    let repo = dict_string(listing, "repo");
    // code-instantiated: per-listing action row.
    let mut actions = HBoxContainer::new_alloc();
    let mut open_repo = factory::button(&bt("@mods.github.open"));
    {
        let url = format!("https://github.com/{repo}");
        let callable = Callable::from_fn("browser_open_repo", move |_args: &[&Variant]| {
            mods_open_url(&url);
            Variant::nil()
        });
        open_repo.connect("pressed", &callable);
    }
    actions.add_child(&open_repo);
    let mut add = factory::button(&bt("@mods.browser.add"));
    {
        let slot = state.clone();
        let repo = repo.clone();
        let callable = Callable::from_fn("browser_install_details", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let repo = repo.clone();
            defer_action("browser_install_details_now", move || {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    browser_install_repo(&mut st, &repo);
                }
            });
            Variant::nil()
        });
        add.connect("pressed", &callable);
    }
    actions.add_child(&add);
    box_.add_child(&actions);
    {
        let mut bound = list.bind_mut();
        bound.add(&box_).grow_x().pad(6.0);
        bound.row();
    }
}

/// `ModListing.getMatchingRelease` via `mind-core`.
fn browser_matching_release(st: &BrowserState, listing: &VarDictionary) -> Option<VarDictionary> {
    let mut mods = mods_node()?;
    if !mods.has_method("matching_release") {
        return None;
    }
    let result = mods
        .call(
            "matching_release",
            &[
                Json::stringify(&listing.to_variant()).to_variant(),
                st.game_build.to_variant(),
                st.game_revision.to_variant(),
            ],
        )
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if dict_bool(&result, "found", false) {
        Some(result)
    } else {
        None
    }
}

/// `_toggle_order()`.
fn browser_toggle_order(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState) {
    st.order_date = !st.order_date;
    if let Some(mut button) = st.sort_button.clone()
        && button.is_instance_valid()
    {
        button.set_tooltip_text(&if st.order_date {
            bt("@mods.browser.sortdate")
        } else {
            bt("@mods.browser.sortstars")
        });
    }
    browser_rebuild(state, st);
}

/// `install_repos(items)`.
fn browser_install_repos(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    items: &[String],
) {
    let mut deferred: Vec<String> = Vec::new();
    for item in items {
        let repo = browser_repo_for(st, item);
        if repo.is_empty() {
            deferred.push(item.clone());
        } else {
            st.queue.push(repo);
        }
    }
    if !deferred.is_empty() {
        st.pending_install = deferred;
        browser_fetch_listings(state, st);
    }
    if !st.queue.is_empty() {
        browser_order_next(st);
    }
}

/// Resolves an `owner/repo` string or an internal mod name via the listings.
fn browser_repo_for(st: &BrowserState, item: &str) -> String {
    let cleaned = item.trim();
    if cleaned.contains('/') {
        return cleaned.to_string();
    }
    for listing in &st.listings {
        let internal = {
            let value = dict_string(listing, "internal_name");
            if value.is_empty() {
                dict_string(listing, "internalName")
            } else {
                value
            }
        };
        if internal == cleaned {
            return dict_string(listing, "repo");
        }
    }
    String::new()
}

/// Listing for `repo` (case-insensitive), or `None` when absent.
fn browser_find_listing(st: &BrowserState, repo: &str) -> Option<VarDictionary> {
    st.listings
        .iter()
        .find(|listing| dict_string(listing, "repo").to_lowercase() == repo.to_lowercase())
        .cloned()
}

/// `_install_repo(repo)`: downloads the matching zipball then imports it.
fn browser_install_repo(st: &mut BrowserState, repo: &str) {
    let mut cleaned = repo.trim().replace(' ', "");
    if let Some(rest) = cleaned.strip_prefix("https://github.com/") {
        cleaned = rest.to_string();
    }
    cleaned = cleaned
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string();
    if cleaned.is_empty() || !cleaned.contains('/') {
        return;
    }
    st.pending_repo = cleaned.clone();
    st.pending_release = String::new();
    if let Some(listing) = browser_find_listing(st, &cleaned)
        && let Some(release) = browser_matching_release(st, &listing)
    {
        st.pending_release = dict_string(&release, "id");
    }
    browser_set_status(st, &fmt_key("@mods.downloading", &[cleaned.as_str()]));
    if st.pending_release.is_empty() {
        // No version-specific release: resolve the default branch from the repo.
        let url = format!("{GH_API}/repos/{cleaned}");
        match browser_request(st, HttpSlot::Api, &url) {
            Some(godot::global::Error::OK) => {}
            Some(error) => browser_install_failed(st, &error.ord().to_string()),
            None => browser_install_failed(st, "no api request node"),
        }
        return;
    }
    browser_request_release_zipball(st, &cleaned);
}

/// Fetches the release object (`githubImportBranch` release path) for its
/// zipball.
fn browser_request_release_zipball(st: &mut BrowserState, repo: &str) {
    let url = format!("{GH_API}/repos/{repo}/releases/{}", st.pending_release);
    if !matches!(
        browser_request(st, HttpSlot::Release, &url),
        Some(godot::global::Error::OK)
    ) {
        browser_install_failed(st, &fmt_key("@connectfail", &["release"]));
    }
}

/// `_on_api_completed`.
fn browser_on_api_completed(st: &mut BrowserState, result: i64, code: i64, body: &PackedByteArray) {
    if result != HTTP_RESULT_SUCCESS || code != 200 {
        let message = fmt_key("@connectfail", &[code.to_string().as_str()]);
        browser_install_failed(st, &message);
        return;
    }
    let parsed = Json::parse_string(&body.get_string_from_utf8());
    let mut branch = String::from("master");
    if let Ok(dict) = parsed.try_to::<VarDictionary>() {
        let value = dict_string(&dict, "default_branch");
        if !value.is_empty() {
            branch = value;
        }
    }
    let url = format!("{GH_API}/repos/{}/zipball/{branch}", st.pending_repo);
    browser_request_zipball(st, &url);
}

/// `_on_release_completed`.
fn browser_on_release_completed(
    st: &mut BrowserState,
    result: i64,
    code: i64,
    body: &PackedByteArray,
) {
    if result != HTTP_RESULT_SUCCESS || code != 200 {
        // Release lookup failed; fall back to the default-branch zipball.
        st.pending_release = String::new();
        let url = format!("{GH_API}/repos/{}", st.pending_repo);
        match browser_request(st, HttpSlot::Api, &url) {
            Some(godot::global::Error::OK) => {}
            Some(error) => browser_install_failed(st, &error.ord().to_string()),
            None => browser_install_failed(st, "no api request node"),
        }
        return;
    }
    let parsed = Json::parse_string(&body.get_string_from_utf8());
    let zipball = parsed
        .try_to::<VarDictionary>()
        .map(|dict| dict_string(&dict, "zipball_url"))
        .unwrap_or_default();
    if zipball.is_empty() {
        browser_install_failed(st, &fmt_key("@connectfail", &["zipball"]));
        return;
    }
    browser_request_zipball(st, &zipball);
}

/// `_request_zipball(url)`.
fn browser_request_zipball(st: &mut BrowserState, url: &str) {
    if !matches!(
        browser_request(st, HttpSlot::Zip, url),
        Some(godot::global::Error::OK)
    ) {
        browser_install_failed(st, &fmt_key("@connectfail", &["zipball"]));
        return;
    }
    st.downloading = true;
    let text = format!(
        "{} 0%",
        fmt_key("@mods.downloading", &[st.pending_repo.as_str()])
    );
    browser_set_status(st, &text);
    if let Some(mut timer) = st.progress_timer.clone()
        && timer.is_instance_valid()
    {
        timer.start();
    }
}

/// `_on_zip_completed`.
fn browser_on_zip_completed(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    result: i64,
    code: i64,
    body: &PackedByteArray,
) {
    st.downloading = false;
    if let Some(mut timer) = st.progress_timer.clone()
        && timer.is_instance_valid()
    {
        timer.stop();
    }
    if result != HTTP_RESULT_SUCCESS || code != 200 {
        let message = fmt_key("@connectfail", &[code.to_string().as_str()]);
        browser_install_failed(st, &message);
        return;
    }
    let cache = ProjectSettings::singleton().globalize_path(CACHE_PATH);
    let _ = DirAccess::make_dir_recursive_absolute(&cache);
    let path = format!("{CACHE_PATH}/{}.zip", st.pending_repo.replace('/', "_"));
    let global = ProjectSettings::singleton().globalize_path(path.as_str());
    let Some(mut file) = FileAccess::open(&global, godot::classes::file_access::ModeFlags::WRITE)
    else {
        browser_install_failed(st, &bt("@mod.delete.error"));
        return;
    };
    file.store_buffer(body);
    file.close();
    let mut imported = VarDictionary::new();
    if let Some(mut mods) = mods_node()
        && mods.has_method("import_mod")
    {
        imported = mods
            .call("import_mod", &[GString::from(path.as_str()).to_variant()])
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        if !dict_bool(&imported, "ok", false) {
            // Duplicate install (reinstall): drop the existing copy and retry once.
            if !st
                .installed_repos
                .iter()
                .any(|(repo, _)| *repo == st.pending_repo)
            {
                browser_refresh_installed(st);
            }
            let existing = st
                .installed_repos
                .iter()
                .find(|(repo, _)| *repo == st.pending_repo)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            if !existing.is_empty() && mods.has_method("remove_mod") {
                let _ = mods.call(
                    "remove_mod",
                    &[GString::from(existing.as_str()).to_variant()],
                );
                imported = mods
                    .call("import_mod", &[GString::from(path.as_str()).to_variant()])
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
            }
        }
    }
    let _ = DirAccess::remove_absolute(&global);
    if !dict_bool(&imported, "ok", false) {
        let error = dict_string(&imported, "error");
        browser_install_failed(
            st,
            &if error.is_empty() {
                bt("@mod.delete.error")
            } else {
                error
            },
        );
        return;
    }
    if let Some(mut status) = st.status.clone()
        && status.is_instance_valid()
    {
        status.set_visible(false);
    }
    browser_refresh_installed(st);
    browser_rebuild(state, st);
    browser_order_next(st);
}

/// `_install_failed(message)`.
fn browser_install_failed(st: &mut BrowserState, message: &str) {
    st.downloading = false;
    if let Some(mut timer) = st.progress_timer.clone()
        && timer.is_instance_valid()
    {
        timer.stop();
    }
    browser_set_status(st, message);
    let _ = ui_call("show_info", &[GString::from(message).to_variant()]);
    browser_order_next(st);
}

/// `_order_next()`.
fn browser_order_next(st: &mut BrowserState) {
    if st.queue.is_empty() {
        return;
    }
    let next = st.queue.remove(0);
    browser_install_repo(st, &next);
}

/// `_on_import_progress(path, ratio)`.
fn browser_on_import_progress(st: &BrowserState, ratio: f32) {
    let Some(mut status) = st.status.clone() else {
        return;
    };
    if !status.is_instance_valid() || !status.is_visible() {
        return;
    }
    let text = format!(
        "{} {}%",
        fmt_key("@mods.downloading", &[st.pending_repo.as_str()]),
        (ratio * 100.0) as i64
    );
    status.set_text(&GString::from(text.as_str()));
}

/// `_process`: streams the real `HTTPRequest` byte counters while the zipball
/// downloads (timer-driven; see the module docs).
fn browser_progress_tick(st: &mut BrowserState) {
    if !st.downloading {
        return;
    }
    let Some(zip_http) = st.zip_http.clone() else {
        return;
    };
    if !zip_http.is_instance_valid() {
        return;
    }
    let total = zip_http.get_body_size();
    if total <= 0 {
        return;
    }
    let downloaded = zip_http.get_downloaded_bytes();
    let percent = (f64::from(downloaded) / f64::from(total) * 100.0) as i64;
    let text = format!(
        "{} {percent}%",
        fmt_key("@mods.downloading", &[st.pending_repo.as_str()])
    );
    browser_set_status(st, &text);
}

/// The status line (`_status.text = …; _status.visible = true`).
fn browser_set_status(st: &mut BrowserState, text: &str) {
    if let Some(mut status) = st.status.clone()
        && status.is_instance_valid()
    {
        status.set_text(&GString::from(text));
        status.set_visible(true);
    }
}

/// Sends one request on `slot` (`None` when the node is missing).
fn browser_request(
    st: &mut BrowserState,
    slot: HttpSlot,
    url: &str,
) -> Option<godot::global::Error> {
    let headers = http_headers();
    let http = match slot {
        HttpSlot::Listings => st.list_http.as_mut(),
        HttpSlot::Api => st.api_http.as_mut(),
        HttpSlot::Release => st.release_http.as_mut(),
        HttpSlot::Zip => st.zip_http.as_mut(),
    }?;
    Some(http.request_ex(url).custom_headers(&headers).done())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_is_numeric_per_component() {
        assert!(browser_version_newer("1.2.1", "1.2"));
        assert!(!browser_version_newer("1.2", "1.2.1"));
        assert!(!browser_version_newer("1.2", "1.2"));
        assert!(!browser_version_newer("2.0", "10.0"));
    }

    #[test]
    fn version_parts_ignore_non_numeric_suffixes() {
        assert_eq!(browser_version_parts("1.2.3-beta"), vec![1, 2, 3]);
        assert_eq!(browser_version_parts(""), Vec::<i64>::new());
    }
}
