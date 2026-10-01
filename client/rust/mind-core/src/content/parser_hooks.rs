// SPDX-License-Identifier: GPL-3.0-only

//! Registry-side mod/patch interfaces consumed by plan 20.
//!
//! Ported from the interface shapes of `mod/ContentParser.java`,
//! `mod/DataPatcher.java`, `mod/DataManager.java` and
//! `Mods.handleContentError` (`ContentLoader.initialize` error routing). Plan 20
//! implements these traits; plan 02 ships the trait definitions and registry APIs
//! they call. The `ModSet` parameter from plan 02 §3.6 is deferred to plan 20
//! (reconcile at M6), so the hooks only see the registry.

use super::load::ContentRegistry;
use super::{ContentError, ContentRef, ContentType};

/// One content asset parsed from a data mod/save (`ContentParser` input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentAsset {
    /// Asset name (unprefixed mod name; `transform_name` applies the `dp`/mod prefix).
    pub name: String,
    /// Declared content type.
    pub type_: ContentType,
    /// Source file/line text for error messages.
    pub source_file: Option<String>,
}

/// One data-patch asset (`DataPatcher` input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchAsset {
    /// Patch name.
    pub name: String,
    /// Raw JSON payload (plan 20 owns the schema).
    pub json: String,
}

/// A single field-restore closure produced by a patch (`ResetAction`).
///
/// Index membership is restored separately by
/// [`ContentRegistry::restore_index`](super::load::ContentRegistry::restore_index).
pub type ResetAction = Box<dyn FnOnce(&mut ContentRegistry) + 'static>;

/// Collections of registry errors returned by bulk mod operations.
pub type ContentErrors = Vec<ContentError>;

/// Parse failure surface (plan 20 formats the message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentParseError {
    /// Human-readable failure.
    pub message: String,
}

impl ContentParseError {
    /// Builds an error from any displayable message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ContentParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ContentParseError {}

/// `mods.loadContent()` equivalent: plan 20 adds mod content into the registry.
pub trait ModContentProvider {
    /// Loads all mod content; per-content failures are routed by the caller.
    fn load_content(&mut self, registry: &mut ContentRegistry) -> Result<(), ContentErrors>;
}

/// `ContentParser.parse` equivalent.
pub trait ContentParserHook {
    /// Parses one content asset and registers it; returns the new reference.
    fn parse(
        &mut self,
        registry: &mut ContentRegistry,
        asset: &ContentAsset,
    ) -> Result<ContentRef, ContentParseError>;
}

/// `DataPatcher.apply` equivalent.
pub trait PatchHook {
    /// Applies patches and returns field-restore actions for `logic.reset()`.
    fn apply(
        &mut self,
        registry: &mut ContentRegistry,
        patches: &[PatchAsset],
        content: &[ContentAsset],
    ) -> Result<Vec<ResetAction>, ContentError>;
}

/// `Mods.handleContentError` equivalent: per-content mod failures are reported
/// here instead of aborting the sweep.
pub trait ModErrorSink {
    /// Reports a lifecycle failure for a mod content record.
    fn handle_content_error(&mut self, content: ContentRef, error: &ContentError);
}
