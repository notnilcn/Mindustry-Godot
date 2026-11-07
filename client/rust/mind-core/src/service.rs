// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Platform service seam — port of the anonymous `GameService` in
//! `desktop/src/mindustry/desktop/DesktopLauncher.java` (plan 22 §3.9, OD4).
//!
//! Achievements/stats call sites (plans 12/19) go through this trait, so the
//! Steam and Discord implementations can land later without touching gameplay.
//! [`NullService`] is the default everywhere until OD4 is accepted: every method
//! is a documented no-op and [`GameService::enabled`] is `false`.
//!
//! GAPS #69's Steam half (`SNet`/`SStats`/`SUser`/`SWorkshop`, app id
//! [`STEAM_APP_ID`]) is **not** ported here — it needs the Steamworks SDK and
//! remains out of scope. This trait carries the platform-neutral surface those
//! backends plug into: the seven `DesktopLauncher` methods plus the forward
//! seams newer platform integrations use — leaderboard submission, the
//! `deleteAchievement` / `hasAchievement` aliases, guarded
//! `openLink`/`openFile`, and the `SettingsMenuDialog.exportData`/`importData`
//! data flow. Every default is the upstream no-op (with a log for the guarded
//! calls), so both [`NullService`] and `LocalGameService` stay source-compatible.
//!
//! Upstream citations: `DesktopLauncher.java` (the seven base methods),
//! `core/src/mindustry/ui/dialogs/SettingsMenuDialog.java:575,610`
//! (`exportData`/`importData`), and `desktop/src/mindustry/desktop/steam/SStats.java`
//! (the achievement/stat backend this seam would front once Steam lands).

use std::path::Path;

/// URL schemes the platform is allowed to hand to the OS (`openLink`).
///
/// The game only ever opens its own website/wiki/Discord links and
/// `mindustry://` connect URIs; anything else (`file:`, `javascript:`, `data:`)
/// is refused so a mod/UI string cannot launch an arbitrary handler.
pub const LINK_SCHEMES: [&str; 3] = ["http", "https", "mindustry"];

/// Returns the scheme of `url` when it has the `scheme:rest` shape.
fn link_scheme(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once(':')?;
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
    (valid && !rest.is_empty()).then_some(scheme)
}

/// Whether `url` is safe to hand to the OS handler (`openLink` guard).
///
/// Accepts only [`LINK_SCHEMES`], requires a non-empty target, and rejects
/// surrounding whitespace and control characters (which can smuggle a second
/// argument past a shell-based opener).
pub fn is_safe_external_link(url: &str) -> bool {
    if url.is_empty() || url.trim() != url || url.chars().any(char::is_control) {
        return false;
    }
    match link_scheme(url) {
        Some(scheme) => LINK_SCHEMES
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed)),
        None => false,
    }
}

/// Whether `path` may be revealed/opened by the platform (`openFile` guard):
/// an existing regular file. Directories and missing paths are refused so a
/// stale UI id cannot make the OS open something unexpected.
pub fn is_openable_file(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.is_file()
}

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

    /// Submits a leaderboard score (`submitScore`).
    ///
    /// Steam leaderboards (`SteamUserStats.uploadLeaderboardScore`) and the
    /// Android Play Games bridge are the two upstream backends; neither is
    /// compiled into this build yet (GAPS #69), and the Java 146 tree this port
    /// targets has no leaderboard call at all, so this is a forward seam.
    /// Returns whether a backend stored the score; the default is `false`.
    fn submit_score(&mut self, _leaderboard: &str, _score: i32) -> bool {
        false
    }

    /// Deletes/relocks an achievement by name (`deleteAchievement`).
    ///
    /// Alias of [`GameService::clear_achievement`] for newer platform SDKs that
    /// name the operation "delete"; the default forwards so any service that
    /// implements `clearAchievement` (for example `LocalGameService`) gets the
    /// real local behavior for free.
    fn delete_achievement(&mut self, name: &str) {
        self.clear_achievement(name);
    }

    /// Whether an achievement is unlocked (`hasAchievement`).
    ///
    /// Alias of [`GameService::is_achieved`]; the default forwards so a real
    /// backend only implements one of the two names.
    fn has_achievement(&self, name: &str) -> bool {
        self.is_achieved(name)
    }

    /// Opens an external link with the OS handler (`openLink`).
    ///
    /// The URL passes [`is_safe_external_link`] first; an unsafe link is
    /// refused and logged without reaching the OS. Returns whether the OS
    /// accepted the URL — the core default has no host opener, so it logs and
    /// returns `false`; hosts override this with `OS.shell_open`.
    fn open_link(&mut self, url: &str) -> bool {
        if !is_safe_external_link(url) {
            log::warn!("[service] refusing to open unsafe link: {url:?}");
            return false;
        }
        log::debug!("[service] open_link({url}) ignored: no platform opener");
        false
    }

    /// Reveals/opens a local file with the OS handler (`openFile`).
    ///
    /// The path passes [`is_openable_file`] first. Returns whether the OS
    /// accepted the path; the core default logs and returns `false`, hosts
    /// override it with `OS.shell_open`.
    fn open_file(&mut self, path: &Path) -> bool {
        if !is_openable_file(path) {
            log::warn!(
                "[service] refusing to open missing/non-file path: {}",
                path.display()
            );
            return false;
        }
        log::debug!(
            "[service] open_file({}) ignored: no platform opener",
            path.display()
        );
        false
    }

    /// Exports platform data to `destination`
    /// (`SettingsMenuDialog.exportData`: settings + saves + screenshots/maps/
    /// mods/schematics as a zip).
    ///
    /// The local/host data flow is owned by the client; the platform service
    /// default is a logged no-op that reports failure.
    fn export_data(&mut self, destination: &Path) -> bool {
        log::info!(
            "[service] export_data({}) not supported by this service",
            destination.display()
        );
        false
    }

    /// Imports platform data from `source` (`SettingsMenuDialog.importData`).
    ///
    /// Logged no-op by default; see [`GameService::export_data`].
    fn import_data(&mut self, source: &Path) -> bool {
        log::info!(
            "[service] import_data({}) not supported by this service",
            source.display()
        );
        false
    }
}

/// The no-op service used until Steam/Discord (OD4) land (plan 22 §3.9).
///
/// Every trait method is inherited from the documented defaults, so the null
/// service is a complete `GameService`: achievements/stats do nothing, the
/// guarded `open_link`/`open_file` refuse unsafe targets (and log), and
/// `export_data`/`import_data` are logged no-ops.
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
    fn null_service_covers_the_extended_surface() {
        let mut service = NullService;
        assert!(!service.submit_score("campaign-wave", 42));
        service.delete_achievement("first");
        assert!(!service.has_achievement("first"));
        assert!(!service.open_link("https://mindustrygame.github.io"));
        assert!(!service.open_file(Path::new("/definitely/not/here.txt")));
        assert!(!service.export_data(Path::new("/tmp/export.zip")));
        assert!(!service.import_data(Path::new("/tmp/import.zip")));
    }

    #[test]
    fn aliases_forward_to_the_primary_methods() {
        /// Service that only implements the upstream (Java) surface.
        struct UpstreamOnly {
            achieved: bool,
            cleared: bool,
        }
        impl GameService for UpstreamOnly {
            fn clear_achievement(&mut self, _name: &str) {
                self.achieved = false;
                self.cleared = true;
            }
            fn is_achieved(&self, _name: &str) -> bool {
                self.achieved
            }
        }

        let mut service = UpstreamOnly {
            achieved: true,
            cleared: false,
        };
        assert!(service.has_achievement("first"));
        service.delete_achievement("first");
        assert!(service.cleared);
        assert!(!service.has_achievement("first"));
    }

    #[test]
    fn aliases_back_the_local_service_with_real_state() {
        // The platform aliases must reach `LocalGameService`'s real local
        // behavior, not just the null default.
        let mut local = crate::game::service::LocalGameService::new();
        assert!(!local.has_achievement("buildT5"));
        local.complete_achievement("buildT5");
        assert!(local.has_achievement("buildT5"));
        local.delete_achievement("buildT5");
        assert!(!local.has_achievement("buildT5"));

        // Achievement completions request a store flush; the alias delete does too.
        assert!(local.store_count() >= 2);
    }

    #[test]
    fn link_guard_accepts_only_known_schemes() {
        for url in [
            "https://mindustrygame.github.io",
            "http://example.com/wiki",
            "HTTPS://EXAMPLE.COM",
            "mindustry://join?id=42",
        ] {
            assert!(is_safe_external_link(url), "expected safe: {url}");
        }
        for url in [
            "",
            "https:",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "mailto:a@b.c",
            "https://example.com\nrm -rf /",
            " https://example.com",
            "https://example.com ",
            "//example.com",
            "/etc/passwd",
        ] {
            assert!(!is_safe_external_link(url), "expected refused: {url:?}");
        }
    }

    #[test]
    fn file_guard_requires_a_regular_file() {
        let dir = std::env::temp_dir().join(format!("mind-service-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("report.txt");
        std::fs::write(&file, b"ok").unwrap();

        assert!(!is_openable_file(Path::new("")));
        assert!(!is_openable_file(&dir));
        assert!(!is_openable_file(&dir.join("missing.txt")));
        assert!(is_openable_file(&file));

        std::fs::remove_dir_all(&dir).unwrap();
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
