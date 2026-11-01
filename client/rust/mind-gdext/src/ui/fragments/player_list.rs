// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PlayerListFragment.java (plan 14 M7).
//
// Player-list shell: search field + row host + bans/admins/close actions. Rows
// come from `MindUi.player_list_json()` (empty until plan 21 supplies live
// views); row actions go through `MindUi.player_action`, which is the documented
// plan-21 relay seam and never fakes a server action (task rule).

//! Player-list overlay (`PlayerListFragment`).

use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::{
    CenterContainer, Control, HBoxContainer, IControl, Json, PanelContainer, VBoxContainer,
};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::kit;
use crate::ui::kit::widgets::factory;

/// `PlayerListFragment` bottom action row, in order.
const MENU_ACTIONS: [&str; 3] = ["server.bans", "server.admins", "close"];

/// `MindPlayerListFragment` — the multiplayer player list.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindPlayerListFragment {
    base: Base<Control>,
    content: Option<Gd<VBoxContainer>>,
    search: String,
}

#[godot_api]
impl IControl for MindPlayerListFragment {
    fn ready(&mut self) {
        self.base_mut().set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().set_visible(false);
        self.build();
    }
}

#[godot_api]
impl MindPlayerListFragment {
    /// Emitted when the close action (or Escape) dismisses the overlay.
    #[signal]
    fn close_requested();

    /// Rebuilds the row host from `MindUi.player_list_json()`.
    #[func]
    pub fn rebuild(&mut self) {
        let Some(mut content) = self.content.clone() else {
            return;
        };
        for child in content.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        let mut rows: Vec<VarDictionary> = Vec::new();
        if let Some(mut ui) = kit::ui()
            && ui.has_method("player_list_json")
        {
            let payload = ui.call("player_list_json", &[]);
            if let Ok(text) = payload.try_to::<GString>() {
                let parsed = Json::parse_string(&text);
                if let Ok(array) = parsed.try_to::<VarArray>() {
                    for row in array.iter_shared() {
                        if let Ok(row) = row.try_to::<VarDictionary>() {
                            rows.push(row);
                        }
                    }
                }
            }
        }
        for row in rows {
            let name = row
                .get("name")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default()
                .to_string();
            if !matches_search(&name, &self.search) {
                continue;
            }
            // code-instantiated: player rows are server-driven (plan 21 relay
            // views).
            let mut button = factory::button(&name);
            let mut target = self.to_gd();
            let player_name = GString::from(name.as_str());
            let callable = Callable::from_fn("player_row", move |_args: &[&Variant]| {
                let _ = target.call("player_selected", &[player_name.to_variant()]);
            });
            button.connect("pressed", &callable);
            content.add_child(&button);
        }
    }

    /// Shows/hides the overlay, rebuilding the rows when it opens.
    #[func]
    pub fn toggle(&mut self) {
        let visible = !self.base().is_visible();
        self.base_mut().set_visible(visible);
        if visible {
            self.rebuild();
        }
    }

    /// Whether the overlay is visible.
    #[func]
    pub fn shown(&self) -> bool {
        self.base().is_visible()
    }

    /// Search field filter (`text_changed`).
    #[func]
    fn search_changed(&mut self, text: GString) {
        self.search = text.to_string().to_lowercase();
        self.rebuild();
    }

    /// A player row was activated: `PlayerListFragment` opens spectate.
    #[func]
    fn player_selected(&mut self, player_name: GString) {
        self.player_action(&player_name.to_string(), "spectate");
    }

    /// Bottom action row (`server.bans` / `server.admins` / `close`).
    #[func]
    fn menu_action(&mut self, action: GString) {
        match action.to_string().as_str() {
            "server.bans" => self.open_dialog("@server.bans"),
            "server.admins" => self.open_dialog("@server.admins"),
            _ => {
                self.base_mut().set_visible(false);
                self.base_mut().emit_signal("close_requested", &[]);
            }
        }
    }
}

impl MindPlayerListFragment {
    fn build(&mut self) {
        // code-instantiated: the playerlist panel is a runtime overlay whose
        // rows are server-driven, so its structure is built once in code
        // (plan 14 §3.10).
        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().add_child(&center);

        let mut panel = PanelContainer::new_alloc();
        panel.set_theme_type_variation("defaultDialog");
        panel.set_custom_minimum_size(Vector2::new(360.0, 420.0));
        center.add_child(&panel);

        let mut column = VBoxContainer::new_alloc();
        panel.add_child(&column);

        let mut field = factory::field(&kit::t("@players.search").to_string());
        let callable = self.base().callable("search_changed");
        field.connect("text_changed", &callable);
        column.add_child(&field);

        let mut content = VBoxContainer::new_alloc();
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
        column.add_child(&content);

        let mut menu = HBoxContainer::new_alloc();
        for action in MENU_ACTIONS {
            let mut button = factory::button(&kit::t(&format!("@{action}")).to_string());
            let mut target = self.to_gd();
            let action_name = GString::from(action);
            let callable = Callable::from_fn("player_menu", move |_args: &[&Variant]| {
                let _ = target.call("menu_action", &[action_name.to_variant()]);
            });
            button.connect("pressed", &callable);
            menu.add_child(&button);
        }
        column.add_child(&menu);

        self.content = Some(content);
        self.rebuild();
    }

    /// Documents the relay seam: per-player actions go through `MindUi`.
    fn player_action(&mut self, player_name: &str, action: &str) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("player_action") {
            return;
        }
        let _ = ui.call(
            "player_action",
            &[
                GString::from(player_name).to_variant(),
                GString::from(action).to_variant(),
            ],
        );
    }

    /// `MindUi.open_dialog(name, "")` with the bundle-style `@` trimmed.
    fn open_dialog(&self, dialog: &str) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let key = dialog.strip_prefix('@').unwrap_or(dialog);
        let _ = ui.call(
            "open_dialog",
            &[GString::from(key).to_variant(), GString::new().to_variant()],
        );
    }
}

/// The search filter (`PlayerListFragment.matches`): empty shows every row.
fn matches_search(player_name: &str, search: &str) -> bool {
    search.is_empty() || player_name.to_lowercase().contains(search)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_filter_is_case_insensitive() {
        assert!(matches_search("Alice", "ali"));
        assert!(matches_search("Alice", ""));
        assert!(!matches_search("Alice", "bob"));
    }

    #[test]
    fn menu_actions_match_the_upstream_row() {
        assert_eq!(MENU_ACTIONS, ["server.bans", "server.admins", "close"]);
    }
}
