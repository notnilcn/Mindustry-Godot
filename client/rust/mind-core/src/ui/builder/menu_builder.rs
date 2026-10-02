// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/MenuBuilder.java.

//! Server-side menu builder fields (plan 14 §3.6).
//!
//! `show`/`show_all`/`update` are transport operations owned by plan 21; the
//! Godot-free `mind-core` half holds the fields and the DSL constructor so tests
//! and the headless harness can build menus without a network.

use crate::ui::builder::dsl;
use crate::ui::builder::ui_node::UiNode;

/// Builder for presenting a complex menu to the player.
#[derive(Debug, Clone)]
pub struct MenuBuilder {
    /// Title; `None` hides the orange title bar entirely.
    pub title: Option<String>,
    /// Immediately hide the dialog after a click listener fires.
    pub hide_on_click: bool,
    /// Hide existing menus with this id.
    pub hide_existing: bool,
    /// Fill the screen (required for large menus).
    pub fill_screen: bool,
    /// Token echoed back in the result.
    pub token: i64,
    /// Menu callback id.
    pub id: i32,
    /// Menu body (implicit root table).
    pub ui: UiNode,
}

impl Default for MenuBuilder {
    fn default() -> Self {
        Self {
            title: None,
            hide_on_click: true,
            hide_existing: true,
            fill_screen: true,
            token: 0,
            id: 0,
            ui: UiNode::new(crate::ui::builder::ui_key::UiKey::Table),
        }
    }
}

impl MenuBuilder {
    /// `MenuBuilder.of(NodeBuilder)`.
    pub fn of(ui: UiNode) -> Self {
        Self {
            ui,
            ..Self::default()
        }
    }

    /// `MenuBuilder.of(String)` — parses UI DSL, surfacing errors as `Err`.
    pub fn of_dsl(source: &str) -> Result<Self, dsl::DslError> {
        Ok(Self::of(dsl::parse(source)?))
    }

    /// Builder: hide existing.
    pub fn hide_existing(mut self, value: bool) -> Self {
        self.hide_existing = value;
        self
    }

    /// Builder: token.
    pub fn token(mut self, value: i64) -> Self {
        self.token = value;
        self
    }

    /// Builder: title.
    pub fn title(mut self, value: impl Into<String>) -> Self {
        self.title = Some(value.into());
        self
    }

    /// Builder: hide on click.
    pub fn hide_on_click(mut self, value: bool) -> Self {
        self.hide_on_click = value;
        self
    }

    /// Builder: fill screen.
    pub fn fill_screen(mut self, value: bool) -> Self {
        self.fill_screen = value;
        self
    }

    /// Builder: id.
    pub fn id(mut self, value: i32) -> Self {
        self.id = value;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_builder_fields() {
        let builder = MenuBuilder::of_dsl("button: \"ok\"\n")
            .unwrap()
            .title("test")
            .id(3)
            .token(99)
            .hide_existing(false);
        assert_eq!(builder.title.as_deref(), Some("test"));
        assert_eq!(builder.id, 3);
        assert_eq!(builder.token, 99);
        assert!(!builder.hide_existing);
        assert!(builder.hide_on_click);
        assert!(builder.fill_screen);

        let default = MenuBuilder::default();
        assert!(default.title.is_none());
    }
}
