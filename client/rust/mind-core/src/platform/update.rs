// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bleeding-edge update check — the host-neutral half of
//! `core/src/mindustry/net/BeControl.java` (GAPS #70).
//!
//! The Java class schedules a 60 s HTTP poll of the GitHub
//! `MindustryBuilds/releases/latest` endpoint, parses `tag_name` as the build
//! number, picks the desktop/server asset, and offers a self-replacing download.
//! The network/dialog/download half is **deliberately not ported** (GAPS #70:
//! "BeControl updater is stubbed on purpose"; plan 22 §3.10/P22-3), but the
//! disposition half is real and unit-tested here:
//!
//! - [`is_bleeding_edge`] is `BeControl.active()` (`Version.type` is
//!   `bleeding-edge` and this is not the Steam build);
//! - [`parse_release`] is the `Jval` parse plus asset selection;
//! - [`is_update_available`] is `newBuild > Version.build`;
//! - [`UpdateStatus`] holds `updateAvailable`/`updateBuild`/`updateUrl`;
//! - [`read_build_info`] reads an on-disk `version.properties` (the same
//!   `Version.init` document the embedded build id comes from), so a
//!   dropped-in/packaged `version.properties` can be compared without a feed.
//!
//! Hosts decide whether to call the network at all; the Godot client keeps the
//! upstream notice (`updater not available in this build`) and never polls.

use std::path::Path;

use serde_json::Value;

use crate::version::BuildInfo;

/// The release feed `BeControl.checkUpdate` polls.
pub const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/Anuken/MindustryBuilds/releases/latest";

/// Asset name prefix for desktop builds (`BeControl.checkUpdate`).
pub const DESKTOP_ASSET_PREFIX: &str = "Mindustry-BE-Desktop";

/// Asset name prefix for headless server builds (`BeControl.checkUpdate`).
pub const SERVER_ASSET_PREFIX: &str = "Mindustry-BE-Server";

/// Poll interval in seconds (`BeControl.updateInterval`).
pub const UPDATE_INTERVAL_SECS: u64 = 60;

/// `Version.type` value that activates the updater (`BeControl.active`).
pub const BLEEDING_EDGE_TYPE: &str = "bleeding-edge";

/// One parsed release (`BeControl.checkUpdate`'s `newBuild`/`updateUrl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// `tag_name` parsed as a build number.
    pub build: i32,
    /// The selected asset's `name`.
    pub asset_name: String,
    /// The selected asset's `browser_download_url` (`""` when absent).
    pub url: String,
}

/// Why a release JSON document could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseError {
    /// The body is not valid JSON (`Jval.read` failure).
    #[error("invalid release JSON: {0}")]
    InvalidJson(String),
    /// `tag_name` is missing.
    #[error("release JSON has no tag_name")]
    MissingTag,
    /// `tag_name` is not a build number (`Strings.parseInt` is lenient; this
    /// seam is strict so a malformed feed is visible in logs/tests).
    #[error("release tag_name {0:?} is not a build number")]
    InvalidTag(String),
    /// No asset name starts with the desktop/server prefix.
    #[error("release JSON has no asset starting with {0}")]
    MissingAsset(&'static str),
}

/// `BeControl.active()`: the updater only runs for bleeding-edge non-Steam
/// builds (`Version.type.equals("bleeding-edge") && !steam`).
pub fn is_bleeding_edge(build_type: &str, steam: bool) -> bool {
    build_type == BLEEDING_EDGE_TYPE && !steam
}

/// Parses the release JSON and selects the asset for this host
/// (`BeControl.checkUpdate`'s `Jval.read` + `assets.find`).
///
/// `headless` selects the `Mindustry-BE-Server` prefix, otherwise
/// `Mindustry-BE-Desktop`, exactly like upstream.
pub fn parse_release(json: &str, headless: bool) -> Result<ReleaseInfo, ReleaseError> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| ReleaseError::InvalidJson(error.to_string()))?;

    let tag = value
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or(ReleaseError::MissingTag)?;
    let build: i32 = tag
        .trim()
        .parse()
        .map_err(|_| ReleaseError::InvalidTag(tag.to_owned()))?;

    let prefix = if headless {
        SERVER_ASSET_PREFIX
    } else {
        DESKTOP_ASSET_PREFIX
    };
    let assets = value
        .get("assets")
        .and_then(Value::as_array)
        .ok_or(ReleaseError::MissingAsset(prefix))?;
    let asset = assets
        .iter()
        .find(|asset| {
            asset
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.starts_with(prefix))
        })
        .ok_or(ReleaseError::MissingAsset(prefix))?;

    Ok(ReleaseInfo {
        build,
        asset_name: asset
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        url: asset
            .get("browser_download_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// `newBuild > Version.build` from `BeControl.checkUpdate`.
///
/// A custom build (`build = -1`/`0`) is older than any stamped release, matching
/// the upstream integer comparison.
pub fn is_update_available(local_build: i32, release_build: i32) -> bool {
    release_build > local_build
}

/// `BeControl`'s update state (`updateAvailable`/`updateBuild`/`updateUrl`),
/// plus the `active` gate so a probe can report the whole disposition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateStatus {
    /// Whether the updater is active for this build (`BeControl.active`).
    pub active: bool,
    /// Whether a newer release was seen (`isUpdateAvailable`).
    pub available: bool,
    /// The newer build number (`updateBuild`); `0` before a hit.
    pub update_build: i32,
    /// The newer build's download URL (`updateUrl`); empty before a hit.
    pub update_url: String,
}

impl UpdateStatus {
    /// Status for an installed build, with no feed consulted yet.
    pub fn for_build(info: &BuildInfo) -> Self {
        Self {
            active: is_bleeding_edge(&info.r#type, info.is_steam),
            ..Self::default()
        }
    }

    /// Applies a parsed release (`BeControl.checkUpdate`): returns whether a
    /// newer build was found and updates the state only in that case.
    pub fn consider(&mut self, local_build: i32, release: &ReleaseInfo) -> bool {
        if !self.active || !is_update_available(local_build, release.build) {
            return false;
        }
        self.available = true;
        self.update_build = release.build;
        self.update_url = release.url.clone();
        true
    }
}

/// Parses a release feed against an installed [`BuildInfo`]
/// (`BeControl.checkUpdate` with the network call removed).
///
/// An inactive build returns a default inactive status without looking at the
/// body, exactly like the scheduler gate upstream.
pub fn check_release(
    installed: &BuildInfo,
    json: &str,
    headless: bool,
) -> Result<UpdateStatus, ReleaseError> {
    let mut status = UpdateStatus::for_build(installed);
    if !status.active {
        return Ok(status);
    }
    let release = parse_release(json, headless)?;
    status.consider(installed.build, &release);
    Ok(status)
}

/// Reads and parses an on-disk `version.properties` (`Version.init`).
///
/// Returns `None` when the file is missing/unreadable or the document is
/// invalid; the host logs that and keeps the embedded build id.
pub fn read_build_info(path: &Path) -> Option<BuildInfo> {
    let source = std::fs::read_to_string(path).ok()?;
    BuildInfo::init_from(&source).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP_RELEASE: &str = r#"{
        "tag_name": "12345",
        "assets": [
            {"name": "Mindustry-BE-Desktop-12345.jar", "browser_download_url": "https://example.com/desktop.jar"},
            {"name": "Mindustry-BE-Server-12345.jar", "browser_download_url": "https://example.com/server.jar"}
        ]
    }"#;

    fn bleeding_edge() -> BuildInfo {
        BuildInfo {
            r#type: BLEEDING_EDGE_TYPE.to_owned(),
            build: 12_344,
            ..BuildInfo::default()
        }
    }

    #[test]
    fn updater_is_active_only_for_bleeding_edge_non_steam() {
        assert!(is_bleeding_edge("bleeding-edge", false));
        assert!(!is_bleeding_edge("bleeding-edge", true));
        assert!(!is_bleeding_edge("official", false));
        assert!(!is_bleeding_edge("custom", false));
    }

    #[test]
    fn parses_desktop_and_server_assets() {
        let desktop = parse_release(DESKTOP_RELEASE, false).unwrap();
        assert_eq!(desktop.build, 12_345);
        assert_eq!(desktop.asset_name, "Mindustry-BE-Desktop-12345.jar");
        assert_eq!(desktop.url, "https://example.com/desktop.jar");

        let server = parse_release(DESKTOP_RELEASE, true).unwrap();
        assert_eq!(server.asset_name, "Mindustry-BE-Server-12345.jar");
        assert_eq!(server.url, "https://example.com/server.jar");
    }

    #[test]
    fn rejects_malformed_releases() {
        assert!(matches!(
            parse_release("not json", false),
            Err(ReleaseError::InvalidJson(_))
        ));
        assert_eq!(
            parse_release(r#"{"assets": []}"#, false),
            Err(ReleaseError::MissingTag)
        );
        assert_eq!(
            parse_release(r#"{"tag_name": "build-123", "assets": []}"#, false),
            Err(ReleaseError::InvalidTag("build-123".to_owned()))
        );
        assert_eq!(
            parse_release(r#"{"tag_name": "9", "assets": []}"#, false),
            Err(ReleaseError::MissingAsset(DESKTOP_ASSET_PREFIX))
        );
        assert_eq!(
            parse_release(r#"{"tag_name": "9"}"#, true),
            Err(ReleaseError::MissingAsset(SERVER_ASSET_PREFIX))
        );
    }

    #[test]
    fn newer_build_numbers_win() {
        assert!(is_update_available(12_344, 12_345));
        assert!(!is_update_available(12_345, 12_345));
        assert!(!is_update_available(12_346, 12_345));
        assert!(is_update_available(-1, 0));
    }

    #[test]
    fn check_release_sets_the_update_fields() {
        let installed = bleeding_edge();
        let status = check_release(&installed, DESKTOP_RELEASE, false).unwrap();
        assert!(status.active);
        assert!(status.available);
        assert_eq!(status.update_build, 12_345);
        assert_eq!(status.update_url, "https://example.com/desktop.jar");
    }

    #[test]
    fn check_release_ignores_older_builds() {
        let installed = BuildInfo {
            build: 99_999,
            ..bleeding_edge()
        };
        let status = check_release(&installed, DESKTOP_RELEASE, false).unwrap();
        assert!(status.active);
        assert!(!status.available);
        assert_eq!(status.update_build, 0);
        assert!(status.update_url.is_empty());
    }

    #[test]
    fn check_release_short_circuits_inactive_builds() {
        let official = BuildInfo {
            r#type: "official".to_owned(),
            ..BuildInfo::default()
        };
        // Invalid JSON never parsed because the updater is inactive.
        let status = check_release(&official, "not json", false).unwrap();
        assert!(!status.active);
        assert!(!status.available);
    }

    #[test]
    fn reads_version_properties_from_disk() {
        let root = std::env::temp_dir().join(format!("mind-update-version-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("version.properties");
        std::fs::write(
            &path,
            "type=bleeding-edge\nnumber=8\nmodifier=release\nbuild=12345.2\n",
        )
        .unwrap();

        let info = read_build_info(&path).unwrap();
        assert_eq!(info.r#type, "bleeding-edge");
        assert_eq!(info.build, 12_345);
        assert_eq!(info.revision, 2);
        assert_eq!(info.build_string(), "12345.2");

        assert!(read_build_info(&root.join("missing.properties")).is_none());

        std::fs::write(&path, "number=not-a-number\n").unwrap();
        assert!(read_build_info(&path).is_none());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
