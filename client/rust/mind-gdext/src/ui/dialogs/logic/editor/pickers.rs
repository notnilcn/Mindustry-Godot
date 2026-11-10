// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `pickers` support code.
//! Upstream: logic/LogicDialog.java, logic/LStatement.java

use super::*;

// ---------------------------------------------------------------------------
// content pickers (`showSelectTable`)
// ---------------------------------------------------------------------------

/// A content picker callback (`on_pick(entry_name)` at each `showSelectTable`
/// call site).
pub(super) type PickFn = Rc<dyn Fn(&str)>;

/// `showSelectTable`: a transient `PaneSolid`-style popup anchored over the
/// editor. Godot's `PopupPanel` owns the click-outside dismissal.
pub(super) fn picker_popup(dialog: &mut MindDialog) -> (Gd<PopupPanel>, Gd<VBoxContainer>) {
    // code-instantiated: one transient picker per open (`showSelectTable`).
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    connect_popup_free(&popup);
    let root = factory::vbox(2);
    popup.add_child(&root);
    (popup, root)
}

/// Commits one picker selection and rebuilds the element (`build(table)` after
/// every content selection in `LStatements.java`).
pub(super) fn picker_handler(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    popup: &Gd<PopupPanel>,
    index: i64,
    field_index: i64,
    prefix: &'static str,
) -> PickFn {
    let dialog = dialog.clone();
    let state = state.clone();
    let popup = popup.clone();
    Rc::new(move |value: &str| {
        if popup.is_instance_valid() {
            let mut popup = popup.clone();
            popup.hide();
        }
        commit_field_value(&state, &format!("{prefix}{value}"), index, field_index);
        if dialog.is_instance_valid() {
            let mut handle = dialog.clone();
            let mut bound = handle.bind_mut();
            rebuild_list(&mut bound, &state);
        }
    })
}

/// `Picker` rows from the live content snapshot (`MindSimHost.logic_picker_json`
/// `{name, hidden, unlocked_now, ...}`); falls back to `content_list` when the
/// extension predates the picker seam.
pub(super) fn picker_rows(kind: &str) -> Vec<VarDictionary> {
    let Some(mut host) = kit::sim_host() else {
        return Vec::new();
    };
    if host.has_method("logic_picker_json") {
        let text = host
            .call("logic_picker_json", &[gs(kind).to_variant()])
            .try_to::<GString>()
            .unwrap_or_default();
        let parsed = Json::parse_string(&text);
        let Ok(array) = parsed.try_to::<VarArray>() else {
            return Vec::new();
        };
        return array
            .iter_shared()
            .filter_map(|entry| entry.try_to::<VarDictionary>().ok())
            .collect();
    }
    if host.has_method("content_list") {
        let names = host.call("content_list", &[gs(kind).to_variant()]);
        let Ok(names) = names.try_to::<PackedStringArray>() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for name in names.as_slice() {
            let mut row = VarDictionary::new();
            row.set("name", &gs(name.to_string()).to_variant());
            row.set("unlocked_now", true);
            rows.push(row);
        }
        return rows;
    }
    Vec::new()
}

/// Java content filters at each `showSelectTable` call site.
pub(super) fn picker_visible(row: &VarDictionary, filter: &str) -> bool {
    let entry_name = dict_string(row, "name");
    if entry_name.is_empty() {
        return false;
    }
    let is_hidden = dict_bool(row, "hidden", false);
    let unlocked_now = dict_bool(row, "unlocked_now", true);
    match filter {
        "unit-control" => unlocked_now && !is_hidden && dict_bool(row, "logic_controllable", true),
        "unit" | "block" | "item" => unlocked_now && !is_hidden,
        "item-prop" => !is_hidden,
        "item-ore" => unlocked_now,
        "liquid-prop" => unlocked_now && !is_hidden,
        "status" => entry_name != "none",
        "status-prop" => entry_name != "none" && unlocked_now && dict_bool(row, "show", true),
        _ => true,
    }
}

pub(super) fn content_label(kind: &str, entry_name: &str) -> String {
    if kind == "weather" {
        return field_label_text(entry_name);
    }
    if kind == "laccess" {
        let access_key = format!("laccess.label.{}", entry_name.to_lowercase());
        let access_text = bundle(&access_key);
        return if access_text == access_key {
            entry_name.to_string()
        } else {
            access_text
        };
    }
    let key = format!("{kind}.{entry_name}.name");
    let label = bundle(&key);
    if label == key {
        entry_name.to_string()
    } else {
        label
    }
}

pub(super) fn content_texture(kind: &str, entry_name: &str) -> Option<Gd<Texture2D>> {
    let mut texture = kit::icon_texture(&format!("{kind}-{entry_name}-ui"));
    if texture.is_none() && kind == "block" {
        texture = kit::icon_texture(&format!("block-{entry_name}-full"));
    }
    texture
}

/// `t.button(new TextureRegionDrawable(item.uiIcon), Styles.flati, iconSmall,
/// ...).size(40f)` (`LStatements.java:544-552`, 6 per row).
pub(super) fn icon_grid(
    kind: &str,
    filter: &str,
    on_pick: &PickFn,
    columns: i32,
    cell: f32,
    height: f32,
) -> Gd<Control> {
    // code-instantiated: the scrolling content grid of one picker.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(300.0, height));
    // code-instantiated: the content grid of one picker (cell count is data-driven).
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(columns);
    scroll.add_child(&grid);
    let mut count = 0;
    for row in picker_rows(kind) {
        if !picker_visible(&row, filter) {
            continue;
        }
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one content cell per picker entry.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flati");
        button.set_custom_minimum_size(Vector2::new(cell, cell));
        if let Some(texture) = content_texture(kind, &entry_name) {
            // code-instantiated: the content icon inside a picker cell.
            let mut image = TextureRect::new_alloc();
            image.set_texture(&texture);
            image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            image.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&image);
        } else {
            let label = content_label(kind, &entry_name);
            let first = label
                .chars()
                .next()
                .map(|ch| ch.to_uppercase().to_string())
                .unwrap_or_default();
            button.set_text(&gs(first));
        }
        button.set_tooltip_text(&gs(content_label(kind, &entry_name)));
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&button);
        count += 1;
    }
    if count == 0 {
        grid.add_child(&factory::label(&bundle("@none.found")));
    }
    scroll.upcast()
}

/// `showSelectTable` icon grid (ubind units, ore items).
pub(super) fn open_icon_grid(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    kind: &str,
    filter: &str,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, root) = picker_popup(dialog);
    let prefix: &'static str = if kind == "status" { "@status-" } else { "@" };
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, prefix);
    let grid = icon_grid(kind, filter, &on_pick, 6, 40.0, 300.0);
    let root: Gd<Container> = root.upcast();
    add_any(&root, &grid);
    popup.popup_centered();
}

/// `ApplyStatusStatement`'s 240x40 status rows (`LStatements.java:1450-1461`).
pub(super) fn open_status_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@status-");
    // code-instantiated: the status list pane of the picker.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(260.0, 360.0));
    root.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    scroll.add_child(&list);
    for row in picker_rows("status") {
        if !picker_visible(&row, "status") {
            continue;
        }
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one status row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(content_label("status", &entry_name)));
        if let Some(texture) = content_texture("status", &entry_name) {
            button.set_button_icon(&texture);
        }
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick_status", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    if list.get_child_count() == 0 {
        list.add_child(&factory::label(&bundle("@none.found")));
    }
    popup.popup_centered();
}

/// `WeatherSense`/`WeatherSet`'s two-column weather list
/// (`LStatements.java:1512-1531`, `:1560-1579`).
pub(super) fn open_weather_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@");
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(2);
    root.add_child(&grid);
    for row in picker_rows("weather") {
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one weather row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(210.0, 40.0));
        button.set_text(&gs(content_label("weather", &entry_name)));
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick_weather", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&button);
    }
    if grid.get_child_count() == 0 {
        grid.add_child(&factory::label(&bundle("@none.found")));
    }
    popup.popup_centered();
}

/// `SetMarkerStatement`'s draw-layer list (`LStatements.java:2415-2428`):
/// every `Layer` field as `name = value` (`Reflect.get` + `Float.toString`).
pub(super) fn open_layer_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("marker_layers") {
        return;
    }
    let rows = logic.call("marker_layers", &[]);
    let Ok(rows) = rows.try_to::<VarArray>() else {
        return;
    };
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "");
    let mut list = VBoxContainer::new_alloc();
    root.add_child(&list);
    for row in rows.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        let layer_name = dict_string(&row, "name");
        let value = godot_num1(dict_f64(&row, "value", 0.0));
        // code-instantiated: one layer row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("logicTogglet");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(format!("{layer_name} = {value}")));
        {
            let pick = on_pick.clone();
            let value = value.clone();
            let callable = Callable::from_fn("logic_pick_layer", move |_args: &[&Variant]| {
                pick(&value);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    popup.popup_centered();
}

/// `String.num(value, 1)`: one decimal, trailing zeros trimmed.
pub(super) fn godot_num1(value: f64) -> String {
    let text = format!("{value:.1}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        text
    } else {
        trimmed.to_string()
    }
}

/// A `squareTogglei` tab row over a `Stack` of content tables
/// (`LStatements.java:2160-2176` sensor, `:2110-2125` setprop).
pub(super) fn picker_tabs(root: &Gd<VBoxContainer>, icons: &[&str], tables: Vec<Gd<Control>>) {
    let mut root = root.clone();
    // code-instantiated: the `squareTogglei` tab row and its page stack.
    let mut tabs = factory::hbox(0);
    root.add_child(&tabs);
    let mut holder = VBoxContainer::new_alloc();
    holder.set_v_size_flags(SizeFlags::EXPAND_FILL);
    root.add_child(&holder);
    let mut pages: Vec<Gd<Control>> = Vec::new();
    for _ in icons {
        // code-instantiated: one tab page per picker tab.
        let mut page = Control::new_alloc();
        page.set_h_size_flags(SizeFlags::EXPAND_FILL);
        page.set_v_size_flags(SizeFlags::EXPAND_FILL);
        pages.push(page);
    }
    for (index, page) in pages.iter().enumerate() {
        let mut page = page.clone();
        page.set_visible(index == 0);
        holder.add_child(&page);
    }
    let pages: Rc<Vec<Gd<Control>>> = Rc::new(pages);
    for (index, icon) in icons.iter().enumerate() {
        // code-instantiated: one square toggle per picker tab.
        let mut toggle = factory::glyph_button(icon, "", "squareTogglei");
        toggle.set_custom_minimum_size(Vector2::new(0.0, 50.0));
        toggle.set_h_size_flags(SizeFlags::EXPAND_FILL);
        toggle.set_toggle_mode(true);
        toggle.set_pressed(index == 0);
        {
            let pages_for_tab = pages.clone();
            let tabs_for_tab = tabs.clone();
            let callable = Callable::from_fn("logic_picker_tab", move |_args: &[&Variant]| {
                for (page_index, page) in pages_for_tab.iter().enumerate() {
                    let mut page = page.clone();
                    page.set_visible(page_index == index);
                }
                for (tab_index, child) in tabs_for_tab.get_children().iter_shared().enumerate() {
                    if let Ok(mut button) = child.try_cast::<Button>() {
                        button.set_pressed(tab_index == index);
                    }
                }
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        tabs.add_child(&toggle);
        if let (Some(table), Some(page)) = (tables.get(index), pages.get(index)) {
            let mut page = page.clone();
            page.add_child(table);
        }
    }
}

/// `SetPropStatement`'s item/liquid/status/sensor tabs (`LStatements.java`
/// `:2106-2175`; the sensor tab is `LAccess.settable`).
pub(super) fn open_prop_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, root) = picker_popup(dialog);
    // The item/liquid/sensor rows commit `@name`; statuses commit `@status-name`.
    let content_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@");
    let status_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@status-");
    let item = icon_grid("item", "item-prop", &content_pick, 6, 40.0, 300.0);
    let liquid = icon_grid("liquid", "liquid-prop", &content_pick, 6, 40.0, 300.0);
    let status = legacy_status_list(&status_pick);
    let sensors = access_list("settable", state.borrow().privileged, &content_pick);
    picker_tabs(
        &root,
        &["box", "liquid", "effect", "tree"],
        vec![item, liquid, status, sensors],
    );
    popup.popup_centered();
}

/// `SensorStatement`'s item/liquid/unit+block/sensor tabs
/// (`LStatements.java:545-630`; the unit and block lists share the third tab).
pub(super) fn open_sensor_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, root) = picker_popup(dialog);
    // Every sensor row commits `@name` (`LStatements.java:545-630`).
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@");
    let item = icon_grid("item", "item", &on_pick, 6, 40.0, 300.0);
    let liquid = icon_grid("liquid", "liquid-prop", &on_pick, 6, 40.0, 300.0);
    // code-instantiated: the unit and block grids share the third sensor tab.
    let mut units_blocks = VBoxContainer::new_alloc();
    units_blocks.add_child(&icon_grid("unit", "unit", &on_pick, 6, 40.0, 220.0));
    units_blocks.add_child(&icon_grid("block", "block", &on_pick, 6, 40.0, 220.0));
    let sensors = access_list("senseable", state.borrow().privileged, &on_pick);
    picker_tabs(
        &root,
        &["box", "liquid", "units", "tree"],
        vec![item, liquid, units_blocks.upcast(), sensors],
    );
    popup.popup_centered();
}

/// A 240x40 name list from `MindLogic.settable_accesses`/`senseable_accesses`
/// (`bundle(LAccess)` + `LCanvas.tooltip(cell, sensor)`).
pub(super) fn access_list(kind: &str, privileged: bool, on_pick: &PickFn) -> Gd<Control> {
    let Some(mut logic) = kit::logic() else {
        return factory::label(&bundle("@none.found")).upcast();
    };
    // code-instantiated: the access list pane of a picker tab.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(260.0, 300.0));
    let mut list = VBoxContainer::new_alloc();
    scroll.add_child(&list);
    let mut names: Vec<String> = Vec::new();
    if kind == "settable" && logic.has_method("settable_accesses") {
        names = logic
            .call("settable_accesses", &[])
            .try_to::<PackedStringArray>()
            .map(|packed| {
                packed
                    .as_slice()
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default();
    } else if kind == "senseable" && logic.has_method("senseable_accesses") {
        names = logic
            .call("senseable_accesses", &[privileged.to_variant()])
            .try_to::<PackedStringArray>()
            .map(|packed| {
                packed
                    .as_slice()
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default();
    }
    for access_name in names {
        // code-instantiated: one access row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(content_label("laccess", &access_name)));
        button.set_tooltip_text(&gs(bundle_tooltip(&format!(
            "laccess.{}",
            access_name.to_lowercase()
        ))));
        {
            let pick = on_pick.clone();
            let name = access_name.clone();
            let callable = Callable::from_fn("logic_pick_access", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    if list.get_child_count() == 0 {
        list.add_child(&factory::label(&bundle("@none.found")));
    }
    scroll.upcast()
}

/// ApplyStatus's effect rows are reused by SetProp's status tab
/// (`LStatements.java:2138-2150`).
pub(super) fn legacy_status_list(on_pick: &PickFn) -> Gd<Control> {
    // code-instantiated: the status list pane of the setprop picker tab.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(260.0, 300.0));
    let mut list = VBoxContainer::new_alloc();
    scroll.add_child(&list);
    for row in picker_rows("status") {
        if !picker_visible(&row, "status-prop") {
            continue;
        }
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one status row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(content_label("status", &entry_name)));
        if let Some(texture) = content_texture("status", &entry_name) {
            button.set_button_icon(&texture);
        }
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick_status", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    if list.get_child_count() == 0 {
        list.add_child(&factory::label(&bundle("@none.found")));
    }
    scroll.upcast()
}

/// `PlaySoundStatement.showSoundSelect` (`LStatements.java:2282-2380`): the
/// category tab list on the left and the sound rows (preview + select) on the
/// right, with the current sound's category preselected.
pub(super) fn open_sound_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let Some(assets) = kit::assets() else {
        return;
    };
    let mut assets_node: Gd<Node> = assets.upcast();
    if !assets_node.has_method("sound_entries") {
        return;
    }
    let entries = assets_node.call("sound_entries", &[]);
    let Ok(entries) = entries.try_to::<VarArray>() else {
        return;
    };
    let mut choices: Vec<SoundEntry> = Vec::new();
    let mut categories: BTreeMap<String, Vec<SoundEntry>> = BTreeMap::new();
    for entry in entries.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let name = dict_string(&entry, "name");
        if name.is_empty() {
            continue;
        }
        let mut id = dict_string(&entry, "id");
        if id.is_empty() {
            id = name.clone();
        }
        let sound = SoundEntry {
            name,
            id,
            category: dict_string(&entry, "category"),
        };
        choices.push(sound.clone());
        categories
            .entry(sound.category.clone())
            .or_default()
            .push(sound);
    }
    if choices.is_empty() {
        return;
    }
    choices.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    let mut category_names: Vec<String> = vec!["all".to_string()];
    category_names.extend(categories.keys().cloned());
    category_names.sort();
    let current_full = current_field_value(state, index, field_index);
    let current = current_full
        .strip_prefix("@sfx-")
        .unwrap_or(&current_full)
        .to_string();
    let current_category = choices
        .iter()
        .find(|sound| sound.name == current)
        .map(|sound| sound.category.clone())
        .unwrap_or_else(|| "all".to_string());
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    // code-instantiated: the category rail + sound list of the sound browser.
    let mut body = factory::hbox(4);
    root.add_child(&body);
    let mut tabs = VBoxContainer::new_alloc();
    tabs.set_custom_minimum_size(Vector2::new(160.0, 0.0));
    body.add_child(&tabs);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(450.0, 420.0));
    body.add_child(&scroll);
    let list = factory::vbox(3);
    scroll.add_child(&list);
    let selected: Rc<RefCell<String>> = Rc::new(RefCell::new(current_category));
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@sfx-");
    let rebuild: Rc<dyn Fn()> = {
        let list = list.clone();
        let selected = selected.clone();
        let choices = choices.clone();
        let categories = categories.clone();
        let current = current.clone();
        let on_pick = on_pick.clone();
        Rc::new(move || {
            rebuild_sound_list(&list, &selected, &choices, &categories, &current, &on_pick);
        })
    };
    for category in &category_names {
        // code-instantiated: one sound category tab.
        let mut toggle = Button::new_alloc();
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_custom_minimum_size(Vector2::new(140.0, 34.0));
        toggle.set_text(&gs(field_label_text(category)));
        toggle.set_toggle_mode(true);
        toggle.set_meta("category", &gs(category.as_str()).to_variant());
        toggle.set_pressed(*selected.borrow() == *category);
        {
            let selected_for_tab = selected.clone();
            let tabs_for_tab = tabs.clone();
            let rebuild_for_tab = rebuild.clone();
            let category_for_tab = category.clone();
            let callable =
                Callable::from_fn("logic_sound_category", move |_args: &[&Variant]| {
                    *selected_for_tab.borrow_mut() = category_for_tab.clone();
                    for child in tabs_for_tab.get_children().iter_shared() {
                        if let Ok(mut button) = child.try_cast::<Button>() {
                            if !button.has_meta("category") {
                                continue;
                            }
                            let current_category = button
                                .get_meta("category")
                                .try_to::<GString>()
                                .map(|value| value.to_string())
                                .unwrap_or_default();
                            button.set_pressed(current_category == category_for_tab);
                        }
                    }
                    rebuild_for_tab();
                    Variant::nil()
                });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        tabs.add_child(&toggle);
    }
    {
        let callable = Callable::from_fn("logic_sound_stop", move |_args: &[&Variant]| {
            if let Some(mut audio) = kit::node("/root/Spine/MindAudio")
                && audio.has_method("stop_sound_preview")
            {
                let _ = audio.call("stop_sound_preview", &[]);
            }
            Variant::nil()
        });
        popup.connect("popup_hide", &callable);
    }
    rebuild();
    popup.popup_centered();
}

/// The sound rows of the selected category (rebuilds the right-hand list).
pub(super) fn rebuild_sound_list(
    list: &Gd<VBoxContainer>,
    selected: &Rc<RefCell<String>>,
    choices: &[SoundEntry],
    categories: &BTreeMap<String, Vec<SoundEntry>>,
    current: &str,
    on_pick: &PickFn,
) {
    let mut list = list.clone();
    kit::clear_children(&mut list);
    let category = selected.borrow().clone();
    let shown: Vec<SoundEntry> = if category == "all" {
        choices.to_vec()
    } else {
        categories.get(&category).cloned().unwrap_or_default()
    };
    if shown.is_empty() {
        list.add_child(&factory::label(&bundle("@none.found")));
        return;
    }
    for sound in shown {
        // code-instantiated: one sound row (preview + select).
        let mut row = HBoxContainer::new_alloc();
        row.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        list.add_child(&row);
        let mut play = factory::icon_button("play", "", "cleari", AlignmentMode::BEGIN);
        play.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        {
            let sound_id = sound.id.clone();
            let callable = Callable::from_fn("logic_sound_play", move |_args: &[&Variant]| {
                if let Some(mut audio) = kit::node("/root/Spine/MindAudio") {
                    if audio.has_method("preview_sound") {
                        let _ = audio.call(
                            "preview_sound",
                            &[gs(sound_id.as_str()).to_variant(), 1.0f64.to_variant()],
                        );
                    } else if audio.has_method("on_ui_sound") {
                        let _ = audio.call("on_ui_sound", &[gs(sound_id.as_str()).to_variant()]);
                    }
                }
                Variant::nil()
            });
            let mut node: Gd<Node> = play.clone().upcast();
            node.connect("pressed", &callable);
        }
        row.add_child(&play);
        let label = if category == "all" {
            format!("{}/{}", field_label_text(&sound.category), sound.name)
        } else {
            sound.name.clone()
        };
        // code-instantiated: one sound select toggle.
        let mut toggle = Button::new_alloc();
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        toggle.set_h_size_flags(SizeFlags::EXPAND_FILL);
        toggle.set_text_alignment(HorizontalAlignment::LEFT);
        toggle.set_text(&gs(label));
        toggle.set_toggle_mode(true);
        toggle.set_pressed(sound.name == current);
        {
            let pick = on_pick.clone();
            let sound_name = sound.name.clone();
            let callable = Callable::from_fn("logic_sound_pick", move |_args: &[&Variant]| {
                pick(&sound_name);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        row.add_child(&toggle);
    }
}

/// `ui.effects.show(entry -> ...)` for `EffectStatement`
/// (`LStatements.java:1806`).
pub(super) fn effect_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    color: Color,
) -> Gd<Button> {
    // code-instantiated: one effect picker button per element.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("logict");
    apply_logict(&mut button, color);
    button.set_custom_minimum_size(Vector2::new(150.0, 40.0));
    button.set_autowrap_mode(AutowrapMode::WORD_SMART);
    button.set_text(&gs(field_label_text(&current_field_value(
        state,
        index,
        field_index,
    ))));
    on_pressed(&button, dialog, state, move |d, s| {
        open_effects(d, s, index, field_index);
    });
    button
}

/// A sibling manifest dialog from the static `UiRoot` tree
/// (`wave_info_dialog.gd:_dialog`).
pub(super) fn sibling_dialog(dialog_name: &str) -> Option<Gd<Node>> {
    let mut ui_root = kit::node("/root/Spine/Ui/UiRoot")?;
    if !ui_root.has_method("dialog") {
        return None;
    }
    ui_root
        .call("dialog", &[gs(dialog_name).to_variant()])
        .try_to::<Gd<Node>>()
        .ok()
}

pub(super) fn open_effects(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let ui = kit::ui();
    let effects = sibling_dialog("effects");
    let (Some(mut ui), Some(mut effects)) = (ui, effects) else {
        open_select(dialog, state, index, field_index, "effect");
        return;
    };
    if !ui.has_method("open_dialog") {
        open_select(dialog, state, index, field_index, "effect");
        return;
    }
    state.borrow_mut().effect_target = (index, field_index);
    let needs_effect_binding = !state.borrow().effect_connected;
    if effects.has_signal("effect_selected") && needs_effect_binding {
        let dialog_gd = dialog.to_gd();
        let state_for_effect = state.clone();
        let callable = Callable::from_fn("logic_effect_selected", move |args: &[&Variant]| {
            let effect_name = args
                .first()
                .map(|value| value.stringify().to_string())
                .unwrap_or_default();
            let target = state_for_effect.borrow().effect_target;
            if target.0 < 0 || effect_name.is_empty() {
                return Variant::nil();
            }
            state_for_effect.borrow_mut().effect_target = (-1, -1);
            commit_field_value(&state_for_effect, &effect_name, target.0, target.1);
            if dialog_gd.is_instance_valid() {
                let mut handle = dialog_gd.clone();
                let mut bound = handle.bind_mut();
                rebuild_list(&mut bound, &state_for_effect);
            }
            Variant::nil()
        });
        effects.connect("effect_selected", &callable);
        state.borrow_mut().effect_connected = true;
    }
    let _ = ui.call(
        "open_dialog",
        &[gs("effects").to_variant(), gs("{}").to_variant()],
    );
}
