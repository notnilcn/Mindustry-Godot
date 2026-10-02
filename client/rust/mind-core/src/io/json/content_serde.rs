// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Content serializers with upstream fallbacks (plan 04 §3.6).
//!
//! Ported from the `JsonIO` static initializer in
//! `core/src/mindustry/io/JsonIO.java`: mappable content is written as its
//! **name** and read back through the documented fallback table
//! (`Blocks.air`, `Items.copper`, `Liquids.water`, `UnitTypes.dagger`,
//! `Planets.serpulo`, `SaveFileReader.fallback` renames).
//!
//! Content values are typed IDs (`ContentRef`), so the registry is threaded
//! explicitly through the serializer functions instead of a global instance
//! (plan 02 §2.4.1). The JSON shape types themselves stay registry-free and
//! store names; plan 12 resolves them onto runtime types when applying rules.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::IoResult;
use super::super::save::chunk::map_fallback;
use crate::content::stacks::ItemStack;
use crate::content::{
    Attribute, BlockId, ContentRef, ContentRegistry, ContentType, ItemId, LiquidId, PlanetId, Rgba,
    SectorId, StatusId, TeamEntryId, UnitCommandId, UnitStanceId, UnitTypeId, WeatherId,
};

/// Content name of a reference (`MappableContent.name`); `None` for content
/// kinds without a live registry in this build (units until plan 02 M5).
pub fn content_name(registry: &ContentRegistry, content: ContentRef) -> Option<String> {
    let id = content.id;
    let name = match content.type_ {
        ContentType::Item => registry.item(ItemId::new(id)).map(|r| r.name.as_str()),
        ContentType::Block => registry.block(BlockId::new(id)).map(|r| r.name.as_str()),
        ContentType::Liquid => registry.liquid(LiquidId::new(id)).map(|r| r.name.as_str()),
        ContentType::Status => registry.status(StatusId::new(id)).map(|r| r.name.as_str()),
        ContentType::Weather => registry
            .weather(WeatherId::new(id))
            .map(|r| r.name.as_str()),
        ContentType::Sector => registry.sector(SectorId::new(id)).map(|r| r.name.as_str()),
        ContentType::Planet => registry.planet(PlanetId::new(id)).map(|r| r.name.as_str()),
        ContentType::Team => registry.team(TeamEntryId::new(id)).map(|r| r.name.as_str()),
        ContentType::UnitCommand => registry
            .unit_command(UnitCommandId::new(id))
            .map(|r| r.name.as_str()),
        ContentType::UnitStance => registry
            .unit_stance(UnitStanceId::new(id))
            .map(|r| r.name.as_str()),
        _ => None,
    };
    name.map(str::to_owned)
}

/// `JsonIO` Item read: null/unknown → `Items.copper`.
pub fn item_from_name(registry: &ContentRegistry, name: Option<&str>) -> ItemId {
    name.and_then(|name| registry.item_by_name(name))
        .map(|record| record.id)
        .unwrap_or(ItemId::COPPER)
}

/// `JsonIO` Liquid read: null/unknown → `Liquids.water`.
pub fn liquid_from_name(registry: &ContentRegistry, name: Option<&str>) -> LiquidId {
    name.and_then(|name| registry.liquid_by_name(name))
        .map(|record| record.id)
        .unwrap_or(LiquidId::WATER)
}

/// `JsonIO` Block read: exact name, then the `SaveFileReader.fallback` rename
/// table, else `Blocks.air`.
pub fn block_from_name(registry: &ContentRegistry, name: Option<&str>) -> BlockId {
    let Some(name) = name else {
        return BlockId::AIR;
    };
    if let Some(record) = registry.block_by_name(name) {
        return record.id;
    }
    registry
        .block_by_name(map_fallback(name))
        .map(|record| record.id)
        .unwrap_or(BlockId::AIR)
}

/// `JsonIO` Planet read: null → `None`; unknown → `Planets.serpulo`.
pub fn planet_from_name(registry: &ContentRegistry, name: Option<&str>) -> Option<PlanetId> {
    let name = name?;
    Some(
        registry
            .planet_by_name(name)
            .map(|record| record.id)
            .unwrap_or_else(|| {
                registry
                    .planet_by_name("serpulo")
                    .map(|record| record.id)
                    .unwrap_or(PlanetId::new(0))
            }),
    )
}

/// `JsonIO` UnitType read: null/unknown → `UnitTypes.dagger`.
///
/// The unit registry lands with plan 02 M5; until then every unit name
/// resolves to id `0` and logs a warning when the name is non-empty. The
/// seam is here so the call sites never change.
pub fn unit_from_name(registry: &ContentRegistry, name: Option<&str>) -> UnitTypeId {
    if let Some(name) = name
        && !name.is_empty()
        && registry.type_len(ContentType::Unit) == 0
    {
        log::warn!("unit content registry absent; `{name}` falls back to dagger");
    }
    UnitTypeId::new(0)
}

/// `JsonIO` Weather read: null/unknown → `None` (upstream `getByName`).
pub fn weather_from_name(registry: &ContentRegistry, name: Option<&str>) -> Option<WeatherId> {
    name.and_then(|name| registry.weather_by_name(name))
        .map(|record| record.id)
}

/// `JsonIO` StatusEffect read: null/unknown → `None`.
pub fn status_from_name(registry: &ContentRegistry, name: Option<&str>) -> Option<StatusId> {
    name.and_then(|name| registry.status_by_name(name))
        .map(|record| record.id)
}

/// `JsonIO` SectorPreset read: null/unknown → `None`.
pub fn sector_preset_from_name(registry: &ContentRegistry, name: Option<&str>) -> Option<SectorId> {
    name.and_then(|name| registry.sector_by_name(name))
        .map(|record| record.id)
}

/// `JsonIO` UnlockableContent read: `content.byName` restricted to unlockable
/// kinds.
pub fn unlockable_from_name(registry: &ContentRegistry, name: Option<&str>) -> Option<ContentRef> {
    let content = registry.by_name(name?)?;
    is_unlockable(content.type_).then_some(content)
}

/// Whether a content kind carries `UnlockableContent` semantics upstream.
pub const fn is_unlockable(type_: ContentType) -> bool {
    matches!(
        type_,
        ContentType::Item
            | ContentType::Block
            | ContentType::Liquid
            | ContentType::Status
            | ContentType::Unit
            | ContentType::Sector
            | ContentType::Planet
    )
}

/// `Attribute.name`.
pub const fn attribute_name(attribute: Attribute) -> &'static str {
    attribute.name()
}

/// `Attribute.getOrNull`.
pub fn attribute_from_name(name: &str) -> Option<Attribute> {
    [
        Attribute::Heat,
        Attribute::Spores,
        Attribute::Water,
        Attribute::Light,
    ]
    .into_iter()
    .find(|attribute| attribute.name() == name)
}

/// `Team.get(id)`: ids are bytes (0–255).
pub const fn team_from_id(id: i32) -> u8 {
    id as u8
}

/// `Team.get(id).id` for a serialized team number.
pub const fn team_id(team: u8) -> i32 {
    team as i32
}

/// `Sector` JSON key: `"<planet.name>-<sector.id>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SectorKey(pub String);

impl SectorKey {
    /// Formats `"<planet>-<id>"` (`Sector.write`).
    pub fn format(planet: &str, sector: u16) -> Self {
        Self(format!("{planet}-{sector}"))
    }

    /// Builds a key from a registered planet id + sector index.
    pub fn from_ref(registry: &ContentRegistry, planet: PlanetId, sector: u16) -> Option<Self> {
        registry
            .planet(planet)
            .map(|record| Self::format(&record.name, sector))
    }

    /// Planet name before the last `-` (`Sector.read`).
    pub fn planet(&self) -> Option<&str> {
        let idx = self.0.rfind('-')?;
        Some(&self.0[..idx])
    }

    /// Sector index after the last `-`.
    pub fn sector(&self) -> Option<u16> {
        let idx = self.0.rfind('-')?;
        self.0[idx + 1..].parse().ok()
    }

    /// Resolves to a registered planet id + index.
    pub fn resolve(&self, registry: &ContentRegistry) -> Option<(PlanetId, u16)> {
        let planet = registry.planet_by_name(self.planet()?)?;
        Some((planet.id, self.sector()?))
    }
}

impl fmt::Display for SectorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Arc `Color` JSON form: 8-digit RRGGBBAA hex, lowercase, un-prefixed
/// (`Color.toString()` / `JsonIO` `Color` serializer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorHex(pub Rgba);

impl ColorHex {
    /// `Color.valueOf(hex)`: `RRGGBB` or `RRGGBBAA`, optional `0x`/`#`.
    /// Malformed input degrades to transparent black like the clamped float
    /// parse upstream (no panic).
    pub fn parse(hex: &str) -> Self {
        Self(Rgba::from_hex(hex).unwrap_or(Rgba::CLEAR))
    }
}

impl Default for ColorHex {
    fn default() -> Self {
        Self(Rgba::CLEAR)
    }
}

impl Serialize for ColorHex {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:08x}", self.0.to_rgba8888()))
    }
}

impl<'de> Deserialize<'de> for ColorHex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw))
    }
}

/// `ItemStack` JSON shape: `{"item": name, "amount": n}` (`JsonIO` serializer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonItemStack {
    /// Item name (`Items.copper` when absent on read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// Stack amount.
    #[serde(default)]
    pub amount: i32,
}

impl JsonItemStack {
    /// Writes a stack through the registry (`ItemStack` serializer).
    pub fn from_stack(registry: &ContentRegistry, stack: &ItemStack) -> Self {
        Self {
            item: content_name(registry, ContentRef::item(stack.item)),
            amount: stack.amount,
        }
    }

    /// Reads a stack, applying the `Items.copper` fallback.
    pub fn to_stack(&self, registry: &ContentRegistry) -> ItemStack {
        ItemStack::new(item_from_name(registry, self.item.as_deref()), self.amount)
    }
}

/// `MusicContainer` JSON form: the music name string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MusicContainer(pub String);

impl MusicContainer {
    /// Name of the container.
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// `JsonIO.write`/`read` round-trip helper for a content id set
/// (`ObjectSet<Block>` etc.): writes names in insertion order.
pub fn content_names(
    registry: &ContentRegistry,
    values: impl IntoIterator<Item = ContentRef>,
) -> Vec<String> {
    values
        .into_iter()
        .filter_map(|content| content_name(registry, content))
        .collect()
}

/// Resolves a list of content names of one type, skipping unknown entries with
/// a warning (upstream `content.getByName` returning null + callers dropping).
pub fn content_ids_by_name(
    registry: &ContentRegistry,
    type_: ContentType,
    names: impl IntoIterator<Item = String>,
) -> Vec<u16> {
    names
        .into_iter()
        .filter_map(|name| match registry.get_by_name(type_, &name) {
            Some(content) => Some(content.id),
            None => {
                log::warn!("unknown {type_} name `{name}`; skipped");
                None
            }
        })
        .collect()
}

/// Re-exported for `SectorInfo`-style key/value maps.
pub type NamedMap<T> = indexmap::IndexMap<String, T>;

/// Convenience alias used by rules shapes.
pub type ContentNames = Vec<String>;

/// Result alias for JSON parse helpers.
pub type JsonResult<T> = IoResult<T>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn item_liquid_block_fallbacks_match_jsonio() {
        let registry = test_registry();
        assert_eq!(item_from_name(&registry, Some("copper")), ItemId::COPPER);
        assert_eq!(item_from_name(&registry, Some("nope")), ItemId::COPPER);
        assert_eq!(item_from_name(&registry, None), ItemId::COPPER);

        assert_eq!(liquid_from_name(&registry, Some("water")), LiquidId::WATER);
        assert_eq!(liquid_from_name(&registry, Some("nope")), LiquidId::WATER);

        let wall = registry.block_id("stone-wall").unwrap();
        assert_eq!(block_from_name(&registry, Some("stone-wall")), wall);
        // Legacy fallback table (`SaveFileReader.fallback`): water → shallow-water.
        assert_eq!(
            block_from_name(&registry, Some("water")),
            registry.block_id("shallow-water").unwrap()
        );
        assert_eq!(block_from_name(&registry, Some("nope")), BlockId::AIR);
        assert_eq!(block_from_name(&registry, None), BlockId::AIR);
    }

    #[test]
    fn planet_and_unlockable_fallbacks() {
        let registry = test_registry();
        let serpulo = registry.planet_id("serpulo").unwrap();
        assert_eq!(planet_from_name(&registry, Some("serpulo")), Some(serpulo));
        assert_eq!(planet_from_name(&registry, Some("nope")), Some(serpulo));
        assert_eq!(planet_from_name(&registry, None), None);

        let copper = registry.by_name("copper").unwrap();
        assert_eq!(
            unlockable_from_name(&registry, Some("copper")),
            Some(copper)
        );
        assert_eq!(unlockable_from_name(&registry, Some("nope")), None);
    }

    #[test]
    fn sector_keys_roundtrip() {
        let registry = test_registry();
        let serpulo = registry.planet_id("serpulo").unwrap();
        let key = SectorKey::from_ref(&registry, serpulo, 170).unwrap();
        assert_eq!(key.0, "serpulo-170");
        assert_eq!(key.planet(), Some("serpulo"));
        assert_eq!(key.sector(), Some(170));
        assert_eq!(key.resolve(&registry), Some((serpulo, 170)));

        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"serpulo-170\"");
        assert_eq!(serde_json::from_str::<SectorKey>(&json).unwrap(), key);
        assert_eq!(SectorKey("bogus".into()).sector(), None);
    }

    #[test]
    fn color_hex_matches_arc_tostring() {
        let color = ColorHex(Rgba::new(1.0, 0.5, 0.0, 1.0));
        let json = serde_json::to_string(&color).unwrap();
        // Arc truncates `(int)(255 * 0.5f)` to 0x7f, so half-green is `ff7f00ff`.
        assert_eq!(json, "\"ff7f00ff\"");
        assert_eq!(
            serde_json::from_str::<ColorHex>(&json)
                .unwrap()
                .0
                .to_rgba8888(),
            color.0.to_rgba8888()
        );
        // 6-digit input gets alpha 255.
        assert_eq!(ColorHex::parse("ffd37f").0, Rgba::from_rgba8888(0xffd37fff));
        // Malformed input never panics.
        assert_eq!(ColorHex::parse("nope").0, Rgba::CLEAR);
    }

    #[test]
    fn item_stack_json_shape() {
        let registry = test_registry();
        let stack = ItemStack::new(ItemId::COPPER, 100);
        let json = JsonItemStack::from_stack(&registry, &stack);
        assert_eq!(
            serde_json::to_string(&json).unwrap(),
            "{\"item\":\"copper\",\"amount\":100}"
        );
        let parsed: JsonItemStack =
            serde_json::from_str("{\"item\":\"nope\",\"amount\":3}").unwrap();
        assert_eq!(
            parsed.to_stack(&registry),
            ItemStack::new(ItemId::COPPER, 3)
        );
    }

    #[test]
    fn unit_fallback_is_dagger_id_until_registry_lands() {
        let registry = test_registry();
        // No unit registry in this build: every name maps to the dagger slot.
        assert_eq!(
            unit_from_name(&registry, Some("dagger")),
            UnitTypeId::new(0)
        );
        assert_eq!(unit_from_name(&registry, None), UnitTypeId::new(0));
    }
}
