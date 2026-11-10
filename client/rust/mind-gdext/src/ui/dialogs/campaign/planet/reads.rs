// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Campaign/view read models used by the planet dialog page.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use super::util::*;
use super::*;

// ---- campaign reads -------------------------------------------------------

/// `refresh_campaign_views(planet)`: the live `MindCampaign` projection.
pub(super) fn planet_refresh_views(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> VarDictionary {
    let views = dialog.campaign_json("campaign_views_json", &[GString::from(planet).to_variant()]);
    if !views.is_empty() {
        st.views = views.clone();
        st.views_planet = planet.to_string();
    }
    st.views.clone()
}

/// Ensures the cached projection was read for `planet`.
pub(super) fn planet_ensure_views(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> VarDictionary {
    if st.views_planet == planet && !st.views.is_empty() {
        return st.views.clone();
    }
    planet_refresh_views(dialog, st, planet)
}

/// The live projection for a planet without touching the dialog's cache.
pub(super) fn planet_live_views(dialog: &mut MindDialog, planet: &str) -> VarDictionary {
    dialog.campaign_json("campaign_views_json", &[GString::from(planet).to_variant()])
}

pub(super) fn planet_find_planet(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    name: &str,
) -> VarDictionary {
    let views = planet_refresh_views(dialog, st, "");
    if views.is_empty() {
        return VarDictionary::new();
    }
    for planet in row_rows(&views, "planets") {
        if row_str(&planet, "name") == name {
            return planet;
        }
    }
    VarDictionary::new()
}

/// `PlanetDialog` sectors for a planet: the active projection's grid, else the
/// synthesized start sector from the planet row (`Planet.getStartSector`).
pub(super) fn planet_sectors(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> Vec<VarDictionary> {
    let views = planet_ensure_views(dialog, st, planet);
    if row_str(&views, "planet") == planet {
        return row_rows(&views, "sectors");
    }
    let planet_row = {
        let mut out = VarDictionary::new();
        for row in row_rows(&views, "planets") {
            if row_str(&row, "name") == planet {
                out = row;
                break;
            }
        }
        out
    };
    if planet_row.is_empty() {
        return Vec::new();
    }
    let mut row = VarDictionary::new();
    row.set("id", row_i64(&planet_row, "start_sector"));
    let localized = row_str(&planet_row, "localized");
    row.set(
        "name",
        &GString::from(if localized.is_empty() {
            planet
        } else {
            &localized
        }),
    );
    row.set("planet", &GString::from(planet));
    row.set("threat_band", &GString::from("low"));
    row.set("locked", false);
    row.set("has_base", false);
    vec![row]
}

pub(super) fn planet_find_sector(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
    sector_id: i64,
) -> VarDictionary {
    for sector in planet_sectors(dialog, st, planet) {
        if row_i64(&sector, "id") == sector_id {
            return sector;
        }
    }
    VarDictionary::new()
}

pub(super) fn planet_find_planet_row(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    name: &str,
) -> VarDictionary {
    planet_find_planet(dialog, st, name)
}

/// Localized planet label (`planet.<name>.name`), falling back to the row.
pub(super) fn planet_localized_planet(planet: &VarDictionary) -> String {
    let name = row_str(planet, "name");
    let localized = bundle_or(&format!("planet.{name}.name"), "");
    if localized.is_empty() {
        let fallback = row_str(planet, "localized");
        return if fallback.is_empty() { name } else { fallback };
    }
    localized
}

/// Localized sector label (`sector.<preset>.name`).
pub(super) fn planet_localized_sector(sector: &VarDictionary) -> String {
    if row_bool(sector, "named") {
        return row_str(sector, "name");
    }
    let preset = row_str(sector, "preset");
    if !preset.is_empty() {
        let localized = bundle_or(&format!("sector.{preset}.name"), "");
        if !localized.is_empty() {
            return localized;
        }
    }
    row_str(sector, "name")
}

/// `requirement.<kind>` argument localization.
pub(super) fn planet_localized_objective_arg(kind: &str, argument: &str) -> String {
    if argument.is_empty() {
        return String::from("???");
    }
    match kind {
        "capture" | "onsector" => {
            let key = format!("sector.{argument}.name");
            let value = kit::t(&format!("@{key}")).to_string();
            if value.is_empty() || value == key {
                argument.to_string()
            } else {
                value
            }
        }
        "onplanet" => {
            let key = format!("planet.{argument}.name");
            let value = kit::t(&format!("@{key}")).to_string();
            if value.is_empty() || value == key {
                argument.to_string()
            } else {
                value
            }
        }
        _ => {
            for type_name in ["block", "item", "unit", "liquid", "status"] {
                let key = format!("{type_name}.{argument}.name");
                let value = kit::t(&format!("@{key}")).to_string();
                if !value.is_empty() && value != key {
                    return value;
                }
            }
            argument.to_string()
        }
    }
}

/// Visible state lines in `PlanetDialog.updateSelected` order.
pub(super) fn planet_sector_state_labels(sector: &VarDictionary) -> Vec<String> {
    let mut out = Vec::new();
    if row_bool(sector, "attacked") {
        out.push(String::from("@sectors.underattack"));
    } else if row_bool(sector, "vulnerable") {
        out.push(String::from("@sectors.vulnerable"));
    } else if !row_bool(sector, "has_base") && row_bool(sector, "has_enemy_base") {
        out.push(String::from("@sectors.enemybase"));
    }
    out
}
