// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Menus.java (`menuBuilder`/`menuBuilderUpdate`/
//         `hideMenuBuilder`/`menuBuilderChoose` dialog lifecycle).

//! Server-menu host lifecycle (plan 14 §3.6).
//!
//! The pure state machine behind `Menus.menuBuilder`: show/replace/update/hide a
//! `MenuBuilder` dialog per `id`, capture a `MenuResult` on click, and emit a
//! cancelled result when a dialog is closed without a choice. The Godot node
//! (`mind-gdext::ui::menu_host`) and the GDScript dialog are thin renderers over
//! this module; it has no Godot, network or assets dependency.

use indexmap::IndexMap;

use crate::ui::builder::menu_builder::MenuBuilder;
use crate::ui::builder::menu_result::{MenuResult, MenuValue};
use crate::ui::builder::tree_builder::{BuildContext, build, validate_caps};
use crate::ui::builder::tree_builder::TreeCapsError;
use crate::ui::builder::ui_node::UiNode;

/// A captured click: the result id plus the values of every id-bearing element.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuSelection {
    /// Clicked result id (`clicked`/`enter`).
    pub result: String,
    /// Id → value at click time.
    pub values: Vec<(String, MenuValue)>,
}

impl MenuSelection {
    /// Selection with no element values (a plain button).
    pub fn new(result: impl Into<String>) -> Self {
        Self {
            result: result.into(),
            values: Vec::new(),
        }
    }

    /// Adds an element value.
    pub fn with(mut self, id: impl Into<String>, value: MenuValue) -> Self {
        self.values.push((id.into(), value));
        self
    }
}

/// A live menu dialog.
#[derive(Debug, Clone)]
pub struct MenuEntry {
    /// Callback id.
    pub id: i32,
    /// Token echoed in the result.
    pub token: i64,
    /// Title (`None` removes the title bar).
    pub title: Option<String>,
    /// Hide after a click listener fires.
    pub hide_on_click: bool,
    /// Hide an existing menu with the same id.
    pub hide_existing: bool,
    /// Fill the screen.
    pub fill_screen: bool,
    /// Current menu body.
    pub ui: UiNode,
    /// Id-bearing elements collected at materialization time.
    pub ids: Vec<String>,
    /// Server-streamed image regions collected at materialization time.
    pub images: Vec<String>,
    /// Set once a click listener fired (suppresses the cancelled result).
    pub was_hidden: bool,
}

/// Events the renderer/transport layer consumes after a host call.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuHostEvent {
    /// Show (or replace) `id`. `had_previous` marks a `hide_existing` swap.
    Show {
        /// Menu id.
        id: i32,
        /// Whether an old dialog with this id existed.
        had_previous: bool,
    },
    /// Rebuild the `table_id` element inside `id`.
    Update {
        /// Menu id.
        id: i32,
        /// Target element id.
        table_id: String,
    },
    /// Hide `id` (no result).
    Hide {
        /// Menu id.
        id: i32,
    },
    /// Send this result to the host via the relay.
    Choose(MenuResult),
}

/// Menu-host validation failure (R9 server-menu spam caps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuHostError {
    /// The tree exceeds the node cap.
    Caps(TreeCapsError),
}

impl std::fmt::Display for MenuHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MenuHostError::Caps(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for MenuHostError {}

impl From<TreeCapsError> for MenuHostError {
    fn from(error: TreeCapsError) -> Self {
        MenuHostError::Caps(error)
    }
}

/// Pure server-menu host (`Menus` static maps).
#[derive(Debug, Default)]
pub struct MenuHost {
    menus: IndexMap<i32, MenuEntry>,
}

impl MenuHost {
    /// Creates an empty host.
    pub fn new() -> Self {
        Self::default()
    }

    /// Live menu ids in show order.
    pub fn active_ids(&self) -> Vec<i32> {
        self.menus.keys().copied().collect()
    }

    /// Whether `id` is live.
    pub fn contains(&self, id: i32) -> bool {
        self.menus.contains_key(&id)
    }

    /// Live entry by id.
    pub fn get(&self, id: i32) -> Option<&MenuEntry> {
        self.menus.get(&id)
    }

    /// Number of live menus.
    pub fn len(&self) -> usize {
        self.menus.len()
    }

    /// Whether no menu is live.
    pub fn is_empty(&self) -> bool {
        self.menus.is_empty()
    }

    /// `Menus.menuBuilder`: validates, replaces/hides the old dialog and shows
    /// the new one. Returns the events in upstream order (`Show` then any
    /// `MenuBuilderChoose`/`Hide` for the replaced dialog).
    pub fn show(
        &mut self,
        builder: MenuBuilder,
        ctx: &BuildContext,
    ) -> Result<Vec<MenuHostEvent>, MenuHostError> {
        validate_caps(&builder.ui)?;
        let materialized = build(&builder.ui, ctx);
        let id = builder.id;
        let mut events = Vec::new();
        let had_previous = self.menus.contains_key(&id);
        if had_previous {
            // Upstream `menuDialogs.remove(id)` then hides only when
            // `hideExisting`; a replaced dialog still fires its `hidden`
            // listener (cancelled result) before the new one is kept.
            let old = self.menus.shift_remove(&id).expect("checked");
            if builder.hide_existing {
                events.extend(cancelled_events(&old));
                events.push(MenuHostEvent::Hide { id });
            }
        }
        events.push(MenuHostEvent::Show { id, had_previous });
        self.menus.insert(
            id,
            MenuEntry {
                id,
                token: builder.token,
                title: builder.title.clone(),
                hide_on_click: builder.hide_on_click,
                hide_existing: builder.hide_existing,
                fill_screen: builder.fill_screen,
                ui: builder.ui,
                ids: materialized.ids,
                images: materialized.images,
                was_hidden: false,
            },
        );
        Ok(events)
    }

    /// `Menus.menuBuilderUpdate`: replaces one table inside a live menu.
    pub fn update(
        &mut self,
        id: i32,
        table_id: &str,
        ui: UiNode,
        ctx: &BuildContext,
    ) -> Result<Vec<MenuHostEvent>, MenuHostError> {
        validate_caps(&ui)?;
        let Some(entry) = self.menus.get_mut(&id) else {
            return Ok(Vec::new());
        };
        let materialized = build(&ui, ctx);
        entry.ui = ui;
        entry.ids = materialized.ids;
        entry.images = materialized.images;
        Ok(vec![MenuHostEvent::Update {
            id,
            table_id: table_id.to_owned(),
        }])
    }

    /// `Menus.hideMenuBuilder`: hides a live menu, emitting a cancelled result
    /// unless a click already produced one.
    pub fn hide(&mut self, id: i32) -> Vec<MenuHostEvent> {
        let Some(entry) = self.menus.shift_remove(&id) else {
            return Vec::new();
        };
        let mut events = cancelled_events(&entry);
        events.push(MenuHostEvent::Hide { id });
        events
    }

    /// Records a click listener firing: emits the result, then hides when
    /// `hide_on_click` (without a second cancelled result).
    pub fn choose(&mut self, id: i32, selection: MenuSelection) -> Vec<MenuHostEvent> {
        let Some(entry) = self.menus.get_mut(&id) else {
            return Vec::new();
        };
        entry.was_hidden = true;
        let mut result = MenuResult::from_result(selection.result);
        result.token = entry.token;
        for (key, value) in selection.values {
            result.insert(key, value);
        }
        let hide_on_click = entry.hide_on_click;
        let mut events = vec![MenuHostEvent::Choose(result)];
        if hide_on_click {
            self.menus.shift_remove(&id);
            events.push(MenuHostEvent::Hide { id });
        }
        events
    }
}

/// Cancelled-result event for a closed dialog (upstream `hidden` listener).
fn cancelled_events(entry: &MenuEntry) -> Vec<MenuHostEvent> {
    if entry.was_hidden {
        return Vec::new();
    }
    let mut result = MenuResult::new();
    result.token = entry.token;
    vec![MenuHostEvent::Choose(result)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::builder::ui_key::UiKey;

    fn builder(id: i32, token: i64) -> MenuBuilder {
        let ui = UiNode::new(UiKey::Table)
            .child(
                UiNode::new(UiKey::Button)
                    .str(UiKey::Text, "Buy")
                    .str(UiKey::Clicked, "buy"),
            )
            .child(
                UiNode::new(UiKey::Button)
                    .str(UiKey::Text, "Cancel")
                    .str(UiKey::Clicked, "cancel"),
            );
        MenuBuilder::of(ui).id(id).token(token)
    }

    #[test]
    fn show_choose_hide_lifecycle() {
        let ctx = BuildContext::default();
        let mut host = MenuHost::new();
        let events = host.show(builder(1, 9), &ctx).unwrap();
        assert_eq!(
            events,
            vec![MenuHostEvent::Show {
                id: 1,
                had_previous: false
            }]
        );
        assert!(host.contains(1));
        assert_eq!(host.get(1).unwrap().ids, Vec::<String>::new());

        // A click hides due to `hide_on_click` and carries the token.
        let events = host.choose(1, MenuSelection::new("buy"));
        assert_eq!(events.len(), 2);
        match &events[0] {
            MenuHostEvent::Choose(result) => {
                assert!(result.is("buy"));
                assert_eq!(result.token, 9);
            }
            other => panic!("expected choose, got {other:?}"),
        }
        assert_eq!(events[1], MenuHostEvent::Hide { id: 1 });
        assert!(!host.contains(1));
    }

    #[test]
    fn external_hide_sends_cancelled_result() {
        let ctx = BuildContext::default();
        let mut host = MenuHost::new();
        host.show(builder(2, 44), &ctx).unwrap();
        let events = host.hide(2);
        assert_eq!(events.len(), 2);
        match &events[0] {
            MenuHostEvent::Choose(result) => {
                assert!(result.was_cancelled());
                assert_eq!(result.token, 44);
            }
            other => panic!("expected cancelled choose, got {other:?}"),
        }
        assert_eq!(events[1], MenuHostEvent::Hide { id: 2 });

        // Hiding a live menu after a choice produces no extra cancelled result.
        host.show(builder(3, 1), &ctx).unwrap();
        host.choose(3, MenuSelection::new("buy"));
        assert_eq!(host.hide(3), vec![MenuHostEvent::Hide { id: 3 }]);
    }

    #[test]
    fn hide_existing_replaces_and_cancels_old() {
        let ctx = BuildContext::default();
        let mut host = MenuHost::new();
        host.show(builder(4, 100), &ctx).unwrap();
        let events = host.show(builder(4, 200), &ctx).unwrap();
        // Show(new) is last; the old dialog is cancelled then hidden first.
        assert_eq!(
            events.last(),
            Some(&MenuHostEvent::Show {
                id: 4,
                had_previous: true
            })
        );
        assert!(events.iter().any(|event| matches!(
            event,
            MenuHostEvent::Choose(result) if result.was_cancelled() && result.token == 100
        )));
        assert_eq!(host.len(), 1);
        assert_eq!(host.get(4).unwrap().token, 200);
    }

    #[test]
    fn update_replaces_only_live_menu() {
        let ctx = BuildContext::default();
        let mut host = MenuHost::new();
        host.show(builder(5, 1), &ctx).unwrap();
        let replacement = UiNode::new(UiKey::Table).child(
            UiNode::new(UiKey::Label)
                .str(UiKey::Text, "live")
                .str(UiKey::Id, "live"),
        );
        let events = host.update(5, "body", replacement.clone(), &ctx).unwrap();
        assert_eq!(
            events,
            vec![MenuHostEvent::Update {
                id: 5,
                table_id: "body".to_owned()
            }]
        );
        assert_eq!(host.get(5).unwrap().ui, replacement);
        assert_eq!(host.get(5).unwrap().ids, vec!["live".to_owned()]);
        // Unknown id is a no-op.
        assert!(
            host.update(99, "body", replacement, &ctx).unwrap().is_empty()
        );
    }

    #[test]
    fn choose_with_values_carries_id_values() {
        let ctx = BuildContext::default();
        let ui = UiNode::new(UiKey::Table).child(
            UiNode::new(UiKey::Button)
                .str(UiKey::Text, "Buy")
                .str(UiKey::Clicked, "buy")
                .str(UiKey::Id, "buy-button"),
        );
        let mut host = MenuHost::new();
        host.show(MenuBuilder::of(ui).id(6).token(7), &ctx).unwrap();
        let events = host.choose(
            6,
            MenuSelection::new("buy")
                .with("volume", MenuValue::F32(0.25))
                .with("name", MenuValue::Str("base".to_owned())),
        );
        match &events[0] {
            MenuHostEvent::Choose(result) => {
                assert_eq!(result.get_f32("volume"), 0.25);
                assert_eq!(result.get_str("name"), Some("base"));
            }
            other => panic!("expected choose, got {other:?}"),
        }
    }

    #[test]
    fn oversized_tree_is_rejected() {
        let ctx = BuildContext::default();
        let mut host = MenuHost::new();
        // A 64 KiB string exceeds the per-string cap.
        let ui = UiNode::new(UiKey::Label).str(UiKey::Text, "x".repeat(70_000));
        let error = host.show(MenuBuilder::of(ui).id(7), &ctx).unwrap_err();
        assert!(matches!(error, MenuHostError::Caps(_)));
    }
}
