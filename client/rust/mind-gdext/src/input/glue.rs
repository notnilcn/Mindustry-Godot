// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Private `MindInput` helpers: bootstrap, catalog, dialog/focus probes and per-frame state refresh.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

impl MindInput {
    /// Rebuilds the client-local binding state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). The binding state
    /// is overwritten, so a re-run never duplicates.
    pub(super) fn bootstrap(&mut self) {
        self.bindings = bindings::load();
        self.bridge
            .set_command_mode_hold(Self::read_command_mode_hold());
        self.bridge.set_block_replace(Self::read_block_replace());
        self.bridge
            .set_build_autopause(Self::read_build_autopause());
        // Scene wiring (tscn-first): `game.tscn` declares the spine
        // dependencies on this node; re-adopted on every bootstrap so a reload
        // keeps them.
        self.sim_host = wiring::required("MindInput", "sim_host", &self.sim_host);
        self.camera = wiring::required("MindInput", "camera", &self.camera);
        self.ui_root = wiring::required("MindInput", "ui_root", &self.ui_root);
        self.campaign = wiring::optional(&self.campaign);
        self.audio = wiring::optional(&self.audio);
        self.bridge.host = self.sim_host.clone();
        self.bridge.camera = self.camera.clone();
        self.bridge.campaign = self.campaign.clone();
        // `DesktopInput.java:492-493`: the Ctrl+ping `@ping.text` prompt
        // answers through the shared `MindUi.text_input_result` signal.
        if let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") {
            let callable = self.base().callable("_on_text_input_result");
            if ui.has_signal("text_input_result")
                && !ui.is_connected("text_input_result", &callable)
            {
                let _ = ui.connect("text_input_result", &callable);
            }
        }
        // `planConfig.showConfig(splan)` (`DesktopInput.java:808`): the popup's
        // choice is applied to the queued plan (`selected.config = content`).
        if let Some(ui_root) = self.ui_root.clone()
            && let Some(mut node) = ui_root.bind().hud_fragment("plan_config")
        {
            let callable = self.base().callable("_on_plan_config_applied");
            if node.has_signal("plan_config_applied")
                && !node.is_connected("plan_config_applied", &callable)
            {
                let _ = node.connect("plan_config_applied", &callable);
            }
        }
        self.bridge.refresh_items();
        self.sync_selected_block();
        self.load_catalog();
        log::info!(
            "MindInput ready ({} keybinds, {} rebinds)",
            KeyBindTable::len(),
            self.rebind_count()
        );
    }

    /// `InputHandler.locked()` push (PAR-0100): mirrors the live lock set onto
    /// the bridge flag and hands it to the sim host when it changed. Runs from
    /// `input`/`process` and from the two cutscene lock setters.
    pub(super) fn sync_input_locks(&mut self) {
        let locked = self.locks.any();
        if self.bridge.locked != locked {
            self.bridge.locked = locked;
            let _ = self
                .bridge
                .with_host_mut(|host| host.set_input_locked(locked));
        }
    }

    /// Adopts `MindSimHost.selected_block()` as the live placement block.
    fn sync_selected_block(&mut self) {
        let Some(name) = self
            .bridge
            .with_host(|host| host.selected_block().to_string())
        else {
            return;
        };
        let id = self
            .bridge
            .with_host(|host| {
                host.content_registry()
                    .and_then(|content| content.block_id(&name))
            })
            .flatten();
        if let Some(id) = id {
            // `player.block == null` at boot (`BlockId::AIR` is "no selection";
            // the command-mode gate needs it; PAR-0140).
            let selected = if id == mind_core::content::BlockId::AIR {
                None
            } else {
                Some(id)
            };
            self.bridge.controller.state.select_block(selected);
        }
    }

    /// Loads the buildable-block catalogue for the `1`..`0` block-select binds.
    ///
    /// The list must match the palette's (`I07 Pass 3` note 1): the live
    /// `MindUi.block_catalog_json` (the live registry with mod content and the
    /// campaign unlock store), falling back to base content when the UI node is
    /// not up yet.
    fn load_catalog(&mut self) {
        let live = self
            .base()
            .get_node_or_null("/root/MindUi")
            .and_then(|mut node| {
                let json = node
                    .call("block_catalog_json", &[])
                    .try_to::<GString>()
                    .ok()?;
                let value: serde_json::Value =
                    serde_json::from_str(json.to_string().as_str()).ok()?;
                let categories: Vec<Vec<String>> = value
                    .get("categories")?
                    .as_array()?
                    .iter()
                    .map(|category| {
                        category
                            .get("blocks")
                            .and_then(serde_json::Value::as_array)
                            .map(|blocks| {
                                blocks
                                    .iter()
                                    .filter_map(|block| {
                                        block
                                            .get("name")
                                            .and_then(serde_json::Value::as_str)
                                            .map(str::to_owned)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect();
                (!categories.is_empty()).then_some(categories)
            });
        let categories = live.unwrap_or_else(|| {
            mind_core::ui::block_catalog::block_catalog()
                .categories
                .iter()
                .map(|category| {
                    category
                        .blocks
                        .iter()
                        .map(|block| block.name.clone())
                        .collect()
                })
                .collect()
        });
        self.bridge.set_catalog(categories);
    }

    /// `settings.getBool("commandmodehold", true)` from the one shared store
    /// (`Core.settings`; PAR-0164).
    fn read_command_mode_hold() -> bool {
        crate::settings_hub::with_settings(|store| store.get_bool("commandmodehold", true))
    }

    /// `settings.getBool("blockreplace", true)` (`InputHandler.updateLine`:
    /// junction/bridge replacement + `handlePlacementLine`).
    fn read_block_replace() -> bool {
        crate::settings_hub::with_settings(|store| store.get_bool("blockreplace", true))
    }

    /// `settings.getBool("buildautopause", false)` (`SettingsMenuDialog.java:411`).
    fn read_build_autopause() -> bool {
        crate::settings_hub::with_settings(|store| store.get_bool("buildautopause", false))
    }

    /// Re-reads the hold setting when a command-mode key edge arrives so a live
    /// settings-dialog change applies on the next tap (`MindUi.settings_set`
    /// persists the shared store).
    pub(super) fn refresh_command_mode_hold(&mut self, raw: &RawEvent) {
        let code = match raw {
            RawEvent::KeyDown { code } | RawEvent::KeyUp { code } => code,
            _ => return,
        };
        if self.bindings.name(ids::COMMAND_MODE) != Some(code.as_str()) {
            return;
        }
        self.bridge
            .set_command_mode_hold(Self::read_command_mode_hold());
    }

    /// Whether the control under the point is inside a `ScrollContainer`
    /// (`Core.scene.hasScroll()`; PAR-0100). Walks the hovered control's
    /// ancestors, ignoring `mouse_filter`.
    pub(super) fn ui_scroll_at(&self, x: f32, y: f32) -> bool {
        // `Scene.hasScroll()`: only the *hovered* control chain is walked
        // (`gui_get_hovered_control` + ancestors), never the whole scene tree
        // (T8).
        let _ = (x, y);
        let Some(viewport) = self.base().get_viewport() else {
            return false;
        };
        let Some(control) = viewport.gui_get_hovered_control() else {
            return false;
        };
        let mut current: Option<Gd<Node>> = Some(control.upcast());
        while let Some(node) = current {
            if node
                .clone()
                .try_cast::<godot::classes::ScrollContainer>()
                .is_ok()
            {
                return true;
            }
            current = node.get_parent();
        }
        false
    }

    /// Whether the console or chat fragment is visible (`consolefrag.shown()`
    /// / `chatfrag.shown()`; PAR-0100).
    pub(super) fn console_or_chat_visible(&self) -> bool {
        let Some(ui_root) = self.ui_root.clone() else {
            return false;
        };
        for name in ["console", "chat"] {
            let fragment = ui_root.bind().hud_fragment(name);
            if let Some(fragment) = fragment
                && fragment.is_visible_in_tree()
            {
                return true;
            }
        }
        false
    }

    /// Whether a STOP-filter `Control` owns the given event position
    /// (upstream `Core.scene.hasMouse()`). PASS controls fall through to
    /// `_unhandled_input`, so only STOP controls (or a STOP ancestor) block the
    /// world.
    pub(super) fn ui_captures_at(&self, x: f32, y: f32) -> bool {
        let tree = self.base().get_tree();
        let root = tree.get_root();
        // `owned = false`: the default `owned = true` finds nothing under the
        // ownerless root Window, so runtime-created Controls (`Button.new()` in
        // the UI factories) would be missed and their clicks consumed as world
        // presses instead of reaching `_gui_input`.
        let controls = root.find_children_ex("*").owned(false).done();
        let point = Vector2::new(x, y);
        // Later siblings are drawn on top: walk in reverse tree order.
        for index in (0..controls.len()).rev() {
            let Some(node) = controls.get(index) else {
                continue;
            };
            let Ok(control) = node.clone().try_cast::<Control>() else {
                continue;
            };
            if !control.is_visible_in_tree() {
                continue;
            }
            if matches!(control.get("mouse_filter").try_to::<i64>(), Ok(2)) {
                continue;
            }
            if !control.get_global_rect().contains_point(point) {
                continue;
            }
            // `mouse_filter`: 0 = STOP, 1 = PASS, 2 = IGNORE.
            if matches!(control.get("mouse_filter").try_to::<i64>(), Ok(0)) {
                return true;
            }
            let mut parent = control.get_parent();
            while let Some(current) = parent {
                if let Ok(parent_control) = current.clone().try_cast::<Control>() {
                    if !parent_control.is_visible_in_tree() {
                        break;
                    }
                    match parent_control.get("mouse_filter").try_to::<i64>() {
                        Ok(0) => return true,
                        Ok(2) => break,
                        _ => {}
                    }
                }
                parent = current.get_parent();
            }
        }
        false
    }

    /// Consumes the current input dispatch (prevents `unhandled_input`).
    pub(super) fn mark_input_handled(&self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.call("set_input_as_handled", &[]);
        }
    }

    /// Whether a `LineEdit`/`TextEdit` owns keyboard focus
    /// (`Core.scene.hasField()`): field typing must not fire gameplay binds.
    pub(super) fn text_field_focused(&self) -> bool {
        let Some(mut viewport) = self.base().get_viewport() else {
            return false;
        };
        let owner = viewport.call("gui_get_focus_owner", &[]);
        let Ok(control) = owner.try_to::<Gd<Control>>() else {
            return false;
        };
        control.is_class("LineEdit") || control.is_class("TextEdit")
    }

    /// Whether any dialog is open (`scene.hasDialog()`): gameplay binds and
    /// camera pan/zoom are gated off.
    pub(super) fn dialog_open(&self) -> bool {
        let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") else {
            return false;
        };
        if !ui.has_method("dialog_stack") {
            return false;
        }
        ui.call("dialog_stack", &[])
            .try_to::<PackedStringArray>()
            .map(|stack| !stack.is_empty())
            .unwrap_or(false)
    }

    /// `state.isGame()` / `state.isCampaign()` mirrors for the binding gates
    /// (`Control.java:690-760`): a hidden `MenuGroup` means in game; a live
    /// campaign session means campaign. Missing nodes default to in game so
    /// headless/partial scenes keep working.
    pub(super) fn refresh_game_state(&mut self) {
        // The bind guard is dropped before the visibility read, so nothing can
        // re-enter the root's borrow while it is held.
        self.bridge.in_game = self
            .ui_root
            .clone()
            .map(|ui_root| {
                let menu = ui_root.bind().menu_group_node();
                menu.is_none_or(|menu| !menu.is_visible_in_tree())
            })
            .unwrap_or(true);
        self.bridge.campaign_active = self
            .campaign
            .as_ref()
            .is_some_and(|campaign| campaign.bind().campaign_active());
        // `HudFragment.canSkipWave` inputs (`HudFragment.java:1234-1236`); the
        // `MindHud` surface mirrors the campaign read model. Missing nodes keep
        // the default (skip locked).
        self.bridge.wave_gate = self
            .base()
            .try_get_node_as::<Node>("/root/MindHud")
            .map(|hud| desktop::WaveGate {
                waves: hud.get("waves").try_to::<bool>().unwrap_or(false),
                wave_sending: hud.get("wave_sending").try_to::<bool>().unwrap_or(false),
                is_spawning: hud.get("is_spawning").try_to::<bool>().unwrap_or(false),
                enemies: hud.get("enemies").try_to_relaxed::<i64>().unwrap_or(0),
                has_core: hud.get("has_core").try_to::<bool>().unwrap_or(false),
            })
            .unwrap_or_default();
    }

    /// Top of `MindUi.dialog_stack()` (the research/planet close keys read it).
    pub(super) fn refresh_top_dialog(&mut self) {
        let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") else {
            self.bridge.top_dialog.clear();
            return;
        };
        let stack = ui
            .call("dialog_stack", &[])
            .try_to::<PackedStringArray>()
            .unwrap_or_default();
        let top = if stack.is_empty() {
            String::new()
        } else {
            stack
                .get(stack.len() - 1)
                .map(|name| name.to_string())
                .unwrap_or_default()
        };
        self.bridge.top_dialog = top;
    }
}
