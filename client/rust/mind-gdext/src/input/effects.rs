// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Resolution of the desktop bridge's UI effects against the scene tree.
//! Upstream: core/src/mindustry/input/InputHandler.java

use super::*;

impl MindInput {
    /// Resolves UI effects produced by the desktop bridge.
    pub(super) fn drain_effects(&mut self) {
        let effects = self.bridge.take_effects();
        for effect in effects {
            match effect {
                Effect::OpenDialog { name, ctx } => {
                    if let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") {
                        let _ = ui.call(
                            "open_dialog",
                            &[
                                GString::from(name.as_str()).to_variant(),
                                GString::from(ctx.as_str()).to_variant(),
                            ],
                        );
                    }
                }
                Effect::ToggleFragment(name) => {
                    let Some(ui_root) = self.ui_root.clone() else {
                        continue;
                    };
                    let fragment = ui_root.bind().hud_fragment(&name);
                    let Some(mut node) = fragment else {
                        continue;
                    };
                    if node.has_method("toggle") {
                        let _ = node.call("toggle", &[]);
                    } else {
                        let visible = node.get("visible").try_to::<bool>().unwrap_or(false);
                        node.set("visible", &(!visible).to_variant());
                    }
                }
                Effect::OpenBlockConfig { x, y, screen, spec } => {
                    // `input.config.getSelected()` mirror (H03 T3): the panel
                    // hides later; the accessor re-checks `visible`.
                    self.config_tile = Some((x, y));
                    let Some(ui_root) = self.ui_root.clone() else {
                        continue;
                    };
                    let config = ui_root.bind().hud_fragment("block_config");
                    let Some(mut node) = config else {
                        continue;
                    };
                    let _ = node.call(
                        "configure",
                        &[
                            Vector2::new(screen.0, screen.1).to_variant(),
                            GString::from(spec.as_str()).to_variant(),
                        ],
                    );
                    // G07 T14: `InputHandler.java:1985-1993`
                    // `build.block.configureSound.at(build.x, build.y, 1, 1,
                    // control.sound.uiBus)`. Tile centre (size-1 blocks);
                    // `block_offset` would refine multi-tile blocks.
                    if let Some(mut audio) = self.audio.clone() {
                        audio.bind_mut().play_at_ui(
                            GString::from("click"),
                            f64::from(x) * 8.0 + 4.0,
                            f64::from(y) * 8.0 + 4.0,
                            1.0,
                            1.0,
                        );
                    }
                }
                Effect::OpenBlockInventory {
                    x,
                    y,
                    screen,
                    items,
                } => {
                    let Some(ui_root) = self.ui_root.clone() else {
                        continue;
                    };
                    let inventory = ui_root.bind().hud_fragment("block_inventory");
                    let Some(mut node) = inventory else {
                        continue;
                    };
                    node.set("position", &Vector2::new(screen.0, screen.1).to_variant());
                    let _ = node.call(
                        "open_at",
                        &[
                            Vector2i::new(x, y).to_variant(),
                            GString::from(items.as_str()).to_variant(),
                        ],
                    );
                }
                Effect::Screenshot => {
                    if let Some(mut host) = self.bridge.host.clone() {
                        let _ = host.call(
                            "capture",
                            &[GString::from("user://screenshot.png").to_variant()],
                        );
                    }
                }
                Effect::CloseTopDialog => {
                    if let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") {
                        let _ = ui.call("close_top_dialog", &[]);
                    }
                }
                Effect::HideInventory => {
                    self.hide_panel_at("block_inventory");
                }
                Effect::HidePanels => {
                    // `config.hideConfig(); inv.hide(); planConfig.hide();`
                    // (`InputHandler.tileTapped` empty-tile branch).
                    self.hide_panel_at("block_inventory");
                    if let Some(ui_root) = self.ui_root.clone() {
                        let config = ui_root.bind().hud_fragment("block_config");
                        if let Some(mut node) = config {
                            node.set("visible", &false.to_variant());
                        }
                        if let Some(mut node) = ui_root.bind().hud_fragment("plan_config") {
                            node.set("visible", &false.to_variant());
                        }
                    }
                    // The tap dispatch only reacts while a panel is open (H02 T6).
                    self.config_tile = None;
                    self.bridge.config_selected = None;
                }
                Effect::Hotkey(action) => {
                    let _ = self
                        .base_mut()
                        .emit_signal("hotkey", &[GString::from(action.as_str()).to_variant()]);
                }
                Effect::UiCall(name) => {
                    // T9: `toggle_power_lines` (F5) / `toggle_fullscreen` (F11)
                    // live as `#[func]`s on the `/root/MindUi` autoload.
                    if let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") {
                        let _ = ui.call(&name, &[]);
                    }
                }
                Effect::SkipWave => {
                    // `HudFragment.java:461-467` -> `logic.skipWave()`: the
                    // same campaign facade the HUD skip button calls
                    // (`_on_skip_wave` -> `MindCampaign.run_wave`).
                    if let Some(mut campaign) = self.campaign.clone() {
                        let _ = campaign.bind_mut().run_wave();
                    }
                }
                Effect::Ping { x, y, text } => {
                    // `Call.pingLocation` -> `PlayerComp.pingX/Y/time/text`;
                    // the HUD fragment owns the world marker overlay
                    // (`PlayerComp.drawPing`, `MinimapRenderer.java:173`).
                    let Some(ui_root) = self.ui_root.clone() else {
                        continue;
                    };
                    let hud = ui_root.bind().hud_fragment("hud");
                    let Some(mut node) = hud else {
                        continue;
                    };
                    let _ = node.call(
                        "show_ping",
                        &[
                            x.to_variant(),
                            y.to_variant(),
                            GString::from(text.as_str()).to_variant(),
                        ],
                    );
                }
                Effect::PingPrompt { x, y } => {
                    // `DesktopInput.java:492-493`: Ctrl asks for the label
                    // first; `MindInput` answers through `text_input_result`
                    // (the same prompt round-trip the dialogs use).
                    self.ping_pending = Some((x, y));
                    if let Some(mut ui) = self.base().try_get_node_as::<Node>("/root/MindUi") {
                        let _ = ui.call(
                            "show_text_input",
                            &[
                                GString::new().to_variant(),
                                crate::ui::kit::t("ping.text").to_variant(),
                                40i64.to_variant(),
                                GString::new().to_variant(),
                                false.to_variant(),
                                false.to_variant(),
                            ],
                        );
                    }
                }
                Effect::OpenPlanConfig { spec } => {
                    // `planConfig.showConfig(splan)` (`DesktopInput.java:808`):
                    // the HUD owns the sibling `HudGroup/plan_config` node (the
                    // fragment class is not `#[func]`-exposed itself).
                    let Some(ui_root) = self.ui_root.clone() else {
                        continue;
                    };
                    let hud = ui_root.bind().hud_fragment("hud");
                    let Some(mut node) = hud else {
                        continue;
                    };
                    let _ = node.call(
                        "open_plan_config",
                        &[GString::from(spec.as_str()).to_variant()],
                    );
                }
                Effect::HidePlanConfig => {
                    // `planConfig.hide()` (`DesktopInput.java:809/812`).
                    let Some(ui_root) = self.ui_root.clone() else {
                        continue;
                    };
                    if let Some(mut node) = ui_root.bind().hud_fragment("plan_config") {
                        node.set("visible", &false.to_variant());
                    }
                }
                Effect::SelectionChanged(json) => {
                    let _ = self.base_mut().emit_signal(
                        "selection_changed",
                        &[GString::from(json.as_str()).to_variant()],
                    );
                }
            }
        }
    }
}
