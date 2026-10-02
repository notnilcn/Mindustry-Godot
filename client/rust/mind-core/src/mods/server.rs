// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server parity & multiplayer handshake interfaces (plan 20 §3.11 / plan 21).
//!
//! Ported from `core/src/mindustry/core/NetServer.java` (`sendAssetRequirements`,
//! mod identity checks) and `mod/Mods.java` (`fingerprints`, `getModStrings`,
//! `getIncompatibility`). The transport itself is owned by plan 21; this module
//! only fixes the values that travel.

use serde::{Deserialize, Serialize};

use super::assets::{DataAssetType, sha256};
use super::{LoadedMod, Mods};

/// `ModFingerprint`: identity that must match on both ends of a join.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFingerprint {
    /// Internal mod name.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Whether the mod had isolated content errors.
    pub content_errors: bool,
    /// Hidden mods never add content but must still match.
    pub hidden: bool,
}

impl ModFingerprint {
    /// `"name:version"` (`getModStrings` form).
    pub fn mod_string(&self) -> String {
        format!("{}:{}", self.name, self.version)
    }
}

impl From<&LoadedMod> for ModFingerprint {
    fn from(mod_: &LoadedMod) -> Self {
        Self {
            name: mod_.name.clone(),
            version: mod_.meta.version().to_owned(),
            content_errors: mod_.has_content_errors(),
            hidden: mod_.meta.hidden,
        }
    }
}

/// `required_mods` handshake payload (plan 21).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredMods {
    /// Required mods (including hidden ones that must match).
    pub required: Vec<ModFingerprint>,
    /// Soft/optional mods.
    pub soft: Vec<ModFingerprint>,
}

impl RequiredMods {
    /// Builds the manifest from the enabled mods (hidden included so the client
    /// can reject mismatches, plan 20 §3.11).
    pub fn from_mods(mods: &Mods) -> Self {
        let mut required: Vec<ModFingerprint> = mods
            .list()
            .iter()
            .filter(|mod_| mod_.enabled())
            .map(ModFingerprint::from)
            .collect();
        required.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            required,
            soft: Vec::new(),
        }
    }

    /// Canonical JSON text for the transport.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Parses a manifest.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// One external asset advertised to a joining client (plan 21 transport).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAssetRef {
    /// Asset type.
    pub type_: DataAssetType,
    /// Source path.
    pub path: String,
    /// sha256 of the payload.
    pub sha256: [u8; 32],
    /// Payload length in bytes.
    pub byte_len: u32,
}

impl ExternalAssetRef {
    /// Builds a reference from raw bytes.
    pub fn from_bytes(type_: DataAssetType, path: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            type_,
            path: path.into(),
            sha256: sha256(bytes),
            byte_len: bytes.len() as u32,
        }
    }

    /// sha256 as lowercase hex (transport/log friendly).
    pub fn hash_hex(&self) -> String {
        super::assets::hex32(&self.sha256)
    }
}

/// Transport errors (implemented by `mind-stdb`, plan 21).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The transport is not connected.
    #[error("transport is not connected")]
    Disconnected,
    /// A blob could not be fetched.
    #[error("unknown asset blob")]
    UnknownBlob,
    /// Underlying transport failure.
    #[error("transport error: {0}")]
    Other(String),
}

/// Asset blob/manifest transport (plan 21 owns the STDB tables/reducers).
pub trait AssetTransport {
    /// Publishes the server's asset manifest.
    fn publish_manifest(&mut self, assets: &[ExternalAssetRef]) -> Result<(), TransportError>;
    /// Fetches a blob by sha256.
    fn fetch_blob(&self, sha256: [u8; 32]) -> Result<Option<Vec<u8>>, TransportError>;
    /// Subscribes to the server manifest.
    fn subscribe_manifest(&mut self) -> Result<(), TransportError>;
}

/// Sink implemented by plan 20: receives delivered blobs/manifests.
pub trait AssetTransportSink {
    /// Called when a requested blob arrives.
    fn on_asset_available(&mut self, sha256: [u8; 32], data: Vec<u8>);
    /// Called when the server manifest changes.
    fn on_asset_requirements_changed(&mut self, assets: &[ExternalAssetRef]);
}

impl Mods {
    /// `Mods.fingerprints`: enabled, non-hidden mods (plan 20 §3.11).
    pub fn fingerprints(&self) -> Vec<ModFingerprint> {
        self.list()
            .iter()
            .filter(|mod_| mod_.enabled() && !mod_.meta.hidden)
            .map(ModFingerprint::from)
            .collect()
    }

    /// Full manifest for the join handshake (hidden included).
    pub fn required_mods(&self) -> RequiredMods {
        RequiredMods::from_mods(self)
    }

    /// Java/script mods whose code hooks cannot run in this build (surfaced,
    /// not silently dropped; OD1 deviation).
    pub fn java_mods(&self) -> Vec<&LoadedMod> {
        self.list()
            .iter()
            .filter(|mod_| mod_.enabled() && mod_.is_java())
            .collect()
    }

    /// Script mods whose code hooks cannot run in this build.
    pub fn scripted_mods(&self) -> Vec<&LoadedMod> {
        self.list()
            .iter()
            .filter(|mod_| mod_.enabled() && mod_.has_scripts)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::fs::FileSystem;
    use crate::io::{MockFs, SettingsStore};
    use std::path::Path;

    fn write_mod(fs: &MockFs, name: &str, meta: &str) {
        fs.write(
            &Path::new("/mods").join(name).join("mod.json"),
            meta.as_bytes(),
        )
        .expect("mod.json");
    }

    fn load(fs: &MockFs) -> Mods {
        let mut mods = Mods::new(true, "/mods");
        mods.load(fs, Path::new("/mods"), &SettingsStore::new())
            .expect("load");
        mods
    }

    /// Plan 20 M7: `server::fingerprints_exclude_hidden`.
    #[test]
    fn fingerprints_exclude_hidden() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha","version":"1.0"}"#);
        write_mod(
            &fs,
            "h",
            r#"{"name":"Hidden","version":"2.0","hidden":true}"#,
        );
        let mods = load(&fs);
        let fps = mods.fingerprints();
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].name, "alpha");
        assert_eq!(fps[0].mod_string(), "alpha:1.0");
        // `required_mods` includes hidden so both ends can match.
        let required = mods.required_mods();
        assert_eq!(required.required.len(), 2);
    }

    /// Plan 20 M7: `server::incompatibility_extra_missing`.
    #[test]
    fn incompatibility_extra_missing() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha","version":"1.0"}"#);
        let mods = load(&fs);
        let mut server = vec!["alpha:1.0".to_owned(), "server-only:3.0".to_owned()];
        let missing = mods.get_incompatibility(&mut server);
        assert!(missing.is_empty(), "client has all server mods");
        assert_eq!(server, vec!["server-only:3.0".to_owned()]);
    }

    /// Plan 20 M7: `server::required_manifest_roundtrip`.
    #[test]
    fn required_manifest_roundtrip() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha","version":"1.0"}"#);
        write_mod(&fs, "b", r#"{"name":"Beta","version":"2.0"}"#);
        let mods = load(&fs);
        let manifest = mods.required_mods();
        let json = manifest.to_json();
        let decoded = RequiredMods::from_json(&json).expect("roundtrip");
        assert_eq!(decoded, manifest);
        assert_eq!(decoded.required.len(), 2);
    }

    /// Plan 20 M7: `server::java_mod_surfaces_unsupported` — Java mods load but
    /// are surfaced as code-hook-unsupported rather than vanishing.
    #[test]
    fn java_mod_surfaces_unsupported() {
        let fs = MockFs::new();
        write_mod(
            &fs,
            "j",
            r#"{"name":"JavaMod","version":"1.0","java":true}"#,
        );
        let mods = load(&fs);
        let java = mods.java_mods();
        assert_eq!(java.len(), 1);
        assert!(java[0].is_java());
        assert_eq!(
            java[0].unsupported_reason,
            Some(super::super::UnsupportedReason::JavaModUnsupported)
        );
    }

    #[test]
    fn external_asset_ref_hash() {
        let reference = ExternalAssetRef::from_bytes(DataAssetType::Image, "x.png", b"abc");
        assert_eq!(reference.byte_len, 3);
        assert_eq!(reference.hash_hex().len(), 64);
    }
}
