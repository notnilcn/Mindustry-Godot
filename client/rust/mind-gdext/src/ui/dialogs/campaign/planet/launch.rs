// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The sector launch flow: loadout, resource totals and entering the game.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{VarArray, VarDictionary};
use godot::classes::Json;
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;

use super::chooser::*;
use super::sector_panel::*;
use super::util::*;
use super::*;

// ---- launch flow ----------------------------------------------------------

/// `_open_launch_loadout`: asks for a loadout before entering a fresh sector.
pub(super) fn planet_open_launch_loadout(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
    launcher_id: i64,
) {
    let Some(loadout_dialog) = dialog_node("launch_loadout") else {
        planet_launch(dialog, st, planet_name, sector_id, -1, false);
        return;
    };
    if !loadout_dialog.has_signal("loadout_chosen") {
        planet_launch(dialog, st, planet_name, sector_id, -1, false);
        return;
    }
    let target = dialog.to_gd();
    let state_for_choice = state.clone();
    let callable = Callable::from_fn("planet_loadout_chosen", move |args: &[&Variant]| {
        let index = args
            .first()
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(-1);
        // Close the launch dialog BEFORE binding the page: `close_dialog`
        // reveals the new stack top and calls its `is_shown`, which re-enters
        // this same `MindDialog` and panics with `bind() failed, already bound`
        // when the page is already borrowed (PAR-0488).
        let _ = ui_call(
            "close_dialog",
            &[GString::from("launch_loadout").to_variant()],
        );
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let mut st = state_for_choice.borrow_mut();
        let planet = st.selected_planet.clone();
        let sector = st.selected_sector;
        planet_launch(&mut bound, &mut st, &planet, sector, index, false);
    });
    connect_once(
        dialog,
        "launch_loadout_chosen",
        &loadout_dialog,
        "loadout_chosen",
        &callable,
    );
    let mut context = VarDictionary::new();
    context.set("planet", &GString::from(planet_name));
    context.set("sector", sector_id);
    context.set("launcher", launcher_id);
    dialog.open_dialog("launch_loadout", &json_stringify(&context));
}

/// `[{item, amount}]` stacks from an ordered totals list.
pub(super) fn planet_stacks_of(totals: &[(String, i64)]) -> Vec<VarDictionary> {
    totals
        .iter()
        .map(|(item, amount)| {
            let mut stack = VarDictionary::new();
            stack.set("item", &GString::from(item.as_str()));
            stack.set("amount", *amount);
            stack
        })
        .collect()
}

/// `[{item, amount}]` as a `VarArray` (the `start_sector_with_loadout` stacks).
pub(super) fn planet_stacks_array(stacks: &[VarDictionary]) -> VarArray {
    let mut array = VarArray::new();
    for stack in stacks {
        array.push(&stack.to_variant());
    }
    array
}

/// JSON array text for a stack list.
pub(super) fn planet_stacks_json(stacks: &[VarDictionary]) -> String {
    Json::stringify(&planet_stacks_array(stacks).to_variant()).to_string()
}

/// `MindCampaign.launch_dialog_json` for the launcher launch.
pub(super) fn planet_launch_plan(
    dialog: &mut MindDialog,
    planet_name: &str,
    launcher: i64,
    sector_id: i64,
    loadout: i64,
) -> VarDictionary {
    dialog.campaign_json(
        "launch_dialog_json",
        &[
            GString::from(planet_name).to_variant(),
            launcher.to_variant(),
            sector_id.to_variant(),
            loadout.to_variant(),
        ],
    )
}

/// `universe.getLaunchResources()` as an item->amount map.
pub(super) fn planet_launch_resource_totals(dialog: &mut MindDialog) -> VarDictionary {
    let Some(mut campaign) = dialog.campaign_node() else {
        return VarDictionary::new();
    };
    if !campaign.has_method("get_launch_resource_stacks") {
        return VarDictionary::new();
    }
    let text = campaign
        .call("get_launch_resource_stacks", &[])
        .to::<GString>();
    parse_object(&text.to_string())
}

/// `PlanetDialog.playSelected` (`PlanetDialog.java:1396-1403,1449`): the
/// stored loadout + launch resources are dropped **only** by the free-launch
/// branch (`mode == look`, no base, `findLauncher == null`). A resume/go entry
/// (`control.playSector(sector)`, `:1449`) and a launcher launch keep them, so
/// resuming a base no longer wipes the player's configured loadout (PAR-0507).
pub(super) fn planet_launch_clears_loadout(resume: bool, launcher_id: i64, loadout: i64) -> bool {
    !resume && launcher_id < 0 && loadout < 0
}

/// `_launch`: enters the sector, either resuming, via a loadout, or free.
pub(super) fn planet_launch(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
    loadout: i64,
    resume: bool,
) {
    if sector_id < 0 {
        return;
    }
    dialog.to_gd().emit_signal(
        "sector_activated",
        &[
            GString::from(planet_name).to_variant(),
            sector_id.to_variant(),
        ],
    );
    let mut campaign = dialog.campaign_node();
    if let Some(campaign) = campaign.as_mut()
        && campaign.has_method("prime_sector_load")
    {
        let _ = campaign.call(
            "prime_sector_load",
            &[
                GString::from(planet_name).to_variant(),
                sector_id.to_variant(),
            ],
        );
    }

    let mut launcher_id = -1i64;
    let mut launch_resources = VarDictionary::new();
    let mut launch_core = String::new();
    let mut launch_cost: Vec<VarDictionary> = Vec::new();
    if loadout >= 0 && !resume {
        launcher_id = planet_sector_launch_from(dialog, st, planet_name, sector_id);
        // `PlanetDialog.java:1409-1410` charges
        // `loadout.requirements() + universe.getLaunchResources()` for the
        // loadout the picker actually selected. `launch_dialog_json` reports
        // that `total` as `launch_cost` from the selected schematic row — the
        // old path looked the index up in the planet-default core's loadout
        // list, which is empty for a larger-core schematic, so its block cost
        // was never charged (PAR-0489).
        let plan = planet_launch_plan(dialog, planet_name, launcher_id, sector_id, loadout);
        launch_resources = row_dict(&plan, "resources");
        if launch_resources.is_empty() {
            launch_resources = planet_launch_resource_totals(dialog);
        }
        launch_core = row_str(&plan, "core");
        launch_cost = row_rows(&plan, "launch_cost");
        if launch_cost.is_empty() {
            // No plan (the facade is absent): charge the staged resources.
            launch_cost = planet_stacks_of(&planet_dict_totals(&launch_resources));
        }
    }

    // `PlanetDialog.playSelected`'s "save before launch"
    // (`PlanetDialog.java:1380-1386`): write the sector being left BEFORE
    // `MindSimHost.load_sector` replaces the live world, so a switch keeps
    // everything built since that sector's last autosave (CG-SECTOR-02).
    // No-op outside a live match.
    if let Some(campaign) = campaign.as_mut()
        && campaign.has_method("save_outgoing_sector")
    {
        let _ = campaign.call("save_outgoing_sector", &[]);
    }

    // H05: stage the launcher's items before the world load.
    if let Some(mut host) = kit::sim_host() {
        if host.has_method("set_pending_launch") {
            if loadout >= 0 && !resume {
                let stacks =
                    planet_stacks_json(&planet_stacks_of(&planet_dict_totals(&launch_resources)));
                let _ = host.call(
                    "set_pending_launch",
                    &[
                        loadout.to_variant(),
                        GString::from(stacks.as_str()).to_variant(),
                    ],
                );
            } else {
                let _ = host.call(
                    "set_pending_launch",
                    &[(-1i64).to_variant(), GString::from("[]").to_variant()],
                );
            }
        }
        if host.has_method("load_sector") {
            let _ = host.call(
                "load_sector",
                &[
                    GString::from(planet_name).to_variant(),
                    sector_id.to_variant(),
                ],
            );
        }
    }

    let mut started = false;
    if let Some(campaign) = campaign.as_mut() {
        // `MindCampaign` has no `play_sector` binding: `start_sector` picks the
        // resume/launch branch itself from the sector record
        // (`start_sector_with_loadout`'s `has_save` decision).
        if loadout >= 0 && campaign.has_method("start_sector_with_loadout") {
            if launcher_id >= 0 {
                let _ = campaign.call(
                    "remove_sector_items",
                    &[
                        GString::from(planet_name).to_variant(),
                        launcher_id.to_variant(),
                        GString::from(planet_stacks_json(&launch_cost).as_str()).to_variant(),
                    ],
                );
            }
            let core_stacks = planet_stacks_of(&planet_dict_totals(&launch_resources));
            let mut payload = VarDictionary::new();
            payload.set("core", &GString::from(launch_core.as_str()));
            payload.set("schematic", loadout);
            payload.set("stacks", &planet_stacks_array(&core_stacks).to_variant());
            payload.set("origin", launcher_id);
            started = campaign
                .call(
                    "start_sector_with_loadout",
                    &[
                        GString::from(planet_name).to_variant(),
                        sector_id.to_variant(),
                        GString::from(json_stringify(&payload).as_str()).to_variant(),
                    ],
                )
                .try_to::<bool>()
                .unwrap_or(false);
        } else if campaign.has_method("start_sector") {
            // `PlanetDialog.java:1396-1403` clears the stored loadout +
            // launch resources only on the *free* launch branch (no launcher
            // sector). A resume/go entry and a launcher launch keep them
            // (PAR-0507); the resume branch previously cleared them because
            // `play_sector` is not a bound method.
            if planet_launch_clears_loadout(resume, launcher_id, loadout)
                && campaign.has_method("clear_loadout_info")
            {
                let _ = campaign.call("clear_loadout_info", &[]);
            }
            started = campaign
                .call(
                    "start_sector",
                    &[
                        GString::from(planet_name).to_variant(),
                        sector_id.to_variant(),
                    ],
                )
                .try_to::<bool>()
                .unwrap_or(false);
        }
    }
    // `MindUi.close_dialog` defers the hide, but the dialog's own `hidden()`
    // can still run synchronously when the page was not opened through the UI
    // shell; the launch callback holds the state borrow until it returns, so
    // close on the next frame instead of re-entering the borrow here.
    let target = dialog.to_gd();
    st.deferred.push(move || {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        bound.close();
    });
    if started {
        planet_enter_game(st);
    }
}

/// The item->amount totals of a `{item: amount}` dictionary.
pub(super) fn planet_dict_totals(dict: &VarDictionary) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for key in dict.keys_array().iter_shared() {
        let item = key
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let amount = dict
            .get(item.as_str())
            .map(|value| variant_i64(&value))
            .unwrap_or(0);
        out.push((item, amount));
    }
    out
}

/// Hides the menu and shows the in-game HUD group (`MindUiRoot`).
pub(super) fn planet_enter_game(st: &mut PlanetState) {
    for (path, visible) in st.saved_layers.iter_mut() {
        if path == MENU_GROUP_PATH || path == MENU_BACKGROUND_PATH {
            *visible = false;
        } else if path == HUD_GROUP_PATH {
            *visible = true;
        }
    }
    if let Some(mut root) = ui_root()
        && root.has_method("set_menu_visible")
    {
        let _ = root.call("set_menu_visible", &[false.to_variant()]);
    }
}
