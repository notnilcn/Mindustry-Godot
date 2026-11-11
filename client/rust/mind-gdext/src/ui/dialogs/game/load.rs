// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/LoadDialog.java,
//         core/src/mindustry/ui/dialogs/SaveDialog.java.

//! The load and save dialog controllers: the search/filter row, the save-slot
//! card grid and the delete/rename/export/import actions.
//! Upstream: ui/dialogs/LoadDialog.java, SaveDialog.java

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::StretchMode;
use godot::classes::{
    Button, Control, HBoxContainer, Json, LineEdit, Panel, PanelContainer, ProjectSettings,
    RichTextLabel, StyleBoxFlat, Texture2D, Time, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use mind_core::io::{FileSystem, NativeFs, SaveIo};

use crate::gdutil::variant::{
    dict_bool_or, dict_i64_or, dict_text_or, to_bool_lenient, to_i64_lenient, to_text_lenient,
};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::common::defer::defer_action;
use crate::ui::dialogs::game::util::{
    campaign_call, dialog_node, format_millis, open_named_dialog, ui_show_confirm,
};
use crate::ui::dialogs::net::util::{
    campaign_bool, clear_table_nodes, close_named_dialog, emit_dialog_signal, file_basename,
    generation_live, hide_dialog_node, install_ui_signal, next_generation, ui_show_error,
    ui_show_text_input,
};
use crate::ui::kit::widgets::border_image::MindBorderImage;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, Align, MindTable, styles};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "load" => Some(Box::new(LoadDialogController::default())),
        // `SaveDialog extends LoadDialog`: same listing, save-mode card action.
        "save" => Some(Box::new(LoadDialogController::save())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// LoadDialogController (manifest ids `load` and `save`)
// ---------------------------------------------------------------------------

/// Gamemode filter toggles (`Gamemode.all`; sandbox uses the terrain glyph).
/// `for(Gamemode mode : Gamemode.all)` (`LoadDialog.java:66-77`) in enum order
/// (`Gamemode.java:10-27`: survival, sandbox, attack, pvp; editor is hidden).
const LOAD_FILTERS: [(&str, &str); 4] = [
    ("survival", "mode-survival"),
    ("sandbox", "terrain"),
    ("attack", "mode-attack"),
    ("pvp", "mode-pvp"),
];
/// Per-controller generation (`ready` re-runs on an extension reload).
const LOAD_GENERATION_META: &str = "_mind_batch_h_load_generation";
/// The installed lasting `MindUi`/file-chooser handlers.
const LOAD_CONFIRM_HOOK: &str = "_mind_batch_h_load_confirm_hook";
const LOAD_TEXT_INPUT_HOOK: &str = "_mind_batch_h_load_text_input_hook";
const LOAD_CHOOSER_HOOK: &str = "_mind_batch_h_load_chooser_hook";

/// Which slot browser a [`LoadDialogController`] drives.
///
/// `SaveDialog extends LoadDialog` upstream: the save view keeps the search
/// row, the gamemode filters, the card grid and the per-card actions, and only
/// swaps the setup button (`@save.new` for `@save.import`) and the card-body
/// action (overwrite prompt for load-and-close). One controller with a mode
/// flag mirrors that inheritance without duplicating the listing code.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadKind {
    /// `load` (`LoadDialog.java`).
    Load,
    /// `save` (`SaveDialog.java`).
    Save,
}

/// `LoadDialog` state: the live slot snapshot, the search/listing filters and
/// the pending card-action prompt targets.
struct LoadState {
    generation: i64,
    /// `LoadDialog` vs `SaveDialog` ([`LoadKind`]).
    kind: LoadKind,
    field: Option<Gd<LineEdit>>,
    grid: Option<Gd<MindTable>>,
    /// Gamemode names whose saves are hidden from the grid (`hidden` upstream).
    hidden: HashSet<String>,
    /// Slot rows for the current open (`Saves.getSaveSlots` snapshot).
    slots: VarArray,
    /// Registered map names (`SaveMeta` map lookup), filled lazily.
    map_names: HashSet<String>,
    pending_delete: String,
    pending_rename: String,
    pending_export: String,
    /// `SaveDialog._pending_slot`: the slot awaiting `@save.overwrite`.
    pending_save: String,
    /// `SaveDialog._request_new`: the next text-input answer names a new slot.
    pending_new_name: bool,
    /// The slot whose pre-flight load check failed; the grid renders the
    /// `@save.corrupted` state with a retry/back choice instead of loading it
    /// (`LoadDialog.java:225-265`, which closes and shows the error modal).
    failed_slot: Option<VarDictionary>,
    /// Human-readable failure detail (missing path / unreadable file).
    failed_detail: String,
}

impl Default for LoadState {
    fn default() -> Self {
        Self {
            generation: 0,
            kind: LoadKind::Load,
            field: None,
            grid: None,
            hidden: HashSet::new(),
            slots: VarArray::new(),
            map_names: HashSet::new(),
            pending_delete: String::new(),
            pending_rename: String::new(),
            pending_export: String::new(),
            pending_save: String::new(),
            pending_new_name: false,
            failed_slot: None,
            failed_detail: String::new(),
        }
    }
}

/// `LoadDialog` — the "Load Game" slot browser (`load`) and its `SaveDialog`
/// subclass view (`save`).
///
/// The listing comes from the live `Saves` store
/// (`MindCampaign.list_save_slots`) with the `slots` context taking precedence
/// when supplied; with no slots the grid renders the empty state
/// (`@save.none`). On `load`, clicking a card body emits `slot_selected(name)`,
/// queues the slot load on the live facade and closes. On `save` the body asks
/// `@save.overwrite` first, then emits `save_requested(slot)` and closes.
pub struct LoadDialogController {
    state: Rc<RefCell<LoadState>>,
}

impl Default for LoadDialogController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LoadState::default())),
        }
    }
}

impl LoadDialogController {
    /// The `save` view (`SaveDialog extends LoadDialog`, `save_dialog.gd`).
    pub fn save() -> Self {
        Self {
            state: Rc::new(RefCell::new(LoadState {
                kind: LoadKind::Save,
                ..LoadState::default()
            })),
        }
    }
}

impl DialogController for LoadDialogController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `LoadDialog(String title)`; `SaveDialog` calls `super("@savegame")`
        // (`SaveDialog.java:13`).
        match self.state.borrow().kind {
            LoadKind::Load => dialog.set_title_key("@loadgame"),
            LoadKind::Save => dialog.set_title_key("@savegame"),
        }
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        let dialog_gd = dialog.to_gd();
        self.state.borrow_mut().generation = next_generation(&dialog_gd, LOAD_GENERATION_META);
        load_build(dialog, &self.state);
        load_connect_prompts(&dialog_gd, &self.state);
        load_connect_chooser(&dialog_gd, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        load_shown(dialog, &self.state);
    }
}

/// `LoadDialog._build`: search row, filter toggles, card grid, setup + back.
fn load_build(dialog: &mut MindDialog, state: &Rc<RefCell<LoadState>>) {
    dialog.clear_content();
    dialog.clear_buttons();
    let dialog_gd = dialog.to_gd();
    let mut root = dialog.content_table();

    // code-instantiated: the search row is data-driven from the gamemode list.
    // The Java `search` table has no cell padding (`LoadDialog.java:59-77`).
    let mut search_row = factory::hbox(0);
    // code-instantiated: the zoom glyph is an icon-font run with no static scene.
    let zoom = factory::glyph_label("zoom", 30, Color::WHITE);
    search_row.add_child(&zoom);
    let mut field = factory::field(&kit::t("@save.search").to_string());
    field.set_max_length(50);
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    // A Table cell centres the field in the 60 px row of mode buttons.
    field.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    {
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("load_search_changed", move |_args: &[&Variant]| {
            // `shown()` clears the field while the dialog is mutably borrowed;
            // re-binding it synchronously from `text_changed` would panic, so
            // the rebuild is queued (`PAR-0066` shape).
            load_rebuild_later(&dialog_for, &state_for);
            Variant::nil()
        });
        field.connect("text_changed", &callable);
    }
    search_row.add_child(&field);
    load_add_filters(&mut search_row, &dialog_gd, state);
    root.bind_mut().add(&search_row).grow_x();
    root.bind_mut().row();
    state.borrow_mut().field = Some(field);

    // code-instantiated: the save grid is a data-driven list of save slots.
    let grid = factory::table();
    // `cont.add(pane).growY()` (`LoadDialog.java:82`): the pane is as wide as
    // the slot table (`slots.marginRight(24).marginLeft(20f)`, `:88`) and
    // centred, and the ScrollPane stretches the table over its height,
    // centring a short list (L01 T3: java/load-search.png centres the card).
    root.bind_mut()
        .add(&grid)
        .grow_y()
        .pad_left(20.0)
        .pad_right(24.0);
    root.bind_mut().row();
    state.borrow_mut().grid = Some(grid);

    // Bottom row: `addCloseButton()` then the dialog's own setup button
    // (`LoadDialog.java:47-48,59-60`).
    dialog.add_default_close_button();
    load_add_setup_buttons(dialog, &dialog_gd, state);
    load_rebuild_list(dialog, state);
}

/// `LoadDialog.addSetup`: `@save.import` for `load`, `@save.new` for `save`
/// (`SaveDialog.addSetup`, `SaveDialog.java:19-27`).
fn load_add_setup_buttons(
    dialog: &mut MindDialog,
    dialog_gd: &Gd<MindDialog>,
    state: &Rc<RefCell<LoadState>>,
) {
    let kind = state.borrow().kind;
    if kind == LoadKind::Save {
        load_add_new_save_button(dialog, state);
        return;
    }
    let dialog_for = dialog_gd.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("load_open_import", move |_args: &[&Variant]| {
        load_open_import(&dialog_for, &state_for);
        Variant::nil()
    });
    let import_text = kit::t("@save.import").to_string();
    let mut import_button = dialog.add_button(&import_text, Some(callable), "add", 210.0);
    let mut size = import_button.get_custom_minimum_size();
    size.y = 64.0;
    import_button.set_custom_minimum_size(size);
}

/// `SaveDialog.addSetup`: `buttons.button("@save.new", Icon.add, …)` replaces
/// Load's `@save.import`; the GDScript port keeps the shared action-button
/// width and tags the node `new_save` for the live probes.
fn load_add_new_save_button(dialog: &mut MindDialog, state: &Rc<RefCell<LoadState>>) {
    let state_for = state.clone();
    let callable = Callable::from_fn("save_request_new", move |_args: &[&Variant]| {
        save_request_new(&state_for);
        Variant::nil()
    });
    let new_text = kit::t("@save.new").to_string();
    let mut new_button = dialog.add_button(&new_text, Some(callable), "add", 210.0);
    new_button.set_name("new_save");
    let mut size = new_button.get_custom_minimum_size();
    size.y = 64.0;
    new_button.set_custom_minimum_size(size);
}

/// `_add_filters`: the gamemode toggles beside the search field.
fn load_add_filters(
    row: &mut HBoxContainer,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LoadState>>,
) {
    // code-instantiated: filter toggles track the local hidden-gamemode set.
    for (mode, icon) in LOAD_FILTERS {
        let mut button = factory::icon_button(icon, "", "emptyTogglei", AlignmentMode::CENTER);
        button.set_toggle_mode(true);
        button.set_pressed(true);
        // `search.button(...).size(60f).padLeft(-12f)` (`LoadDialog.java:71-76`):
        // each button takes 48 px of the row (the -12 overlaps its neighbour),
        // which is what leaves Java's field its width.
        button.set_custom_minimum_size(Vector2::new(48.0, 60.0));
        button.set_tooltip_text(&kit::t(&format!("@mode.{mode}.name")));
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let mode = mode.to_string();
        let callable = Callable::from_fn("load_toggle_filter", move |_args: &[&Variant]| {
            load_toggle_filter(&dialog_for, &state_for, &mode);
            Variant::nil()
        });
        button.connect("pressed", &callable);
        row.add_child(&button);
    }
}

/// `_toggle_filter`: hide/show one gamemode and relist.
fn load_toggle_filter(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, mode: &str) {
    {
        let mut st = state.borrow_mut();
        if st.hidden.contains(mode) {
            st.hidden.remove(mode);
        } else {
            st.hidden.insert(mode.to_string());
        }
    }
    let mut node = dialog.clone();
    if !node.is_instance_valid() {
        return;
    }
    let mut bound = node.bind_mut();
    load_rebuild_list(&mut bound, state);
}

/// `shown`: reset the search string, re-read the live slot list and relist.
fn load_shown(dialog: &mut MindDialog, state: &Rc<RefCell<LoadState>>) {
    // Upstream `shown` resets the search string (the field was built once).
    {
        let st = state.borrow();
        if let Some(mut field) = st.field.clone()
            && !field.get_text().is_empty()
        {
            // The `text_changed` handler only queues its rebuild, so writing
            // here cannot re-enter the bound dialog.
            field.set_text(&GString::new());
        }
    }
    load_refresh_slots(dialog, state);
    load_rebuild_list(dialog, state);
    // `LoadDialog.show`: desktop focuses the search field (LoadDialog.java:277).
    let field = state.borrow().field.clone();
    if let Some(mut field) = field {
        field.call_deferred("grab_focus", &[]);
    }
}

/// Context slots (MCP/test injection) win; otherwise the live `Saves` listing.
fn load_refresh_slots(dialog: &mut MindDialog, state: &Rc<RefCell<LoadState>>) {
    if let Some(injected) = dialog.context_value("slots")
        && let Ok(array) = injected.try_to::<VarArray>()
    {
        state.borrow_mut().slots = array;
        return;
    }
    let slots = campaign_call("list_save_slots", &[])
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    state.borrow_mut().slots = slots;
}

/// Queues one `load_rebuild_list` for the idle phase (see [`defer_action`]).
fn load_rebuild_later(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    defer_action("load_search_rebuild", move || {
        if !dialog_for.is_instance_valid() {
            return;
        }
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, LOAD_GENERATION_META, generation) {
            return;
        }
        let mut node = dialog_for.clone();
        let mut bound = node.bind_mut();
        load_rebuild_list(&mut bound, &state_for);
    });
}

/// One slot row's `SaveSlot.getTimestamp()` (`meta.timestamp`); 0 for injected
/// rows without meta (the sort key, `LoadDialog.java:93`).
fn load_slot_timestamp(entry: &Variant) -> i64 {
    entry
        .try_to::<VarDictionary>()
        .map(|slot| dict_i64_or(&slot, "timestamp_ms", 0))
        .unwrap_or(0)
}

/// `LoadDialog.rebuild`: sort the slots newest-first and lay the cards out in
/// as many columns as the viewport allows.
fn load_rebuild_list(dialog: &mut MindDialog, state: &Rc<RefCell<LoadState>>) {
    let grid = state.borrow().grid.clone();
    let Some(mut grid) = grid else {
        return;
    };
    clear_table_nodes(&mut grid);
    let (slots, hidden, needle, failed_slot, failed_detail) = {
        let st = state.borrow();
        let slots: Vec<Variant> = st.slots.iter_shared().collect();
        let needle = st
            .field
            .clone()
            .map(|field| field.get_text().to_string().trim().to_lowercase())
            .unwrap_or_default();
        (
            slots,
            st.hidden.clone(),
            needle,
            st.failed_slot.clone(),
            st.failed_detail.clone(),
        )
    };
    // `array.sort((slot, other) -> -Long.compare(slot.getTimestamp(),
    // other.getTimestamp()))` — newest first (`LoadDialog.java:93`). The
    // stable sort keeps the listing order on exact timestamp ties.
    let mut slots = slots;
    slots.sort_by_key(|slot| std::cmp::Reverse(load_slot_timestamp(slot)));
    let columns = ((dialog.base().get_viewport_rect().size.x / 470.0) as i64).max(1);
    let dialog_gd = dialog.to_gd();
    // `catch(SaveException)` state (`LoadDialog.java:225-265`) above the grid:
    // the reason + retry/keep-playing choice, so a failed slot never closes the
    // browser (the port's deliberate deviation from Java's close-then-modal).
    if let Some(slot) = failed_slot {
        let panel = load_failure_panel(&dialog_gd, state, &slot, &failed_detail);
        grid.bind_mut()
            .add(&panel)
            .grow_x()
            .colspan(columns as i32)
            .pad(8.0);
        grid.bind_mut().row();
    }
    let mut any = false;
    let mut count = 0i64;
    for entry in &slots {
        let Ok(slot) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        // `SaveSlot.isHidden`: sector saves are hidden from the load list.
        if dict_bool_or(&slot, "sector", false) {
            continue;
        }
        let slot_name = dict_text_or(&slot, "name");
        if !needle.is_empty() && !slot_name.to_lowercase().contains(&needle) {
            continue;
        }
        let mode = dict_text_or(&slot, "mode");
        if !mode.is_empty() && hidden.contains(&mode) {
            continue;
        }
        any = true;
        // `slots.add(button).uniformX().fillX().pad(4).padRight(8f)`
        // (`LoadDialog.java:184`).
        let card = load_build_card(&dialog_gd, state, &slot);
        grid.bind_mut().add(&card).fill_x().pad(4.0).pad_right(8.0);
        count += 1;
        if count % columns == 0 {
            grid.bind_mut().row();
        }
    }
    if !any {
        // code-instantiated: the empty-state text is a data-driven grid cell.
        let mut none = factory::label(&kit::tm("@save.none").to_string());
        let mut rich: Gd<RichTextLabel> = none.clone().upcast();
        rich.set_horizontal_alignment(HorizontalAlignment::CENTER);
        none.set_mouse_filter(MouseFilter::IGNORE);
        grid.bind_mut()
            .add(&none)
            .grow_x()
            .grow_y()
            .pad(16.0)
            .align(Align::Center);
    }
}

/// `_build_card`: one save card (title/actions row, preview, metadata lines).
fn load_build_card(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LoadState>>,
    slot: &VarDictionary,
) -> Gd<Button> {
    let slot_name = dict_text_or(slot, "name");
    // code-instantiated: save cards are data-driven from the save-slot listing.
    let mut card = factory::button("");
    card.set_theme_type_variation("grayt");
    // `slots.add(button)...margin(10f)` around a 40 px title row
    // (`t.defaults().size(40f)`) and the 160 px preview + `padRight(6)` +
    // 260 px meta row (`LoadDialog.java:115-184`): a 446x220 card
    // (java/load.png, L01 T3).
    card.set_custom_minimum_size(Vector2::new(446.0, 220.0));
    {
        let dialog_for = dialog.clone();
        let slot_for = slot.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("load_card_pressed", move |_args: &[&Variant]| {
            load_card_pressed(&dialog_for, &state_for, &slot_for);
            Variant::nil()
        });
        card.connect("pressed", &callable);
    }

    // code-instantiated: card body follows the slot data; ignores clicks so the
    // card button itself handles selection.
    let mut content = VBoxContainer::new_alloc();
    content.set_mouse_filter(MouseFilter::IGNORE);
    content.add_theme_constant_override("separation", 0);
    let content_control: Gd<Control> = content.clone().upcast();
    card.add_child(&content_control);
    content.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    content.set_offset(Side::LEFT, 10.0);
    content.set_offset(Side::TOP, 10.0);
    content.set_offset(Side::RIGHT, -10.0);
    content.set_offset(Side::BOTTOM, -10.0);

    // code-instantiated: per-card title/action row (data-driven icon buttons).
    let mut title = HBoxContainer::new_alloc();
    title.set_mouse_filter(MouseFilter::IGNORE);
    content.add_child(&title);
    // `title.add("[accent]" + slot.getName()).left().growX().width(230f).wrap()`
    // (`LoadDialog.java:105-107`); the markup needs the BBCode translation.
    let name_markup = format!("[accent]{slot_name}");
    let mut name_label =
        factory::styled_label(&kit::markup_text(&name_markup).to_string(), "techLabel");
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    name_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut name_size = name_label.get_custom_minimum_size();
    name_size.x = 230.0;
    name_label.set_custom_minimum_size(name_size);
    name_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
    title.add_child(&name_label);
    // `modifyButton`: `Icon.save` autosave toggle, `Icon.trash` delete,
    // `Icon.pencil` rename, `Icon.export` (`LoadDialog.java:105-145`).
    let mut autosave = load_icon_action(
        "save",
        "emptyTogglei",
        dict_bool_or(slot, "autosave", false),
    );
    {
        let slot_for = slot.clone();
        let callable = Callable::from_fn("load_set_autosave", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| to_bool_lenient(value))
                .unwrap_or(false);
            load_set_slot_autosave(&slot_for, pressed);
            Variant::nil()
        });
        autosave.connect("toggled", &callable);
    }
    title.add_child(&autosave);
    let mut delete_button = load_icon_action("trash", "emptyi", false);
    {
        let state_for = state.clone();
        let slot_for = slot.clone();
        let callable = Callable::from_fn("load_delete_slot", move |_args: &[&Variant]| {
            load_delete_slot(&state_for, &slot_for);
            Variant::nil()
        });
        delete_button.connect("pressed", &callable);
    }
    title.add_child(&delete_button);
    let mut rename_button = load_icon_action("pencil", "emptyi", false);
    {
        let state_for = state.clone();
        let slot_for = slot.clone();
        let callable = Callable::from_fn("load_rename_slot", move |_args: &[&Variant]| {
            load_rename_slot(&state_for, &slot_for);
            Variant::nil()
        });
        rename_button.connect("pressed", &callable);
    }
    title.add_child(&rename_button);
    let mut export_button = load_icon_action("export", "emptyi", false);
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let slot_for = slot.clone();
        let callable = Callable::from_fn("load_export_slot", move |_args: &[&Variant]| {
            load_export_slot(&dialog_for, &state_for, &slot_for);
            Variant::nil()
        });
        export_button.connect("pressed", &callable);
    }
    title.add_child(&export_button);

    // code-instantiated: preview/meta row is per-slot presentation data.
    let mut card_body = HBoxContainer::new_alloc();
    card_body.set_mouse_filter(MouseFilter::IGNORE);
    card_body.add_theme_constant_override("separation", 6);
    card_body.set_v_size_flags(SizeFlags::EXPAND_FILL);
    content.add_child(&card_body);

    // `Saves.SaveSlot.previewTexture()`: the saved `<previews>/save_slot_<stem>.png`
    // (`Saves.java:359-370`); the `nomap` glyph stays until one exists.
    let texture = load_preview_texture(slot);
    if let Some(texture) = texture {
        // code-instantiated: save preview textures come from the live slot store.
        let mut image = MindBorderImage::new_alloc();
        image.set_texture(&texture);
        image.bind_mut().thickness = 3.0;
        image.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        image.set_custom_minimum_size(Vector2::new(160.0, 160.0));
        image.set_mouse_filter(MouseFilter::IGNORE);
        card_body.add_child(&image);
    } else {
        // code-instantiated: `nomap` placeholder glyph is an icon-font run.
        let mut preview = Panel::new_alloc();
        preview.set_mouse_filter(MouseFilter::IGNORE);
        preview.set_custom_minimum_size(Vector2::new(160.0, 160.0));
        card_body.add_child(&preview);
        let mut map_icon = factory::glyph_label("map", 40, Color::WHITE);
        map_icon.set_mouse_filter(MouseFilter::IGNORE);
        preview.add_child(&map_icon);
        map_icon.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    }

    // code-instantiated: metadata column lines come from the slot dictionary.
    let mut meta = VBoxContainer::new_alloc();
    meta.set_mouse_filter(MouseFilter::IGNORE);
    meta.set_h_size_flags(SizeFlags::EXPAND_FILL);
    // `button.table(meta -> ...).left().growX().width(260f)` (`LoadDialog.java:181`):
    // the cell is 260 wide (its rows default to 280 and overflow it), which
    // gives Java's ~446 px card (L01 T3, java/load.png).
    meta.set_custom_minimum_size(Vector2::new(260.0, 0.0));
    card_body.add_child(&meta);
    for line in load_meta_lines(state, slot) {
        let mut label = factory::label(&line);
        label.set_mouse_filter(MouseFilter::IGNORE);
        label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        meta.add_child(&label);
    }

    card
}

/// `_resolve_map`: the registered map whose name matches the save's `mapname`,
/// else `@unknown`.
fn load_resolve_map(map_name: &str, state: &Rc<RefCell<LoadState>>) -> String {
    if map_name.is_empty() {
        return kit::t("@unknown").to_string();
    }
    let needs_load = state.borrow().map_names.is_empty();
    if needs_load {
        let mut names = HashSet::new();
        if let Some(mut preview) = kit::node("/root/Spine/MindPreview")
            && preview.has_method("maps_list")
        {
            let listed = preview.call("maps_list", &[]);
            if let Ok(rows) = listed.try_to::<VarArray>() {
                for entry in rows.iter_shared() {
                    if let Ok(row) = entry.try_to::<VarDictionary>() {
                        let name = dict_text_or(&row, "name");
                        if !name.is_empty() {
                            names.insert(name);
                        }
                    }
                }
            }
        }
        state.borrow_mut().map_names = names;
    }
    let known = state.borrow().map_names.contains(map_name);
    if known {
        map_name.to_string()
    } else {
        kit::t("@unknown").to_string()
    }
}

/// `SaveSlot.getDate` (`DateFormat.getDateTimeInstance()`, en-US medium:
/// `Oct 12, 2026, 12:54:33 AM`) in the local time zone.
fn load_format_date(timestamp_ms: i64) -> String {
    let bias = Time::singleton()
        .get_time_zone_from_system()
        .get("bias")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(0)
        * 60;
    let dt = Time::singleton().get_datetime_dict_from_unix_time(timestamp_ms / 1000 + bias);
    let month = dt
        .get("month")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(1)
        .clamp(1, 12) as usize;
    let day = dt
        .get("day")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(1);
    let year = dt
        .get("year")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(0);
    let hour = dt
        .get("hour")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(0);
    let minute = dt
        .get("minute")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(0);
    let second = dt
        .get("second")
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(0);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut hour12 = hour % 12;
    if hour12 == 0 {
        hour12 = 12;
    }
    format!(
        "{} {}, {}, {}:{:02}:{:02} {}",
        months[month - 1],
        day,
        year,
        hour12,
        minute,
        second,
        if hour >= 12 { "PM" } else { "AM" }
    )
}

/// `Saves.SaveSlot.previewTexture()` for a slot row (`save_preview_texture`).
fn load_preview_texture(slot: &VarDictionary) -> Option<Gd<Texture2D>> {
    let file = dict_text_or(slot, "file");
    let mut stem = file_basename(&file);
    if stem.is_empty() {
        stem = dict_text_or(slot, "name");
    }
    if stem.is_empty() {
        return None;
    }
    campaign_call(
        "save_preview_texture",
        &[GString::from(stem.as_str()).to_variant()],
    )
    .and_then(|value| value.try_to::<Gd<Texture2D>>().ok())
}

/// `_icon_action`: a 40x40 icon-font card action button.
fn load_icon_action(icon_name: &str, style: &str, is_pressed: bool) -> Gd<Button> {
    // code-instantiated: card action buttons are per-slot (no static scene).
    let mut button = factory::icon_button(icon_name, "", style, AlignmentMode::CENTER);
    button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    if style == "emptyTogglei" {
        button.set_toggle_mode(true);
        button.set_pressed(is_pressed);
    }
    button
}

/// `_meta_lines`: the map/mode/wave/autosave/playtime/date lines.
fn load_meta_lines(state: &Rc<RefCell<LoadState>>, slot: &VarDictionary) -> Vec<String> {
    let mut map_name = dict_text_or(slot, "map");
    if map_name.is_empty() {
        map_name = load_resolve_map(&dict_text_or(slot, "map_name"), state);
    }
    let mode = dict_text_or(slot, "mode");
    let mode = if mode.is_empty() {
        mode
    } else {
        kit::t(&format!("@mode.{mode}.name")).to_string()
    };
    let wave = dict_i64_or(slot, "wave", 0);
    let autosave = dict_bool_or(slot, "autosave", false);
    let mut playtime = dict_text_or(slot, "playtime");
    if slot.contains_key("playtime_ms") {
        playtime = format_millis(dict_i64_or(slot, "playtime_ms", 0));
    }
    let mut date = dict_text_or(slot, "date");
    if date.is_empty() && dict_i64_or(slot, "timestamp_ms", 0) > 0 {
        date = load_format_date(dict_i64_or(slot, "timestamp_ms", 0));
    }
    // `String color = "[lightgray]"` (`LoadDialog.java:147`): every meta value is
    // light grey, the mode name stays white.
    let color = "[lightgray]";
    let map_value = format!("{color}{map_name}");
    let wave_value = format!("{color}{wave}");
    let autosave_value = format!("{color}{}", kit::t(if autosave { "@on" } else { "@off" }));
    let playtime_value = format!("{color}{playtime}");
    let mut lines = vec![
        kit::tm_format("@save.map", &[map_value.as_str()]).to_string(),
        format!(
            "{} /{}",
            mode,
            kit::tm_format("@save.wave", &[wave_value.as_str()])
        ),
        kit::tm_format("@save.autosave", &[autosave_value.as_str()]).to_string(),
        kit::tm_format("@save.playtime", &[playtime_value.as_str()]).to_string(),
    ];
    if !date.is_empty() {
        lines.push(kit::markup_text(&format!("{color}{date}")).to_string());
    }
    lines
}

/// Card body click.
///
/// `load`: emits `slot_selected(name)`, queues the load and closes.
/// `save`: asks `@save.overwrite` first (`SaveDialog.modifyButton`,
/// `SaveDialog.java:31-39`) and answers on `confirm_result`; the per-card
/// autosave/delete/rename/export buttons stay the `LoadDialog` ones.
fn load_card_pressed(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LoadState>>,
    slot: &VarDictionary,
) {
    if state.borrow().kind == LoadKind::Save {
        let stem = load_slot_stem(slot);
        if stem.is_empty() {
            return;
        }
        state.borrow_mut().pending_save = stem;
        ui_show_confirm(&kit::t("@save.overwrite").to_string());
        return;
    }
    load_attempt(dialog, state, slot);
}

/// A save slot's pre-flight state (`SaveIO.isSaveValid`/`File.exists`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveFileCheck {
    Ok,
    Missing,
    Invalid,
}

/// `LoadDialog.runLoadSave` (`LoadDialog.java:225-265`) with the port's
/// pre-flight validation: Java closes the browser, then catches `SaveException`
/// and shows `@save.corrupted` as a modal. Validating first keeps the browser
/// open so the `@save.corrupted` state can render in place with the
/// retry/keep-playing choice.
fn load_attempt(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, slot: &VarDictionary) {
    let check = load_check_slot(slot);
    if check != SaveFileCheck::Ok {
        let detail = load_failure_detail(slot, check);
        {
            let mut st = state.borrow_mut();
            st.failed_slot = Some(slot.clone());
            st.failed_detail = detail;
        }
        if dialog.is_instance_valid() {
            let mut node = dialog.clone();
            let mut bound = node.bind_mut();
            load_rebuild_list(&mut bound, state);
        }
        return;
    }
    {
        let mut st = state.borrow_mut();
        st.failed_slot = None;
        st.failed_detail.clear();
    }
    emit_dialog_signal(
        dialog,
        "slot_selected",
        &[GString::from(dict_text_or(slot, "name").as_str()).to_variant()],
    );
    load_load_slot(slot);
    load_close(dialog, LoadKind::Load);
    schedule_missing_asset_check();
}

/// `SaveIO.isSaveValid` (`LoadDialog.java:225-265`): the primary `.msav` (or
/// its backup) must parse far enough to read meta.
fn load_check_slot(slot: &VarDictionary) -> SaveFileCheck {
    let stem = load_slot_stem(slot);
    if stem.is_empty() {
        return SaveFileCheck::Missing;
    }
    let path = format!("user://saves/{stem}.msav");
    let native = ProjectSettings::singleton().globalize_path(&path);
    save_file_check(std::path::Path::new(native.to_string().as_str()))
}

/// Missing vs unreadable/corrupt for one `.msav` path (`SaveIo::is_save_valid`
/// accepts the backup next to it, like `SaveIO.isSaveValid`).
fn save_file_check(path: &std::path::Path) -> SaveFileCheck {
    if NativeFs.read(path).is_err() {
        return SaveFileCheck::Missing;
    }
    if SaveIo::is_save_valid(&NativeFs, path) {
        SaveFileCheck::Ok
    } else {
        SaveFileCheck::Invalid
    }
}

/// The light-gray failure line under `@save.corrupted`.
fn load_failure_detail(slot: &VarDictionary, check: SaveFileCheck) -> String {
    let stem = load_slot_stem(slot);
    let file = format!("user://saves/{stem}.msav");
    match check {
        SaveFileCheck::Ok => String::new(),
        SaveFileCheck::Missing => format!("save file not found: {file}"),
        SaveFileCheck::Invalid => format!("save file is unreadable or corrupt: {file}"),
    }
}

/// `@save.corrupted` failure state: the reason plus a retry (`@loadgame`) /
/// keep-playing (`@back`) choice, rendered above the slot grid.
fn load_failure_panel(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LoadState>>,
    slot: &VarDictionary,
    detail: &str,
) -> Gd<PanelContainer> {
    // code-instantiated: the failure text/actions are data-driven.
    let mut panel = PanelContainer::new_alloc();
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(styles::BLACK_3);
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        box_.set_content_margin(side, 12.0);
    }
    panel.add_theme_stylebox_override("panel", &box_);
    let mut column = factory::vbox(6);
    panel.add_child(&column);
    let reason_text =
        kit::markup_text(&format!("[accent]{}", kit::t("@save.corrupted"))).to_string();
    let reason = factory::label(&reason_text);
    {
        let mut rich: Gd<RichTextLabel> = reason.clone().upcast();
        rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        rich.set_horizontal_alignment(HorizontalAlignment::CENTER);
    }
    column.add_child(&reason);
    if !detail.is_empty() {
        let detail_text = kit::markup_text(&format!("[lightgray]({detail})")).to_string();
        let detail_label = factory::label(&detail_text);
        {
            let mut rich: Gd<RichTextLabel> = detail_label.clone().upcast();
            rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
            rich.set_horizontal_alignment(HorizontalAlignment::CENTER);
        }
        column.add_child(&detail_label);
    }
    let mut actions = HBoxContainer::new_alloc();
    actions.set_alignment(AlignmentMode::CENTER);
    actions.add_theme_constant_override("separation", 8);
    // Retry the same slot (`@loadgame` = the browser's own action label).
    let mut retry = factory::button(&kit::t("@loadgame").to_string());
    retry.set_custom_minimum_size(Vector2::new(180.0, 48.0));
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let slot_for = slot.clone();
        let callable = Callable::from_fn("load_failure_retry", move |_args: &[&Variant]| {
            load_attempt(&dialog_for, &state_for, &slot_for);
            Variant::nil()
        });
        retry.connect("pressed", &callable);
    }
    actions.add_child(&retry);
    // Keep playing: dismiss the failure and close the browser.
    let mut back = factory::button(&kit::t("@back").to_string());
    back.set_custom_minimum_size(Vector2::new(140.0, 48.0));
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("load_failure_back", move |_args: &[&Variant]| {
            {
                let mut st = state_for.borrow_mut();
                st.failed_slot = None;
                st.failed_detail.clear();
            }
            load_close(&dialog_for, LoadKind::Load);
            Variant::nil()
        });
        back.connect("pressed", &callable);
    }
    actions.add_child(&back);
    column.add_child(&actions);
    panel
}

/// `LoadDialog.runLoadSave`'s missing-asset branch (`LoadDialog.java:231-249`):
/// after the load applies, `state.data.getMissingAssets()` rows are listed in
/// the `@save.assets.missing` modal. The port has no save-scoped data manager,
/// so the check runs once the queued load completed and renders the global
/// manager's missing external assets inline (no collapser toggle).
fn schedule_missing_asset_check() {
    let Some(root) = kit::tree_root() else {
        return;
    };
    let mut tree = root.get_tree();
    let mut timer = tree.create_timer(0.5);
    let callable = Callable::from_fn("load_missing_assets_check", |_args: &[&Variant]| {
        report_missing_assets();
        Variant::nil()
    });
    let _ = timer.connect("timeout", &callable);
}

/// Shows `@save.assets.missing` with the `type: path` rows when the completed
/// load left external data assets without a cache file.
fn report_missing_assets() {
    let Some(mut host) = kit::node("/root/Spine/SimHost") else {
        return;
    };
    if host.has_method("io_pending")
        && host.call("io_pending", &[]).try_to::<i64>().unwrap_or(0) > 0
    {
        // The load has not applied yet; the next boot/map will re-check.
        return;
    }
    let Some(mut mods) = kit::node("/root/MindMods") else {
        return;
    };
    if !mods.has_method("missing_assets") {
        return;
    }
    let Ok(rows) = mods.call("missing_assets", &[]).try_to::<VarArray>() else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    let mut body = kit::tm("@save.assets.missing.info").to_string();
    for entry in rows.iter_shared() {
        let Ok(row) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let kind = dict_text_or(&row, "type");
        let path = dict_text_or(&row, "path");
        body.push_str(&format!("\n[accent]{kind}: [lightgray]{path}"));
    }
    if let Some(mut ui) = kit::ui()
        && ui.has_method("show_text")
    {
        let _ = ui.call(
            "show_text",
            &[
                kit::t("@save.assets.missing").to_variant(),
                GString::from(body.as_str()).to_variant(),
            ],
        );
    }
}

/// `SaveDialog._emit_save`: hand the slot to the live facade (which queues the
/// `.msav` write, PAR-0438) and close the dialog.
fn save_emit_and_close(dialog: &Gd<MindDialog>, stem: &str) {
    save_emit(dialog, stem);
    load_close(dialog, LoadKind::Save);
}

/// The `save_requested` seam: `_on_save_requested` queues
/// `MindCampaign.save_slot` (`save_dialog.gd:110-114`) and listeners observe the
/// slot for MCP/live probes.
fn save_emit(dialog: &Gd<MindDialog>, stem: &str) {
    if stem.is_empty() {
        return;
    }
    emit_dialog_signal(
        dialog,
        "save_requested",
        &[GString::from(stem).to_variant()],
    );
    campaign_call("save_slot", &[GString::from(stem).to_variant()]);
}

/// `SaveDialog._request_new`: `ui.showTextInput("@save", "@save.newslot", 30,
/// "", …)` (`SaveDialog.java:22-25`); the answer arrives on
/// `text_input_result`.
fn save_request_new(state: &Rc<RefCell<LoadState>>) {
    state.borrow_mut().pending_new_name = true;
    ui_show_text_input(
        &kit::t("@save").to_string(),
        &kit::t("@save.newslot").to_string(),
        30,
        "",
    );
}

/// `text -> ui.loadAnd("@saving", () -> control.saves.addSave(text))`
/// (`SaveDialog.java:22-27`).
fn save_on_new_name(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, text: &str) {
    let name = text.to_string();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let run = Callable::from_fn("save_add_save", move |_args: &[&Variant]| {
        save_add_save(&dialog_for, &state_for, &name);
        Variant::nil()
    });
    let Some(mut ui_root) = kit::node("/root/Spine/Ui/UiRoot") else {
        save_add_save(dialog, state, text);
        return;
    };
    if !ui_root.has_method("load_and") {
        save_add_save(dialog, state, text);
        return;
    }
    let text_arg = GString::from("@saving");
    let _ = ui_root.call("load_and", &[run.to_variant(), text_arg.to_variant()]);
}

/// `SaveDialog._add_save`: emit the request, then relist (the dialog stays
/// open, `Core.app.post(this::setup)` upstream).
fn save_add_save(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, text: &str) {
    if text.is_empty() || !dialog.is_instance_valid() {
        return;
    }
    save_emit(dialog, text);
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let mut bound = node.bind_mut();
    load_refresh_slots(&mut bound, state);
    load_rebuild_list(&mut bound, state);
}

/// `LoadDialog.runLoadSave`: hands the slot to the live facade, which queues the
/// load at the next IO boundary (`MindCampaign.load_slot`).
fn load_load_slot(slot: &VarDictionary) {
    let stem = load_slot_stem(slot);
    if stem.is_empty() {
        return;
    }
    campaign_call("load_slot", &[GString::from(stem.as_str()).to_variant()]);
}

/// The file stem used by the main `save_slot`/`load_slot` calls.
fn load_slot_stem(slot: &VarDictionary) -> String {
    let file = dict_text_or(slot, "file");
    let stem = file_basename(&file);
    if stem.is_empty() {
        dict_text_or(slot, "name")
    } else {
        stem
    }
}

/// `_set_slot_autosave`: `slot.setAutosave(!slot.isAutosave())`.
fn load_set_slot_autosave(slot: &VarDictionary, on: bool) {
    let stem = load_slot_stem(slot);
    if stem.is_empty() {
        return;
    }
    campaign_call(
        "set_slot_autosave",
        &[GString::from(stem.as_str()).to_variant(), on.to_variant()],
    );
}

/// `Icon.trash`: `ui.showConfirm("@save.delete.confirm", ...)`.
fn load_delete_slot(state: &Rc<RefCell<LoadState>>, slot: &VarDictionary) {
    state.borrow_mut().pending_delete = load_slot_stem(slot);
    ui_show_confirm(&kit::t("@save.delete.confirm").to_string());
}

/// `Icon.pencil`: `ui.showTextInput("@save.rename", "@save.rename.text", ...)`.
fn load_rename_slot(state: &Rc<RefCell<LoadState>>, slot: &VarDictionary) {
    state.borrow_mut().pending_rename = load_slot_stem(slot);
    ui_show_text_input(
        &kit::t("@save.rename").to_string(),
        &kit::t("@save.rename.text").to_string(),
        64,
        &dict_text_or(slot, "name"),
    );
}

/// `Icon.export`: `FileChooser.export(slot.getName(), "msav", slot::exportFile)`;
/// a save with embedded assets must be loaded first (`@save.export.needsload`).
fn load_export_slot(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, slot: &VarDictionary) {
    if dict_bool_or(slot, "external_assets", false) {
        ui_show_error(&kit::t("@save.export.needsload").to_string());
        return;
    }
    state.borrow_mut().pending_export = load_slot_stem(slot);
    let Some(ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    load_connect_chooser(dialog, state);
    open_named_dialog("file_chooser", &load_chooser_context(false));
}

/// `LoadDialog.addSetup`'s `@save.import`: `FileChooser.open("msav")` with the
/// `.msav` filter; the chosen file goes through `MindCampaign.import_save`.
fn load_open_import(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>) {
    let Some(ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    load_connect_chooser(dialog, state);
    open_named_dialog("file_chooser", &load_chooser_context(true));
}

/// The `file_chooser` context (`{"open": …, "extensions": ["msav"]}`).
fn load_chooser_context(is_open: bool) -> String {
    let mut context = VarDictionary::new();
    context.set("open", &is_open.to_variant());
    let mut extensions = VarArray::new();
    extensions.push(&GString::from("msav").to_variant());
    context.set("extensions", &extensions.to_variant());
    Json::stringify(&context.to_variant()).to_string()
}

/// `_connect_chooser`: route the chooser's `file_chosen` to this controller
/// generation (the hook is replaced, never stacked).
fn load_connect_chooser(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>) {
    let Some(mut chooser) = dialog_node("file_chooser") else {
        return;
    };
    if !chooser.has_signal("file_chosen") {
        return;
    }
    let mut node = dialog.clone();
    if node.has_meta(LOAD_CHOOSER_HOOK) {
        if let Ok(previous) = node.get_meta(LOAD_CHOOSER_HOOK).try_to::<Callable>()
            && chooser.is_connected("file_chosen", &previous)
        {
            chooser.disconnect("file_chosen", &previous);
        }
        node.remove_meta(LOAD_CHOOSER_HOOK);
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("load_file_chosen", move |args: &[&Variant]| {
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, LOAD_GENERATION_META, generation) {
            return Variant::nil();
        }
        let path = args
            .first()
            .and_then(|value| value.try_to::<GString>().ok())
            .unwrap_or_default()
            .to_string();
        load_on_file_chosen(&dialog_for, &state_for, &path);
        Variant::nil()
    });
    chooser.connect("file_chosen", &callable);
    node.set_meta(LOAD_CHOOSER_HOOK, &callable.to_variant());
}

/// `_on_file_chosen`: export result, else the `MindCampaign.import_save`
/// outcome (`SaveIO.isSaveValid` -> `getMeta` sector check -> `Saves.importSave`).
fn load_on_file_chosen(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, path: &str) {
    let pending_export = state.borrow().pending_export.clone();
    if !pending_export.is_empty() {
        state.borrow_mut().pending_export.clear();
        close_named_dialog("file_chooser");
        let exported = campaign_bool(
            "export_save_slot",
            &[
                GString::from(pending_export.as_str()).to_variant(),
                GString::from(path).to_variant(),
            ],
        );
        if !exported {
            ui_show_error(&kit::t("@editor.errorsave").to_string());
        }
        return;
    }
    close_named_dialog("file_chooser");
    let result = campaign_call("import_save", &[GString::from(path).to_variant()])
        .map(|value| to_text_lenient(&value))
        .unwrap_or_default();
    // `_close_chooser()` then the result branch; errors keep the dialog open.
    let imported = result.is_empty();
    if imported {
        let mut node = dialog.clone();
        if node.is_instance_valid() {
            let mut bound = node.bind_mut();
            load_refresh_slots(&mut bound, state);
        }
    }
    match result.as_str() {
        "" => {}
        "invalid" => ui_show_error(&kit::t("@save.import.invalid").to_string()),
        "nocampaign" => ui_show_error(&kit::t("@save.nocampaign").to_string()),
        other => {
            let message = other.strip_prefix("fail:").unwrap_or(other);
            ui_show_error(&kit::tm_format("@save.import.fail", &[message]).to_string());
        }
    }
    let mut node = dialog.clone();
    if node.is_instance_valid() {
        let mut bound = node.bind_mut();
        load_rebuild_list(&mut bound, state);
    }
}

/// `_connect_prompts`: the shared `MindUi` confirm/text-input answers.
fn load_connect_prompts(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>) {
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("load_confirm_result", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| to_bool_lenient(value))
                .unwrap_or(false);
            let generation = state_for.borrow().generation;
            if !generation_live(&dialog_for, LOAD_GENERATION_META, generation) {
                return Variant::nil();
            }
            let target = dialog_for.clone();
            let state_for_action = state_for.clone();
            defer_action("load_confirm_apply", move || {
                if !target.is_instance_valid() {
                    return;
                }
                let generation = state_for_action.borrow().generation;
                if !generation_live(&target, LOAD_GENERATION_META, generation) {
                    return;
                }
                load_on_confirm(&target, &state_for_action, confirmed);
            });
            Variant::nil()
        });
        install_ui_signal(dialog, "confirm_result", LOAD_CONFIRM_HOOK, &callable);
    }
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("load_text_input_result", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| value.to::<GString>().to_string())
                .unwrap_or_default();
            let generation = state_for.borrow().generation;
            if !generation_live(&dialog_for, LOAD_GENERATION_META, generation) {
                return Variant::nil();
            }
            let target = dialog_for.clone();
            let state_for_action = state_for.clone();
            defer_action("load_text_input_apply", move || {
                if !target.is_instance_valid() {
                    return;
                }
                let generation = state_for_action.borrow().generation;
                if !generation_live(&target, LOAD_GENERATION_META, generation) {
                    return;
                }
                load_on_text_input(&target, &state_for_action, &text);
            });
            Variant::nil()
        });
        install_ui_signal(dialog, "text_input_result", LOAD_TEXT_INPUT_HOOK, &callable);
    }
}

/// `_on_confirm`: answer the `@save.delete.confirm` prompt (both views) or the
/// `save` view's `@save.overwrite` prompt.
fn load_on_confirm(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, confirmed: bool) {
    // `SaveDialog` asks before overwriting a card (`save_dialog.gd:52-64`); the
    // pending slot is cleared whatever the answer so a cancel cannot fire a
    // later confirm.
    let pending_save = std::mem::take(&mut state.borrow_mut().pending_save);
    if !pending_save.is_empty() {
        if confirmed {
            save_emit_and_close(dialog, &pending_save);
        }
        return;
    }
    let stem = {
        let mut st = state.borrow_mut();
        if !confirmed || st.pending_delete.is_empty() {
            st.pending_delete.clear();
            return;
        }
        std::mem::take(&mut st.pending_delete)
    };
    let deleted = campaign_bool(
        "delete_save_slot",
        &[GString::from(stem.as_str()).to_variant()],
    );
    if !deleted {
        return;
    }
    let mut node = dialog.clone();
    if !node.is_instance_valid() {
        return;
    }
    let mut bound = node.bind_mut();
    load_refresh_slots(&mut bound, state);
    load_rebuild_list(&mut bound, state);
}

/// `_on_text_input`: answer the `save` view's `@save.newslot` prompt, else the
/// `@save.rename` prompt.
fn load_on_text_input(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LoadState>>, text: &str) {
    let naming_new = std::mem::take(&mut state.borrow_mut().pending_new_name);
    if naming_new {
        // `SaveDialog._on_name` (`save_dialog.gd:86-95`): an empty answer is a
        // cancel.
        if !text.is_empty() {
            save_on_new_name(dialog, state, text);
        }
        return;
    }
    let stem = {
        let mut st = state.borrow_mut();
        if st.pending_rename.is_empty() || text.trim().is_empty() {
            st.pending_rename.clear();
            return;
        }
        std::mem::take(&mut st.pending_rename)
    };
    let renamed = campaign_bool(
        "rename_save_slot",
        &[
            GString::from(stem.as_str()).to_variant(),
            GString::from(text).to_variant(),
        ],
    );
    if !renamed {
        return;
    }
    let mut node = dialog.clone();
    if !node.is_instance_valid() {
        return;
    }
    let mut bound = node.bind_mut();
    load_refresh_slots(&mut bound, state);
    load_rebuild_list(&mut bound, state);
}

/// `_close`: closes this view's registry entry (`load`/`save`), not whichever
/// dialog happens to be on top.
fn load_close(dialog: &Gd<MindDialog>, kind: LoadKind) {
    let name = match kind {
        LoadKind::Load => "load",
        LoadKind::Save => "save",
    };
    if !close_named_dialog(name) {
        hide_dialog_node(dialog);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// A fresh half-unique temp path for the save-check tests.
    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-savecheck-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    /// `SaveIO.isSaveValid` pre-flight: a missing file is `Missing`, a garbage
    /// file is `Invalid`, and a written save header is `Ok`. This is the
    /// `@save.corrupted` discriminator behind the load dialog's failure state.
    #[test]
    fn save_file_check_classifies_missing_invalid_and_valid() {
        let missing = temp_file("missing.msav");
        assert_eq!(save_file_check(&missing), SaveFileCheck::Missing);

        let corrupt = temp_file("corrupt.msav");
        let _ = std::fs::write(&corrupt, b"not-a-save");
        assert_eq!(save_file_check(&corrupt), SaveFileCheck::Invalid);

        use mind_core::io::save::versions::v1::base_meta_tags;
        use mind_core::io::save::{SaveOptions, WriteContext};
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 2, "test-map"));
        let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new())
            .expect("write a meta-only save fixture");
        let valid = temp_file("valid.msav");
        let _ = std::fs::write(&valid, bytes);
        assert_eq!(save_file_check(&valid), SaveFileCheck::Ok);

        let _ = std::fs::remove_file(&corrupt);
        let _ = std::fs::remove_file(&valid);
    }
}
