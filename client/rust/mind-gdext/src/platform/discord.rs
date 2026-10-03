// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Optional Discord Rich Presence seam (plan 22 §3.9, OD4).
//!
//! Compile-gated behind the `discord` Cargo feature (default **off**). The
//! presence-string mapping is ported and unit-tested in `mind_core::service`
//! (Godot-free); this module only re-exports it and owns the RPC start hook. No
//! IPC connection is opened: with the feature on but no plugin linked, [`start`]
//! logs a single disabled notice and the game starts cleanly.

pub use mind_core::service::{
    DISCORD_APP_ID, DiscordPresence as Presence, discord_opted_out, discord_presence as presence,
};

/// Starts Discord RPC when the `discord` feature is compiled in.
///
/// The feature is default-off and, even when on, no RPC plugin is linked yet, so
/// this only logs that presence is disabled (acceptance for plan 22 M4).
pub fn start() {
    #[cfg(feature = "discord")]
    {
        if discord_opted_out(|key| std::env::var(key).ok()) {
            log::info!("[discord] disabled via nodiscord");
        } else {
            log::info!("[discord] RPC plugin not linked; presence disabled");
        }
    }
    #[cfg(not(feature = "discord"))]
    {
        let _ = discord_opted_out(|key| std::env::var(key).ok());
    }
}
