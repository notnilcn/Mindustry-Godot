// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/CustomRulesDialog.java,
//         core/src/mindustry/ui/dialogs/EditorMapsDialog.java,
//         core/src/mindustry/ui/dialogs/CanvasEditDialog.java,
//         core/src/mindustry/ui/dialogs/MapListDialog.java (`displayType`,
//         `showMap` override), core/src/mindustry/logic/CanvasBlock.java
//         (LSB-first pixel packing).

//! The custom-rules editor, the editor map browser and the logic-canvas painter.
//!
//! * `custom_rules` mirrors `CustomRulesDialog`: the category list (`waves`,
//!   `resourcesbuilding`, `unit`, `enemy`, `environment`, `light`, `music`,
//!   `planet`, `teams`), the `check`/`number`/`numberi`/`text`/`team` row
//!   builders, the per-row enable conditions (`update(a -> a.setDisabled(...))`),
//!   the search filter, the team collapsers, the in-dialog `_overlay` panels
//!   (`@edit` clipboard menu, `@configure` loadout, `@rules.weather`, the
//!   banned-block/unit pickers and the `@rules.ambientlight` colour picker) and
//!   the hide-time `rules_edited` hand-off.
//! * `editor_maps` mirrors `EditorMapsDialog extends MapListDialog`
//!   (`displayType = true`): the shared `maplist_*` base in `batch_campaign2`
//!   supplies the search row, filters, grid and card, and this controller adds
//!   `@editor.newmap`/`@editor.importmap`, the New Map text prompt, the
//!   file-chosen import route with the overwrite confirm and the per-map
//!   `editor_map` page.
//! * `canvas_edit` mirrors `CanvasEditDialog`: the paint surface over the
//!   block's palette (left-click paint, drag Bresenham line, middle-click pick,
//!   `fill`/`grid` toggles), `@import`/`@export` PNG and the 2-second auto-save
//!   that pushes LSB-first packed pixels through
//!   `MindInput.configure_building_bytes`.
//!
//! Behaviour (rules JSON, editor map state, the canvas model) stays in the Rust
//! facades; these controllers only read their projections and call the same
//! dynamic method names the GDScript did.

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{
    PackedByteArray, PackedInt64Array, PackedStringArray, VarArray, VarDictionary, VariantType,
};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::image::{Format, Interpolation};
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, DisplayServer, FileDialog, HBoxContainer,
    IControl, Image, ImageTexture, InputEvent, InputEventMouseButton, InputEventMouseMotion, Json,
    LineEdit, Node, PanelContainer, RichTextLabel, StyleBoxEmpty, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, MouseButtonMask};
use godot::obj::{EngineBitfield, NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::check::MindCheck;
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindTable};
use super::batch_campaign2::{
    Deferred, MapListState, install_frame_tick, maplist_add_pane_button, maplist_build,
    maplist_load_settings, maplist_maps, maplist_rebuild, maplist_refresh_registry,
};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "custom_rules" => Some(Box::new(CustomRulesDialog::default())),
        "editor_maps" => Some(Box::new(EditorMapsDialog::default())),
        "canvas_edit" => Some(Box::new(CanvasEditDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// `str()`-style text for one variant (empty when unconvertible).
fn variant_text(value: &Variant) -> String {
    match value.get_type() {
        VariantType::STRING | VariantType::STRING_NAME => value.to::<GString>().to_string(),
        VariantType::INT => value.to::<i64>().to_string(),
        VariantType::FLOAT => value.to::<f64>().to_string(),
        VariantType::BOOL => value.to::<bool>().to_string(),
        _ => String::new(),
    }
}

/// GDScript `bool(value)`.
fn variant_bool(value: &Variant) -> bool {
    match value.get_type() {
        VariantType::BOOL => value.to::<bool>(),
        VariantType::INT => value.to::<i64>() != 0,
        VariantType::FLOAT => value.to::<f64>() != 0.0,
        VariantType::STRING | VariantType::STRING_NAME => !value.to::<GString>().is_empty(),
        _ => false,
    }
}

/// GDScript `float(value)`.
fn variant_f64(value: &Variant) -> f64 {
    match value.get_type() {
        VariantType::FLOAT => value.to::<f64>(),
        VariantType::INT => value.to::<i64>() as f64,
        VariantType::BOOL => {
            if value.to::<bool>() {
                1.0
            } else {
                0.0
            }
        }
        VariantType::STRING | VariantType::STRING_NAME => value
            .to::<GString>()
            .to_string()
            .trim()
            .parse()
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// GDScript `int(value)`.
fn variant_i64(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.to::<i64>(),
        VariantType::FLOAT => value.to::<f64>() as i64,
        VariantType::BOOL => i64::from(value.to::<bool>()),
        VariantType::STRING | VariantType::STRING_NAME => value
            .to::<GString>()
            .to_string()
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

/// One dictionary value as text (`str(dict.get(key, ""))`).
fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

/// One dictionary value as an integer.
fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// `JSON.stringify(dictionary)`.
fn json_stringify(dict: &VarDictionary) -> String {
    Json::stringify(&dict.to_variant()).to_string()
}

/// A rich label's engine handle (text/color/alignment).
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// `dict.get(key, default)` on a captured dictionary.
fn dict_at(dict: &Rc<RefCell<VarDictionary>>, key: &str, default: bool) -> bool {
    dict.borrow()
        .get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// `dict.get(key)` on a captured dictionary, coerced to a float.
fn dict_at_f64(dict: &Rc<RefCell<VarDictionary>>, key: &str, default: f64) -> f64 {
    dict.borrow()
        .get(key)
        .map(|value| variant_f64(&value))
        .unwrap_or(default)
}

/// `dict.get(key)` on a captured dictionary, coerced to an integer.
fn dict_at_i64(dict: &Rc<RefCell<VarDictionary>>, key: &str, default: i64) -> i64 {
    dict.borrow()
        .get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// `dict.get(key)` on a captured dictionary, coerced to text.
fn dict_at_text(dict: &Rc<RefCell<VarDictionary>>, key: &str) -> String {
    dict.borrow()
        .get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

/// `dict[key] = value` on a captured dictionary.
fn set_at(dict: &Rc<RefCell<VarDictionary>>, key: &str, value: Variant) {
    dict.borrow_mut().set(key, &value);
}

/// Calls a `MindUi` facade method (`None` when the facade/method is absent).
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Calls a `MindCampaign` facade method (`None` when the facade/method is absent).
fn campaign_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut campaign = kit::campaign()?;
    if !campaign.has_method(method) {
        return None;
    }
    Some(campaign.call(method, args))
}

/// The `UiRoot.dialog(name)` registry node (falls back to a tree search).
fn dialog_node(name: &str) -> Option<Gd<Node>> {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("dialog")
        && let Ok(node) = root
            .call("dialog", &[GString::from(name).to_variant()])
            .try_to::<Gd<Node>>()
    {
        return Some(node);
    }
    kit::tree_root()?.find_child(name)
}

/// Connects a `MindUi` prompt signal once per dialog node.
fn connect_prompt_once(dialog: &mut MindDialog, key: &str, signal: &str, callable: &Callable) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    let meta = format!("_batch_i_{key}");
    if dialog.base().has_meta(&meta) || !ui.has_signal(signal) {
        return;
    }
    let _ = ui.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase: a prompt is emitted from inside a bound
/// `MindUi` method, and re-entering a bound GDExtension object panics
/// (`PAR-0066`).
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}

/// `String.is_valid_int()`: an optional sign followed by digits only.
fn is_valid_int(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.chars().all(|character| character.is_ascii_digit())
}

/// `String.is_valid_float()`.
fn is_valid_float(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (body, None),
    };
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.chars().all(|value| value.is_ascii_digit()) {
            return false;
        }
    }
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    if integer.is_empty() && fraction.is_none() {
        return false;
    }
    if !integer.chars().all(|value| value.is_ascii_digit()) {
        return false;
    }
    match fraction {
        Some(fraction) => {
            if !fraction.chars().all(|value| value.is_ascii_digit()) {
                return false;
            }
            !(integer.is_empty() && fraction.is_empty())
        }
        None => true,
    }
}

/// `String.is_valid_identifier()`.
fn is_valid_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    characters.all(|value| value.is_ascii_alphanumeric() || value == '_')
}

/// `Strings.canParsePositiveFloat` (`number`) / `Strings.parseInt` (`numberi`).
fn parse_number(text: &str, integer: bool) -> bool {
    if text.is_empty() {
        return false;
    }
    if integer {
        return is_valid_int(text);
    }
    if !is_valid_float(text) {
        return false;
    }
    text.parse::<f64>()
        .map(|value| value >= 0.0)
        .unwrap_or(false)
}

/// `_number_text`: `String.num(value, 6)` with a forced `.0` for whole floats.
fn number_text(value: f64, integer: bool) -> String {
    if integer {
        return format!("{}", value as i64);
    }
    if !value.is_finite() {
        return format!("{value}.0");
    }
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    if !text.contains('.') {
        text.push_str(".0");
    }
    text
}

/// `_invalid_field`: tints the field red while its text does not parse.
fn invalid_field(field: &Gd<LineEdit>, invalid: bool) {
    let mut field = field.clone();
    field.add_theme_color_override(
        "font_color",
        if invalid {
            Color::from_rgba(1.0, 0.35, 0.35, 1.0)
        } else {
            Color::WHITE
        },
    );
}

/// `_in_game()` (`GameState.isGame`: playing/paused, not menu).
fn in_game() -> bool {
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("get_state") {
        return false;
    }
    host.call("get_state", &[]).to::<GString>() != "menu"
}

/// `Pixmap`/`Color8` RGBA8888 -> `Color`.
fn rgba8_color(value: i64) -> Color {
    let packed = value as u32;
    Color::from_rgba8(
        ((packed >> 24) & 0xff) as u8,
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

/// `_notify(key)`: the bundle lookup as an info prompt.
fn notify_key(key: &str) {
    let _ = ui_call("show_info", &[kit::t(key).to_variant()]);
}

/// `Bundle.format(key, args)`: `{0}`-style substitution (`_fmt`).
fn fmt_key(key: &str, args: &[&str]) -> String {
    let mut result = kit::t(key).to_string();
    for (index, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{index}}}"), arg);
    }
    result
}

// ---------------------------------------------------------------------------
// CustomRulesDialog (`custom_rules`)
// ---------------------------------------------------------------------------

/// `Vars.tilesize` (world pixels per tile).
const TILESIZE: f64 = 8.0;
/// Category accent rule height (`addToMain`, 3 px).
const RULE_HEIGHT: f32 = 3.0;

/// A per-row enable condition (`update(a -> a.setDisabled(!condition))`).
type Cond = Rc<dyn Fn() -> bool>;

/// `Callable()`: a row with no condition is always enabled.
fn always() -> Cond {
    Rc::new(|| true)
}

/// `_rules.get(key, default)` as a condition.
fn flag(dict: &Rc<RefCell<VarDictionary>>, key: &'static str, default: bool) -> Cond {
    let dict = dict.clone();
    Rc::new(move || dict_at(&dict, key, default))
}

/// `not condition`.
fn not(cond: Cond) -> Cond {
    Rc::new(move || !cond())
}

/// `a and b`.
fn and(first: Cond, second: Cond) -> Cond {
    Rc::new(move || first() && second())
}

/// `_in_game()` as a condition.
fn game_cond() -> Cond {
    Rc::new(in_game)
}

/// One `number`/`numberi` row value (`_number` cons receives a float,
/// `_numberi` cons receives an int).
#[derive(Clone, Copy)]
enum NumberValue {
    Int(i64),
    Float(f64),
}

/// One registered condition row (`CustomRulesDialog._rows`).
struct RuleRow {
    cond: Cond,
    apply: Rc<dyn Fn(bool)>,
}

/// One `(condition, apply)` pair cloned out for a condition refresh.
type RowEntry = (Cond, Rc<dyn Fn(bool)>);

/// The `(button, team id)` toggles of one `team` row (`button_row`/`team_ids`).
type TeamButtons = Rc<RefCell<Vec<(Gd<Button>, i64)>>>;

/// The `(button, planet name)` toggles of the `planet` table.
type PlanetButtons = Rc<RefCell<Vec<(Gd<Button>, String)>>>;

/// `Team.baseTeams` rows cached per rebuild.
type Teams = Vec<VarDictionary>;

/// `CustomRulesDialog` view state.
struct CustomRulesState {
    /// The working `Rules` copy (`_rules`); shared mutably with every row.
    rules: Rc<RefCell<VarDictionary>>,
    /// The `reset` context dictionary (`_reset`).
    reset: Rc<RefCell<VarDictionary>>,
    /// `showRuleEdit` (`@rules.allowedit` row).
    show_rule_edit: bool,
    /// `ruleSearch` (lowercased; empty = null).
    search: String,
    /// The scrolling rule column (`_main`).
    main: Option<Gd<MindTable>>,
    /// The table the next row lands in (`_current`).
    current: Option<Gd<MindTable>>,
    /// `_categories`: `(name, table)` in `setupMain` order.
    categories: Vec<(String, Gd<MindTable>)>,
    /// `_rows`: every registered condition row.
    rows: Vec<RuleRow>,
    /// `_team_rows`: `(team, content, header)` per team collapser.
    team_rows: Vec<(i64, Gd<VBoxContainer>, Gd<Button>)>,
    /// `Team.baseTeams` rows (`[{id, name, color}]`), cached per rebuild.
    teams: Teams,
    /// `_erekir_env`: Erekir's default env id (`_planet_env("erekir")`).
    erekir_env: i64,
    /// The open in-dialog overlay (`_overlay`).
    overlay: Option<Gd<Control>>,
    /// The pending `picker.color_selected` listener (`_show_color_picker`).
    picker_listener: Option<Rc<dyn Fn(Color)>>,
}

impl Default for CustomRulesState {
    fn default() -> Self {
        Self {
            rules: Rc::new(RefCell::new(VarDictionary::new())),
            reset: Rc::new(RefCell::new(VarDictionary::new())),
            show_rule_edit: false,
            search: String::new(),
            main: None,
            current: None,
            categories: Vec::new(),
            rows: Vec::new(),
            team_rows: Vec::new(),
            teams: Vec::new(),
            erekir_env: 17,
            overlay: None,
            picker_listener: None,
        }
    }
}

/// `@mode.custom` — the full custom-rules editor (`CustomRulesDialog`).
#[derive(Default)]
pub struct CustomRulesDialog {
    state: Rc<RefCell<CustomRulesState>>,
}

impl DialogController for CustomRulesDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@mode.custom");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        dialog.add_close_button(210.0);
        let target = dialog.to_gd();
        let state = self.state.clone();
        let callable = Callable::from_fn("custom_rules_edit", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            open_edit_menu(&mut bound, &state);
        });
        let text = dialog.t("@edit").to_string();
        dialog.add_button(&text, Some(callable), "pencil", 210.0);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let rules = dialog
            .context_value("rules")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let reset = dialog
            .context_value("reset")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let show_rule_edit = dialog
            .context_value("show_rule_edit")
            .map(|value| variant_bool(&value))
            .unwrap_or(false);
        {
            let state = self.state.borrow();
            *state.rules.borrow_mut() = rules;
            *state.reset.borrow_mut() = reset;
        }
        self.state.borrow_mut().show_rule_edit = show_rule_edit;
    }

    /// `shown(this::setup)`: rebuild on every open (the rules may be new).
    fn shown(&mut self, dialog: &mut MindDialog) {
        setup(dialog, &self.state);
    }

    /// `rules.hidden(...)`: the edited rules JSON goes back to the opener.
    fn hidden(&mut self, dialog: &mut MindDialog) {
        let rules = self.state.borrow().rules.clone();
        if rules.borrow().is_empty() {
            return;
        }
        let json = json_stringify(&rules.borrow());
        dialog
            .base_mut()
            .emit_signal("rules_edited", &[GString::from(json.as_str()).to_variant()]);
    }
}

/// `setup()`: the `players.search` filter row plus the category body.
fn setup(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    // code-instantiated: the search row mirrors `Icon.zoom` + field.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 8);
    let zoom = factory::glyph_label("zoom", 20, Color::WHITE);
    search_row.add_child(&zoom);
    let search = state.borrow().search.clone();
    let mut field = factory::field(&kit::t("@players.search").to_string());
    field.set_text(&GString::from(search.as_str()));
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let target = dialog.to_gd();
        let state_for_search = state.clone();
        let callable = Callable::from_fn("custom_rules_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            on_search(&mut bound, &state_for_search, &text);
        });
        field.connect("text_changed", &callable);
    }
    // `field.setCursorPosition(ruleSearch.length())` + a deferred focus grab.
    field.set_caret_column(search.chars().count() as i32);
    field.call_deferred("grab_focus", &[]);
    search_row.add_child(&field);
    // `cont.table(...).fillX()` above `cont.pane(main)` (`CustomRulesDialog.java:
    // 126-134`).
    root.bind_mut().add(&search_row).fill_x().pad(8.0);
    root.bind_mut().row();
    // code-instantiated: the rule table is data-driven from the Rules model.
    let mut main = factory::table();
    main.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&main).fill_x();
    state.borrow_mut().main = Some(main);
    setup_main(dialog, state);
}

/// `_on_search`: `strip_edges`, collapse runs of spaces, lowercase, rebuild.
fn on_search(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>, text: &str) {
    let mut search = text.trim().to_string();
    while search.contains("  ") {
        search = search.replace("  ", " ");
    }
    let search = search.to_lowercase();
    state.borrow_mut().search = search;
    setup_main(dialog, state);
}

/// `_check`: one check row into the current category.
fn check_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: bool,
    cond: Cond,
) {
    let checked = dict_at(dict, field, default);
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |value: bool| {
        set_at(&dict_cons, field, value.to_variant());
    });
    if let Some(row) = build_check_row(state, key, checked, on_change, cond) {
        add_current(state, &row, false);
    }
}

/// `_check_in`: one check row into a plain container (the team collapsers).
fn check_rule_in(
    state: &Rc<RefCell<CustomRulesState>>,
    target: &Gd<Control>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: bool,
    cond: Cond,
) {
    let checked = dict_at(dict, field, default);
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |value: bool| {
        set_at(&dict_cons, field, value.to_variant());
    });
    if let Some(row) = build_check_row(state, key, checked, on_change, cond) {
        add_in(target, &row, false);
    }
}

/// The team collapser's multiplier rows: `_check_in` with a `func(v: float)`
/// consumer, so the toggle stores `1.0`/`0.0` in the multiplier field.
fn check_num_in(
    state: &Rc<RefCell<CustomRulesState>>,
    target: &Gd<Control>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: f64,
    cond: Cond,
) {
    let checked = dict_at_f64(dict, field, default) != 0.0;
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |value: bool| {
        set_at(
            &dict_cons,
            field,
            (if value { 1.0 } else { 0.0 }).to_variant(),
        );
    });
    if let Some(row) = build_check_row(state, key, checked, on_change, cond) {
        add_in(target, &row, false);
    }
}

/// The `cons` of one `number`/`numberi` row: write `value * scale` back.
fn number_cons(
    dict: &Rc<RefCell<VarDictionary>>,
    field: &'static str,
    scale: f64,
    integer: bool,
    store_float: bool,
) -> Rc<dyn Fn(NumberValue)> {
    let dict = dict.clone();
    Rc::new(move |value: NumberValue| {
        let stored = match value {
            NumberValue::Int(number) => {
                if integer && !store_float {
                    (((number as f64) * scale) as i64).to_variant()
                } else {
                    ((number as f64) * scale).to_variant()
                }
            }
            NumberValue::Float(number) => (number * scale).to_variant(),
        };
        set_at(&dict, field, stored);
    })
}

/// `_number`/`_numberi`: one number row into the current category.
fn number_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    spec: NumberSpec,
    cond: Cond,
) {
    let raw = dict_at_f64(dict, spec.field, spec.default);
    let shown = (raw / spec.div).min(spec.cap);
    let on_change = number_cons(dict, spec.field, spec.scale, spec.integer, spec.store_float);
    if let Some(row) = build_number_row(
        state,
        spec.key,
        spec.integer,
        shown,
        spec.minimum,
        spec.maximum,
        on_change,
        cond,
    ) {
        add_current(state, &row, true);
    }
}

/// `_number_in`/`_numberi_in`: one number row into a plain container.
fn number_rule_in(
    state: &Rc<RefCell<CustomRulesState>>,
    target: &Gd<Control>,
    dict: &Rc<RefCell<VarDictionary>>,
    spec: NumberSpec,
    cond: Cond,
) {
    let raw = dict_at_f64(dict, spec.field, spec.default);
    let shown = (raw / spec.div).min(spec.cap);
    let on_change = number_cons(dict, spec.field, spec.scale, spec.integer, spec.store_float);
    if let Some(row) = build_number_row(
        state,
        spec.key,
        spec.integer,
        shown,
        spec.minimum,
        spec.maximum,
        on_change,
        cond,
    ) {
        add_in(target, &row, true);
    }
}

/// One `number`/`numberi` rule row spec (`_number`/`_numberi` arguments).
struct NumberSpec {
    key: &'static str,
    field: &'static str,
    default: f64,
    integer: bool,
    /// `cons`: the rules value is `value * scale`.
    scale: f64,
    /// `prov`: the field shows `min(raw / div, cap)`.
    div: f64,
    cap: f64,
    minimum: f64,
    maximum: f64,
    /// `_numberi` rows whose consumer declares `func(v: float)` (the GDScript
    /// stores `float(v)` even though the field is integer-formatted).
    store_float: bool,
}

impl NumberSpec {
    fn new(key: &'static str, field: &'static str, default: f64) -> Self {
        Self {
            key,
            field,
            default,
            integer: false,
            scale: 1.0,
            div: 1.0,
            cap: f64::INFINITY,
            minimum: 0.0,
            maximum: f64::INFINITY,
            store_float: false,
        }
    }

    /// `_numberi` (`integer = true`).
    fn integer(mut self) -> Self {
        self.integer = true;
        self
    }

    /// A `numberi` row whose consumer stores `float(v)`.
    fn float_store(mut self) -> Self {
        self.store_float = true;
        self
    }

    fn scale(mut self, value: f64) -> Self {
        self.scale = value;
        self
    }

    fn div(mut self, value: f64) -> Self {
        self.div = value;
        self
    }

    fn cap(mut self, value: f64) -> Self {
        self.cap = value;
        self
    }

    fn min(mut self, value: f64) -> Self {
        self.minimum = value;
        self
    }

    fn max(mut self, value: f64) -> Self {
        self.maximum = value;
        self
    }
}

/// `_text`: one text row (the music track lists).
fn text_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    valid: fn(&str) -> bool,
    cond: Cond,
) {
    let value = dict
        .borrow()
        .get(field)
        .map(|value| music_text(&value))
        .unwrap_or_default();
    let dict_cons = dict.clone();
    let on_change = Rc::new(move |text: String| {
        set_at(&dict_cons, field, music_value(&text));
    });
    if let Some(row) = build_text_row(state, key, &value, Rc::new(valid), on_change, cond) {
        add_current(state, &row, true);
    }
}

/// `setupMain`: every category and row, in Java order.
fn setup_main(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    {
        let mut st = state.borrow_mut();
        st.categories.clear();
        st.rows.clear();
        st.team_rows.clear();
        st.current = None;
    }
    let Some(mut main) = state.borrow().main.clone() else {
        return;
    };
    main.bind_mut().clear();
    main.bind_mut().row();
    let erekir_env = planet_env("erekir");
    let teams = base_teams();
    {
        let mut st = state.borrow_mut();
        st.erekir_env = erekir_env;
        st.teams = teams;
    }
    let rules = state.borrow().rules.clone();

    // category("waves") ---------------------------------------------------
    category(state, "waves");
    check_rule(state, &rules, "@rules.waves", "waves", false, always());
    check_rule(
        state,
        &rules,
        "@rules.wavesending",
        "waveSending",
        true,
        flag(&rules, "waves", false),
    );
    check_rule(
        state,
        &rules,
        "@rules.wavetimer",
        "waveTimer",
        true,
        flag(&rules, "waves", false),
    );
    check_rule(
        state,
        &rules,
        "@rules.waitForWaveToEnd",
        "waitEnemies",
        false,
        and(
            flag(&rules, "waves", false),
            flag(&rules, "waveTimer", true),
        ),
    );
    check_rule(
        state,
        &rules,
        "@rules.randomwaveai",
        "randomWaveAi",
        false,
        flag(&rules, "waves", false),
    );
    check_rule(
        state,
        &rules,
        "@rules.wavespawnatcores",
        "wavesSpawnAtCores",
        true,
        flag(&rules, "waves", false),
    );
    check_rule(
        state,
        &rules,
        "@rules.airUseSpawns",
        "airUseSpawns",
        false,
        flag(&rules, "waves", false),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.wavelimit", "winWave", 0.0)
            .integer()
            .float_store(),
        flag(&rules, "waves", false),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.wavespacing", "waveSpacing", 7200.0)
            .scale(60.0)
            .div(60.0)
            .min(1.0),
        and(
            flag(&rules, "waves", false),
            flag(&rules, "waveTimer", true),
        ),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.initialwavespacing", "initialWaveSpacing", 0.0)
            .scale(60.0)
            .div(60.0),
        and(
            flag(&rules, "waves", false),
            flag(&rules, "waveTimer", true),
        ),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.dropzoneradius", "dropZoneRadius", 300.0)
            .scale(TILESIZE)
            .div(TILESIZE),
        flag(&rules, "waves", false),
    );

    // category("resourcesbuilding") ---------------------------------------
    category(state, "resourcesbuilding");
    check_rule(
        state,
        &rules,
        "@rules.alloweditworldprocessors",
        "allowEditWorldProcessors",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.infiniteresources",
        "infiniteResources",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.corebuildandconfig",
        "coreBuildAndConfig",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.onlydepositcore",
        "onlyDepositCore",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.coreunloaders",
        "allowCoreUnloaders",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.derelictrepair",
        "derelictRepair",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.reactorexplosions",
        "reactorExplosions",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.schematic",
        "schematicsAllowed",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.coreincinerates",
        "coreIncinerates",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.cleanupdeadteams",
        "cleanupDeadTeams",
        true,
        flag(&rules, "pvp", false),
    );
    check_rule(
        state,
        &rules,
        "@rules.disableworldprocessors",
        "disableWorldProcessors",
        false,
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.buildcostmultiplier", "buildCostMultiplier", 1.0),
        not(flag(&rules, "infiniteResources", false)),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.buildspeedmultiplier", "buildSpeedMultiplier", 1.0)
            .min(0.001)
            .max(50.0),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new(
            "@rules.deconstructrefundmultiplier",
            "deconstructRefundMultiplier",
            0.5,
        )
        .max(1.0),
        not(flag(&rules, "infiniteResources", false)),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.blockhealthmultiplier", "blockHealthMultiplier", 1.0),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.blockdamagemultiplier", "blockDamageMultiplier", 1.0),
        always(),
    );
    if matches_key(state, "@configure") {
        let mut button = wide_button("@configure", "boxSmall", None, 300.0);
        {
            let target = dialog.to_gd();
            let state_for = state.clone();
            let callable =
                Callable::from_fn("custom_rules_loadout", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    open_loadout(&mut bound, &state_for);
                });
            button.connect("pressed", &callable);
        }
        push_current(state, &button);
    }
    if matches_key(state, "@bannedblocks") {
        let mut button = wide_button("@bannedblocks", "cancelSmall", None, 300.0);
        {
            let target = dialog.to_gd();
            let state_for = state.clone();
            let callable =
                Callable::from_fn("custom_rules_banned_blocks", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    open_banned(&mut bound, &state_for, "block");
                });
            button.connect("pressed", &callable);
        }
        push_current(state, &button);
    }
    check_rule(
        state,
        &rules,
        "@rules.hidebannedblocks",
        "hideBannedBlocks",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@bannedblocks.whitelist",
        "blockWhitelist",
        false,
        always(),
    );

    // category("unit") ----------------------------------------------------
    category(state, "unit");
    check_rule(
        state,
        &rules,
        "@rules.unitcapvariable",
        "unitCapVariable",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.unitpayloadsexplode",
        "unitPayloadsExplode",
        false,
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.unitcap", "unitCap", 0.0)
            .integer()
            .float_store()
            .min(-999.0)
            .max(999.0),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new(
            "@rules.unitfactoryactivation",
            "unitFactoryActivationDelay",
            0.0,
        )
        .scale(60.0)
        .div(60.0),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.unitdamagemultiplier", "unitDamageMultiplier", 1.0),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new(
            "@rules.unitcrashdamagemultiplier",
            "unitCrashDamageMultiplier",
            1.0,
        ),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new(
            "@rules.unitminespeedmultiplier",
            "unitMineSpeedMultiplier",
            1.0,
        ),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new(
            "@rules.unitbuildspeedmultiplier",
            "unitBuildSpeedMultiplier",
            1.0,
        )
        .max(50.0),
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.unitcostmultiplier", "unitCostMultiplier", 1.0),
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.logicunitcontrol",
        "logicUnitControl",
        true,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.logicunitbuild",
        "logicUnitBuild",
        true,
        flag(&rules, "logicUnitControl", true),
    );
    check_rule(
        state,
        &rules,
        "@rules.logicunitdeconstruct",
        "logicUnitDeconstruct",
        false,
        flag(&rules, "logicUnitControl", true),
    );
    if matches_key(state, "@bannedunits") {
        let mut button = wide_button("@bannedunits", "unitsSmall", None, 300.0);
        {
            let target = dialog.to_gd();
            let state_for = state.clone();
            let callable =
                Callable::from_fn("custom_rules_banned_units", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    open_banned(&mut bound, &state_for, "unit");
                });
            button.connect("pressed", &callable);
        }
        push_current(state, &button);
    }
    check_rule(
        state,
        &rules,
        "@bannedunits.whitelist",
        "unitWhitelist",
        false,
        always(),
    );

    // category("enemy") ---------------------------------------------------
    category(state, "enemy");
    check_rule(
        state,
        &rules,
        "@rules.attack",
        "attackMode",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.corecapture",
        "coreCapture",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.placerangecheck",
        "placeRangeCheck",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.polygoncoreprotection",
        "polygonCoreProtection",
        false,
        always(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.enemycorebuildradius", "enemyCoreBuildRadius", 400.0)
            .scale(TILESIZE)
            .div(TILESIZE)
            .cap(200.0),
        not(flag(&rules, "polygonCoreProtection", false)),
    );

    // category("environment") ---------------------------------------------
    category(state, "environment");
    check_rule(
        state,
        &rules,
        "@rules.pauseDisabled",
        "pauseDisabled",
        false,
        always(),
    );
    check_rule(
        state,
        &rules,
        "@rules.explosions",
        "damageExplosions",
        true,
        always(),
    );
    check_rule(state, &rules, "@rules.fire", "fire", true, always());
    check_rule(state, &rules, "@rules.fog", "fog", false, always());
    check_rule(state, &rules, "@rules.lighting", "lighting", true, always());
    check_rule(
        state,
        &rules,
        "@rules.limitarea",
        "limitMapArea",
        false,
        not(game_cond()),
    );
    let limit_area = and(flag(&rules, "limitMapArea", false), not(game_cond()));
    number_rule(
        state,
        &rules,
        NumberSpec::new("x", "limitX", 0.0)
            .integer()
            .float_store()
            .max(10_000.0),
        limit_area.clone(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("y", "limitY", 0.0)
            .integer()
            .float_store()
            .max(10_000.0),
        limit_area.clone(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("w", "limitWidth", 1.0)
            .integer()
            .float_store()
            .max(10_000.0),
        limit_area.clone(),
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("h", "limitHeight", 1.0)
            .integer()
            .float_store()
            .max(10_000.0),
        limit_area,
    );
    number_rule(
        state,
        &rules,
        NumberSpec::new("@rules.solarmultiplier", "solarMultiplier", 1.0),
        always(),
    );
    if matches_key(state, "@rules.weather") {
        let mut button = wide_button("@rules.weather", "rainSmall", None, 250.0);
        {
            let target = dialog.to_gd();
            let state_for = state.clone();
            let callable =
                Callable::from_fn("custom_rules_weather", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    open_weather(&mut bound, &state_for);
                });
            button.connect("pressed", &callable);
        }
        push_current(state, &button);
    }

    // category("light") ---------------------------------------------------
    category(state, "light");
    if matches_key(state, "@rules.ambientlight") {
        let row = ambient_light_button(state);
        if let Some(mut current) = state.borrow().current.clone() {
            let control: Gd<Control> = row.upcast();
            current.bind_mut().add(&control).left();
            current.bind_mut().row();
        }
    }
    check_rule(
        state,
        &rules,
        "@rules.lighting.unitlight",
        "unitLight",
        true,
        always(),
    );

    // category("music") ---------------------------------------------------
    category(state, "music");
    check_rule(
        state,
        &rules,
        "@rules.alwaysplaymusic",
        "alwaysPlayMusic",
        false,
        not(flag(&rules, "disableMusic", false)),
    );
    check_rule(
        state,
        &rules,
        "@rules.nomusic",
        "disableMusic",
        false,
        always(),
    );
    text_rule(
        state,
        &rules,
        "@rules.ambientmusic",
        "ambientMusic",
        music_valid,
        not(flag(&rules, "disableMusic", false)),
    );
    text_rule(
        state,
        &rules,
        "@rules.darkmusic",
        "darkMusic",
        music_valid,
        not(flag(&rules, "disableMusic", false)),
    );

    // category("planet") --------------------------------------------------
    category(state, "planet");
    if matches_key(state, "@rules.title.planet") {
        let table = planet_table(state);
        if let Some(mut current) = state.borrow().current.clone() {
            let control: Gd<Control> = table.upcast();
            current.bind_mut().add(&control).left();
            current.bind_mut().row();
        }
    }

    // category("teams") ---------------------------------------------------
    category(state, "teams");
    let show_rule_edit = state.borrow().show_rule_edit;
    if show_rule_edit {
        check_rule(
            state,
            &rules,
            "@rules.allowedit",
            "allowEditRules",
            false,
            always(),
        );
    }
    team_rule(state, &rules, "@rules.playerteam", "defaultTeam", 1);
    team_rule(state, &rules, "@rules.enemyteam", "waveTeam", 2);
    let team_ids: Vec<i64> = state
        .borrow()
        .teams
        .iter()
        .map(|team| dict_i64(team, "id", 0))
        .collect();
    for team in team_ids {
        build_team_collapser(state, &rules, team);
    }

    // `addToMain`: only categories with rows get their title + accent rule.
    let categories = state.borrow().categories.clone();
    for (name, table) in categories {
        if table.get_child_count() == 0 {
            continue;
        }
        let title = kit::t(&format!("@rules.title.{name}"));
        let label = factory::styled_label(
            &kit::markup_text(&title.to_string()).to_string(),
            "techLabel",
        );
        main.bind_mut().add(&label).left().grow_x().pad(5.0);
        main.bind_mut().row();
        // code-instantiated: the accent rule under each category title.
        let mut rule = ColorRect::new_alloc();
        rule.set_color(kit::styles::ACCENT);
        rule.set_custom_minimum_size(Vector2::new(0.0, RULE_HEIGHT));
        // `set_pad_right(100f).pad(5f)`: the trailing `pad(5)` overwrites the
        // right inset, exactly like the GDScript chain.
        main.bind_mut()
            .add(&rule)
            .left()
            .grow_x()
            .pad_right(100.0)
            .pad(5.0);
        main.bind_mut().row();
        main.bind_mut().add(&table).left().grow_x();
        main.bind_mut().row();
    }
    // A rebuilt table needs an immediate layout pass.
    let node: Gd<Node> = main.clone().upcast();
    sort_all(&node);
    refresh_conditions(state);
}

/// Bottom-up `sort_now` over every nested table (a rebuild must place the new
/// cells in the same frame).
fn sort_all(node: &Gd<Node>) {
    if let Ok(mut table) = node.clone().try_cast::<MindTable>() {
        for child in node.get_children().iter_shared() {
            sort_all(&child);
        }
        table.bind_mut().sort_now();
    }
}

/// `category(name)`: a fresh left-aligned rule table.
fn category(state: &Rc<RefCell<CustomRulesState>>, name: &str) {
    let mut table = factory::table();
    table.set_h_size_flags(SizeFlags::EXPAND_FILL);
    // `current.left().defaults().fillX().expandX().left().pad(5)`
    // (`CustomRulesDialog.java:377-383`): every rule cell has a 5 px pad.
    {
        let mut bound = table.bind_mut();
        let defaults = bound.defaults();
        defaults.pad_left = 5.0;
        defaults.pad_right = 5.0;
        defaults.pad_top = 5.0;
        defaults.pad_bottom = 5.0;
    }
    {
        let mut st = state.borrow_mut();
        st.current = Some(table.clone());
        st.categories.push((name.to_string(), table));
    }
}

/// Bundle text with markup tags stripped for plain `Label`/`Button` widgets.
fn plain(key: &str) -> String {
    let text = kit::t(key);
    let mut out = String::new();
    let mut depth = 0i32;
    for character in text.to_string().chars() {
        match character {
            '[' => depth += 1,
            ']' => depth = (depth - 1).max(0),
            _ => {
                if depth == 0 {
                    out.push(character);
                }
            }
        }
    }
    out
}

/// Whether the bundle carries a key (`Core.bundle.has`).
fn has_key(key: &str) -> bool {
    let Some(assets) = kit::assets() else {
        return false;
    };
    let resolved = key.strip_prefix('@').unwrap_or(key);
    assets.bind().bundle_get(GString::from(resolved)) != resolved
}

/// `ruleInfo`: the `<key>.info` tooltip when the bundle has that key.
fn rule_info(control: &mut Gd<Control>, key: &str) {
    let info = format!("{}.info", key.strip_prefix('@').unwrap_or(key));
    if has_key(&info) {
        control.set_tooltip_text(&GString::from(plain(&format!("@{info}")).as_str()));
    }
}

/// `Core.bundle.get(key).toLowerCase().contains(ruleSearch)`.
fn matches_key(state: &Rc<RefCell<CustomRulesState>>, key: &str) -> bool {
    let search = state.borrow().search.clone();
    if search.is_empty() {
        return true;
    }
    kit::t(key).to_string().to_lowercase().contains(&search)
}

/// Adds a built row to a `MindTable` (`add` + `row`) or a plain container.
///
/// `pad_top_zero`: the table-built rows (`number`, `team`) end in `.padTop(0)`
/// (`CustomRulesDialog.java:404, 437, 452, 467`); checks keep the category's
/// `pad(5)` (`:478`).
fn row_into(target: &Gd<Control>, row: &Gd<Control>, pad_top_zero: bool) {
    // `main.left().defaults().fillX().left()` (`CustomRulesDialog.java:144`).
    if let Ok(mut table) = row.clone().try_cast::<MindTable>() {
        table.bind_mut().set_table_left(true);
    }
    if let Ok(mut table) = target.clone().try_cast::<MindTable>() {
        {
            let mut bound = table.bind_mut();
            let cell = bound.add(row).left().grow_x();
            if pad_top_zero {
                // Dropping the guard after `padTop(0)` re-runs the layout pass.
                cell.pad_top(0.0);
            }
        }
        table.bind_mut().row();
        return;
    }
    let mut parent: Gd<Node> = target.clone().upcast();
    let mut row = row.clone();
    row.set_h_size_flags(SizeFlags::EXPAND_FILL);
    parent.add_child(&row);
}

/// Adds a row to the current category table.
fn add_current(state: &Rc<RefCell<CustomRulesState>>, row: &Gd<MindTable>, pad_top_zero: bool) {
    let Some(current) = state.borrow().current.clone() else {
        return;
    };
    let target: Gd<Control> = current.upcast();
    let control: Gd<Control> = row.clone().upcast();
    row_into(&target, &control, pad_top_zero);
}

/// Adds a row into a plain container (the team collapsers).
fn add_in(target: &Gd<Control>, row: &Gd<MindTable>, pad_top_zero: bool) {
    let control: Gd<Control> = row.clone().upcast();
    row_into(target, &control, pad_top_zero);
}

/// `_register_row` + the first `apply`.
fn register_row(state: &Rc<RefCell<CustomRulesState>>, cond: Cond, apply: Rc<dyn Fn(bool)>) {
    let enabled = cond();
    apply(enabled);
    state.borrow_mut().rows.push(RuleRow { cond, apply });
}

/// Re-evaluates every row condition (Java does this each frame; the port runs it
/// on every edit and rebuild).
fn refresh_conditions(state: &Rc<RefCell<CustomRulesState>>) {
    let rows: Vec<RowEntry> = state
        .borrow()
        .rows
        .iter()
        .map(|row| (row.cond.clone(), row.apply.clone()))
        .collect();
    for (cond, apply) in rows {
        apply(cond());
    }
}

/// One `check` row (`check(text, cons, prov, condition)` + `ruleInfo`).
fn build_check_row(
    state: &Rc<RefCell<CustomRulesState>>,
    key: &str,
    checked: bool,
    on_change: Rc<dyn Fn(bool)>,
    cond: Cond,
) -> Option<Gd<MindTable>> {
    if !matches_key(state, key) {
        return None;
    }
    let mut row = factory::table();
    // code-instantiated: one themed check per rules row (`Elems.check`).
    let mut check = MindCheck::new_alloc();
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_check_toggle", move |args: &[&Variant]| {
            let value = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            on_change(value);
            refresh_conditions(&state_for);
        });
        check
            .bind_mut()
            .setup(GString::from(plain(key).as_str()), checked, callable);
    }
    // `current.check(text, cons)` is a plain `CheckBox` (`Styles.defaultCheck`,
    // no background) left-aligned at its own width (`CustomRulesDialog.java:
    // 478-479`), not `Elems.check`'s grayPanel strip.
    for style in [
        "normal",
        "hover",
        "pressed",
        "hover_pressed",
        "disabled",
        "focus",
    ] {
        let empty = StyleBoxEmpty::new_gd();
        check.add_theme_stylebox_override(style, &empty);
    }
    check.bind_mut().set_content_inset(0.0, 0.0);
    {
        let mut control: Gd<Control> = check.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&check).left();
    let check_for_apply = check.clone();
    register_row(
        state,
        cond,
        Rc::new(move |enabled: bool| {
            let mut button: Gd<Button> = check_for_apply.clone().upcast();
            button.set_disabled(!enabled);
            button.set_modulate(if enabled {
                Color::from_rgba(1.0, 1.0, 1.0, 1.0)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.6)
            });
        }),
    );
    Some(row)
}

/// One `number`/`numberi` row (width-120 field, min/max validated).
///
/// The argument list mirrors `_make_number_row(key, integer, cons, prov, cond,
/// min, max)`; the value/cons pair replaces the two GDScript callables.
#[allow(clippy::too_many_arguments)]
fn build_number_row(
    state: &Rc<RefCell<CustomRulesState>>,
    key: &str,
    integer: bool,
    value: f64,
    minimum: f64,
    maximum: f64,
    on_change: Rc<dyn Fn(NumberValue)>,
    cond: Cond,
) -> Option<Gd<MindTable>> {
    if !matches_key(state, key) {
        return None;
    }
    let mut row = factory::table();
    let label = factory::label(&kit::tm(key).to_string());
    {
        let mut control: Gd<Control> = label.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&label).left().pad_right(5.0);
    let mut field = factory::field(&number_text(value, integer));
    let mut minimum_size = field.get_custom_minimum_size();
    minimum_size.x = 120.0;
    field.set_custom_minimum_size(minimum_size);
    field.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    {
        let state_for = state.clone();
        let field_for = field.clone();
        let callable = Callable::from_fn("rules_number_changed", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let trimmed = text.trim().to_string();
            if !parse_number(&trimmed, integer) {
                invalid_field(&field_for, true);
                return;
            }
            let parsed = trimmed.parse::<f64>().unwrap_or(0.0);
            if parsed < minimum || parsed > maximum {
                invalid_field(&field_for, true);
                return;
            }
            invalid_field(&field_for, false);
            if integer {
                on_change(NumberValue::Int(parsed as i64));
            } else {
                on_change(NumberValue::Float(parsed));
            }
            refresh_conditions(&state_for);
        });
        field.connect("text_changed", &callable);
    }
    row.bind_mut()
        .add(&field)
        .left()
        .pad_right(if integer { 100.0 } else { 50.0 });
    let field_for_apply = field.clone();
    let label_for_apply = label.clone();
    register_row(
        state,
        cond,
        Rc::new(move |enabled: bool| {
            let mut field = field_for_apply.clone();
            field.set_editable(enabled);
            let mut label = rich(&label_for_apply);
            label.add_theme_color_override(
                "default_color",
                if enabled {
                    Color::WHITE
                } else {
                    Color::from_rgba(0.6, 0.6, 0.6, 1.0)
                },
            );
        }),
    );
    Some(row)
}

/// One `text` row (width-300 field, `valid` predicate).
fn build_text_row(
    state: &Rc<RefCell<CustomRulesState>>,
    key: &str,
    value: &str,
    valid: Rc<dyn Fn(&str) -> bool>,
    on_change: Rc<dyn Fn(String)>,
    cond: Cond,
) -> Option<Gd<MindTable>> {
    if !matches_key(state, key) {
        return None;
    }
    let mut row = factory::table();
    let label = factory::label(&kit::tm(key).to_string());
    {
        let mut control: Gd<Control> = label.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&label).left().pad_right(5.0);
    let mut field = factory::field(value);
    let mut minimum_size = field.get_custom_minimum_size();
    minimum_size.x = 300.0;
    field.set_custom_minimum_size(minimum_size);
    field.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    {
        let state_for = state.clone();
        let field_for = field.clone();
        let callable = Callable::from_fn("rules_text_changed", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let ok = valid(&text);
            invalid_field(&field_for, !ok);
            if !ok {
                return;
            }
            on_change(text);
            refresh_conditions(&state_for);
        });
        field.connect("text_changed", &callable);
    }
    row.bind_mut().add(&field).left().pad_right(50.0);
    let field_for_apply = field.clone();
    let label_for_apply = label.clone();
    register_row(
        state,
        cond,
        Rc::new(move |enabled: bool| {
            let mut field = field_for_apply.clone();
            field.set_editable(enabled);
            let mut label = rich(&label_for_apply);
            label.add_theme_color_override(
                "default_color",
                if enabled {
                    Color::WHITE
                } else {
                    Color::from_rgba(0.6, 0.6, 0.6, 1.0)
                },
            );
        }),
    );
    Some(row)
}

/// A 300/250-wide category action button (`current.button(...).width(...)`).
fn wide_button(key: &str, icon: &str, listener: Option<Callable>, width: f32) -> Gd<Button> {
    let mut button = factory::icon_button(
        icon,
        &plain(key),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let mut minimum = button.get_custom_minimum_size();
    minimum.x = width;
    button.set_custom_minimum_size(minimum);
    button.set_text_alignment(HorizontalAlignment::LEFT);
    if let Some(listener) = listener
        && listener.is_valid()
    {
        button.connect("pressed", &listener);
    }
    button
}

/// Adds a built control to the current category table (`add(...).left(); row()`).
fn push_current<N: Inherits<Control>>(state: &Rc<RefCell<CustomRulesState>>, control: &Gd<N>) {
    let Some(mut table) = state.borrow().current.clone() else {
        return;
    };
    table.bind_mut().add(control).left();
    table.bind_mut().row();
}

/// `Team.baseTeams` rows (`MindEditor.palette_teams`; `[{id, name, color}]`).
fn base_teams() -> Teams {
    let Some(mut editor) = kit::editor() else {
        return Vec::new();
    };
    if !editor.has_method("palette_teams") {
        return Vec::new();
    }
    editor
        .call("palette_teams", &[])
        .try_to::<Array<VarDictionary>>()
        .unwrap_or_default()
        .iter_shared()
        .collect()
}

/// `Team.<id>.localizedName()` (`team.<name>.name`).
fn team_name(state: &Rc<RefCell<CustomRulesState>>, id: i64) -> String {
    let teams = state.borrow().teams.clone();
    for team in teams {
        if dict_i64(&team, "id", 0) == id {
            return kit::t(&format!("@team.{}.name", dict_text(&team, "name"))).to_string();
        }
    }
    id.to_string()
}

/// `Team.coloredName()` tint (the packed `Team.color`).
fn team_font_color(state: &Rc<RefCell<CustomRulesState>>, id: i64) -> Color {
    let teams = state.borrow().teams.clone();
    for team in teams {
        if dict_i64(&team, "id", 0) == id {
            return rgba8_color(dict_i64(&team, "color", 0xFFFF_FFFF));
        }
    }
    Color::WHITE
}

/// One base team's `TeamRule` (`rules.teams.get(team)`, created on demand).
fn team_rules(rules: &Rc<RefCell<VarDictionary>>, team: i64) -> Rc<RefCell<VarDictionary>> {
    let mut teams = rules
        .borrow()
        .get("teams")
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    let key = team.to_string();
    let dict = teams
        .get(key.as_str())
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    teams.set(key.as_str(), &dict.to_variant());
    rules.borrow_mut().set("teams", &teams.to_variant());
    Rc::new(RefCell::new(dict))
}

/// `team(text, cons, prov)`: a label plus one 60x60 `squareTogglei` per base team.
fn team_rule(
    state: &Rc<RefCell<CustomRulesState>>,
    dict: &Rc<RefCell<VarDictionary>>,
    key: &str,
    field: &'static str,
    default: i64,
) {
    if !matches_key(state, key) {
        return;
    }
    let selected = dict_at_i64(dict, field, default);
    let teams = state.borrow().teams.clone();
    let mut row = factory::table();
    let label = factory::label(&kit::tm(key).to_string());
    {
        let mut control: Gd<Control> = label.clone().upcast();
        rule_info(&mut control, key);
    }
    row.bind_mut().add(&label).left().pad_right(5.0);
    let buttons: TeamButtons = Rc::new(RefCell::new(Vec::new()));
    for team in &teams {
        let id = dict_i64(team, "id", 0);
        let color = rgba8_color(dict_i64(team, "color", 0xFFFF_FFFF));
        // code-instantiated: one tinted toggle per base team (`Team.baseTeams`).
        let mut button = Button::new_alloc();
        button.set_toggle_mode(true);
        button.set_pressed_no_signal(selected == id);
        button.set_theme_type_variation("squareTogglei");
        button.set_custom_minimum_size(Vector2::new(60.0, 60.0));
        if let Some(texture) = kit::icon_texture("whiteui") {
            button.set_button_icon(&texture);
        }
        for override_name in [
            "icon_normal_color",
            "icon_pressed_color",
            "icon_hover_color",
        ] {
            button.add_theme_color_override(override_name, color);
        }
        button.set_tooltip_text(&kit::t(&format!("@team.{}.name", dict_text(team, "name"))));
        {
            let dict_cons = dict.clone();
            let state_for = state.clone();
            let buttons_for = buttons.clone();
            let callable = Callable::from_fn("rules_team_pick", move |_args: &[&Variant]| {
                set_at(&dict_cons, field, (id as f64).to_variant());
                for (button, button_id) in buttons_for.borrow().iter() {
                    let mut button = button.clone();
                    button.set_pressed_no_signal(*button_id == id);
                }
                refresh_conditions(&state_for);
            });
            button.connect("pressed", &callable);
        }
        row.bind_mut().add(&button).pad(1.0);
        buttons.borrow_mut().push((button, id));
    }
    add_current(state, &row, true);
}

/// `_build_team_collapser`: one base team's `TeamRule` rows behind a header.
fn build_team_collapser(
    state: &Rc<RefCell<CustomRulesState>>,
    rules: &Rc<RefCell<VarDictionary>>,
    team: i64,
) {
    let mut row = factory::table();
    let mut header = Button::new_alloc();
    header.set_text(&GString::from(team_name(state, team).as_str()));
    header.add_theme_color_override("font_color", team_font_color(state, team));
    header.set_toggle_mode(true);
    header.set_theme_type_variation("togglet");
    header.set_custom_minimum_size(Vector2::new(260.0, 55.0));
    header.set_text_alignment(HorizontalAlignment::LEFT);
    // code-instantiated: one collapser body per base team.
    let mut content = VBoxContainer::new_alloc();
    content.set_visible(false);
    {
        let header_for = header.clone();
        let mut content_for = content.clone();
        let mut row_for = row.clone();
        let callable = Callable::from_fn("rules_team_header", move |_args: &[&Variant]| {
            content_for.set_visible(header_for.is_pressed());
            row_for.queue_sort();
            row_for.bind_mut().sort_now();
        });
        header.connect("pressed", &callable);
    }
    row.bind_mut().add(&header).left();
    row.bind_mut().add(&content).left().grow_x();
    state
        .borrow_mut()
        .team_rows
        .push((team, content.clone(), header));
    let team_dict = team_rules(rules, team);
    let target: Gd<Control> = content.clone().upcast();
    let erekir_env = state.borrow().erekir_env;

    check_num_in(
        state,
        &target,
        &team_dict,
        "@rules.blockhealthmultiplier",
        "blockHealthMultiplier",
        1.0,
        always(),
    );
    check_num_in(
        state,
        &target,
        &team_dict,
        "@rules.blockdamagemultiplier",
        "blockDamageMultiplier",
        1.0,
        always(),
    );
    {
        let rules_for = rules.clone();
        check_rule_in(
            state,
            &target,
            &team_dict,
            "@rules.rtsai",
            "rtsAi",
            false,
            Rc::new(move || team != dict_at_i64(&rules_for, "defaultTeam", 1)),
        );
    }
    let rts_ai = flag(&team_dict, "rtsAi", false);
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.rtsminsquadsize", "rtsMinSquad", 0.0)
            .integer()
            .max(100.0),
        rts_ai.clone(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.rtsmaxsquadsize", "rtsMaxSquad", 1.0)
            .integer()
            .min(1.0)
            .max(1000.0),
        rts_ai.clone(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.rtsminattackweight", "rtsMinWeight", 0.0),
        rts_ai,
    );
    {
        let rules_for = rules.clone();
        check_rule_in(
            state,
            &target,
            &team_dict,
            "@rules.buildai",
            "buildAi",
            false,
            Rc::new(move || {
                team != dict_at_i64(&rules_for, "defaultTeam", 1)
                    && dict_at_i64(&rules_for, "env", 233) != erekir_env
                    && !dict_at(&rules_for, "pvp", false)
            }),
        );
    }
    {
        let rules_for = rules.clone();
        let team_for = team_dict.clone();
        let cond = Rc::new(move || {
            dict_at(&team_for, "buildAi", false)
                && dict_at_i64(&rules_for, "env", 233) != erekir_env
                && !dict_at(&rules_for, "pvp", false)
        });
        number_rule_in(
            state,
            &target,
            &team_dict,
            NumberSpec::new("@rules.buildaitier", "buildAiTier", 0.0)
                .integer()
                .float_store()
                .max(1.0),
            cond,
        );
    }
    check_rule_in(
        state,
        &target,
        &team_dict,
        "@rules.protectcores",
        "protectCores",
        false,
        always(),
    );
    {
        let rules_for = rules.clone();
        let team_for = team_dict.clone();
        let cond = Rc::new(move || {
            !dict_at(&rules_for, "polygonCoreProtection", false)
                && dict_at(&team_for, "protectCores", false)
        });
        number_rule_in(
            state,
            &target,
            &team_dict,
            NumberSpec::new("@rules.extracorebuildradius", "extraCoreBuildRadius", 0.0)
                .scale(TILESIZE)
                .div(TILESIZE)
                .cap(200.0),
            cond,
        );
    }
    check_rule_in(
        state,
        &target,
        &team_dict,
        "@rules.checkplacement",
        "checkPlacement",
        false,
        always(),
    );
    check_rule_in(
        state,
        &target,
        &team_dict,
        "@rules.infiniteresources",
        "infiniteResources",
        false,
        always(),
    );
    check_rule_in(
        state,
        &target,
        &team_dict,
        "@rules.fillitems",
        "fillItems",
        false,
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.buildspeedmultiplier", "buildSpeedMultiplier", 1.0)
            .min(0.001)
            .max(50.0),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new(
            "@rules.unitfactoryactivation",
            "unitFactoryActivationDelay",
            0.0,
        )
        .scale(60.0)
        .div(60.0),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.unitdamagemultiplier", "unitDamageMultiplier", 1.0),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new(
            "@rules.unitcrashdamagemultiplier",
            "unitCrashDamageMultiplier",
            1.0,
        ),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new(
            "@rules.unitminespeedmultiplier",
            "unitMineSpeedMultiplier",
            1.0,
        ),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new(
            "@rules.unitbuildspeedmultiplier",
            "unitBuildSpeedMultiplier",
            1.0,
        )
        .min(0.001)
        .max(50.0),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.unitcostmultiplier", "unitCostMultiplier", 1.0),
        always(),
    );
    number_rule_in(
        state,
        &target,
        &team_dict,
        NumberSpec::new("@rules.unithealthmultiplier", "unitHealthMultiplier", 1.0),
        always(),
    );
    if content.get_child_count() == 0 {
        // `if(!current.hasChildren()) teamRules.clear()`: an empty (or fully
        // filtered-out) team row never joins the category.
        return;
    }
    // `wasCurrent.add(teamRules).row()`: the collapser rows live in the "teams"
    // category table, not directly in `_main`.
    let control: Gd<Control> = row.upcast();
    if let Some(current) = state.borrow().current.clone() {
        let target: Gd<Control> = current.upcast();
        row_into(&target, &control, true);
    }
}

/// `@rules.ambientlight` colour picker row (`ui.picker.show`).
fn ambient_light_button(state: &Rc<RefCell<CustomRulesState>>) -> Gd<MindTable> {
    let mut row = factory::table();
    // code-instantiated: the swatch reflects `rules.ambientLight`.
    let mut swatch = ColorRect::new_alloc();
    swatch.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    swatch.set_color(ambient_color(state));
    row.bind_mut().add(&swatch).pad(4.0);
    let label = factory::label(&kit::t("@rules.ambientlight").to_string());
    row.bind_mut().add(&label).left().pad_right(10.0);
    let mut button = Button::new_alloc();
    button.set_text("");
    button.set_custom_minimum_size(Vector2::new(250.0, 50.0));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_ambient", move |_args: &[&Variant]| {
            let rules = state_for.borrow().rules.clone();
            let swatch_for = swatch.clone();
            let listener: Rc<dyn Fn(Color)> = Rc::new(move |color: Color| {
                let hex = format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    (color.r * 255.0) as i32,
                    (color.g * 255.0) as i32,
                    (color.b * 255.0) as i32,
                    (color.a * 255.0) as i32
                );
                rules
                    .borrow_mut()
                    .set("ambientLight", &GString::from(hex.as_str()).to_variant());
                if !swatch_for.is_instance_valid() {
                    return;
                }
                let mut swatch = swatch_for.clone();
                swatch.set_color(color);
            });
            show_color_picker(&state_for, listener);
        });
        button.connect("pressed", &callable);
    }
    row.bind_mut().add(&button).left();
    row
}

/// `rules.ambientLight` as a colour (`_ambient_color`).
fn ambient_color(state: &Rc<RefCell<CustomRulesState>>) -> Color {
    let hex = state
        .borrow()
        .rules
        .borrow()
        .get("ambientLight")
        .map(|value| variant_text(&value))
        .unwrap_or_else(|| String::from("00000000"));
    if hex.len() == 8 {
        let channel = |index: usize| -> u8 {
            hex.get(index..index + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .unwrap_or(0)
        };
        return Color::from_rgba8(channel(0), channel(2), channel(4), channel(6));
    }
    Color::from_rgba(0.0, 0.0, 0.0, 0.0)
}

/// `ui.picker.show(col, col::set)` (`ColorPicker` signal `color_selected`).
fn show_color_picker(state: &Rc<RefCell<CustomRulesState>>, listener: Rc<dyn Fn(Color)>) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    let Some(mut ui_root) = kit::node("/root/Spine/Ui/UiRoot") else {
        return;
    };
    if !ui_root.has_method("dialog") {
        return;
    }
    let _ = ui.call(
        "open_dialog",
        &[
            GString::from("picker").to_variant(),
            GString::new().to_variant(),
        ],
    );
    let Ok(mut node) = ui_root
        .call("dialog", &[GString::from("picker").to_variant()])
        .try_to::<Gd<Node>>()
    else {
        return;
    };
    if !node.has_signal("color_selected") {
        return;
    }
    let color = ambient_color(state);
    state.borrow_mut().picker_listener = Some(listener);
    if node.has_method("set_color") {
        node.call("set_color", &[color.to_variant()]);
    }
    if node.has_meta("_batch_i_picker_hook") {
        return;
    }
    let state_for = state.clone();
    let callable = Callable::from_fn("custom_rules_color_selected", move |args: &[&Variant]| {
        let color = args
            .first()
            .map(|value| (*value).to::<Color>())
            .unwrap_or(Color::WHITE);
        let listener = state_for.borrow().picker_listener.clone();
        if let Some(listener) = listener {
            listener(color);
        }
    });
    node.connect("color_selected", &callable);
    node.set_meta("_batch_i_picker_hook", &true.to_variant());
}

/// `category("planet")`: planet toggle buttons (`planet.applyRules(rules, true)`)
/// plus `@rules.anyenv`.
fn planet_table(state: &Rc<RefCell<CustomRulesState>>) -> Gd<MindTable> {
    let mut table = factory::table();
    let rows: Vec<VarDictionary> = campaign_call("rules_planets", &[])
        .and_then(|value| value.try_to::<Array<VarDictionary>>().ok())
        .unwrap_or_default()
        .iter_shared()
        .collect();
    let rules = state.borrow().rules.clone();
    let buttons: PlanetButtons = Rc::new(RefCell::new(Vec::new()));
    let selected_planet = rules
        .borrow()
        .get("planet")
        .map(|value| variant_text(&value))
        .unwrap_or_else(|| String::from("serpulo"));
    let mut index = 0;
    for row in &rows {
        let planet_name = dict_text(row, "name");
        // code-instantiated: one planet toggle per landable planet.
        let mut button = Button::new_alloc();
        button.set_text(&kit::t(&format!("@planet.{planet_name}.name")));
        button.set_theme_type_variation("flatTogglet");
        button.set_toggle_mode(true);
        button.set_pressed_no_signal(selected_planet == planet_name);
        button.set_custom_minimum_size(Vector2::new(140.0, 50.0));
        {
            let name_for = planet_name.clone();
            let rules_for = rules.clone();
            let state_for = state.clone();
            let buttons_for = buttons.clone();
            let callable = Callable::from_fn("rules_planet_pick", move |_args: &[&Variant]| {
                apply_planet(&rules_for, &name_for);
                for (button, name) in buttons_for.borrow().iter() {
                    let mut button = button.clone();
                    button.set_pressed_no_signal(*name == name_for);
                }
                refresh_conditions(&state_for);
            });
            button.connect("pressed", &callable);
        }
        table.bind_mut().add(&button).pad(1.0);
        buttons.borrow_mut().push((button, planet_name));
        index += 1;
        if index % 3 == 0 {
            table.bind_mut().row();
        }
    }
    let mut any = Button::new_alloc();
    any.set_text(&kit::t("@rules.anyenv"));
    any.set_theme_type_variation("flatTogglet");
    any.set_toggle_mode(true);
    any.set_pressed_no_signal(selected_planet == "sun");
    any.set_custom_minimum_size(Vector2::new(140.0, 50.0));
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let buttons_for = buttons.clone();
        let any_for = any.clone();
        let callable = Callable::from_fn("rules_anyenv", move |_args: &[&Variant]| {
            rules_for
                .borrow_mut()
                .set("attributes", &VarDictionary::new().to_variant());
            rules_for
                .borrow_mut()
                .set("env", &(default_env() as f64).to_variant());
            rules_for
                .borrow_mut()
                .set("planet", &GString::from("sun").to_variant());
            for (button, _) in buttons_for.borrow().iter() {
                let mut button = button.clone();
                button.set_pressed_no_signal(false);
            }
            let mut any_button: Gd<Button> = any_for.clone();
            any_button.set_pressed_no_signal(true);
            refresh_conditions(&state_for);
        });
        any.connect("pressed", &callable);
    }
    table.bind_mut().add(&any).pad(1.0);
    table
}

/// `planet.applyRules(rules, true)` through the campaign facade.
fn apply_planet(rules: &Rc<RefCell<VarDictionary>>, planet_name: &str) {
    let json = json_stringify(&rules.borrow());
    let Some(updated) = campaign_call(
        "apply_planet_rules",
        &[
            GString::from(planet_name).to_variant(),
            GString::from(json.as_str()).to_variant(),
        ],
    ) else {
        return;
    };
    let text = updated.to::<GString>().to_string();
    if text.is_empty() {
        return;
    }
    // `if parsed is Dictionary`: an unparseable reply leaves the rules alone, an
    // empty object clears them (the GDScript replaces the whole dictionary).
    let Ok(parsed) = Json::parse_string(&GString::from(text.as_str())).try_to::<VarDictionary>()
    else {
        return;
    };
    replace_rules(rules, &parsed);
}

/// `_planet_env(planet)` (`MindCampaign.planet_default_env`).
fn planet_env(planet_name: &str) -> i64 {
    campaign_call(
        "planet_default_env",
        &[GString::from(planet_name).to_variant()],
    )
    .map(|value| variant_i64(&value))
    .unwrap_or(0)
}

/// `_default_env()` (`MindCampaign.default_env`).
fn default_env() -> i64 {
    campaign_call("default_env", &[])
        .map(|value| variant_i64(&value))
        .unwrap_or(233)
}

/// `@configure`: the loadout editor (`LoadoutDialog.show(999999, rules.loadout,
/// i -> true, reset -> copper x100)`).
fn open_loadout(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@configure", 420.0);
    let rules = state.borrow().rules.clone();
    let stacks: VarArray = rules
        .borrow()
        .get("loadout")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    for stack in stacks.iter_shared() {
        let Ok(stack) = stack.try_to::<VarDictionary>() else {
            continue;
        };
        let stack = Rc::new(RefCell::new(stack));
        // code-instantiated: one amount row per loadout stack.
        let mut row = factory::table();
        let item = dict_at_text(&stack, "item");
        let label = factory::label(&item);
        row.bind_mut().add(&label).left().pad_right(6.0);
        let amount = dict_at_i64(&stack, "amount", 0);
        let mut field = factory::field(&amount.to_string());
        let mut minimum = field.get_custom_minimum_size();
        minimum.x = 120.0;
        field.set_custom_minimum_size(minimum);
        {
            let stack_for = stack.clone();
            let field_for = field.clone();
            let callable = Callable::from_fn("rules_loadout_amount", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| (*value).to::<GString>().to_string())
                    .unwrap_or_default();
                let trimmed = text.trim().to_string();
                if !parse_number(&trimmed, false)
                    || trimmed.parse::<f64>().unwrap_or(0.0) > 999_999.0
                {
                    invalid_field(&field_for, true);
                    return;
                }
                invalid_field(&field_for, false);
                let value = trimmed.parse::<f64>().unwrap_or(0.0) as i64;
                stack_for.borrow_mut().set("amount", &value.to_variant());
            });
            field.connect("text_changed", &callable);
        }
        row.bind_mut().add(&field).left();
        panel.bind_mut().add(&row).left().grow_x().pad(2.0);
        panel.bind_mut().row();
    }
    let mut reset_button = wide_button("@settings.reset", "refresh", None, 300.0);
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_loadout_reset", move |_args: &[&Variant]| {
            let mut entry = VarDictionary::new();
            entry.set("item", &GString::from("copper").to_variant());
            entry.set("amount", &100i64.to_variant());
            let mut loadout = VarArray::new();
            loadout.push(&entry.to_variant());
            rules_for.borrow_mut().set("loadout", &loadout.to_variant());
            close_overlay(&state_for);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            setup_main(&mut bound, &state_for);
        });
        reset_button.connect("pressed", &callable);
    }
    panel.bind_mut().add(&reset_button).left();
    panel.bind_mut().row();
    finish_overlay(&mut panel);
}

/// `_open_banned_blocks`/`_open_banned_units` (`CustomRulesDialog.java:189,210`):
/// the two-pane `BannedContentDialog` edits the working `_rules` copy; edits come
/// back through `banned_edited`.
fn open_banned(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>, kind: &str) {
    let rules = state.borrow().rules.clone();
    let mut context = VarDictionary::new();
    context.set("kind", &GString::from(kind).to_variant());
    context.set("rules", &rules.borrow().to_variant());
    dialog.open_dialog("banned_content", &json_stringify(&context));
    let Some(mut banned) = dialog_node("banned_content") else {
        return;
    };
    if !banned.has_signal("banned_edited") {
        return;
    }
    let mut node = dialog.to_gd();
    if node.has_meta("_batch_i_banned_hook") {
        return;
    }
    let rules_for = rules.clone();
    let callable = Callable::from_fn("custom_rules_banned_edited", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let parsed = Json::parse_string(&GString::from(text.as_str()));
        if let Ok(parsed) = parsed.try_to::<VarDictionary>() {
            *rules_for.borrow_mut() = parsed;
        }
    });
    banned.connect("banned_edited", &callable);
    node.set_meta("_batch_i_banned_hook", &true.to_variant());
}

/// `@rules.weather`: weather-entry editor (`CustomRulesDialog.weatherDialog`).
fn open_weather(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@rules.weather", 460.0);
    let rules = state.borrow().rules.clone();
    let raw: VarArray = rules
        .borrow()
        .get("weather")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    // Drop `null` weather names (`rules.weather.removeAll(w -> w.weather == null)`).
    let mut entries = VarArray::new();
    for entry in raw.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_text(&entry, "weather").is_empty() {
            continue;
        }
        entries.push(&entry.to_variant());
    }
    rules.borrow_mut().set("weather", &entries.to_variant());
    let entries = Rc::new(RefCell::new(entries));
    let count = entries.borrow().len();
    for index in 0..count {
        let Some(entry_variant) = entries.borrow().get(index) else {
            continue;
        };
        let Ok(entry) = entry_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let entry = Rc::new(RefCell::new(entry));
        // code-instantiated: one weather entry block (duration/frequency minutes).
        let mut block = factory::table();
        let mut header = factory::table();
        let name = dict_at_text(&entry, "weather");
        let label = factory::label(&name);
        header.bind_mut().add(&label).left().pad_right(6.0);
        let mut remove_entry = wide_button("@cancel", "cancel", None, 300.0);
        {
            let entries_for = entries.clone();
            let entry_for = entry.clone();
            let rules_for = rules.clone();
            let state_for = state.clone();
            let target = dialog.to_gd();
            let callable =
                Callable::from_fn("rules_weather_remove", move |_args: &[&Variant]| {
                    entries_for
                        .borrow_mut()
                        .erase(&entry_for.borrow().to_variant());
                    rules_for
                        .borrow_mut()
                        .set("weather", &entries_for.borrow().to_variant());
                    close_overlay(&state_for);
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    open_weather(&mut bound, &state_for);
                });
            remove_entry.connect("pressed", &callable);
        }
        header.bind_mut().add(&remove_entry).left();
        block.bind_mut().add(&header).left().grow_x();
        block.bind_mut().row();
        let always_value = dict_at(&entry, "always", false);
        let mut duration = factory::table();
        duration
            .bind_mut()
            .add(&factory::label(
                &kit::t("@rules.weather.duration").to_string(),
            ))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&minutes_field(&entry, "minDuration", !always_value))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&factory::label(&kit::t("@waves.to").to_string()))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&minutes_field(&entry, "maxDuration", !always_value))
            .left()
            .pad_right(4.0);
        duration
            .bind_mut()
            .add(&factory::label(&kit::t("@unit.minutes").to_string()))
            .left();
        block.bind_mut().add(&duration).left().grow_x();
        block.bind_mut().row();
        let mut frequency = factory::table();
        frequency
            .bind_mut()
            .add(&factory::label(
                &kit::t("@rules.weather.frequency").to_string(),
            ))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&minutes_field(&entry, "minFrequency", !always_value))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&factory::label(&kit::t("@waves.to").to_string()))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&minutes_field(&entry, "maxFrequency", !always_value))
            .left()
            .pad_right(4.0);
        frequency
            .bind_mut()
            .add(&factory::label(&kit::t("@unit.minutes").to_string()))
            .left();
        block.bind_mut().add(&frequency).left().grow_x();
        block.bind_mut().row();
        // `MindWidgets.check(_t("@rules.weather.always"), always, listener)`.
        let mut check = MindCheck::new_alloc();
        {
            let entry_for = entry.clone();
            let state_for = state.clone();
            let target = dialog.to_gd();
            let callable = Callable::from_fn("rules_weather_always", move |args: &[&Variant]| {
                let value = args
                    .first()
                    .map(|value| (*value).to::<bool>())
                    .unwrap_or(false);
                entry_for.borrow_mut().set("always", &value.to_variant());
                close_overlay(&state_for);
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                open_weather(&mut bound, &state_for);
            });
            check
                .bind_mut()
                .setup(kit::t("@rules.weather.always"), always_value, callable);
        }
        block.bind_mut().add(&check).left();
        panel.bind_mut().add(&block).left().grow_x().pad(3.0);
        panel.bind_mut().row();
    }
    let mut add_button = wide_button("@add", "add", None, 300.0);
    {
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_weather_add", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            open_weather_add(&mut bound, &state_for);
        });
        add_button.connect("pressed", &callable);
    }
    panel.bind_mut().add(&add_button).left();
    panel.bind_mut().row();
    finish_overlay(&mut panel);
}

/// One minutes field of a weather entry (`field(f, entry.minDuration / toMinutes,
/// v -> entry.minDuration = v * toMinutes)`).
fn minutes_field(
    entry: &Rc<RefCell<VarDictionary>>,
    key: &'static str,
    enabled: bool,
) -> Gd<LineEdit> {
    let value = dict_at_f64(entry, key, 0.0) / 60.0;
    let mut field = factory::field(&number_text(value, false));
    field.set_custom_minimum_size(Vector2::new(90.0, 40.0));
    field.set_editable(enabled);
    let entry_for = entry.clone();
    let field_for = field.clone();
    let callable = Callable::from_fn("rules_weather_minutes", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let trimmed = text.trim().to_string();
        if !parse_number(&trimmed, false) {
            invalid_field(&field_for, true);
            return;
        }
        invalid_field(&field_for, false);
        let value = trimmed.parse::<f64>().unwrap_or(0.0);
        entry_for
            .borrow_mut()
            .set(key, &(value * 60.0).to_variant());
    });
    field.connect("text_changed", &callable);
    field
}

/// `_open_weather_add`: one button per weather content type.
fn open_weather_add(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@add", 420.0);
    let names: PackedStringArray = kit::sim_host()
        .filter(|host| host.has_method("content_list"))
        .and_then(|mut host| {
            host.call("content_list", &[GString::from("weather").to_variant()])
                .try_to::<PackedStringArray>()
                .ok()
        })
        .unwrap_or_default();
    let rules = state.borrow().rules.clone();
    let entries = Rc::new(RefCell::new(
        rules
            .borrow()
            .get("weather")
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default(),
    ));
    let mut index = 0;
    for weather_name in names.as_slice() {
        let weather_name = weather_name.to_string();
        let mut button =
            factory::button(&kit::t(&format!("@weather.{weather_name}.name")).to_string());
        button.set_custom_minimum_size(Vector2::new(140.0, 50.0));
        {
            let entries_for = entries.clone();
            let rules_for = rules.clone();
            let state_for = state.clone();
            let target = dialog.to_gd();
            let name_for = weather_name.clone();
            let callable = Callable::from_fn("rules_weather_new", move |_args: &[&Variant]| {
                let mut entry = VarDictionary::new();
                entry.set("weather", &GString::from(name_for.as_str()).to_variant());
                entry.set("minFrequency", &97200.0f64.to_variant());
                entry.set("maxFrequency", &194400.0f64.to_variant());
                entry.set("minDuration", &16200.0f64.to_variant());
                entry.set("maxDuration", &48600.0f64.to_variant());
                entry.set("cooldown", &0.0f64.to_variant());
                entry.set("intensity", &1.0f64.to_variant());
                entry.set("always", &false.to_variant());
                entries_for.borrow_mut().push(&entry.to_variant());
                rules_for
                    .borrow_mut()
                    .set("weather", &entries_for.borrow().to_variant());
                close_overlay(&state_for);
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                open_weather(&mut bound, &state_for);
            });
            button.connect("pressed", &callable);
        }
        panel.bind_mut().add(&button).pad(1.0);
        index += 1;
        if index % 2 == 0 {
            panel.bind_mut().row();
        }
    }
    finish_overlay(&mut panel);
}

/// `@edit`: the clipboard menu (`CustomRulesDialog.java:60-108`).
fn open_edit_menu(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
    let mut panel = overlay_open(dialog, state, "@edit.menu", 320.0);
    let rules = state.borrow().rules.clone();
    let reset = state.borrow().reset.clone();
    let mut copy_button = wide_button("@copy.clipboard", "copy", None, 300.0);
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_copy_clipboard", move |_args: &[&Variant]| {
            let mut copy = rules_for.borrow().duplicate_deep();
            copy.remove("spawns");
            DisplayServer::singleton().clipboard_set(&json_stringify(&copy));
            let _ = ui_call("show_info", &[kit::t("@copied").to_variant()]);
            close_overlay(&state_for);
        });
        copy_button.connect("pressed", &callable);
    }
    panel.bind_mut().add(&copy_button).left();
    panel.bind_mut().row();
    let clipboard = DisplayServer::singleton().clipboard_get().to_string();
    let loadable = !clipboard.is_empty() && clipboard.trim().starts_with('{');
    let mut load_button = wide_button("@load.clipboard", "download", None, 300.0);
    {
        let rules_for = rules.clone();
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_load_clipboard", move |_args: &[&Variant]| {
            let text = DisplayServer::singleton().clipboard_get().to_string();
            let parsed = Json::parse_string(&GString::from(text.as_str()));
            let Ok(parsed) = parsed.try_to::<VarDictionary>() else {
                show_error("@rules.invaliddata");
                close_overlay(&state_for);
                return;
            };
            let mut loaded = parsed;
            // `newRules.spawns/objectives = rules.spawns/objectives` (map-specific).
            let spawns = rules_for
                .borrow()
                .get("spawns")
                .unwrap_or_else(|| VarArray::new().to_variant());
            loaded.set("spawns", &spawns);
            let objectives = rules_for
                .borrow()
                .get("objectives")
                .unwrap_or_else(|| VarArray::new().to_variant());
            loaded.set("objectives", &objectives);
            replace_rules(&rules_for, &loaded);
            close_overlay(&state_for);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            setup_main(&mut bound, &state_for);
        });
        load_button.connect("pressed", &callable);
    }
    load_button.set_disabled(!loadable);
    panel.bind_mut().add(&load_button).left();
    panel.bind_mut().row();
    let mut reset_menu = wide_button("@settings.reset", "refresh", None, 300.0);
    {
        let rules_for = rules.clone();
        let reset_for = reset.clone();
        let state_for = state.clone();
        let target = dialog.to_gd();
        let callable = Callable::from_fn("rules_reset_menu", move |_args: &[&Variant]| {
            let source = if reset_for.borrow().is_empty() {
                rules_for.borrow().clone()
            } else {
                reset_for.borrow().clone()
            };
            let mut copy = VarDictionary::new();
            for key in source.keys_array().iter_shared() {
                if let Some(value) = source.get(&key) {
                    copy.set(&key, &value);
                }
            }
            replace_rules(&rules_for, &copy);
            close_overlay(&state_for);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            setup_main(&mut bound, &state_for);
        });
        reset_menu.connect("pressed", &callable);
    }
    panel.bind_mut().add(&reset_menu).left();
    panel.bind_mut().row();
    finish_overlay(&mut panel);
}

/// `_rules.clear()` + copy every key of `source` (the GDScript in-place replace).
fn replace_rules(rules: &Rc<RefCell<VarDictionary>>, source: &VarDictionary) {
    let mut target = rules.borrow_mut();
    target.clear();
    for key in source.keys_array().iter_shared() {
        if let Some(value) = source.get(&key) {
            target.set(&key, &value);
        }
    }
}

/// `_show_error(key)`: `ui.showText("@error.title", _t(key))`.
fn show_error(key: &str) {
    let _ = ui_call(
        "show_text",
        &[
            kit::t("@error.title").to_variant(),
            kit::t(key).to_variant(),
        ],
    );
}

/// Places a freshly built overlay's cells immediately (a new table is not sorted
/// until the next layout pass).
fn finish_overlay(panel: &mut Gd<MindTable>) {
    panel.bind_mut().sort_now();
    if let Some(parent) = panel.get_parent()
        && let Ok(mut container) = parent.try_cast::<godot::classes::Container>()
    {
        container.queue_sort();
    }
}

/// Opens a code-instantiated overlay panel (`BaseDialog` sub-dialogs upstream).
fn overlay_open(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<CustomRulesState>>,
    title: &str,
    width: f32,
) -> Gd<MindTable> {
    close_overlay(state);
    // code-instantiated: per-action popup panels have no static scene.
    let mut root = ColorRect::new_alloc();
    root.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    root.set_anchors_preset(LayoutPreset::FULL_RECT);
    root.set_mouse_filter(MouseFilter::STOP);
    let mut center = CenterContainer::new_alloc();
    center.set_anchors_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    let mut panel_root = PanelContainer::new_alloc();
    panel_root.set_theme_type_variation("defaultDialog");
    panel_root.set_custom_minimum_size(Vector2::new(width, 0.0));
    let mut layout = VBoxContainer::new_alloc();
    layout.add_theme_constant_override("separation", 6);
    let title_label = factory::styled_label(
        &kit::markup_text(&kit::t(title).to_string()).to_string(),
        "techLabel",
    );
    layout.add_child(&title_label);
    let mut panel_body = factory::table();
    panel_body.set_h_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&panel_body);
    let mut close = factory::button(&kit::t("@back").to_string());
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("rules_overlay_close", move |_args: &[&Variant]| {
            close_overlay(&state_for);
        });
        close.connect("pressed", &callable);
    }
    layout.add_child(&close);
    panel_root.add_child(&layout);
    center.add_child(&panel_root);
    root.add_child(&center);
    let root_control: Gd<Control> = root.upcast();
    let control_for_meta = root_control.clone();
    dialog.base_mut().add_child(&root_control);
    state.borrow_mut().overlay = Some(control_for_meta);
    panel_body
}

/// `_close_overlay`.
fn close_overlay(state: &Rc<RefCell<CustomRulesState>>) {
    let overlay = state.borrow_mut().overlay.take();
    if let Some(mut overlay) = overlay
        && overlay.is_instance_valid()
    {
        overlay.queue_free();
    }
}

/// `MusicContainer` list text (`Seq<MusicContainer>.toString(", ")`); the port's
/// JSON shape is `{"tracks": ["name", ...]}`.
fn music_text(value: &Variant) -> String {
    if value.get_type() != VariantType::DICTIONARY {
        return String::new();
    }
    let Ok(dict) = value.try_to::<VarDictionary>() else {
        return String::new();
    };
    let Ok(tracks) = dict
        .get("tracks")
        .unwrap_or_else(|| VarArray::new().to_variant())
        .try_to::<VarArray>()
    else {
        return String::new();
    };
    tracks
        .iter_shared()
        .map(|track| variant_text(&track))
        .collect::<Vec<String>>()
        .join(", ")
}

/// `_music_value`: null for an empty list, else `{"tracks": [...]}`.
fn music_value(text: &str) -> Variant {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Variant::nil();
    }
    let mut tracks = VarArray::new();
    for part in trimmed.split(',') {
        let name = part.trim();
        if !name.is_empty() {
            tracks.push(&GString::from(name).to_variant());
        }
    }
    let mut dict = VarDictionary::new();
    dict.set("tracks", &tracks.to_variant());
    dict.to_variant()
}

/// `MusicContainer::new` accepts any comma-separated identifier list.
fn music_valid(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    for part in trimmed.split(',') {
        let name = part.trim();
        if name.is_empty() || !is_valid_identifier(name) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// EditorMapsDialog (`editor_maps`)
// ---------------------------------------------------------------------------

/// `EditorMapsDialog`/`ui.editor.show()` default size for New Map.
const NEW_MAP_SIZE: i32 = 200;
/// `_batch_d_maps_rebuild`: set by the `editor_map` page after a delete/import
/// (`batch_d::editor_maps_request_rebuild`).
const MAPS_REBUILD_META: &str = "_batch_d_maps_rebuild";

/// `EditorMapsDialog` state: the shared `MapListState` plus the prompt flags
/// (`_chooser_action`, `_chooser_connected`, `_pending_new_map`,
/// `_pending_import`).
struct EditorMapsState {
    list: Rc<RefCell<MapListState>>,
    chooser_action: String,
    chooser_connected: bool,
    pending_new_map: bool,
    pending_import: String,
}

impl Default for EditorMapsState {
    fn default() -> Self {
        Self {
            list: Rc::new(RefCell::new(MapListState::new())),
            chooser_action: String::new(),
            chooser_connected: false,
            pending_new_map: false,
            pending_import: String::new(),
        }
    }
}

/// `@maps` — the editor map browser (`EditorMapsDialog extends MapListDialog`).
#[derive(Default)]
pub struct EditorMapsDialog {
    state: Rc<RefCell<EditorMapsState>>,
}

impl DialogController for EditorMapsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@maps");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        // `makeButtonOverlay()` (`MapListDialog.java:103`).
        dialog.apply_button_overlay(true);
        let list = self.state.borrow().list.clone();
        // `displayType = true` (`EditorMapsDialog.java:17`).
        list.borrow_mut().display_type = true;
        // `showMap(map)` override (`EditorMapsDialog._show_map`).
        list.borrow_mut().show_map =
            Some(Rc::new(|dialog: &mut MindDialog, map: &VarDictionary| {
                let mut context = VarDictionary::new();
                context.set(
                    "map",
                    &GString::from(dict_text(map, "name").as_str()).to_variant(),
                );
                dialog.open_dialog("editor_map", &json_stringify(&context));
            }));
        maplist_load_settings(dialog, &list);
        maplist_build(dialog, &list);
        // `buildButtons`: `@editor.newmap` (`Icon.add`) then `@editor.importmap`
        // (`Icon.upload`), both 210x64, after the shared `@back`.
        let target = dialog.to_gd();
        let state_for_new = self.state.clone();
        let new_map = {
            let target = target.clone();
            Callable::from_fn("editor_maps_new_map", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                new_map_prompt(&mut bound, &state_for_new);
            })
        };
        maplist_add_pane_button(dialog, "@editor.newmap", "add", Some(new_map));
        let state_for_import = self.state.clone();
        let import = {
            let target = target.clone();
            Callable::from_fn("editor_maps_import", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                open_import(&mut bound, &state_for_import);
            })
        };
        maplist_add_pane_button(dialog, "@editor.importmap", "upload", Some(import));
        connect_prompts(dialog, &self.state);
        let deferred = self.state.borrow().list.borrow().deferred.clone();
        install_frame_tick(dialog, "editor_maps", deferred, || {});
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let list = self.state.borrow().list.clone();
        maplist_load_settings(dialog, &list);
        maplist_refresh_registry(dialog, &list);
        // `setup`: `searchString = null` and a fresh (empty) search field.
        list.borrow_mut().search = String::new();
        let field = list.borrow().field.clone();
        if let Some(mut field) = field {
            field.set_text("");
        }
        maplist_rebuild(dialog, &list);
        // Desktop focuses the search field (`MapListDialog.java:367-369`).
        if let Some(mut field) = list.borrow().field.clone() {
            field.call_deferred("grab_focus", &[]);
        }
        // The `editor_map` page asked for a rebuild (delete/import).
        if dialog.base().has_meta(MAPS_REBUILD_META) {
            dialog.base_mut().remove_meta(MAPS_REBUILD_META);
        }
    }
}

/// `buildButtons`: `ui.showTextInput("@editor.newmap", "@editor.mapname", "", ...)`.
fn new_map_prompt(_dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_text_input") {
        return;
    }
    state.borrow_mut().pending_new_map = true;
    let _ = ui.call(
        "show_text_input",
        &[
            kit::t("@editor.newmap").to_variant(),
            kit::t("@editor.mapname").to_variant(),
            50i64.to_variant(),
            GString::new().to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `buildButtons`: `@editor.importmap` -> `FileChooser.open(mapExtension)`.
fn open_import(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>) {
    let Some(ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("open_dialog") {
        return;
    }
    state.borrow_mut().chooser_action = String::from("import");
    connect_chooser(dialog, state, true);
    let mut context = VarDictionary::new();
    context.set("open", &true.to_variant());
    let mut extensions = VarArray::new();
    extensions.push(&GString::from("msav").to_variant());
    context.set("extensions", &extensions.to_variant());
    dialog.open_dialog("file_chooser", &json_stringify(&context));
}

/// `_connect_chooser(true/false)`: one `file_chosen` listener at a time.
fn connect_chooser(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<EditorMapsState>>,
    should_connect: bool,
) {
    let Some(mut chooser) = dialog_node("file_chooser") else {
        return;
    };
    if !chooser.has_signal("file_chosen") {
        return;
    }
    let mut node = dialog.to_gd();
    if !should_connect {
        if node.has_meta("_batch_i_chooser") {
            if let Ok(callable) = node.get_meta("_batch_i_chooser").try_to::<Callable>()
                && chooser.is_connected("file_chosen", &callable)
            {
                chooser.disconnect("file_chosen", &callable);
            }
            node.remove_meta("_batch_i_chooser");
        }
        state.borrow_mut().chooser_connected = false;
        return;
    }
    if state.borrow().chooser_connected {
        return;
    }
    let target = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("editor_maps_file_chosen", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        file_chosen(&mut bound, &state_for, &path);
    });
    chooser.connect("file_chosen", &callable);
    node.set_meta("_batch_i_chooser", &callable.to_variant());
    state.borrow_mut().chooser_connected = true;
}

/// `_on_file_chosen`: drop the route and start the import.
fn file_chosen(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, path: &str) {
    if state.borrow().chooser_action.is_empty() {
        return;
    }
    state.borrow_mut().chooser_action.clear();
    connect_chooser(dialog, state, false);
    let _ = ui_call(
        "close_dialog",
        &[GString::from("file_chooser").to_variant()],
    );
    state.borrow_mut().pending_import = path.to_string();
    import_map(dialog, state, false);
}

/// `EditorMapsDialog.tryImportMap` (`:55-98`): image -> `@editor.errorimage`;
/// a built-in name -> `editor.import.exists`; a custom conflict -> the
/// `editor.overwrite.confirm` confirm, then re-import with `overwrite`.
fn import_map(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, overwrite: bool) {
    let pending = state.borrow().pending_import.clone();
    let Some(mut editor) = kit::editor() else {
        state.borrow_mut().pending_import.clear();
        return;
    };
    let result = editor
        .call(
            "import_map",
            &[
                GString::from(pending.as_str()).to_variant(),
                overwrite.to_variant(),
            ],
        )
        .to::<GString>()
        .to_string();
    if result.is_empty() {
        state.borrow_mut().pending_import.clear();
        let list = state.borrow().list.clone();
        maplist_refresh_registry(dialog, &list);
        maplist_rebuild(dialog, &list);
        return;
    }
    if let Some(name) = result.strip_prefix("overwrite:").map(str::to_string) {
        confirm_overwrite(state, &name);
        return;
    }
    state.borrow_mut().pending_import.clear();
    if let Some(name) = result.strip_prefix("exists:") {
        notify_key(&fmt_key("@editor.import.exists", &[name]));
    } else if result == "image" {
        notify_key("@editor.errorimage");
    } else {
        notify_key("@editor.errorload");
    }
}

/// `_confirm_overwrite`: `ui.showConfirm("@editor.overwrite.confirm", name)`.
fn confirm_overwrite(state: &Rc<RefCell<EditorMapsState>>, map_name: &str) {
    let _ = state;
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let text = kit::markup_text(&fmt_key("@editor.overwrite.confirm", &[map_name]));
    let _ = ui.call("show_confirm", &[text.to_variant()]);
}

/// `_connect_prompts`: `text_input_result` and `confirm_result` handlers.
fn connect_prompts(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>) {
    let target = dialog.to_gd();
    let state_for_text = state.clone();
    let text_callable = {
        let target = target.clone();
        Callable::from_fn("editor_maps_text_input", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let target = target.clone();
            let state_for_text = state_for_text.clone();
            defer_action("editor_maps_new_map_deferred", move || {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                on_text_input(&mut bound, &state_for_text, &text);
            });
        })
    };
    connect_prompt_once(
        dialog,
        "maps_text_input",
        "text_input_result",
        &text_callable,
    );
    let state_for_confirm = state.clone();
    let confirm_callable = {
        let target = target.clone();
        Callable::from_fn("editor_maps_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            let target = target.clone();
            let state_for_confirm = state_for_confirm.clone();
            defer_action("editor_maps_confirm_deferred", move || {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                on_confirm(&mut bound, &state_for_confirm, confirmed);
            });
        })
    };
    connect_prompt_once(dialog, "maps_confirm", "confirm_result", &confirm_callable);
}

/// `_on_text_input`: a taken name shows `@editor.exists`, else a fresh 200x200
/// map with the `name` tag (`EditorMapsDialog.java:27-42`).
fn on_text_input(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, text: &str) {
    if !state.borrow().pending_new_map {
        return;
    }
    state.borrow_mut().pending_new_map = false;
    let map_name = text.trim().to_string();
    if map_name.is_empty() {
        return;
    }
    if row_for(dialog, &map_name).is_some() {
        notify_key("@editor.exists");
        return;
    }
    let Some(mut editor) = kit::editor() else {
        notify_key("@editor.errorload");
        return;
    };
    let started = editor
        .call(
            "begin_new",
            &[NEW_MAP_SIZE.to_variant(), NEW_MAP_SIZE.to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if !started {
        notify_key("@editor.errorload");
        return;
    }
    let _ = editor.call(
        "set_tag",
        &[
            GString::from("name").to_variant(),
            GString::from(map_name.as_str()).to_variant(),
        ],
    );
    dialog.close();
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("show_dialog")
    {
        let _ = shell.call("show_dialog", &[]);
    }
}

/// `_on_confirm`: the overwrite answer for a pending import.
fn on_confirm(dialog: &mut MindDialog, state: &Rc<RefCell<EditorMapsState>>, confirmed: bool) {
    if state.borrow().pending_import.is_empty() {
        return;
    }
    if !confirmed {
        state.borrow_mut().pending_import.clear();
        return;
    }
    import_map(dialog, state, true);
}

/// `_row_for(mapName)`: the registry row with that display name.
fn row_for(dialog: &mut MindDialog, map_name: &str) -> Option<VarDictionary> {
    maplist_maps(dialog)
        .into_iter()
        .find(|row| dict_text(row, "name") == map_name)
}

/// `showMap`'s `@editor.openin`: load the map into the editor and show the shell.
///
/// The GDScript keeps this helper unused (`editor_map` owns the working route);
/// it is ported for parity with the source.
#[allow(dead_code)]
fn open_in_editor(path: &str) {
    if path.is_empty() {
        notify_key("@editor.errorload");
        return;
    }
    let Some(mut editor) = kit::editor() else {
        notify_key("@editor.errorload");
        return;
    };
    let opened = editor
        .call("begin_edit_map", &[GString::from(path).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false);
    if !opened {
        notify_key("@editor.errorload");
        return;
    }
    let _ = ui_call("close_dialog", &[GString::from("editor_maps").to_variant()]);
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("show_dialog")
    {
        let _ = shell.call("show_dialog", &[]);
    }
}

// ---------------------------------------------------------------------------
// CanvasEditDialog (`canvas_edit`)
// ---------------------------------------------------------------------------

/// `CanvasEditDialog.java:185`: 480 px paint surface on desktop.
const CANVAS_EXTENT: f32 = 480.0;
/// `refreshTime = 60 * 2` (`CanvasEditDialog.java:24`): auto-save accumulator.
const CANVAS_SAVE_TIME: f64 = 120.0;
/// The default canvas size (`CanvasBlock.canvasSize`) before the row arrives.
const CANVAS_DEFAULT_SIZE: i32 = 12;
/// The default bits per pixel (`CanvasBlock.defaultBpp`) before the row arrives.
const CANVAS_DEFAULT_BPP: i64 = 3;

/// Display-side paint state shared with [`MindCanvasSurface`] (`_pixels`,
/// `_palette`, `_texture`, `_size`, `_grid`).
#[derive(Default)]
pub(crate) struct CanvasSurfaceState {
    size: i32,
    /// `_palette`: packed RGBA8888 colours.
    palette: Vec<u32>,
    /// `_pixels`: one palette index per pixel (`pos = y * size + x`, row 0 = top).
    pixels: Vec<u8>,
    texture: Option<Gd<ImageTexture>>,
    grid: bool,
}

/// The paint surface (`CanvasEditDialog`'s `cont.table(Tex.pane, body -> ...)`).
///
/// A dedicated `Control` subclass implements the custom `_draw` (pixel texture
/// plus the grid overlay); the controller owns the state it renders.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindCanvasSurface {
    base: Base<Control>,
    shared: Rc<RefCell<CanvasSurfaceState>>,
}

#[godot_api]
impl IControl for MindCanvasSurface {
    fn draw(&mut self) {
        let (texture, size, grid) = {
            let state = self.shared.borrow();
            (state.texture.clone(), state.size, state.grid)
        };
        let Some(texture) = texture else {
            return;
        };
        let texture: Gd<godot::classes::Texture2D> = texture.upcast();
        let rect = self.base().get_size();
        self.base_mut()
            .draw_texture_rect(&texture, Rect2::new(Vector2::ZERO, rect), false);
        if !grid || size <= 0 {
            return;
        }
        // `CanvasEditDialog`'s grid (`:152-170`): a line every `jump` cells.
        let cell = Vector2::new(
            (rect.x / size as f32).max(1.0),
            (rect.y / size as f32).max(1.0),
        );
        let min_space = 10.0f32;
        let jump_x = ((min_space.max(cell.x) / cell.x) as i32).max(1);
        let jump_y = ((min_space.max(cell.y) / cell.y) as i32).max(1);
        let line = Color::from_rgba(0.0, 0.0, 0.0, 0.35);
        let mut column = 0i32;
        while column <= size {
            self.base_mut().draw_rect(
                Rect2::new(
                    Vector2::new(column as f32 * cell.x - 1.0, 0.0),
                    Vector2::new(2.0, rect.y),
                ),
                line,
            );
            column += jump_x;
        }
        let mut row = 0i32;
        while row <= size {
            self.base_mut().draw_rect(
                Rect2::new(
                    Vector2::new(0.0, row as f32 * cell.y - 1.0),
                    Vector2::new(rect.x, 2.0),
                ),
                line,
            );
            row += jump_y;
        }
    }
}

impl MindCanvasSurface {
    /// Installs the controller-owned paint state.
    fn install(&mut self, shared: Rc<RefCell<CanvasSurfaceState>>) {
        self.shared = shared;
    }
}

/// `CanvasEditDialog` view state.
struct CanvasEditState {
    x: i64,
    y: i64,
    bpp: i64,
    cur_color: i64,
    fill: bool,
    modified: bool,
    save_timer: f64,
    surface: Option<Gd<MindCanvasSurface>>,
    shared: Rc<RefCell<CanvasSurfaceState>>,
    palette_row: Option<Gd<HBoxContainer>>,
    swatches: Vec<Gd<Button>>,
    /// `_last_cell`: the last painted cell (`(-1, -1)` = none).
    last_cell: (i32, i32),
}

impl Default for CanvasEditState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            bpp: CANVAS_DEFAULT_BPP,
            cur_color: 0,
            fill: false,
            modified: false,
            save_timer: 0.0,
            surface: None,
            shared: Rc::new(RefCell::new(CanvasSurfaceState {
                size: CANVAS_DEFAULT_SIZE,
                grid: true,
                ..CanvasSurfaceState::default()
            })),
            palette_row: None,
            swatches: Vec::new(),
            last_cell: (-1, -1),
        }
    }
}

/// The logic-block canvas painter (`CanvasEditDialog`).
pub struct CanvasEditDialog {
    state: Rc<RefCell<CanvasEditState>>,
}

impl Default for CanvasEditDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(CanvasEditState::default())),
        }
    }
}

impl DialogController for CanvasEditDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `CanvasEditDialog` removes the title table (`:36`).
        dialog.should_pause = false;
        dialog.set_title_literal("");
        build_canvas(dialog, &self.state);
        let target = dialog.to_gd();
        let state = self.state.clone();
        install_frame_tick(dialog, "canvas_edit", Deferred::default(), move || {
            canvas_tick(&state, &target);
        });
    }

    /// `shown()`: seed the surface from `SimHost.logic_canvas_sync`.
    fn shown(&mut self, dialog: &mut MindDialog) {
        let x = dialog.context_int("x");
        let y = dialog.context_int("y");
        canvas_load(dialog, &self.state, x, y);
    }

    /// `hidden()`: apply the packed pixmap (`CanvasEditDialog.hidden`).
    fn hidden(&mut self, _dialog: &mut MindDialog) {
        canvas_save(&self.state);
    }
}

/// `_build()`: the paint surface, the grid/fill toggles, the palette swatches
/// and the `@import`/`@export`/`@back` button row.
fn build_canvas(dialog: &mut MindDialog, state: &Rc<RefCell<CanvasEditState>>) {
    let mut table = dialog.content_table();
    let shared = state.borrow().shared.clone();
    // code-instantiated: the paint surface size is the block's canvas size; it
    // has no scene-authorable shape.
    let mut surface = MindCanvasSurface::new_alloc();
    surface.bind_mut().install(shared);
    surface.set_custom_minimum_size(Vector2::new(CANVAS_EXTENT, CANVAS_EXTENT));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("canvas_edit_input", move |args: &[&Variant]| {
            let Some(event) = args
                .first()
                .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
            else {
                return;
            };
            canvas_input(&state_for, &event);
        });
        surface.connect("gui_input", &callable);
    }
    table.bind_mut().add(&surface);
    table.bind_mut().row();
    state.borrow_mut().surface = Some(surface.clone());

    let mut controls = HBoxContainer::new_alloc();
    table.bind_mut().add(&controls);
    let mut grid_button = factory::glyph_button("grid", "grid", "clearNoneTogglei");
    grid_button.set_toggle_mode(true);
    grid_button.set_pressed_no_signal(true);
    grid_button.set_custom_minimum_size(Vector2::new(44.0, 44.0));
    grid_button.set_tooltip_text(&kit::t("@grid"));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("canvas_edit_grid", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            state_for.borrow().shared.borrow_mut().grid = pressed;
            canvas_redraw(&state_for);
        });
        grid_button.connect("toggled", &callable);
    }
    controls.add_child(&grid_button);

    let palette_row = HBoxContainer::new_alloc();
    controls.add_child(&palette_row);
    state.borrow_mut().palette_row = Some(palette_row.clone());

    // `Icon.fill` has no icon-font alias yet; `glyph_button` falls back to text.
    let mut fill_button = factory::glyph_button("fill", "fill", "clearNoneTogglei");
    fill_button.set_toggle_mode(true);
    fill_button.set_custom_minimum_size(Vector2::new(44.0, 44.0));
    fill_button.set_tooltip_text(&kit::t("@fill"));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("canvas_edit_fill", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            state_for.borrow_mut().fill = pressed;
        });
        fill_button.connect("toggled", &callable);
    }
    controls.add_child(&fill_button);

    dialog.add_close_button(160.0);
    let target = dialog.to_gd();
    let import_target = target.clone();
    let state_import = state.clone();
    let callable = Callable::from_fn("canvas_edit_import", move |_args: &[&Variant]| {
        canvas_import_png(&import_target, &state_import);
    });
    let text = dialog.t("@import").to_string();
    let mut import_button = dialog.add_button(&text, Some(callable), "download", 160.0);
    let mut minimum = import_button.get_custom_minimum_size();
    minimum.y = 64.0;
    import_button.set_custom_minimum_size(minimum);
    let export_target = target;
    let state_export = state.clone();
    let callable = Callable::from_fn("canvas_edit_export", move |_args: &[&Variant]| {
        canvas_export_png(&export_target, &state_export);
    });
    let text = dialog.t("@export").to_string();
    let mut export_button = dialog.add_button(&text, Some(callable), "upload", 160.0);
    let mut minimum = export_button.get_custom_minimum_size();
    minimum.y = 64.0;
    export_button.set_custom_minimum_size(minimum);
}

/// `CanvasEditDialog.update`: `if(refreshTime >= 60*2) save()`.
fn canvas_tick(state: &Rc<RefCell<CanvasEditState>>, dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() || !dialog.is_visible() || !state.borrow().modified {
        return;
    }
    let delta = dialog.get_process_delta_time();
    let should_save = {
        let mut st = state.borrow_mut();
        st.save_timer += delta * 60.0;
        if st.save_timer >= CANVAS_SAVE_TIME {
            st.save_timer = 0.0;
            true
        } else {
            false
        }
    };
    if should_save {
        canvas_save(state);
    }
}

/// Seeds the surface from `SimHost.logic_canvas_sync` (`CanvasEditDialog.update`).
fn canvas_load(dialog: &mut MindDialog, state: &Rc<RefCell<CanvasEditState>>, x: i64, y: i64) {
    let _ = dialog;
    {
        let mut st = state.borrow_mut();
        st.x = x;
        st.y = y;
    }
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    let mut size = CANVAS_DEFAULT_SIZE;
    let mut bpp = CANVAS_DEFAULT_BPP;
    let mut palette: Vec<u32> = Vec::new();
    if host.has_method("logic_canvas_sync") {
        let rows = host.call(
            "logic_canvas_sync",
            &[
                (-1.0e9f32).to_variant(),
                (-1.0e9f32).to_variant(),
                1.0e9f32.to_variant(),
                1.0e9f32.to_variant(),
            ],
        );
        if let Ok(rows) = rows.try_to::<VarArray>() {
            for row in rows.iter_shared() {
                let Ok(row) = row.try_to::<VarDictionary>() else {
                    continue;
                };
                if dict_i64(&row, "x", -1) != x || dict_i64(&row, "y", -1) != y {
                    continue;
                }
                size = dict_i64(&row, "pixels", i64::from(size)).max(1) as i32;
                bpp = dict_i64(&row, "bpp", bpp).max(1);
                if let Some(value) = row.get("palette")
                    && let Ok(packed) = value.try_to::<PackedInt64Array>()
                {
                    palette = packed
                        .as_slice()
                        .iter()
                        .map(|value| *value as u32)
                        .collect();
                }
                break;
            }
        }
    }
    let mut pixels: Vec<u8> = Vec::new();
    if host.has_method("logic_canvas_pixels")
        && let Ok(data) = host
            .call("logic_canvas_pixels", &[x.to_variant(), y.to_variant()])
            .try_to::<PackedByteArray>()
        && data.len() == (size * size) as usize
    {
        pixels = data.as_slice().to_vec();
    }
    if pixels.is_empty() {
        pixels = vec![0; (size * size) as usize];
    }
    {
        let mut st = state.borrow_mut();
        st.bpp = bpp;
        st.modified = false;
        st.save_timer = 0.0;
        st.last_cell = (-1, -1);
        st.cur_color = st.cur_color.clamp(0, (palette.len() as i64 - 1).max(0));
        let mut shared = st.shared.borrow_mut();
        shared.size = size;
        shared.palette = palette;
        shared.pixels = pixels;
    }
    refresh_texture(state);
    rebuild_swatches(state);
    canvas_redraw(state);
}

/// `_rebuild_swatches`: one tinted toggle per palette colour
/// (`CanvasEditDialog.java:194-207`).
fn rebuild_swatches(state: &Rc<RefCell<CanvasEditState>>) {
    let Some(mut palette_row) = state.borrow().palette_row.clone() else {
        return;
    };
    for child in palette_row.get_children().iter_shared() {
        let mut child = child.clone();
        child.queue_free();
    }
    state.borrow_mut().swatches.clear();
    let (palette, cur_color) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.palette.clone(), st.cur_color)
    };
    for (index, packed) in palette.iter().enumerate() {
        // code-instantiated: one palette swatch per canvas palette colour.
        let mut button = Button::new_alloc();
        button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        button.set_toggle_mode(true);
        button.set_theme_type_variation("squareTogglei");
        button.set_pressed_no_signal(index as i64 == cur_color);
        button.set_modulate(rgba8_color(i64::from(*packed)));
        {
            let state_for = state.clone();
            let callable = Callable::from_fn("canvas_edit_swatch", move |_args: &[&Variant]| {
                select_color(&state_for, index as i64);
            });
            button.connect("pressed", &callable);
        }
        palette_row.add_child(&button);
        state.borrow_mut().swatches.push(button);
    }
}

/// `_select_color`.
fn select_color(state: &Rc<RefCell<CanvasEditState>>, index: i64) {
    state.borrow_mut().cur_color = index;
    refresh_swatches(state);
}

/// `_refresh_swatches`.
fn refresh_swatches(state: &Rc<RefCell<CanvasEditState>>) {
    let (swatches, cur_color) = {
        let st = state.borrow();
        (st.swatches.clone(), st.cur_color)
    };
    for (index, button) in swatches.iter().enumerate() {
        let mut button = button.clone();
        button.set_pressed_no_signal(index as i64 == cur_color);
    }
}

/// `packPixmap`: LSB-first `bpp`-bit packing (`CanvasBlock.java:216-227`).
fn pack_pixels(state: &Rc<RefCell<CanvasEditState>>) -> PackedByteArray {
    let (size, pixels, bpp) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.size, shared.pixels.clone(), st.bpp)
    };
    let count = (size * size).max(0) as usize;
    let bytes = ((size * size) as i64 * bpp + 7) / 8;
    let mut packed = PackedByteArray::new();
    packed.resize(bytes.max(0) as usize);
    let bpp = bpp.max(1);
    let bytes_len = packed.len();
    for pos in 0..count {
        let value = i64::from(pixels.get(pos).copied().unwrap_or(0));
        let bit_offset = pos as i64 * bpp;
        for bit in 0..bpp {
            let word = ((bit_offset + bit) >> 3) as usize;
            let mask = 1i64 << ((bit_offset + bit) & 7);
            if word >= bytes_len {
                continue;
            }
            let current = i64::from(packed.as_slice()[word]);
            let updated = if (value >> bit) & 1 != 0 {
                current | mask
            } else {
                current & !mask
            };
            packed.as_mut_slice()[word] = updated as u8;
        }
    }
    packed
}

/// `CanvasEditDialog.save`: `canvas.configure(canvas.packPixmap(pix))`.
fn canvas_save(state: &Rc<RefCell<CanvasEditState>>) {
    if !state.borrow().modified {
        return;
    }
    let (x, y) = {
        let st = state.borrow();
        (st.x, st.y)
    };
    let Some(mut input) = kit::node("/root/Spine/Input") else {
        return;
    };
    if !input.has_method("configure_building_bytes") {
        return;
    }
    let packed = pack_pixels(state);
    let _ = input.call(
        "configure_building_bytes",
        &[x.to_variant(), y.to_variant(), packed.to_variant()],
    );
    state.borrow_mut().modified = false;
}

/// `@export`: `file.writePng(pix)` (`CanvasEditDialog.java:216-222`).
fn canvas_export_png(dialog: &Gd<MindDialog>, state: &Rc<RefCell<CanvasEditState>>) {
    // code-instantiated: `@export` opens a runtime native save dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_file_mode(FileMode::SAVE_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.add_filter(&GString::from("*.png"));
    let state_for = state.clone();
    let target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_export_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = target.clone();
        target.queue_free();
        if let Some(image) = pixels_image(&state_for) {
            let error = image.save_png(&GString::from(path.as_str()));
            if error != godot::global::Error::OK {
                log::warn!("[ui] canvas export `{path}`: {error:?}");
            }
        }
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_export_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut node: Gd<Node> = dialog.clone().upcast();
    node.add_child(&file_dialog);
    file_dialog.popup_centered();
}

/// `@import`: opens the native file browser, then [`canvas_import_image`].
fn canvas_import_png(dialog: &Gd<MindDialog>, state: &Rc<RefCell<CanvasEditState>>) {
    // code-instantiated: `@import` opens a runtime native open dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_file_mode(FileMode::OPEN_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.add_filter(&GString::from("*.png,*.jpg,*.jpeg"));
    let state_for = state.clone();
    let target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_import_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = target.clone();
        target.queue_free();
        canvas_import_image(&state_for, &path);
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_import_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut node: Gd<Node> = dialog.clone().upcast();
    node.add_child(&file_dialog);
    file_dialog.popup_centered();
}

/// `_import_image`: decode, scale/centre and map every pixel to the nearest
/// palette entry (`CanvasEditDialog.java:224-274`).
fn canvas_import_image(state: &Rc<RefCell<CanvasEditState>>, path: &str) {
    let Some(mut image) = Image::load_from_file(&GString::from(path)) else {
        return;
    };
    image.convert(Format::RGBA8);
    let (target, palette) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.size, shared.palette.clone())
    };
    if target <= 0 {
        return;
    }
    // Scale preserving aspect when larger, else fill with palette[0] and centre.
    let base = palette
        .first()
        .map(|packed| rgba8_color(i64::from(*packed)))
        .unwrap_or(Color::BLACK);
    let source_width = image.get_width().max(1);
    let source_height = image.get_height().max(1);
    let mut scaled_width = source_width;
    let mut scaled_height = source_height;
    if source_width > target || source_height > target {
        let ratio = (target as f64 / source_width as f64).min(target as f64 / source_height as f64);
        scaled_width = ((source_width as f64 * ratio).round() as i32).max(1);
        scaled_height = ((source_height as f64 * ratio).round() as i32).max(1);
        image
            .resize_ex(scaled_width, scaled_height)
            .interpolation(Interpolation::BILINEAR)
            .done();
    }
    let origin_x = (target - scaled_width) / 2;
    let origin_y = (target - scaled_height) / 2;
    let mut pixels = vec![0u8; (target * target) as usize];
    for y in 0..target {
        for x in 0..target {
            let source_x = x - origin_x;
            let source_y = y - origin_y;
            let mut color = base;
            if source_x >= 0 && source_y >= 0 && source_x < scaled_width && source_y < scaled_height
            {
                let pixel = image.get_pixel(source_x, source_y);
                // `findClosest`: blend the source alpha over palette[0].
                color = if pixel.a < 1.0 {
                    base.blend(pixel)
                } else {
                    pixel
                };
            }
            pixels[(y * target + x) as usize] = nearest_palette(&palette, color) as u8;
        }
    }
    state.borrow().shared.borrow_mut().pixels = pixels;
    state.borrow_mut().modified = true;
    refresh_texture(state);
    canvas_redraw(state);
}

/// `_pixels_image`: the palette-mapped RGBA8 image.
fn pixels_image(state: &Rc<RefCell<CanvasEditState>>) -> Option<Gd<Image>> {
    let (size, palette, pixels) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.size, shared.palette.clone(), shared.pixels.clone())
    };
    if size <= 0 {
        return None;
    }
    let mut image = Image::create(size, size, false, Format::RGBA8)?;
    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) as usize;
            let palette_index = pixels.get(index).copied().unwrap_or(0) as usize;
            let color = palette
                .get(palette_index)
                .map(|packed| rgba8_color(i64::from(*packed)))
                .unwrap_or(Color::BLACK);
            image.set_pixel(x, y, color);
        }
    }
    Some(image)
}

/// `_nearest_palette`: the closest palette entry by squared RGB distance.
fn nearest_palette(palette: &[u32], color: Color) -> i64 {
    let mut best = 0i64;
    let mut best_distance = f64::INFINITY;
    for (index, packed) in palette.iter().enumerate() {
        let candidate = rgba8_color(i64::from(*packed));
        let distance = f64::from(color.r - candidate.r).powi(2)
            + f64::from(color.g - candidate.g).powi(2)
            + f64::from(color.b - candidate.b).powi(2);
        if distance < best_distance {
            best_distance = distance;
            best = index as i64;
        }
    }
    best
}

/// `_refresh_texture`: rebuilds the pixel texture from `_pixels` (top row first).
fn refresh_texture(state: &Rc<RefCell<CanvasEditState>>) {
    let Some(image) = pixels_image(state) else {
        return;
    };
    let texture = ImageTexture::create_from_image(&image);
    state.borrow().shared.borrow_mut().texture = texture;
    canvas_redraw(state);
}

/// `_set_pixel_texture`: one pixel of the live texture.
fn set_pixel_texture(state: &Rc<RefCell<CanvasEditState>>, x: i32, y: i32) {
    let (texture, palette, pixels, size) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (
            shared.texture.clone(),
            shared.palette.clone(),
            shared.pixels.clone(),
            shared.size,
        )
    };
    let Some(mut texture) = texture else {
        return;
    };
    let Some(mut image) = texture.get_image() else {
        return;
    };
    let index = (y * size + x) as usize;
    let palette_index = pixels.get(index).copied().unwrap_or(0) as usize;
    let color = palette
        .get(palette_index)
        .map(|packed| rgba8_color(i64::from(*packed)))
        .unwrap_or(Color::BLACK);
    image.set_pixel(x, y, color);
    texture.update(&image);
    canvas_redraw(state);
}

/// `_cell_size`: the screen size of one canvas cell.
fn cell_size(shared: &CanvasSurfaceState, rect: Vector2) -> Vector2 {
    if shared.size <= 0 {
        return Vector2::ONE;
    }
    Vector2::new(
        (rect.x / shared.size as f32).max(1.0),
        (rect.y / shared.size as f32).max(1.0),
    )
}

/// `convertX`/`convertY` (`:78-84`): screen position -> canvas cell (row 0 = top).
fn cell_at(shared: &CanvasSurfaceState, rect: Vector2, position: Vector2) -> (i32, i32) {
    let cell = cell_size(shared, rect);
    ((position.x / cell.x) as i32, (position.y / cell.y) as i32)
}

/// `_index_at`.
fn index_at(size: i32, cell: (i32, i32)) -> Option<usize> {
    if cell.0 < 0 || cell.1 < 0 || cell.0 >= size || cell.1 >= size {
        return None;
    }
    Some((cell.1 * size + cell.0) as usize)
}

/// `draw(x, y)`: set when in bounds and different (`:138-144`).
fn canvas_paint(state: &Rc<RefCell<CanvasEditState>>, cell: (i32, i32)) {
    let (size, cur_color) = {
        let st = state.borrow();
        (st.shared.borrow().size, st.cur_color)
    };
    let Some(index) = index_at(size, cell) else {
        return;
    };
    {
        let st = state.borrow();
        let mut shared = st.shared.borrow_mut();
        if index >= shared.pixels.len() || i64::from(shared.pixels[index]) == cur_color {
            return;
        }
        shared.pixels[index] = cur_color as u8;
    }
    state.borrow_mut().modified = true;
    set_pixel_texture(state, cell.0, cell.1);
}

/// `fill` mode flood fill (`:94-114`).
fn canvas_flood_fill(state: &Rc<RefCell<CanvasEditState>>, cell: (i32, i32)) {
    let (size, cur_color) = {
        let st = state.borrow();
        (st.shared.borrow().size, st.cur_color)
    };
    let Some(start) = index_at(size, cell) else {
        return;
    };
    let mut stack = vec![start];
    {
        let st = state.borrow();
        let mut shared = st.shared.borrow_mut();
        let Some(&target) = shared.pixels.get(start) else {
            return;
        };
        if i64::from(target) == cur_color {
            return;
        }
        while let Some(pos) = stack.pop() {
            if pos >= shared.pixels.len() || shared.pixels[pos] != target {
                continue;
            }
            shared.pixels[pos] = cur_color as u8;
            let cx = (pos as i32) % size;
            let cy = (pos as i32) / size;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= size || ny >= size {
                    continue;
                }
                stack.push((ny * size + nx) as usize);
            }
        }
    }
    state.borrow_mut().modified = true;
    refresh_texture(state);
}

/// `Bresenham2.line`: the cells between two canvas cells.
fn line_cells(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    let mut x = from.0;
    let mut y = from.1;
    let dx = (to.0 - x).abs();
    let dy = (to.1 - y).abs();
    let sx = if x < to.0 { 1 } else { -1 };
    let sy = if y < to.1 { 1 } else { -1 };
    let mut err = dx - dy;
    loop {
        cells.push((x, y));
        if x == to.0 && y == to.1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
    cells
}

/// Paint surface input (`CanvasEditDialog`'s `InputListener`; `:87-135`).
fn canvas_input(state: &Rc<RefCell<CanvasEditState>>, event: &Gd<InputEvent>) {
    let Some(rect) = state
        .borrow()
        .surface
        .clone()
        .map(|surface| surface.get_rect().size)
    else {
        return;
    };
    if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
        let cell = {
            let st = state.borrow();
            let shared = st.shared.borrow();
            cell_at(&shared, rect, button.get_position())
        };
        let index = button.get_button_index();
        let pressed = button.is_pressed();
        if index == MouseButton::LEFT && pressed {
            let fill = state.borrow().fill;
            if fill {
                canvas_flood_fill(state, cell);
            } else {
                canvas_paint(state, cell);
                state.borrow_mut().last_cell = cell;
            }
            canvas_redraw(state);
        } else if index == MouseButton::MIDDLE && pressed {
            let (size, pixels) = {
                let st = state.borrow();
                let shared = st.shared.borrow();
                (shared.size, shared.pixels.clone())
            };
            if let Some(position) = index_at(size, cell)
                && let Some(value) = pixels.get(position)
            {
                state.borrow_mut().cur_color = i64::from(*value);
                refresh_swatches(state);
            }
        }
        return;
    }
    let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() else {
        return;
    };
    let last_cell = state.borrow().last_cell;
    if last_cell.0 < 0 {
        return;
    }
    if motion.get_button_mask().ord() & MouseButtonMask::LEFT.ord() == 0 {
        return;
    }
    if state.borrow().fill {
        return;
    }
    let target = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        cell_at(&shared, rect, motion.get_position())
    };
    // `Bresenham2.line(lastX, lastY, cx, cy, this::draw)`.
    for cell in line_cells(last_cell, target) {
        canvas_paint(state, cell);
    }
    state.borrow_mut().last_cell = target;
    canvas_redraw(state);
}

/// Queues a redraw of the paint surface.
fn canvas_redraw(state: &Rc<RefCell<CanvasEditState>>) {
    let surface = state.borrow().surface.clone();
    if let Some(mut surface) = surface {
        surface.queue_redraw();
    }
}
