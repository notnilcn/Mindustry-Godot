// SPDX-License-Identifier: GPL-3.0-only

//! Content base data model: `Content`, `MappableContent`, `UnlockableContent`.
//!
//! Ported from `core/src/mindustry/ctype/{Content,MappableContent,UnlockableContent}.java`
//! and `core/src/mindustry/type/ErrorContent.java`. Behavior-facing lifecycle
//! hooks are defaulted no-ops; plans 05/07/10/11/17 own the real behavior.

use super::bundle::BundleView;
use super::id::PlanetId;
use super::parser_hooks::ContentAsset;
use super::settings_store::UnlockStore;
use super::tech::TechNodeRef;
use super::{ContentError, ContentType};

/// Mod identifier (folder name without extension).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModId(pub String);

impl ModId {
    /// Borrows the mod name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ModId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// `Content.ModContentInfo`: provenance/error bookkeeping for one record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModContentInfo {
    /// The mod that loaded this piece of content (`None` = vanilla).
    pub mod_id: Option<ModId>,
    /// File this content was loaded from.
    pub source_file: Option<String>,
    /// Error message from loading, if applicable.
    pub error: Option<String>,
    /// Base throwable text that caused the error.
    pub base_error: Option<String>,
    /// Save-content asset this was loaded as part of.
    pub asset: Option<ContentAsset>,
}

impl ModContentInfo {
    /// `hasErrored()`.
    pub fn has_errored(&self) -> bool {
        self.error.is_some()
    }

    /// `isVanilla()`.
    pub fn is_vanilla(&self) -> bool {
        self.mod_id.is_none()
    }

    /// `isModded()`.
    pub fn is_modded(&self) -> bool {
        !self.is_vanilla()
    }

    /// `isPatchContent()` — plan 20 marks data-patch records with the `dp` mod.
    pub fn is_patch_content(&self) -> bool {
        self.mod_id
            .as_ref()
            .is_some_and(|mod_id| mod_id.name() == "dp")
    }
}

/// `UnlockableContent` common fields, factored out so each record type does not
/// duplicate ~20 fields. Localized strings are captured at construction from the
/// bundle (plans 03/20 load the bundle before `create_base_content`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnlockFields {
    /// Content type this record belongs to.
    pub content_type: ContentType,
    /// Localized, formal name; never empty (falls back to the internal name).
    pub localized_name: String,
    /// Localized description (may be absent).
    pub description: Option<String>,
    /// Localized details (may be absent).
    pub details: Option<String>,
    /// Localized credit line (may be absent).
    pub credit: Option<String>,
    /// Whether this content is always unlocked in the tech tree.
    pub always_unlocked: bool,
    /// Whether to show the description in the research dialog preview.
    pub inline_description: bool,
    /// Whether details are hidden in custom games if not unlocked in campaign.
    pub hide_details: bool,
    /// Whether this is hidden from the Core Database.
    pub hide_database: bool,
    /// If false, icon generation is disabled for this content.
    pub generate_icons: bool,
    /// How big the content appears in certain selection menus.
    pub selection_size: f32,
    /// Override for the full icon region name (empty = none).
    pub full_override: String,
    /// If true, this content appears in all database tabs.
    pub all_database_tabs: bool,
    /// Planets this content is made for.
    pub shown_planets: Vec<PlanetId>,
    /// Content (usually a planet) dictating database tabs.
    pub database_tabs: Vec<super::ContentRef>,
    /// Primary database category (defaults to the content type name).
    pub database_category: Option<String>,
    /// Secondary database tag (defaults to `default`).
    pub database_tag: Option<String>,
    /// The tech-tree node for this content, if any.
    pub tech_node: Option<TechNodeRef>,
    /// Tech nodes for all trees this content is part of.
    pub tech_nodes: Vec<TechNodeRef>,
    /// Unlock state loaded from settings (`<name>-unlocked`).
    pub unlocked: bool,
    /// Cached icon region-name expectation (plan 03 resolves the texture).
    pub icon_region: Option<String>,
}

impl UnlockFields {
    /// Builds fields for `content_type`/`name`, reading bundle keys
    /// `<type>.<name>.name|description|details|credit` and the `<name>-unlocked`
    /// setting exactly like `UnlockableContent`.
    pub fn new(
        content_type: ContentType,
        name: &str,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Self {
        let key = |suffix: &str| format!("{}.{}.{}", content_type.name(), name, suffix);
        Self {
            content_type,
            localized_name: bundle
                .get(&key("name"))
                .map(str::to_owned)
                .unwrap_or_else(|| name.to_owned()),
            description: bundle.get(&key("description")).map(str::to_owned),
            details: bundle.get(&key("details")).map(str::to_owned),
            credit: bundle.get(&key("credit")).map(str::to_owned),
            always_unlocked: false,
            inline_description: true,
            hide_details: true,
            hide_database: false,
            generate_icons: true,
            selection_size: 24.0,
            full_override: String::new(),
            all_database_tabs: false,
            shown_planets: Vec::new(),
            database_tabs: Vec::new(),
            database_category: None,
            database_tag: None,
            tech_node: None,
            tech_nodes: Vec::new(),
            unlocked: store.get_bool(&format!("{name}-unlocked")),
            icon_region: None,
        }
    }

    /// `UnlockableContent.postInit` database defaults.
    pub fn post_init(&mut self) {
        if self.database_category.as_deref().is_none_or(str::is_empty) {
            self.database_category = Some(self.content_type.name().to_owned());
        }
        if self.database_tag.as_deref().is_none_or(str::is_empty) {
            self.database_tag = Some(String::from("default"));
        }
        for planet in &self.shown_planets {
            let reference = super::ContentRef::of(ContentType::Planet, *planet);
            if !self.database_tabs.contains(&reference) {
                self.database_tabs.push(reference);
            }
        }
    }

    /// `<name>-unlocked` settings key.
    pub fn unlock_key(&self, name: &str) -> String {
        format!("{name}-unlocked")
    }

    /// `unlocked()` — `alwaysUnlocked` bypasses the stored flag.
    pub fn unlocked(&self) -> bool {
        self.unlocked || self.always_unlocked
    }

    /// `locked()`.
    pub fn locked(&self) -> bool {
        !self.unlocked()
    }

    /// `unlock()` — persists and returns whether the state changed.
    pub fn unlock(&mut self, name: &str, store: &mut dyn UnlockStore) -> bool {
        if !self.unlocked && !self.always_unlocked {
            self.unlocked = true;
            store.set_bool(&self.unlock_key(name), true);
            true
        } else {
            false
        }
    }

    /// `quietUnlock()` — no event side effects.
    pub fn quiet_unlock(&mut self, name: &str, store: &mut dyn UnlockStore) {
        if !self.unlocked() {
            self.unlocked = true;
            store.set_bool(&self.unlock_key(name), true);
        }
    }

    /// `clearUnlock()` — locks the content again.
    pub fn clear_unlock(&mut self, name: &str, store: &mut dyn UnlockStore) {
        if self.unlocked {
            self.unlocked = false;
            store.set_bool(&self.unlock_key(name), false);
        }
    }

    /// Research requirements by default empty (`UnlockableContent` base).
    pub fn research_requirements(&self) -> &[super::stacks::ItemStack] {
        &[]
    }

    /// Whether this content is always hidden in the database.
    pub fn is_hidden(&self) -> bool {
        false
    }
}

/// Root content trait. `TYPE` is the per-type ID-space tag; every record stores
/// its dense id (assigned by the registry at registration time).
pub trait Content {
    /// The content type (Java `getContentType()`).
    const TYPE: ContentType;

    /// Dense id (`Content.id`).
    fn content_id(&self) -> u16;

    /// Assigns the dense id during registration.
    fn set_content_id(&mut self, id: u16);

    /// Mod/provenance info.
    fn minfo(&self) -> &ModContentInfo;

    /// Mutable mod/provenance info.
    fn minfo_mut(&mut self) -> &mut ModContentInfo;

    /// Whether this content was removed by a data patch.
    fn removed(&self) -> bool;

    /// Marks the record removed.
    fn set_removed(&mut self, removed: bool);

    /// Java class-ish kind tag for audit output.
    fn kind_name(&self) -> &'static str;

    /// Mappable name, when this kind is mappable.
    fn content_name(&self) -> Option<&str> {
        None
    }

    /// Unlock fields, when this kind is unlockable.
    fn unlock_fields(&self) -> Option<&UnlockFields> {
        None
    }

    /// `Content.removeContent()` — called when content is removed by a patch.
    fn remove_content(&mut self) {}

    /// `Content.init()` — self-only derived state; cross-content mutation runs in
    /// the registry `link()` pass (plan 02 §3.4).
    fn init_self(&mut self) -> Result<(), ContentError> {
        Ok(())
    }

    /// `Content.postInit()`.
    fn post_init(&mut self) -> Result<(), ContentError> {
        Ok(())
    }

    /// `Content.loadIcon()` — client only.
    fn load_icon(&mut self) -> Result<(), ContentError> {
        Ok(())
    }

    /// `Content.load()` — client only.
    fn load(&mut self) -> Result<(), ContentError> {
        Ok(())
    }

    /// `Content.afterPatch()`.
    fn after_patch(&mut self) -> Result<(), ContentError> {
        Ok(())
    }
}

/// Mappable content: globally unique `name`, registered in the registry name maps.
pub trait Mappable: Content {
    /// Content name (already mod-prefixed by `transform_name`).
    fn name(&self) -> &str;
}

/// Unlockable content: localized strings + unlock/database state.
pub trait Unlockable: Mappable {
    /// Immutable unlock fields.
    fn unlock(&self) -> &UnlockFields;

    /// Mutable unlock fields.
    fn unlock_mut(&mut self) -> &mut UnlockFields;
}

/// `ErrorContent`: blank record used as the fallback for failed parses.
#[derive(Debug, Clone)]
pub struct ErrorContent {
    /// Dense id inside `ContentType::Error`.
    pub id: u16,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Human-readable original failure, if known.
    pub error: Option<String>,
}

impl ErrorContent {
    /// Creates an empty error record.
    pub fn new(error: Option<String>) -> Self {
        Self {
            id: 0,
            minfo: ModContentInfo::default(),
            removed: false,
            error,
        }
    }
}

impl Content for ErrorContent {
    const TYPE: ContentType = ContentType::Error;

    fn content_id(&self) -> u16 {
        self.id
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = id;
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        "ErrorContent"
    }
}
