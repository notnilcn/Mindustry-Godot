// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `add_menu` support code.
//! Upstream: logic/LogicDialog.java

use super::*;

// ---------------------------------------------------------------------------
// add menu (`LogicDialog.showAddDialog`)
// ---------------------------------------------------------------------------

/// The nested sub-dialog shell: `black9` stage, accent title + 3 px rule, a
/// centred content cell and the button row with `addCloseButton()`.
pub(super) struct SubDialog {
    pub(super) cont: Gd<CenterContainer>,
}

/// `_open_sub_dialog`: a nested full-screen `BaseDialog` stacked over the editor.
pub(super) fn open_sub_dialog(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    title_key: &str,
) -> SubDialog {
    close_sub_dialog(state, false);
    let dialog_gd = dialog.to_gd();
    // code-instantiated: a transient nested dialog (one per open), stacked
    // over the editor like Java's `new BaseDialog(...)`.
    let mut root = Control::new_alloc();
    root.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    root.set_mouse_filter(MouseFilter::STOP);
    dialog.base_mut().add_child(&root);
    let mut dim = ColorRect::new_alloc();
    dim.set_color(styles::BLACK_9);
    dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    dim.set_mouse_filter(MouseFilter::IGNORE);
    root.add_child(&dim);
    let mut layout = VBoxContainer::new_alloc();
    layout.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    layout.add_theme_constant_override("separation", 0);
    root.add_child(&layout);
    let mut title = Label::new_alloc();
    title.set_text(&gs(bundle(title_key)));
    title.set_horizontal_alignment(HorizontalAlignment::CENTER);
    title.add_theme_color_override("font_color", styles::ACCENT);
    layout.add_child(&title);
    let mut rule_margin = MarginContainer::new_alloc();
    for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
        rule_margin.add_theme_constant_override(side, 4);
    }
    layout.add_child(&rule_margin);
    let mut rule = ColorRect::new_alloc();
    rule.set_color(styles::ACCENT);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    rule_margin.add_child(&rule);
    let mut center = CenterContainer::new_alloc();
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&center);
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(AlignmentMode::CENTER);
    layout.add_child(&row);
    let mut back = factory::icon_button("left", &bundle("@back"), "defaultt", AlignmentMode::BEGIN);
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    on_pressed(&back, &dialog_gd, state, |_d, s| {
        close_sub_dialog(s, true);
    });
    row.add_child(&back);
    let mut bottom = Control::new_alloc();
    bottom.set_custom_minimum_size(Vector2::new(0.0, 8.0));
    layout.add_child(&bottom);
    state.borrow_mut().sub_dialog = Some(root);
    SubDialog { cont: center }
}

/// Hides the nested dialog (`BaseDialog.hidden` plays `uiBack`).
pub(super) fn close_sub_dialog(state: &Rc<RefCell<LogicState>>, play_sound: bool) {
    let sub = state.borrow_mut().sub_dialog.take();
    let Some(mut root) = sub else {
        return;
    };
    if root.is_instance_valid() {
        root.queue_free();
    }
    if play_sound
        && let Some(mut ui) = kit::ui()
        && ui.has_method("play_ui_sound")
    {
        let _ = ui.call("play_ui_sound", &[gs("uiBack").to_variant()]);
    }
}

/// `showAddDialog(position)` (`LogicDialog.java:260-347`): a nested
/// `BaseDialog("@add")` whose `Tex.button` table holds the search row
/// (`Icon.zoom` + field, `@players.search`) over a pane of category sections:
/// a header (`Pal.darkishGray` icon 15 px, name with the description tooltip,
/// 5 px rule) then 130x50 `flatt` buttons in the category colour, three per
/// row, each with its `lst.<key>` tooltip. Enter adds the first match.
pub(super) fn open_add_dialog(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    at_index: i64,
) {
    if state.borrow().statements.len() >= MAX_INSTRUCTIONS {
        return;
    }
    let entries = add_entries(state);
    let dialog_gd = dialog.to_gd();
    let parts = open_sub_dialog(dialog, state, "@add");
    let cont: Gd<Container> = parts.cont.clone().upcast();
    // code-instantiated: the `Tex.button` table of the add menu (one per open).
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &button_box(state));
    add_any(&cont, &panel);
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 4);
    panel.add_child(&column);
    // code-instantiated: the `Icon.zoom` search row of the add menu.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 8);
    column.add_child(&search_row);
    let glyph = factory::glyph_label("zoom", 22, Color::WHITE);
    search_row.add_child(&glyph);
    let mut search = factory::field(&bundle("@players.search"));
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    search_row.add_child(&search);
    // code-instantiated: the scrolling category list of the add menu.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    column.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", 0);
    scroll.add_child(&list);
    let matched: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let rebuild: Rc<dyn Fn()> = {
        let list = list.clone();
        let search = search.clone();
        let entries = entries.clone();
        let matched = matched.clone();
        let dialog_gd = dialog_gd.clone();
        let state = state.clone();
        Rc::new(move || {
            rebuild_add_list(
                &list,
                &search,
                &entries,
                &matched,
                &dialog_gd,
                &state,
                at_index,
                &search_row,
            );
        })
    };
    {
        let rebuild = rebuild.clone();
        let callable = Callable::from_fn("logic_add_filter", move |_args: &[&Variant]| {
            rebuild();
            Variant::nil()
        });
        search.connect("text_changed", &callable);
    }
    {
        let matched = matched.clone();
        let dialog_gd = dialog_gd.clone();
        let state = state.clone();
        let callable = Callable::from_fn("logic_add_enter", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| value.stringify().to_string())
                .unwrap_or_default();
            let name = matched.borrow().clone();
            if !text.is_empty() && !name.is_empty() && dialog_gd.is_instance_valid() {
                let mut handle = dialog_gd.clone();
                let mut bound = handle.bind_mut();
                add_statement(&mut bound, &state, &name, at_index);
            }
            Variant::nil()
        });
        search.connect("text_submitted", &callable);
    }
    rebuild();
    let mut search = search;
    search.call_deferred("grab_focus", &[]);
}

/// The filtered category list rebuild.
#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild_add_list(
    list: &Gd<VBoxContainer>,
    search: &Gd<LineEdit>,
    entries: &[AddEntry],
    matched: &Rc<RefCell<String>>,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    at_index: i64,
    search_row: &Gd<HBoxContainer>,
) {
    let mut list = list.clone();
    for child in list.get_children().iter_shared() {
        let mut child = child.clone();
        list.remove_child(&child);
        child.queue_free();
    }
    let text = search.get_text().to_string().to_lowercase();
    let mut first_match: Option<String> = None;
    let mut sections: Vec<(String, Gd<GridContainer>)> = Vec::new();
    for entry in entries {
        if !text.is_empty()
            && !entry.label.to_lowercase().contains(&text)
            && !entry.type_name.to_lowercase().contains(&text)
        {
            continue;
        }
        if first_match.is_none() {
            first_match = Some(entry.name.clone());
        }
        let existing = sections
            .iter()
            .find(|(category, _)| category == &entry.category)
            .map(|(_, grid)| grid.clone());
        let grid = match existing {
            Some(grid) => grid,
            None => {
                let header = add_category_header(&entry.category);
                list.add_child(&header);
                // code-instantiated: one button grid per listed category.
                let mut grid = GridContainer::new_alloc();
                grid.set_columns(3);
                grid.add_theme_constant_override("h_separation", 0);
                grid.add_theme_constant_override("v_separation", 0);
                list.add_child(&grid);
                sections.push((entry.category.clone(), grid.clone()));
                grid
            }
        };
        let button = add_statement_button(dialog, state, entry, at_index);
        let mut grid = grid;
        grid.add_child(&button);
    }
    *matched.borrow_mut() = first_match.unwrap_or_default();
    // `.maxHeight(Core.graphics.getHeight() * 0.8f)` on the content table.
    let scroll_target = list
        .get_parent()
        .and_then(|node| node.try_cast::<ScrollContainer>().ok());
    let dialog_for_fit = dialog.clone();
    let search_for_fit = search_row.clone();
    defer_action("logic_fit_add_scroll", move || {
        if let Some(scroll) = scroll_target {
            fit_add_scroll(&scroll, &list, &search_for_fit, &dialog_for_fit);
        }
    });
}

/// The pane takes the list's size up to 80% of the screen
/// (`.maxHeight(Core.graphics.getHeight() * 0.8f)`).
pub(super) fn fit_add_scroll(
    scroll: &Gd<ScrollContainer>,
    list: &Gd<VBoxContainer>,
    search_row: &Gd<HBoxContainer>,
    dialog: &Gd<MindDialog>,
) {
    if !scroll.is_instance_valid() || !list.is_instance_valid() {
        return;
    }
    let wanted = list.get_combined_minimum_size();
    let mut cap = dialog.get_viewport_rect().size.y * 0.8 - 30.0;
    if search_row.is_instance_valid() {
        cap -= search_row.get_size().y + 4.0;
    }
    let height = wanted.y.min(cap.max(100.0));
    let mut width = wanted.x;
    if wanted.y > height {
        width += scroll
            .get_v_scroll_bar()
            .map(|bar| bar.get_combined_minimum_size().x)
            .unwrap_or_default();
    }
    let mut scroll = scroll.clone();
    scroll.set_custom_minimum_size(Vector2::new(width, height));
}

/// The category header row (`LogicDialog.java:313-319`).
pub(super) fn add_category_header(category: &str) -> Gd<Control> {
    // code-instantiated: one header per listed category.
    let mut margin = MarginContainer::new_alloc();
    margin.add_theme_constant_override("margin_left", 5);
    margin.add_theme_constant_override("margin_right", 5);
    margin.add_theme_constant_override("margin_top", 10);
    margin.add_theme_constant_override("margin_bottom", 5);
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 0);
    margin.add_child(&row);
    let icon = category_icon(category);
    if !icon.is_empty() {
        let mut glyph = factory::glyph_label(icon, 15, DARKISH_GRAY);
        glyph.set_custom_minimum_size(Vector2::new(15.0, 15.0));
        row.add_child(&glyph);
        row.add_child(&factory::space(10.0, 0.0));
    }
    let mut title = Label::new_alloc();
    title.set_text(&gs(bundle(&format!("lcategory.{category}"))));
    title.add_theme_color_override("font_color", DARKISH_GRAY);
    title.set_mouse_filter(MouseFilter::STOP);
    title.set_tooltip_text(&gs(plain_markup(&bundle(&format!(
        "lcategory.{category}.description"
    )))));
    row.add_child(&title);
    row.add_child(&factory::space(10.0, 0.0));
    let mut line = ColorRect::new_alloc();
    line.set_color(DARKISH_GRAY);
    line.set_custom_minimum_size(Vector2::new(0.0, 5.0));
    line.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    line.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&line);
    margin.upcast()
}

/// One `cat.button(example.localizedName(), style, ...).size(130f, 50f)` with
/// `style = flatt` + `fontColor = category.color`, `font = Fonts.outline`.
pub(super) fn add_statement_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    entry: &AddEntry,
    at_index: i64,
) -> Gd<Button> {
    // code-instantiated: one button per listed statement.
    let mut button = Button::new_alloc();
    button.set_text(&gs(entry.label.as_str()));
    button.set_custom_minimum_size(Vector2::new(130.0, 50.0));
    button.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_theme_type_variation("flatt");
    button.add_theme_stylebox_override("normal", &flat_box(Color::BLACK));
    button.add_theme_stylebox_override("hover", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("pressed", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("hover_pressed", &flat_box(styles::FLAT_OVER));
    for state_name in [
        "font_color",
        "font_hover_color",
        "font_pressed_color",
        "font_focus_color",
        "font_hover_pressed_color",
    ] {
        button.add_theme_color_override(state_name, entry.color);
    }
    button.add_theme_color_override("font_outline_color", Color::BLACK);
    button.add_theme_constant_override("outline_size", 4);
    button.set_tooltip_text(&gs(entry.tooltip.as_str()));
    let name = entry.name.clone();
    on_pressed(&button, dialog, state, move |d, s| {
        add_statement(d, s, &name, at_index);
    });
    button
}

/// Candidate statements for the add dialog in `LogicIO.allStatements` order,
/// after the Java filters (`LogicDialog.java:300-304`).
pub(super) fn add_entries(state: &Rc<RefCell<LogicState>>) -> Vec<AddEntry> {
    let mut out: Vec<AddEntry> = Vec::new();
    let Some(mut logic) = kit::logic() else {
        return out;
    };
    if !logic.has_method("statement_names") {
        return out;
    }
    let (privileged, editor, filter) = {
        let st = state.borrow();
        (st.privileged, st.editor_mode, st.filter_mode)
    };
    let mut unit_allowed = true;
    if let Some(mut host) = kit::sim_host()
        && !editor
        && !filter
        && host.has_method("logic_rules_probe")
    {
        let probe = host.call("logic_rules_probe", &[]);
        let text = probe
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if let Ok(dict) = Json::parse_string(&gs(text.as_str())).try_to::<VarDictionary>() {
            unit_allowed = dict_bool(&dict, "logicUnitControl", true);
        }
    }
    let names = logic
        .call("statement_names", &[])
        .try_to::<PackedStringArray>()
        .unwrap_or_default();
    for name_value in names.as_slice() {
        let statement_name = name_value.to_string();
        let info = meta(state, &statement_name);
        if info.is_empty() {
            continue;
        }
        if type_name(state, &statement_name) == "Invalid" {
            continue;
        }
        if dict_bool(&info, "hidden", false) {
            continue;
        }
        if dict_bool(&info, "privileged", false) && !privileged {
            continue;
        }
        if dict_bool(&info, "non_privileged", false) && privileged {
            continue;
        }
        let category = {
            let value = dict_string(&info, "category");
            if value.is_empty() {
                "unknown".to_string()
            } else {
                value
            }
        };
        if category == "unit" && !privileged && !unit_allowed {
            continue;
        }
        out.push(AddEntry {
            name: statement_name.clone(),
            category: category.clone(),
            color: category_color(&category),
            label: localized_name(state, &statement_name),
            type_name: type_name(state, &statement_name),
            tooltip: bundle_tooltip(&format!("lst.{}", statement_key(state, &statement_name))),
        });
    }
    out
}

/// Adds a default instance of `name` at `at_index` (`-1` appends):
/// `canvas.addAt(position == -1 ? size : position, prov.get())`, then
/// `dialog.hide()`.
pub(super) fn add_statement(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    statement_name: &str,
    at_index: i64,
) {
    if state.borrow().statements.len() >= MAX_INSTRUCTIONS {
        return;
    }
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("default_statement_row") {
        return;
    }
    let value = logic.call("default_statement_row", &[gs(statement_name).to_variant()]);
    let Ok(dict) = value.try_to::<VarDictionary>() else {
        return;
    };
    if dict.is_empty() {
        return;
    }
    let row = row_from_dict(&dict);
    let count = state.borrow().statements.len() as i64;
    let at = if at_index < 0 {
        count
    } else {
        at_index.clamp(0, count)
    };
    shift_jumps_for_insert(state, at);
    state.borrow_mut().statements.insert(at as usize, row);
    state.borrow_mut().modified = true;
    close_sub_dialog(state, true);
    rebuild_list(dialog, state);
}
