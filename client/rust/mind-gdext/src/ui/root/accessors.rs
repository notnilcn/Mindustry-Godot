// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Owner-mediator accessors and small helpers for migrated classes.
//! Upstream: none (port-only; Rust-only accessors and layout helpers)

use super::*;

impl MindUiRoot {
    // ---- owner-mediator accessors (Rust-only, no ABI change) --------------

    /// The cached HUD layer group (`HudGroup`), resolved lazily when a migrated
    /// class asks before `ready` cached it.
    pub(crate) fn hud_group_node(&self) -> Option<Gd<Control>> {
        self.hud_group
            .clone()
            .or_else(|| self.control_at("HudGroup"))
    }

    /// One HUD fragment by manifest name (`block_config`, `console`, ...): the
    /// mediator seam that replaces the `../Ui/UiRoot/HudGroup/...` walks in
    /// `MindInput`/`MindAudio`/`MindLandingCore`.
    pub(crate) fn hud_fragment(&self, name: &str) -> Option<Gd<Control>> {
        let group = self.hud_group_node()?;
        group.get_node_or_null(name)?.try_cast::<Control>().ok()
    }

    /// The cached menu layer group (`MenuGroup`), resolved lazily.
    pub(crate) fn menu_group_node(&self) -> Option<Gd<Control>> {
        self.menu_group
            .clone()
            .or_else(|| self.control_at("MenuGroup"))
    }

    /// The boot fade cover (`OverlayLayer/fade_in`) used by the menu boot
    /// and by `MindLandingCore` on touchdown.
    pub(crate) fn fade_in_node(&self) -> Option<Gd<Node>> {
        self.base().get_node_or_null("OverlayLayer/fade_in")
    }

    // ---- small helpers ----------------------------------------------------

    /// A direct child of this root as a `Control` (`get_node_or_null(path)`).
    pub(super) fn control_at(&self, path: &str) -> Option<Gd<Control>> {
        self.base()
            .get_node_or_null(path)?
            .try_cast::<Control>()
            .ok()
    }

    /// A direct child of `HudGroup` as a `Control` (the internal short form of
    /// [`Self::hud_fragment`], so there is one lookup implementation).
    pub(super) fn hud_child(&self, name: &str) -> Option<Gd<Control>> {
        self.hud_fragment(name)
    }

    /// The loading fragment under `LoadingLayer`.
    pub(super) fn loading_node(&self) -> Option<Gd<Node>> {
        self.loading_layer.as_ref()?.get_node_or_null("loading")
    }

    /// Hides the loading overlay when the fragment exposes `hide_loading`.
    pub(super) fn hide_loading(&mut self) {
        if let Some(mut loading) = self.loading_node()
            && loading.has_method("hide_loading")
        {
            let _ = loading.call("hide_loading", &[]);
        }
    }

    /// Closes the HUD panels that would otherwise reappear with the HUD group
    /// (`config.hideConfig()`, `inv.hide()`, the fullscreen minimap).
    pub(super) fn close_hud_panels(&mut self) {
        if let Some(mut config) = self.hud_child("block_config")
            && config.is_visible()
        {
            if config.has_method("_close") {
                let _ = config.call("_close", &[]);
            } else {
                config.set_visible(false);
            }
        }
        if let Some(mut inventory) = self.hud_child("block_inventory")
            && inventory.is_visible()
        {
            if inventory.has_method("hide_panel") {
                let _ = inventory.call("hide_panel", &[]);
            } else {
                inventory.set_visible(false);
            }
        }
        for panel_name in ["plan_config", "minimap"] {
            if let Some(mut panel) = self.hud_child(panel_name) {
                panel.set_visible(false);
            }
        }
    }

    /// `Time.get_ticks_msec()` as a float for the toast slot arithmetic.
    pub(super) fn now_ms(&self) -> f64 {
        Time::singleton().get_ticks_msec() as f64
    }

    /// Hosted-state map id: `planet/sector` for a live campaign sector, the
    /// custom map's `Rules.mode_name`, else the default spine world.
    pub(super) fn current_map_id(&self) -> String {
        let Some(campaign) = self.campaign.clone() else {
            return "spine".to_string();
        };
        let state = campaign.bind().get_sector_state();
        let planet = dict_text_try_or(&state, "planet");
        let is_campaign = dict_bool_try_or(&state, "campaign", false);
        if is_campaign && !planet.is_empty() {
            let sector = dict_i64_relaxed_or(&state, "sector", 0);
            return format!("{planet}-{sector}");
        }
        // I08 T4: the live custom map's name (non-campaign worlds).
        let map_name = dict_text_try_or(&state, "mapName");
        if !map_name.is_empty() {
            return map_name;
        }
        let raw = campaign.bind().get_rules_json();
        let parsed = Json::parse_string(&raw);
        if let Ok(rules) = parsed.try_to::<VarDictionary>()
            && let Some(mode_name) = rules.get("modeName")
            && let Ok(name_text) = mode_name.try_to::<GString>()
            && !name_text.is_empty()
        {
            return name_text.to_string();
        }
        "spine".to_string()
    }
}
