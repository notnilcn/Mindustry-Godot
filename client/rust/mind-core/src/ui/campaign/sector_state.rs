// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The typed `get_sector_state` payload: the session/rules scalars plus the
//! played sector's record rows.
//! Upstream: none (port-only; the `MindCampaign` sector-state seam)

use serde::{Deserialize, Serialize};

use crate::content::registries::sectors::SectorPresetDef;
use crate::content::{ContentRegistry, PlanetId};
use crate::game::universe::Campaign;

/// The `get_sector_state` dictionary as a typed view. Field order is the
/// `VarDictionary` insertion order the dialogs read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorStateView {
    /// `state.rules.sector != null` (`PlaySession::is_campaign`).
    pub campaign: bool,
    /// `GameState.isEditor` (`rules.editor`): the pause dialog's desktop
    /// `!state.isEditor()` gate for the `@savegame`/`@loadgame` entries.
    pub editor: bool,
    /// Current wave (`state.wave`).
    pub wave: i32,
    /// `rules.waves`.
    pub waves: bool,
    /// `rules.attackMode`.
    pub attack: bool,
    /// The player team has at least one live core.
    pub has_core: bool,
    /// The live custom map's name (the host match id for non-campaign
    /// worlds); empty for campaign sectors and the menu.
    pub map_name: String,
    /// The played sector's planet content name (empty without a played sector).
    pub planet: String,
    /// The played sector's grid index (`-1` without a played sector).
    pub sector: i64,
    /// `sector.info.wasCaptured`.
    pub was_captured: bool,
    /// `sector.hasSave()`.
    pub has_save: bool,
    /// `sector.isLastSector()`.
    pub is_last: bool,
    /// `state.getSector().preset` name (empty for a generated sector).
    pub preset: String,
    /// The landing announce gate (`CoreBlock.java:414`): campaign +
    /// `Vars.showSectorLandInfo` + `preset.showSectorLandInfo`.
    pub show_land_info: bool,
    /// `sector.isCaptured(None)`.
    pub captured: bool,
    /// The preset description (`UnlockableContent.description` with the live
    /// bundle fallback).
    pub preset_description: String,
    /// The campaign-victory flag (`campaignComplete`).
    pub campaign_complete: bool,
}

/// The inputs `sector_state_view` projects. The facade resolves the host values
/// (`has_core`, `campaign_complete`, `map_name`) and the localized preset
/// description, then passes them in.
pub struct SectorStateInputs<'a> {
    /// The live campaign, when one booted.
    pub campaign: Option<&'a Campaign>,
    /// The content registry, when one booted.
    pub registry: Option<&'a ContentRegistry>,
    /// The played sector (`session.sector`).
    pub sector: Option<(PlanetId, u16)>,
    /// `session.is_campaign()`.
    pub is_campaign: bool,
    /// `rules.editor`.
    pub editor: bool,
    /// `session.wave`.
    pub wave: i32,
    /// `rules.waves`.
    pub waves: bool,
    /// `rules.attack_mode`.
    pub attack: bool,
    /// `session.player_core_count() > 0`.
    pub has_core: bool,
    /// The live custom map's name (empty outside a custom match).
    pub map_name: String,
    /// The campaign-victory flag.
    pub campaign_complete: bool,
    /// The preset's localized description.
    pub preset_description: String,
}

/// Projects the sector-state rows. Empty sector rows (planet/sector/flags) when
/// no sector is being played, exactly like the replaced `sector_state_rows`.
pub fn sector_state_view(inputs: &SectorStateInputs<'_>) -> SectorStateView {
    let mut view = SectorStateView {
        campaign: inputs.is_campaign,
        editor: inputs.editor,
        wave: inputs.wave,
        waves: inputs.waves,
        attack: inputs.attack,
        has_core: inputs.has_core,
        map_name: inputs.map_name.clone(),
        planet: String::new(),
        sector: -1,
        was_captured: false,
        has_save: false,
        is_last: false,
        preset: String::new(),
        show_land_info: false,
        captured: false,
        preset_description: inputs.preset_description.clone(),
        campaign_complete: inputs.campaign_complete,
    };
    let Some((planet, sector)) = inputs.sector else {
        return view;
    };
    let record = inputs
        .campaign
        .and_then(|campaign| campaign.sector(planet, sector));
    let preset = record.and_then(|sector| sector.preset);
    let preset_def: Option<&SectorPresetDef> =
        preset.and_then(|preset| inputs.registry.and_then(|registry| registry.sector(preset)));
    view.planet = inputs
        .campaign
        .and_then(|campaign| campaign.planet_name(planet))
        .unwrap_or_default()
        .to_owned();
    view.sector = i64::from(sector);
    view.was_captured = record.map(|sector| sector.info.info.was_captured) == Some(true);
    view.has_save = record.is_some_and(|sector| sector.has_save());
    view.is_last = record.is_some_and(|sector| sector.is_last_sector());
    view.preset = preset_def.map(|def| def.name.clone()).unwrap_or_default();
    view.show_land_info =
        crate::content::registries::sectors::shows_sector_land_info(inputs.is_campaign, preset_def);
    view.captured = record.is_some_and(|sector| sector.is_captured(None));
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_state_view_without_a_played_sector_keeps_the_scalars() {
        let view = crate::ui::campaign::sector_state_view(&SectorStateInputs {
            campaign: None,
            registry: None,
            sector: None,
            is_campaign: false,
            editor: true,
            wave: 4,
            waves: true,
            attack: false,
            has_core: false,
            map_name: "map".to_owned(),
            campaign_complete: true,
            preset_description: String::new(),
        });
        assert_eq!(view.sector, -1);
        assert!(view.planet.is_empty());
        assert!(!view.was_captured);
        assert!(!view.has_save);
        assert!(!view.is_last);
        assert!(!view.show_land_info);
        assert!(!view.captured);
        assert_eq!(view.wave, 4);
        assert!(view.editor);
        assert!(view.campaign_complete);
        assert_eq!(view.map_name, "map");
    }

    #[test]
    fn sector_state_view_round_trips_serde_in_field_order() {
        let view = SectorStateView {
            campaign: true,
            editor: false,
            wave: 3,
            waves: false,
            attack: true,
            has_core: true,
            map_name: "map".to_owned(),
            planet: "serpulo".to_owned(),
            sector: 12,
            was_captured: false,
            has_save: true,
            is_last: false,
            preset: "groundZero".to_owned(),
            show_land_info: true,
            captured: false,
            preset_description: "desc".to_owned(),
            campaign_complete: false,
        };
        let text = serde_json::to_string(&view).expect("serialize");
        assert!(text.starts_with("{\"campaign\":true,\"editor\":false,\"wave\":3"));
        let back: SectorStateView = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, view);
        assert_eq!(serde_json::to_string(&back).expect("reserialize"), text);
    }
}
