// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building configuration model (`core/src/mindustry/world/Block.java`
//! `config(...)`/`configured(...)` and `meta/ConfigValue`).
//!
//! Ported from the `Object` config path: values are the
//! `Number|Boolean|Content` network whitelist plus the save-only kinds
//! (`Point2`, arrays, byte buffers, building references, strings). Handler
//! dispatch is a function table keyed by [`ConfigKind`] so behavior stays
//! data-driven for mods (plan 07 §2.4.3).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::{BlockId, ContentType, ItemId, LiquidId, UnitTypeId};
use crate::io::typeio::{EntityRef, TypeValue};

/// One configuration value (`Object` in Java).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ConfigValue {
    /// No value / `configClear`.
    #[default]
    None,
    /// Content item.
    Item(ItemId),
    /// Content liquid.
    Liquid(LiquidId),
    /// Content block.
    Block(BlockId),
    /// Content unit type.
    Unit(UnitTypeId),
    /// Any content reference (`(type, id)`).
    Content(ContentType, u16),
    /// A packed point (`Point2`).
    Point2(i32, i32),
    /// A packed point array (`Point2[]`).
    Point2Array(SmallVec<[i32; 2]>),
    /// A number.
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// A building reference (`Building`).
    Building(Entity),
    /// An opaque byte buffer (logic configs).
    Bytes(SmallVec<[u8; 32]>),
    /// A string.
    String(String),
}

impl ConfigValue {
    /// Runtime kind tag (handler dispatch + network whitelist checks).
    pub fn kind(&self) -> ConfigKind {
        match self {
            ConfigValue::None => ConfigKind::None,
            ConfigValue::Item(_) => ConfigKind::Item,
            ConfigValue::Liquid(_) => ConfigKind::Liquid,
            ConfigValue::Block(_) => ConfigKind::Block,
            ConfigValue::Unit(_) => ConfigKind::Unit,
            ConfigValue::Content(..) => ConfigKind::Content,
            ConfigValue::Point2(..) => ConfigKind::Point,
            ConfigValue::Point2Array(_) => ConfigKind::PointArray,
            ConfigValue::Number(_) => ConfigKind::Number,
            ConfigValue::Bool(_) => ConfigKind::Bool,
            ConfigValue::Building(_) => ConfigKind::Building,
            ConfigValue::Bytes(_) => ConfigKind::Bytes,
            ConfigValue::String(_) => ConfigKind::String,
        }
    }

    /// Whether the value is `None` (a clear operation).
    pub fn is_none(&self) -> bool {
        matches!(self, ConfigValue::None)
    }

    /// Whether the value is allowed in a network plan queue
    /// (`Number|Boolean|Content`; plan 07 §6.3).
    pub fn network_allowed(&self) -> bool {
        matches!(
            self,
            ConfigValue::None
                | ConfigValue::Item(_)
                | ConfigValue::Liquid(_)
                | ConfigValue::Block(_)
                | ConfigValue::Unit(_)
                | ConfigValue::Content(..)
                | ConfigValue::Number(_)
                | ConfigValue::Bool(_)
        )
    }
}

/// Configuration value kind (`ConfigKind`); append-only ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ConfigKind {
    /// `None`/clear.
    #[default]
    None,
    /// `Item`.
    Item,
    /// `Liquid`.
    Liquid,
    /// `Block`.
    Block,
    /// `Unit`.
    Unit,
    /// Other `Content`.
    Content,
    /// `Point2`.
    Point,
    /// `Point2[]`.
    PointArray,
    /// Number.
    Number,
    /// Boolean.
    Bool,
    /// `Building`.
    Building,
    /// Byte buffer.
    Bytes,
    /// String.
    String,
}

/// A configuration handler (`Block.configured` closure body).
pub type ConfigureFn = fn(&mut World, Entity, Option<Entity>, &ConfigValue);

/// Dispatches `configure(e, value)` to the block's registered behavior
/// (`Building.configure`). Returns whether the building existed.
pub fn configure(
    world: &mut World,
    entity: Entity,
    player: Option<Entity>,
    value: ConfigValue,
) -> bool {
    let Some(block) = world
        .get::<crate::entities::comp::Building>(entity)
        .map(|building| building.block)
    else {
        return false;
    };
    let Some(inst) = world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return false;
    };
    inst.behavior.configured(world, entity, player, value);
    true
}

/// Reads the current config (`Building.config()`).
pub fn read_config(world: &World, entity: Entity) -> ConfigValue {
    let Some(block) = world
        .get::<crate::entities::comp::Building>(entity)
        .map(|building| building.block)
    else {
        return ConfigValue::None;
    };
    let Some(inst) = world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return ConfigValue::None;
    };
    inst.behavior.config(world, entity)
}

/// Configures a building from a `TypeIO` value (network/plan path).
pub fn configure_type_value(
    world: &mut World,
    entity: Entity,
    player: Option<Entity>,
    value: &TypeValue,
) -> bool {
    let config = type_value_to_config(value);
    configure(world, entity, player, config)
}

/// Lowers a [`ConfigValue`] to plan-04's `TypeIO` [`TypeValue`] (plan 07 §6.3).
///
/// `Building` references keep the raw entity index; the precise id mapping is
/// plan 21's (config-copy is save-only, never network).
pub fn config_to_type_value(value: &ConfigValue) -> TypeValue {
    match value {
        ConfigValue::None => TypeValue::Null,
        ConfigValue::Item(id) => TypeValue::Content(ContentType::Item, id.raw()),
        ConfigValue::Liquid(id) => TypeValue::Content(ContentType::Liquid, id.raw()),
        ConfigValue::Block(id) => TypeValue::Content(ContentType::Block, id.raw()),
        ConfigValue::Unit(id) => TypeValue::Content(ContentType::Unit, id.raw()),
        ConfigValue::Content(content, id) => TypeValue::Content(*content, *id),
        ConfigValue::Point2(x, y) => TypeValue::Point2(*x, *y),
        ConfigValue::Point2Array(points) => {
            TypeValue::Point2Array(points.iter().copied().collect())
        }
        ConfigValue::Number(number) => TypeValue::Double(*number),
        ConfigValue::Bool(flag) => TypeValue::Bool(*flag),
        // Building config-copy is save-only; the entity-id mapping is plan 21's.
        ConfigValue::Building(_entity) => TypeValue::Building(EntityRef::Id(0)),
        ConfigValue::Bytes(bytes) => TypeValue::ByteArray(bytes.to_vec()),
        ConfigValue::String(text) => TypeValue::Str(Some(text.clone())),
    }
}

/// Reads a [`ConfigValue`] back from a `TypeIO` value (plan 07 §6.3).
pub fn type_value_to_config(value: &TypeValue) -> ConfigValue {
    match value {
        TypeValue::Null => ConfigValue::None,
        TypeValue::Content(content, id) => match content {
            ContentType::Item => ConfigValue::Item(ItemId::new(*id)),
            ContentType::Liquid => ConfigValue::Liquid(LiquidId::new(*id)),
            ContentType::Block => ConfigValue::Block(BlockId::new(*id)),
            ContentType::Unit => ConfigValue::Unit(UnitTypeId::new(*id)),
            other => ConfigValue::Content(*other, *id),
        },
        TypeValue::Int(number) => ConfigValue::Number(*number as f64),
        TypeValue::Long(number) => ConfigValue::Number(*number as f64),
        TypeValue::Float(number) => ConfigValue::Number(*number as f64),
        TypeValue::Double(number) => ConfigValue::Number(*number),
        TypeValue::Bool(flag) => ConfigValue::Bool(*flag),
        TypeValue::Point2(x, y) => ConfigValue::Point2(*x, *y),
        TypeValue::Point2Array(points) => {
            ConfigValue::Point2Array(points.iter().copied().collect())
        }
        TypeValue::ByteArray(bytes) => ConfigValue::Bytes(bytes.iter().copied().collect()),
        TypeValue::Str(Some(text)) => ConfigValue::String(text.clone()),
        TypeValue::Str(None) => ConfigValue::None,
        other => {
            let _ = other;
            ConfigValue::None
        }
    }
}

/// Per-block config handler table (`Block.configurations`).
#[derive(Debug, Clone, Default)]
pub struct ConfigHandlers {
    /// Handlers keyed by the accepted value kind.
    pub by_kind: SmallVec<[(ConfigKind, ConfigureFn); 2]>,
    /// `configClear` handler (`configurations.get(null)`).
    pub clear: Option<ConfigureFn>,
}

impl ConfigHandlers {
    /// Looks up the handler for a value kind.
    pub fn handler(&self, kind: ConfigKind) -> Option<ConfigureFn> {
        self.by_kind
            .iter()
            .find(|(candidate, _)| *candidate == kind)
            .map(|(_, handler)| *handler)
    }

    /// Registers a by-kind handler.
    pub fn set(&mut self, kind: ConfigKind, handler: ConfigureFn) {
        if let Some(entry) = self.by_kind.iter_mut().find(|(k, _)| *k == kind) {
            entry.1 = handler;
        } else {
            self.by_kind.push((kind, handler));
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;

    fn set_item(_world: &mut World, _e: Entity, _builder: Option<Entity>, _value: &ConfigValue) {}

    #[test]
    fn config_value_kind_and_network_whitelist() {
        assert_eq!(ConfigValue::Number(1.0).kind(), ConfigKind::Number);
        assert_eq!(ConfigValue::Bool(true).kind(), ConfigKind::Bool);
        assert_eq!(ConfigValue::Item(ItemId::COPPER).kind(), ConfigKind::Item);
        assert_eq!(
            ConfigValue::Content(ContentType::Block, 3).kind(),
            ConfigKind::Content
        );
        assert!(ConfigValue::Number(1.0).network_allowed());
        assert!(ConfigValue::Item(ItemId::COPPER).network_allowed());
        assert!(!ConfigValue::String(String::from("x")).network_allowed());
        assert!(!ConfigValue::Point2(1, 2).network_allowed());
    }

    #[test]
    fn config_value_typeio_roundtrip() {
        let values = vec![
            ConfigValue::None,
            ConfigValue::Item(ItemId::COPPER),
            ConfigValue::Liquid(LiquidId::WATER),
            ConfigValue::Block(BlockId::STONE_WALL),
            ConfigValue::Unit(UnitTypeId::new(3)),
            // Item/Liquid/Block/Unit normalize to their typed variants; a
            // non-typed content kind stays `Content(...)`.
            ConfigValue::Content(ContentType::Status, 2),
            ConfigValue::Point2(-2, 9),
            ConfigValue::Point2Array(smallvec::smallvec![1, 2, 3]),
            ConfigValue::Number(3.5),
            ConfigValue::Bool(true),
            ConfigValue::Bytes(smallvec::smallvec![1, 2, 3]),
            ConfigValue::String(String::from("hi")),
        ];
        for value in values {
            let encoded = config_to_type_value(&value);
            let decoded = type_value_to_config(&encoded);
            assert_eq!(decoded, value, "roundtrip failed for {value:?}");
        }
    }

    #[test]
    fn handler_registration_is_last_write_wins() {
        fn other(_: &mut World, _: Entity, _: Option<Entity>, _: &ConfigValue) {}
        let first: ConfigureFn = set_item;
        let second: ConfigureFn = other;
        let mut handlers = ConfigHandlers::default();
        handlers.set(ConfigKind::Item, first);
        assert!(std::ptr::fn_addr_eq(
            handlers.handler(ConfigKind::Item).expect("item handler"),
            first
        ));
        handlers.set(ConfigKind::Item, second);
        assert!(std::ptr::fn_addr_eq(
            handlers.handler(ConfigKind::Item).expect("item handler"),
            second
        ));
        assert!(handlers.handler(ConfigKind::Liquid).is_none());
    }
}
