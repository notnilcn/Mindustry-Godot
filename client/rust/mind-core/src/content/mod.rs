// SPDX-License-Identifier: GPL-3.0-only

//! Content framework: types, IDs, lifecycle and the vanilla registries.
//!
//! Ported from `core/src/mindustry/ctype/{ContentType,Content,MappableContent,UnlockableContent}.java`
//! and `core/src/mindustry/core/ContentLoader.java`. Plan 02 owns this tree; the
//! implementations replace the P0 placeholder registry that lived in `content.rs`
//! while keeping the scenario-facing names (`air`, `stone-wall`) valid.
//!
//! ## ID stability invariant
//!
//! Construction order defines IDs; IDs are dense `0..len` in creation order and
//! are append-only parity/mod ABI. `ContentType` member order, content names,
//! bundle keys and sprite region names are never renamed (`ctype/AGENTS.md`).

pub mod bundle;
pub mod category;
pub mod color;
pub mod ctype;
pub mod id;
pub mod load;
pub mod names;
pub mod parity;
pub mod parser_hooks;
pub mod registries;
pub mod settings_store;
pub mod snapshot;
pub mod stacks;
pub mod tech;

use serde::{Deserialize, Serialize};

pub use bundle::{BundleView, MemoryBundle};
pub use category::Category;
pub use color::Rgba;
pub use ctype::{Content, ErrorContent, Mappable, ModContentInfo, ModId, UnlockFields, Unlockable};
pub use id::{
    BlockId, BulletId, ContentId, ItemId, LiquidId, PlanetId, SectorId, StatusId, TeamEntryId,
    UnitCommandId, UnitStanceId, UnitTypeId, WeatherId,
};
pub use load::{
    ContentEntry, ContentRegistry, LifecyclePhase, MappedId, TemporaryMapper, content_counts,
};
pub use names::{NameMaps, mod_content_name_map, transform_name};
pub use parity::{AssetManifest, AuditReport, BundleKeysFile, GoldenContent, audit, dump_golden};
pub use registries::blocks::{
    BarSpec, BlockDef, BlockFlag, BlockGroup, BlockKind, BlockSpec, Blocks, BuildVisibility,
    Consume, ConsumeSpec, EnvMask, StatSpec, TILE_SIZE,
};
pub use registries::bullets::{BulletDef, BulletKind};
pub use registries::commands::{ControllerKind, UnitCommandDef};
pub use registries::create_base_content;
pub use registries::fx_meta::{EFFECT_COUNT, EFFECTS, EffectId, EffectMeta, effect_by_name};
pub use registries::items::Item;
pub use registries::liquids::{CellLiquidFields, Liquid};
pub use registries::loadouts::LoadoutDef;
pub use registries::planets::{
    AsteroidSpec, CampaignRuleDefaults, CloudMeshKind, EnvFlag, GeneratorKind, MeshKind, PlanetDef,
    RuleSetterKind, Sector,
};
pub use registries::sectors::{
    IdentityRemap, RuleOverrideKind, SectorPresetDef, SectorRemapProvider,
};
pub use registries::stances::UnitStanceDef;
pub use registries::statuses::{
    AffinityTransition, StatusEffect, TransitionSpec, TransitionTrigger,
};
pub use registries::teams::TeamEntry;
pub use registries::weathers::{
    Attribute, ParticleWeatherFields, RainWeatherFields, WeatherDef, WeatherKind,
};
pub use settings_store::{MemoryUnlockStore, UnlockStore};
pub use stacks::{ItemStack, LiquidStack, PayloadStack};
pub use tech::{
    NodeObjective, ObjectiveSpec, TechNode, TechNodeRef, TechStore, TechTreeBuildReport,
    TechTreeBuilder, TreeId, round_to_10,
};

/// Content type enum. Ported from `ctype/ContentType.java`.
///
/// **Do not rearrange, ever!** The ordinal is the per-type ID-space index and is
/// baked into maps/saves/network traffic. `_UNUSED` members reserve historical
/// ordinals and are never repurposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContentType {
    /// `item` — items (`folderName` `items`).
    #[serde(rename = "item")]
    Item = 0,
    /// `block` — blocks (`blocks`).
    #[serde(rename = "block")]
    Block = 1,
    /// `mech_UNUSED` — historical mech id space (unused).
    #[serde(rename = "mech_UNUSED")]
    MechUnused = 2,
    /// `bullet` — bullet types (`bullets`).
    #[serde(rename = "bullet")]
    Bullet = 3,
    /// `liquid` — liquids (`liquids`).
    #[serde(rename = "liquid")]
    Liquid = 4,
    /// `status` — status effects (`statuses`).
    #[serde(rename = "status")]
    Status = 5,
    /// `unit` — unit types (`units`).
    #[serde(rename = "unit")]
    Unit = 6,
    /// `weather` — weathers (`weather`).
    #[serde(rename = "weather")]
    Weather = 7,
    /// `effect_UNUSED` — historical effect id space (unused).
    #[serde(rename = "effect_UNUSED")]
    EffectUnused = 8,
    /// `sector` — sector presets (`sectors`).
    #[serde(rename = "sector")]
    Sector = 9,
    /// `loadout_UNUSED` — historical loadout id space (unused).
    #[serde(rename = "loadout_UNUSED")]
    LoadoutUnused = 10,
    /// `typeid_UNUSED` — historical type-id space (unused).
    #[serde(rename = "typeid_UNUSED")]
    TypeIdUnused = 11,
    /// `error` — fallback content for failed parses (`unused` folder).
    #[serde(rename = "error")]
    Error = 12,
    /// `planet` — planets (`planets`).
    #[serde(rename = "planet")]
    Planet = 13,
    /// `ammo_UNUSED` — historical ammo id space (unused).
    #[serde(rename = "ammo_UNUSED")]
    AmmoUnused = 14,
    /// `team` — team entries (`teams`).
    #[serde(rename = "team")]
    Team = 15,
    /// `unitCommand` — unit commands (`unitCommands`).
    #[serde(rename = "unitCommand")]
    UnitCommand = 16,
    /// `unitStance` — unit stances (`unitStances`).
    #[serde(rename = "unitStance")]
    UnitStance = 17,
}

impl ContentType {
    /// All variants in declaration order (`ContentType.all`).
    pub const ALL: [ContentType; 18] = [
        ContentType::Item,
        ContentType::Block,
        ContentType::MechUnused,
        ContentType::Bullet,
        ContentType::Liquid,
        ContentType::Status,
        ContentType::Unit,
        ContentType::Weather,
        ContentType::EffectUnused,
        ContentType::Sector,
        ContentType::LoadoutUnused,
        ContentType::TypeIdUnused,
        ContentType::Error,
        ContentType::Planet,
        ContentType::AmmoUnused,
        ContentType::Team,
        ContentType::UnitCommand,
        ContentType::UnitStance,
    ];

    /// Exact Java enum identifier (`ContentType.name()`), used for bundle keys,
    /// `databaseCategory` and debug parity.
    pub const fn name(self) -> &'static str {
        match self {
            ContentType::Item => "item",
            ContentType::Block => "block",
            ContentType::MechUnused => "mech_UNUSED",
            ContentType::Bullet => "bullet",
            ContentType::Liquid => "liquid",
            ContentType::Status => "status",
            ContentType::Unit => "unit",
            ContentType::Weather => "weather",
            ContentType::EffectUnused => "effect_UNUSED",
            ContentType::Sector => "sector",
            ContentType::LoadoutUnused => "loadout_UNUSED",
            ContentType::TypeIdUnused => "typeid_UNUSED",
            ContentType::Error => "error",
            ContentType::Planet => "planet",
            ContentType::AmmoUnused => "ammo_UNUSED",
            ContentType::Team => "team",
            ContentType::UnitCommand => "unitCommand",
            ContentType::UnitStance => "unitStance",
        }
    }

    /// JSON/content folder (`ContentType.folderName`); `_UNUSED`/`error` → `unused`.
    pub const fn folder(self) -> &'static str {
        match self {
            ContentType::Item => "items",
            ContentType::Block => "blocks",
            ContentType::MechUnused => "unused",
            ContentType::Bullet => "bullets",
            ContentType::Liquid => "liquids",
            ContentType::Status => "statuses",
            ContentType::Unit => "units",
            ContentType::Weather => "weather",
            ContentType::EffectUnused => "unused",
            ContentType::Sector => "sectors",
            ContentType::LoadoutUnused => "unused",
            ContentType::TypeIdUnused => "unused",
            ContentType::Error => "unused",
            ContentType::Planet => "planets",
            ContentType::AmmoUnused => "unused",
            ContentType::Team => "teams",
            ContentType::UnitCommand => "unitCommands",
            ContentType::UnitStance => "unitStances",
        }
    }

    /// `contentClass` equivalent (the Java base class of this kind).
    pub const fn kind(self) -> ContentKind {
        match self {
            ContentType::Item => ContentKind::Item,
            ContentType::Block => ContentKind::Block,
            ContentType::Bullet => ContentKind::Bullet,
            ContentType::Liquid => ContentKind::Liquid,
            ContentType::Status => ContentKind::Status,
            ContentType::Unit => ContentKind::Unit,
            ContentType::Weather => ContentKind::Weather,
            ContentType::Sector => ContentKind::Sector,
            ContentType::Planet => ContentKind::Planet,
            ContentType::Team => ContentKind::Team,
            ContentType::UnitCommand => ContentKind::UnitCommand,
            ContentType::UnitStance => ContentKind::UnitStance,
            ContentType::Error => ContentKind::Error,
            ContentType::MechUnused
            | ContentType::EffectUnused
            | ContentType::LoadoutUnused
            | ContentType::TypeIdUnused
            | ContentType::AmmoUnused => ContentKind::Unused,
        }
    }

    /// The ordinal (`ContentType.ordinal()`).
    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// Whether this is a live registry (has content records in this build).
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            ContentType::Item
                | ContentType::Block
                | ContentType::Bullet
                | ContentType::Liquid
                | ContentType::Status
                | ContentType::Unit
                | ContentType::Weather
                | ContentType::Sector
                | ContentType::Planet
                | ContentType::Team
                | ContentType::UnitCommand
                | ContentType::UnitStance
        )
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// `ContentType.contentClass` equivalent, with Rust record types named after the
/// Java classes (`Item`, `Block`, `BulletType`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentKind {
    /// `mindustry.type.Item`.
    Item,
    /// `mindustry.world.Block`.
    Block,
    /// `mindustry.entities.bullet.BulletType`.
    Bullet,
    /// `mindustry.type.Liquid` / `CellLiquid`.
    Liquid,
    /// `mindustry.type.StatusEffect`.
    Status,
    /// `mindustry.type.UnitType`.
    Unit,
    /// `mindustry.type.Weather`.
    Weather,
    /// `mindustry.type.SectorPreset`.
    Sector,
    /// `mindustry.type.Planet`.
    Planet,
    /// `mindustry.type.TeamEntry`.
    Team,
    /// `mindustry.ai.UnitCommand`.
    UnitCommand,
    /// `mindustry.ai.UnitStance`.
    UnitStance,
    /// `mindustry.type.ErrorContent`.
    Error,
    /// `_UNUSED`/historical slots.
    Unused,
}

impl ContentKind {
    /// Java class name (for audit output).
    pub const fn name(self) -> &'static str {
        match self {
            ContentKind::Item => "Item",
            ContentKind::Block => "Block",
            ContentKind::Bullet => "BulletType",
            ContentKind::Liquid => "Liquid",
            ContentKind::Status => "StatusEffect",
            ContentKind::Unit => "UnitType",
            ContentKind::Weather => "Weather",
            ContentKind::Sector => "SectorPreset",
            ContentKind::Planet => "Planet",
            ContentKind::Team => "TeamEntry",
            ContentKind::UnitCommand => "UnitCommand",
            ContentKind::UnitStance => "UnitStance",
            ContentKind::Error => "ErrorContent",
            ContentKind::Unused => "Unused",
        }
    }
}

/// Type-erased reference to one content record (type + raw id).
///
/// Replaces Java object pointers across plan boundaries; resolve through
/// [`ContentRegistry`] accessors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContentRef {
    /// Content kind.
    #[serde(rename = "type")]
    pub type_: ContentType,
    /// Raw id inside that kind.
    pub id: u16,
}

impl ContentRef {
    /// Builds a reference from a content type and raw id.
    pub const fn new(type_: ContentType, id: u16) -> Self {
        Self { type_, id }
    }

    /// Builds a reference from a typed id.
    pub fn of<T>(type_: ContentType, id: ContentId<T>) -> Self {
        Self::new(type_, id.raw())
    }

    /// Item reference.
    pub fn item(id: ItemId) -> Self {
        Self::new(ContentType::Item, id.raw())
    }

    /// Block reference.
    pub fn block(id: BlockId) -> Self {
        Self::new(ContentType::Block, id.raw())
    }

    /// Bullet reference.
    pub fn bullet(id: BulletId) -> Self {
        Self::new(ContentType::Bullet, id.raw())
    }

    /// Liquid reference.
    pub fn liquid(id: LiquidId) -> Self {
        Self::new(ContentType::Liquid, id.raw())
    }

    /// Status reference.
    pub fn status(id: StatusId) -> Self {
        Self::new(ContentType::Status, id.raw())
    }
}

/// Errors raised by the content framework.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ContentError {
    /// A content name may only be registered once per type
    /// (`ContentLoader.handleMappableContent`).
    #[error("Two content objects defined with the same name: '{0}'")]
    DuplicateName(String),
    /// The requested name does not exist.
    #[error("content name `{0}` is unknown")]
    UnknownName(String),
    /// The requested id does not exist.
    #[error("content id {0} is out of range")]
    UnknownId(u16),
    /// The requested content type has no record vector in this build.
    #[error("content type `{0}` has no registry in this build")]
    UnsupportedType(ContentType),
    /// A content name was expected but the record is not mappable.
    #[error("content '{0}' is not mappable")]
    NotMappable(String),
    /// Content IDs must be dense and in creation order (`ContentLoader.logContent`).
    #[error("Out-of-order IDs for content '{name}' (expected {expected} but got {got})")]
    OutOfOrderIds {
        /// Offending content name (or `type#id` for non-mappable records).
        name: String,
        /// Expected dense index.
        expected: usize,
        /// Actual stored id.
        got: u16,
    },
    /// The registry exceeded its `u16` id space.
    #[error("content registry exhausted its 16-bit id space")]
    IdSpaceExhausted,
    /// A parse/patch hook failed (plan 20 formats the details).
    #[error("{0}")]
    Parse(String),
}

/// Shared test fixture: full M1 base content loaded, initialized and linked.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Boots items/statuses/liquids/bullets, runs `init` + `post_init`, and
    /// asserts the dense-ID invariant.
    pub(crate) fn test_registry() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry = create_base_content(&bundle, &store, true).expect("create base content");
        registry.init().expect("content init");
        registry.post_init().expect("content post init");
        registry.log_content().expect("dense content ids");
        registry
    }
}

#[cfg(test)]
mod content_framework {
    use super::*;

    /// `content_framework::content_type_ordinals` (plan 02 §7a): all 18
    /// ordinals/names/folders/kinds match the upstream enum.
    ///
    /// Golden table derived from `core/src/mindustry/ctype/ContentType.java`.
    #[test]
    fn content_type_ordinals() {
        let golden: &[(ContentType, &str, &str, ContentKind)] = &[
            (ContentType::Item, "item", "items", ContentKind::Item),
            (ContentType::Block, "block", "blocks", ContentKind::Block),
            (
                ContentType::MechUnused,
                "mech_UNUSED",
                "unused",
                ContentKind::Unused,
            ),
            (
                ContentType::Bullet,
                "bullet",
                "bullets",
                ContentKind::Bullet,
            ),
            (
                ContentType::Liquid,
                "liquid",
                "liquids",
                ContentKind::Liquid,
            ),
            (
                ContentType::Status,
                "status",
                "statuses",
                ContentKind::Status,
            ),
            (ContentType::Unit, "unit", "units", ContentKind::Unit),
            (
                ContentType::Weather,
                "weather",
                "weather",
                ContentKind::Weather,
            ),
            (
                ContentType::EffectUnused,
                "effect_UNUSED",
                "unused",
                ContentKind::Unused,
            ),
            (
                ContentType::Sector,
                "sector",
                "sectors",
                ContentKind::Sector,
            ),
            (
                ContentType::LoadoutUnused,
                "loadout_UNUSED",
                "unused",
                ContentKind::Unused,
            ),
            (
                ContentType::TypeIdUnused,
                "typeid_UNUSED",
                "unused",
                ContentKind::Unused,
            ),
            (ContentType::Error, "error", "unused", ContentKind::Error),
            (
                ContentType::Planet,
                "planet",
                "planets",
                ContentKind::Planet,
            ),
            (
                ContentType::AmmoUnused,
                "ammo_UNUSED",
                "unused",
                ContentKind::Unused,
            ),
            (ContentType::Team, "team", "teams", ContentKind::Team),
            (
                ContentType::UnitCommand,
                "unitCommand",
                "unitCommands",
                ContentKind::UnitCommand,
            ),
            (
                ContentType::UnitStance,
                "unitStance",
                "unitStances",
                ContentKind::UnitStance,
            ),
        ];

        assert_eq!(ContentType::ALL.len(), 18);
        assert_eq!(golden.len(), ContentType::ALL.len());
        for (index, (ty, name, folder, kind)) in golden.iter().enumerate() {
            assert_eq!(ty.ordinal(), index, "ordinal mismatch for {name}");
            assert_eq!(ty.name(), *name);
            assert_eq!(ty.folder(), *folder);
            assert_eq!(ty.kind(), *kind);
            assert_eq!(ContentType::ALL[index], *ty);
        }
    }

    /// `content_framework::dense_ids` (plan 02 §7a): `log_content()` passes for
    /// all types after base content.
    #[test]
    fn dense_ids() {
        let registry = test_support::test_registry();
        registry.log_content().unwrap();
        assert_eq!(registry.items().len(), 22);
        assert_eq!(registry.liquids().len(), 11);
        assert_eq!(registry.statuses().len(), 23);
        // M5: 6 internal bullets + 106 `UnitTypes.java` weapon bullets (turret
        // ammo from `Blocks.java` lands with plan 10, keeping this an
        // upstream-prefix id space).
        assert_eq!(registry.bullets().len(), 112);
        assert_eq!(registry.units().len(), 65);
    }

    /// Serialization of `ContentType` must match `name()` (used by harness dumps).
    #[test]
    fn content_type_serde_matches_name() {
        for ty in ContentType::ALL {
            let json = serde_json::to_string(&ty).unwrap();
            assert_eq!(json, format!("\"{}\"", ty.name()));
            let back: ContentType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ty);
        }
    }
}
