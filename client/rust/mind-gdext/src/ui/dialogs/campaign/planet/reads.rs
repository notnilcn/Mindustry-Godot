// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Campaign/view read models used by the planet dialog page.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java

use super::util::*;
use super::*;
use mind_core::ui::campaign::{CampaignViews, PlanetView, SectorView};

// ---- campaign reads -------------------------------------------------------

/// `refresh_campaign_views(planet)`: the live typed `MindCampaign` projection.
pub(super) fn planet_refresh_views(
    dialog: &MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> Option<CampaignViews> {
    if let Some(views) = campaign_views_of(dialog, planet) {
        st.views = Some(views);
        st.views_planet = planet.to_string();
    }
    st.views.clone()
}

/// Ensures the cached projection was read for `planet`.
pub(super) fn planet_ensure_views(
    dialog: &MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> Option<CampaignViews> {
    if st.views_planet == planet && st.views.is_some() {
        return st.views.clone();
    }
    planet_refresh_views(dialog, st, planet)
}

/// The live projection for a planet without touching the dialog's cache.
pub(super) fn planet_live_views(dialog: &MindDialog, planet: &str) -> Option<CampaignViews> {
    campaign_views_of(dialog, planet)
}

pub(super) fn planet_find_planet(
    dialog: &MindDialog,
    st: &mut PlanetState,
    name: &str,
) -> Option<PlanetView> {
    let views = planet_refresh_views(dialog, st, "")?;
    views
        .planets
        .iter()
        .find(|planet| planet.name == name)
        .cloned()
}

/// `PlanetDialog` sectors for a planet: the active projection's grid, else the
/// synthesized start sector from the planet card (`Planet.getStartSector`).
pub(super) fn planet_sectors(
    dialog: &MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> Vec<SectorView> {
    let Some(views) = planet_ensure_views(dialog, st, planet) else {
        return Vec::new();
    };
    if views.planet == planet {
        return views.sectors;
    }
    let Some(planet_row) = views.planets.iter().find(|row| row.name == planet) else {
        return Vec::new();
    };
    // The old synthesized row carried id/name/planet, `threat_band: low` and
    // the two false flags; every other read fell back to its empty default.
    let name = if planet_row.localized.is_empty() {
        planet
    } else {
        planet_row.localized.as_str()
    };
    vec![empty_sector_view(planet_row.start_sector, planet, name)]
}

pub(super) fn planet_find_sector(
    dialog: &MindDialog,
    st: &mut PlanetState,
    planet: &str,
    sector_id: i64,
) -> Option<SectorView> {
    planet_sectors(dialog, st, planet)
        .into_iter()
        .find(|sector| i64::from(sector.id) == sector_id)
}

pub(super) fn planet_find_planet_row(
    dialog: &MindDialog,
    st: &mut PlanetState,
    name: &str,
) -> Option<PlanetView> {
    planet_find_planet(dialog, st, name)
}

/// Localized planet label (`planet.<name>.name`), falling back to the card.
pub(super) fn planet_localized_planet(planet: &PlanetView) -> String {
    let name = planet.name.clone();
    let localized = bundle_or(&format!("planet.{name}.name"), "");
    if localized.is_empty() {
        return if planet.localized.is_empty() {
            name
        } else {
            planet.localized.clone()
        };
    }
    localized
}

/// Localized sector label (`sector.<preset>.name`).
pub(super) fn planet_localized_sector(sector: &SectorView) -> String {
    if sector.named {
        return sector.name.clone();
    }
    if let Some(preset) = sector.preset.as_deref()
        && !preset.is_empty()
    {
        let localized = bundle_or(&format!("sector.{preset}.name"), "");
        if !localized.is_empty() {
            return localized;
        }
    }
    sector.name.clone()
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
pub(super) fn planet_sector_state_labels(sector: &SectorView) -> Vec<String> {
    let mut out = Vec::new();
    if sector.attacked {
        out.push(String::from("@sectors.underattack"));
    } else if sector.vulnerable {
        out.push(String::from("@sectors.vulnerable"));
    } else if !sector.has_base && sector.has_enemy_base {
        out.push(String::from("@sectors.enemybase"));
    }
    out
}
