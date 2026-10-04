// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindHud` autoload (plan 14 §3.5/M4): the read-only per-frame HUD surface.
//!
//! The GDScript `HudFragment` binds to these typed properties/signals only; it
//! never reads sim state directly (plan 14 §3.10 rule 2). Values are recomputed
//! at most once per frame from the read-only `MindSimHost` surface. Plan 05/12
//! expose more of the read model over time; the remaining fields
//! (`wave`/`enemies`/`ping`/`tps`) carry upstream defaults until those seams
//! land and are flagged in the plan changelog.

use godot::classes::notify::NodeNotification;
use godot::classes::{Engine, INode, Node as GdNode, Os};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

/// Path to the sim owner the HUD reads (plan 00 §3.5).
const SIM_HOST_PATH: &str = "/root/Spine/SimHost";

/// `MindHud` — the typed per-frame HUD state surface (`/root/MindHud`).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindHud {
    base: Base<GdNode>,
    host: Option<Gd<GdNode>>,
    /// `state.phase` name (`menu`/`playing`/`paused`/…).
    #[var]
    state_name: GString,
    /// `state.isPaused()`.
    #[var]
    paused: bool,
    /// `netServer.isWaitingForPlayers()` (plan 21; false offline).
    #[var]
    waiting: bool,
    /// Completed ticks.
    #[var]
    tick: i64,
    /// `state.wave` (default until plan 05/12 expose it).
    #[var]
    wave: i32,
    /// `state.enemies`.
    #[var]
    enemies: i32,
    /// Engine frames per second.
    #[var]
    fps: i32,
    /// Static memory in MiB (`OS.get_static_memory_usage`).
    #[var]
    memory_mb: i32,
    /// Network ping in ms (plan 21; 0 offline).
    #[var]
    ping: i32,
    /// Target sim ticks per second (fixed 60 Hz).
    #[var]
    tps: i32,
    /// Cursor/position text (`position`/`mouseposition` settings).
    #[var]
    position_text: GString,
    /// Composed wave/status text (`ui::hud_text`).
    #[var]
    status_text: GString,
}

#[godot_api]
impl INode for MindHud {
    fn init(base: Base<GdNode>) -> Self {
        Self {
            base,
            host: None,
            state_name: GString::from("menu"),
            paused: false,
            waiting: false,
            tick: 0,
            wave: 0,
            enemies: 0,
            fps: 0,
            memory_mb: 0,
            ping: 0,
            tps: 60,
            position_text: GString::new(),
            status_text: GString::new(),
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        // Re-acquire the host if it appeared after boot (scenario load).
        if self.host.is_none() {
            self.host = self.base().get_node_or_null(SIM_HOST_PATH);
        }
        self.refresh();
    }
}

#[godot_api]
impl MindHud {
    /// Emitted when server-sent HUD text changes (`HudText` relay payload).
    #[signal]
    fn hud_text(text: GString);

    /// Emitted for a toast (text + icon region).
    #[signal]
    fn toast(text: GString, icon: GString);

    /// Emitted when a content item is unlocked.
    #[signal]
    fn unlock(name: GString);

    /// Emitted for an announcement overlay.
    #[signal]
    fn announce(text: GString, duration: f32);

    /// Emitted when a new wave starts.
    #[signal]
    fn wave_event();

    /// Emitted on sector capture/lose/invasion (`kind`, sector name).
    #[signal]
    fn sector_event(kind: GString, name: GString);

    /// Emits server-sent HUD text (`setHudText`); shown by `CoreInfo/HudText`.
    #[func]
    pub fn set_hud_text(&mut self, text: GString) {
        let _ = self
            .base_mut()
            .emit_signal("hud_text", &[text.to_variant()]);
    }

    /// Emits a toast on behalf of the relay (`Menus.infoToast`).
    #[func]
    pub fn push_toast(&mut self, text: GString, icon: GString) {
        let _ = self
            .base_mut()
            .emit_signal("toast", &[text.to_variant(), icon.to_variant()]);
    }

    /// Emits an announcement on behalf of the relay (`Menus.announce`).
    #[func]
    pub fn push_announce(&mut self, text: GString, duration: f32) {
        let _ = self
            .base_mut()
            .emit_signal("announce", &[text.to_variant(), duration.to_variant()]);
    }

    /// Emits an unlock event (`Menus.…`/`UnlockEvent`).
    #[func]
    pub fn push_unlock(&mut self, name: GString) {
        let _ = self.base_mut().emit_signal("unlock", &[name.to_variant()]);
    }
}

impl MindHud {
    /// Rebuilds the read-only surface from the (possibly reloaded) node; the
    /// host lookup and `refresh()` overwrite, so a re-run never duplicates.
    fn bootstrap(&mut self) {
        self.host = self.base().get_node_or_null(SIM_HOST_PATH);
        if self.host.is_none() {
            log::warn!("MindHud: no MindSimHost at {SIM_HOST_PATH}");
        }
        self.refresh();
    }

    /// Recomputes the read-only surface from the sim host + engine monitors.
    fn refresh(&mut self) {
        self.fps = Engine::singleton().get_frames_per_second() as i32;
        self.memory_mb = (Os::singleton().get_static_memory_usage() / 1_048_576) as i32;
        let Some(mut host) = self.host.clone() else {
            return;
        };
        self.paused = host
            .call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false);
        self.tick = host.call("get_tick", &[]).try_to::<i64>().unwrap_or(0);
        self.state_name = host
            .call("get_state", &[])
            .try_to::<GString>()
            .unwrap_or_default();
    }
}
