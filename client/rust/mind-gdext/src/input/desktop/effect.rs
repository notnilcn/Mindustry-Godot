// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `Effect` enum: UI-side actions the shell resolves against the scene tree.
//! Upstream: core/src/mindustry/input/DesktopInput.java

/// A UI-side action the shell must resolve against the scene tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `MindUi.open_dialog(name, ctx)`.
    OpenDialog {
        /// Registered dialog name.
        name: String,
        /// Context JSON (may be empty).
        ctx: String,
    },
    /// Toggle a HUD fragment (console/minimap): `toggle()` when present,
    /// otherwise `visible = !visible`.
    ToggleFragment(String),
    /// Open the block-config fragment.
    OpenBlockConfig {
        /// Tile x.
        x: i32,
        /// Tile y.
        y: i32,
        /// Viewport anchor (click position).
        screen: (f32, f32),
        /// Config-spec JSON consumed by `block_config_fragment.gd`.
        spec: String,
    },
    /// Open the block-inventory fragment.
    OpenBlockInventory {
        /// Tile x.
        x: i32,
        /// Tile y.
        y: i32,
        /// Viewport anchor (click position).
        screen: (f32, f32),
        /// `{item_name: amount}` JSON.
        items: String,
    },
    /// `inv.hide()` (`InputHandler.tileTapped`: a non-inventory tap hides the
    /// open inventory).
    HideInventory,
    /// `config.hideConfig(); inv.hide(); planConfig.hide();`
    /// (`InputHandler.tileTapped` on an empty tile).
    HidePanels,
    /// Capture the running viewport.
    Screenshot,
    /// `MindUi.close_top_dialog()` (research/planet toggle keys).
    CloseTopDialog,
    /// A binding fired (probe/UI signal).
    Hotkey(String),
    /// Call a `#[func]` on the `/root/MindUi` autoload by name
    /// (`toggle_power_lines` F5 / `toggle_fullscreen` F11).
    UiCall(String),
    /// `logic.skipWave()` from the `skip_wave` binding
    /// (`HudFragment.java:461-467`): `MindInput` calls `MindCampaign.run_wave`,
    /// the same facade path as the HUD skip button (`_on_skip_wave`).
    SkipWave,
    /// `Call.pingLocation(player, x, y, text)` (`DesktopInput.java:491-497`):
    /// show the ping marker at the world point (`PlayerComp.drawPing`,
    /// `MinimapRenderer.java:173`) for its 20 s lifetime.
    Ping {
        /// World x (pixels).
        x: f32,
        /// World y (pixels).
        y: f32,
        /// Optional ping label (`pingText`, stripped of markup).
        text: String,
    },
    /// Ctrl+`ping`: `ui.showTextInput("", "@ping.text", maxPingTextLength,
    /// "", result -> Call.pingLocation(...))` (`DesktopInput.java:492-493`).
    PingPrompt {
        /// World x (pixels).
        x: f32,
        /// World y (pixels).
        y: f32,
    },
    /// `planConfig.showConfig(splan)` (`DesktopInput.java:800-812`): the
    /// `{title, options:[{name,label}], value}` spec for the plan-point popup.
    OpenPlanConfig {
        /// Fragment spec JSON (`MindPlanConfigFragment.configure`).
        spec: String,
    },
    /// `planConfig.hide()` (`DesktopInput.java:809`/`812`).
    HidePlanConfig,
    /// The RTS selection changed; payload is a JSON id array.
    SelectionChanged(String),
}
