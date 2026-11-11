// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The custom rules dialog controller.
//! Upstream: ui/dialogs/CustomRulesDialog.java

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::rules::util::*;
use crate::ui::kit::widgets::check::MindCheck;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable};
use godot::builtin::{PackedStringArray, VarArray, VarDictionary, VariantType};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, DisplayServer, Json, LineEdit, Node,
    PanelContainer, StyleBoxEmpty, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub mod categories;
pub mod overlays;
pub mod planet_env;
pub mod rows;
pub mod team_rules;

use self::categories::*;
use self::overlays::*;
use self::planet_env::*;
use self::rows::*;
use self::team_rules::*;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "custom_rules" => Some(Box::new(CustomRulesDialog::default())),
        _ => None,
    }
}

/// `Vars.tilesize` (world pixels per tile).
const TILESIZE: f64 = 8.0;

/// Category accent rule height (`addToMain`, 3 px).
const RULE_HEIGHT: f32 = 3.0;

/// A per-row enable condition (`update(a -> a.setDisabled(!condition))`).
type Cond = Rc<dyn Fn() -> bool>;

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
        dialog.add_default_close_button();
        let target = dialog.to_gd();
        let state = self.state.clone();
        let callable = Callable::from_fn("custom_rules_edit", move |_args: &[&Variant]| {
            // `@edit` builds an overlay and therefore re-binds the dialog: must
            // not run inside the button's `pressed` emission (PAR-0470).
            let state = state.clone();
            defer_dialog_action("custom_rules_edit_menu", &target, move |dialog| {
                open_edit_menu(dialog, &state);
            });
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
    let mut search_row = factory::hbox(8);
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
            // `on_search` rebuilds the rows: defer the dialog bind out of the
            // field's `text_changed` emission (PAR-0470).
            let state = state_for_search.clone();
            defer_dialog_action("custom_rules_search_rebuild", &target, move |dialog| {
                on_search(dialog, &state, &text);
            });
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
