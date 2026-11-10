// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The rule categories of the custom rules dialog (`setupMain` and the category helpers).
//! Upstream: ui/dialogs/CustomRulesDialog.java (`setupMain`, `category`)

use super::*;

/// `setupMain`: every category and row, in Java order.
pub(super) fn setup_main(dialog: &mut MindDialog, state: &Rc<RefCell<CustomRulesState>>) {
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
                    let state = state_for.clone();
                    defer_dialog_action("custom_rules_loadout_open", &target, move |dialog| {
                        open_loadout(dialog, &state);
                    });
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
                    let state = state_for.clone();
                    defer_dialog_action(
                        "custom_rules_banned_blocks_open",
                        &target,
                        move |dialog| {
                            open_banned(dialog, &state, "block");
                        },
                    );
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
                    let state = state_for.clone();
                    defer_dialog_action("custom_rules_banned_units_open", &target, move |dialog| {
                        open_banned(dialog, &state, "unit");
                    });
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
                    let state = state_for.clone();
                    defer_dialog_action("custom_rules_weather_open", &target, move |dialog| {
                        open_weather(dialog, &state);
                    });
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
pub(super) fn sort_all(node: &Gd<Node>) {
    if let Ok(mut table) = node.clone().try_cast::<MindTable>() {
        for child in node.get_children().iter_shared() {
            sort_all(&child);
        }
        table.bind_mut().sort_now();
    }
}

/// `category(name)`: a fresh left-aligned rule table.
pub(super) fn category(state: &Rc<RefCell<CustomRulesState>>, name: &str) {
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
pub(super) fn plain(key: &str) -> String {
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
pub(super) fn has_key(key: &str) -> bool {
    let Some(assets) = kit::assets() else {
        return false;
    };
    let resolved = key.strip_prefix('@').unwrap_or(key);
    assets.bind().bundle_get(GString::from(resolved)) != resolved
}

/// `ruleInfo`: the `<key>.info` tooltip when the bundle has that key.
pub(super) fn rule_info(control: &mut Gd<Control>, key: &str) {
    let info = format!("{}.info", key.strip_prefix('@').unwrap_or(key));
    if has_key(&info) {
        control.set_tooltip_text(&GString::from(plain(&format!("@{info}")).as_str()));
    }
}

/// `Core.bundle.get(key).toLowerCase().contains(ruleSearch)`.
pub(super) fn matches_key(state: &Rc<RefCell<CustomRulesState>>, key: &str) -> bool {
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
pub(super) fn row_into(target: &Gd<Control>, row: &Gd<Control>, pad_top_zero: bool) {
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
pub(super) fn add_current(
    state: &Rc<RefCell<CustomRulesState>>,
    row: &Gd<MindTable>,
    pad_top_zero: bool,
) {
    let Some(current) = state.borrow().current.clone() else {
        return;
    };
    let target: Gd<Control> = current.upcast();
    let control: Gd<Control> = row.clone().upcast();
    row_into(&target, &control, pad_top_zero);
}

/// Adds a row into a plain container (the team collapsers).
pub(super) fn add_in(target: &Gd<Control>, row: &Gd<MindTable>, pad_top_zero: bool) {
    let control: Gd<Control> = row.clone().upcast();
    row_into(target, &control, pad_top_zero);
}

/// A 300/250-wide category action button (`current.button(...).width(...)`).
pub(super) fn wide_button(
    key: &str,
    icon: &str,
    listener: Option<Callable>,
    width: f32,
) -> Gd<Button> {
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
pub(super) fn push_current<N: Inherits<Control>>(
    state: &Rc<RefCell<CustomRulesState>>,
    control: &Gd<N>,
) {
    let Some(mut table) = state.borrow().current.clone() else {
        return;
    };
    table.bind_mut().add(control).left();
    table.bind_mut().row();
}
