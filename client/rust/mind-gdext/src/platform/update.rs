// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Updater disposition (plan 22 §3.10, P22-3; GAPS #70).
//!
//! The upstream `BeControl` bleeding-edge updater (GitHub releases, self-replace)
//! is **not ported** (no JVM artifacts). `config autoUpdate` is retained but only
//! logs the notice below; no network call is ever made at boot.
//!
//! The host-neutral disposition half lives in
//! [`mind_core::platform::update`]: `active()` gating, the release JSON parse,
//! `newBuild > Version.build`, and reading an on-disk `version.properties`. This
//! module re-exports it and reports the Godot build's current status without
//! polling; [`check_json`] is the offline seam a debug probe can feed a captured
//! release document to.

pub use mind_core::platform::update::{
    DESKTOP_ASSET_PREFIX, RELEASES_LATEST_URL, ReleaseError, ReleaseInfo, SERVER_ASSET_PREFIX,
    UPDATE_INTERVAL_SECS, UpdateStatus, is_bleeding_edge, is_update_available, parse_release,
    read_build_info,
};

use mind_core::version::BuildInfo;

/// The single notice printed when `autoUpdate` is requested.
pub const NOT_AVAILABLE: &str = "updater not available in this build";

/// Whether an update is available (always `false`: no update feed is consulted).
pub fn update_available() -> bool {
    false
}

/// Whether automatic self-updating is supported (always `false`).
pub fn auto_update_supported() -> bool {
    false
}

/// The embedded build id (`client/assets/version.properties` via `BuildInfo`).
pub fn installed() -> &'static BuildInfo {
    BuildInfo::embedded()
}

/// The current disposition for this build: active only for a bleeding-edge,
/// non-Steam build, and never "available" because the client does not poll.
pub fn status() -> UpdateStatus {
    UpdateStatus::for_build(installed())
}

/// Parses a captured release document against the embedded build id without
/// network access (debug/offline test seam).
pub fn check_json(json: &str, headless: bool) -> Result<UpdateStatus, ReleaseError> {
    mind_core::platform::update::check_release(installed(), json, headless)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_never_reports_an_available_update() {
        assert!(!update_available());
        assert!(!auto_update_supported());
        assert!(!status().available);
        // The updater gate is decided only by the build type/Steam flag.
        assert_eq!(
            status().active,
            is_bleeding_edge(&installed().r#type, installed().is_steam)
        );
    }

    #[test]
    fn offline_check_parses_a_release_for_the_active_build() {
        let release = r#"{
            "tag_name": "999999",
            "assets": [
                {"name": "Mindustry-BE-Desktop-999999.jar", "browser_download_url": "https://example.com/x.jar"}
            ]
        }"#;
        let status = check_json(release, false).unwrap();
        if status.active {
            assert!(status.available);
            assert_eq!(status.update_build, 999_999);
        } else {
            // Official/custom builds short-circuit before parsing.
            assert!(!status.available);
        }
    }
}
