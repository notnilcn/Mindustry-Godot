// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The `MindNet` intents forwarded from the host/join/connect dialogs plus the click-sound hook.
//! Upstream: core/src/mindustry/ui/dialogs/HostDialog.java, core/src/mindustry/ui/dialogs/JoinDialog.java

use super::*;

#[godot_api(secondary)]
impl MindUiRoot {
    // ---- MindNet intents --------------------------------------------------

    /// `HostDialog.runHost`: hosts the currently loaded world
    /// (`MindNet.host_current_world` picks up the live map/rules; I08 T4)
    /// instead of creating a bare lobby.
    #[func]
    pub(super) fn on_host_requested(&mut self, host_name: GString, _mode: GString) {
        let Some(mut net) = kit::net() else {
            return;
        };
        if !net.has_method("host_current_world") {
            return;
        }
        // The live world's rules carry the gamemode (`HostDialog.java:80-99`
        // hosts the current state; the dialog's mode row is presentational).
        let map_id = self.current_map_id();
        let _ = net.call(
            "host_current_world",
            &[
                host_name.to_variant(),
                GString::from(map_id.as_str()).to_variant(),
                1i64.to_variant(),
            ],
        );
    }

    /// `JoinDialog.connect`: direct-connect to the addressed SpacetimeDB server
    /// and enter the match browser.
    #[func]
    pub(super) fn on_connect_requested(&mut self, address: GString) {
        if let Some(mut net) = kit::net()
            && net.has_method("connect_to")
        {
            let _ = net.call("connect_to", &[address.to_variant()]);
        }
    }

    /// `JoinDialog.safeConnect`: join the picked public match.
    #[func]
    pub(super) fn on_join_requested(&mut self, match_id: i64) {
        if let Some(mut net) = kit::net()
            && net.has_method("join_match")
        {
            let _ = net.call(
                "join_match",
                &[match_id.to_variant(), GString::new().to_variant()],
            );
        }
    }

    // ---- global button click sound (`UI.java:146`) ------------------------

    /// `Tree.node_added` receiver kept for the ABI: it only queues, never
    /// hooks, so connecting it to the signal by hand cannot re-enter the borrow
    /// (see [`queue_node_hook`]).
    #[func]
    pub(super) fn on_node_added(&self, node: Gd<Node>) {
        queue_node_hook(&node);
    }

    /// `UI.java:146` `ClickListener.clicked = () -> Sounds.uiButton.play()`.
    #[func]
    pub(super) fn on_any_button_pressed(&mut self) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("play_ui_sound", &[GString::from("uiButton").to_variant()]);
        }
    }
}
