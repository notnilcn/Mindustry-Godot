// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The team rules and their collapsers in the custom rules dialog.
//! Upstream: ui/dialogs/CustomRulesDialog.java (`team`, `_build_team_collapser`)

use super::*;

/// `Team.baseTeams` rows (`MindEditor.palette_teams`; `[{id, name, color}]`).
pub(super) fn base_teams() -> Teams {
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
pub(super) fn team_name(state: &Rc<RefCell<CustomRulesState>>, id: i64) -> String {
    let teams = state.borrow().teams.clone();
    for team in teams {
        if dict_i64(&team, "id", 0) == id {
            return kit::t(&format!("@team.{}.name", dict_text(&team, "name"))).to_string();
        }
    }
    id.to_string()
}

/// `Team.coloredName()` tint (the packed `Team.color`).
pub(super) fn team_font_color(state: &Rc<RefCell<CustomRulesState>>, id: i64) -> Color {
    let teams = state.borrow().teams.clone();
    for team in teams {
        if dict_i64(&team, "id", 0) == id {
            return rgba8_color(dict_i64(&team, "color", 0xFFFF_FFFF));
        }
    }
    Color::WHITE
}

/// One base team's `TeamRule` (`rules.teams.get(team)`, created on demand).
pub(super) fn team_rules(
    rules: &Rc<RefCell<VarDictionary>>,
    team: i64,
) -> Rc<RefCell<VarDictionary>> {
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
pub(super) fn team_rule(
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
pub(super) fn build_team_collapser(
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
