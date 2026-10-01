// SPDX-License-Identifier: GPL-3.0-only

//! Typed content IDs.
//!
//! Ported from `core/src/mindustry/ctype/Content.java` (`short id` assigned at
//! construction) and the per-type ID spaces of `ContentType.java`. Per plan 02
//! §2.4.1, cross-content references are typed IDs instead of Java object pointers.
//!
//! IDs are assigned in construction order and are parity/mod ABI: append-only,
//! never reordered (HIGH_LEVEL_PLAN §2.4).

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::registries::blocks::BlockDef;
use super::registries::bullets::BulletDef;
use super::registries::commands::UnitCommandDef;
use super::registries::items::Item;
use super::registries::liquids::Liquid;
use super::registries::planets::PlanetDef;
use super::registries::sectors::SectorPresetDef;
use super::registries::stances::UnitStanceDef;
use super::registries::statuses::StatusEffect;
use super::registries::teams::TeamEntry;
use super::registries::weathers::WeatherDef;

/// Marker types for content kinds whose records land in later milestones.
///
/// M5 replaces the unit marker with the real record type in `registries/units/`;
/// the alias definitions below are the only place that changes.
pub mod markers {
    /// Placeholder for `registries::units::UnitTypeDef` (plan 02 M5).
    pub enum UnitType {}
}

/// A dense, per-`ContentType` content id (`short` upstream), tagged with its
/// record type so IDs never mix across content kinds.
///
/// The tag is phantom-only: layout is exactly `u16` (`#[repr(transparent)]`).
#[repr(transparent)]
pub struct ContentId<T> {
    raw: u16,
    _marker: PhantomData<fn() -> T>,
}

impl<T> ContentId<T> {
    /// Builds an id from its raw `u16` value.
    pub const fn new(raw: u16) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    /// Raw numeric id (`Content.id`).
    pub const fn raw(self) -> u16 {
        self.raw
    }

    /// Raw numeric id; P0-compatible alias for [`ContentId::raw`].
    pub const fn get(self) -> u16 {
        self.raw
    }

    /// Raw id as an index into the record vector.
    pub const fn index(self) -> usize {
        self.raw as usize
    }
}

impl<T> Clone for ContentId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ContentId<T> {}

impl<T> PartialEq for ContentId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for ContentId<T> {}

impl<T> PartialOrd for ContentId<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for ContentId<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl<T> Hash for ContentId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for ContentId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ContentId").field(&self.raw).finish()
    }
}

impl<T> Default for ContentId<T> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<T> Serialize for ContentId<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u16(self.raw)
    }
}

impl<'de, T> Deserialize<'de> for ContentId<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u16::deserialize(deserializer).map(Self::new)
    }
}

/// Id of an item in the item content space (`ContentType.item`).
pub type ItemId = ContentId<Item>;
/// Id of a block in the block content space (`ContentType.block`).
pub type BlockId = ContentId<BlockDef>;
/// Id of a bullet in the bullet content space (`ContentType.bullet`).
pub type BulletId = ContentId<BulletDef>;
/// Id of a liquid in the liquid content space (`ContentType.liquid`).
pub type LiquidId = ContentId<Liquid>;
/// Id of a status effect in the status content space (`ContentType.status`).
pub type StatusId = ContentId<StatusEffect>;
/// Id of a unit type in the unit content space (`ContentType.unit`).
pub type UnitTypeId = ContentId<markers::UnitType>;
/// Id of a weather in the weather content space (`ContentType.weather`).
pub type WeatherId = ContentId<WeatherDef>;
/// Id of a sector preset in the sector content space (`ContentType.sector`).
pub type SectorId = ContentId<SectorPresetDef>;
/// Id of a planet in the planet content space (`ContentType.planet`).
pub type PlanetId = ContentId<PlanetDef>;
/// Id of a team entry in the team content space (`ContentType.team`).
pub type TeamEntryId = ContentId<TeamEntry>;
/// Id of a unit command in the unit command content space.
pub type UnitCommandId = ContentId<UnitCommandDef>;
/// Id of a unit stance in the unit stance content space.
pub type UnitStanceId = ContentId<UnitStanceDef>;

impl BlockId {
    /// The always-present empty block (`air`, id `0`).
    pub const AIR: BlockId = BlockId::new(0);
    /// Upstream `stone-wall` id in `Blocks.java` order (pinned by a test).
    pub const STONE_WALL: BlockId = BlockId::new(79);
}

impl ItemId {
    /// `Items.copper` — the default item for null/unknown references.
    pub const COPPER: ItemId = ItemId::new(0);
}

impl LiquidId {
    /// `Liquids.water` — the default liquid for null/unknown references.
    pub const WATER: LiquidId = LiquidId::new(0);
}

impl StatusId {
    /// `StatusEffects.none` — the default status effect.
    pub const NONE: StatusId = StatusId::new(0);
}
