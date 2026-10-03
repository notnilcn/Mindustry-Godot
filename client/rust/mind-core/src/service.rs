// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Platform service seam — port of the anonymous `GameService` in
//! `desktop/src/mindustry/desktop/DesktopLauncher.java` (plan 22 §3.9, OD4).
//!
//! Achievements/stats call sites (plans 12/19) go through this trait, so the
//! Steam and Discord implementations can land later without touching gameplay.
//! [`NullService`] is the default everywhere until OD4 is accepted: every method
//! is a documented no-op and [`GameService::enabled`] is `false`.

/// Platform achievement/stat service (`GameService` in `DesktopLauncher`).
///
/// All methods have no-op defaults so a real service only overrides what it
/// supports, matching the upstream anonymous implementation exactly.
pub trait GameService: Send {
    /// Whether the service is active this session (`enabled()`).
    fn enabled(&self) -> bool {
        false
    }

    /// Unlocks an achievement by name (`completeAchievement`).
    fn complete_achievement(&mut self, _name: &str) {}

    /// Relocks an achievement by name (`clearAchievement`).
    fn clear_achievement(&mut self, _name: &str) {}

    /// Whether an achievement is currently unlocked (`isAchieved`).
    fn is_achieved(&self, _name: &str) -> bool {
        false
    }

    /// Reads a stat, returning `default` when absent (`getStat`).
    fn get_stat(&self, _name: &str, default: i32) -> i32 {
        default
    }

    /// Writes a stat (`setStat`).
    fn set_stat(&mut self, _name: &str, _amount: i32) {}

    /// Flushes pending stats to the backend (`storeStats`).
    fn store_stats(&mut self) {}
}

/// The no-op service used until Steam/Discord (OD4) land (plan 22 §3.9).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NullService;

impl GameService for NullService {}

/// Upstream Steam app id (`SVars.appId`), kept for the OD4 seam/docs.
pub const STEAM_APP_ID: u32 = 1_127_400;

/// Discord RPC application id (`DesktopLauncher`), kept for the OD4 seam/docs.
pub const DISCORD_APP_ID: u64 = 610_508_934_456_934_412;

/// Discord rich-presence snapshot (plan 22 §3.9 mapping, OD4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordPresence {
    /// `state` line (mode / menu location).
    pub state: String,
    /// `details` line (map / wave), empty when not in game.
    pub details: String,
    /// `largeImageKey` (always `"logo"` upstream).
    pub large_image_key: &'static str,
    /// `largeImageText` (the wave line in game).
    pub large_image_text: String,
}

/// Builds the upstream Discord presence for the current UI state.
#[allow(clippy::too_many_arguments)]
pub fn discord_presence(
    in_game: bool,
    editing: bool,
    selecting: bool,
    mode: &str,
    players: usize,
    map: &str,
    wave: i32,
) -> DiscordPresence {
    if in_game {
        DiscordPresence {
            state: format!("{mode} | {players} Players"),
            details: format!("{map} | Wave {wave}"),
            large_image_key: "logo",
            large_image_text: format!("Wave {wave}"),
        }
    } else {
        let state = if editing {
            "In Editor"
        } else if selecting {
            "In Launch Selection"
        } else {
            "In Menu"
        };
        DiscordPresence {
            state: state.to_owned(),
            details: String::new(),
            large_image_key: "logo",
            large_image_text: String::new(),
        }
    }
}

/// Whether Discord RPC is opted out (`nodiscord`), checked through `getenv`.
pub fn discord_opted_out(getenv: impl Fn(&str) -> Option<String>) -> bool {
    getenv("nodiscord").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_service_is_disabled_and_inert() {
        let mut service = NullService;
        assert!(!service.enabled());
        service.complete_achievement("first");
        service.clear_achievement("first");
        assert!(!service.is_achieved("first"));
        assert_eq!(service.get_stat("waves", 7), 7);
        service.set_stat("waves", 99);
        assert_eq!(service.get_stat("waves", 7), 7);
        service.store_stats();
    }

    #[test]
    fn ids_are_stable() {
        assert_eq!(STEAM_APP_ID, 1_127_400);
        assert_eq!(DISCORD_APP_ID, 610_508_934_456_934_412);
    }

    #[test]
    fn discord_presence_matches_upstream() {
        let in_game = discord_presence(true, false, false, "survival", 3, "groundZero", 12);
        assert_eq!(in_game.state, "survival | 3 Players");
        assert_eq!(in_game.details, "groundZero | Wave 12");
        assert_eq!(in_game.large_image_key, "logo");
        assert_eq!(in_game.large_image_text, "Wave 12");

        assert_eq!(
            discord_presence(false, true, false, "", 0, "", 0).state,
            "In Editor"
        );
        assert_eq!(
            discord_presence(false, false, true, "", 0, "", 0).state,
            "In Launch Selection"
        );
        assert_eq!(
            discord_presence(false, false, false, "", 0, "", 0).state,
            "In Menu"
        );
        assert!(discord_opted_out(
            |key| (key == "nodiscord").then(String::new)
        ));
        assert!(!discord_opted_out(|_| None));
    }
}
