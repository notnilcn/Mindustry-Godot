// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Consumer runtime model (`core/src/mindustry/world/consumers/*`).
//!
//! Plan 02 lowers `BlockDef.consumes` to [`crate::content::ConsumeSpec`]
//! (resolved IDs). This module lowers each spec to a [`ConsumeInstance`] with the
//! exact Java flags (`optional`/`update`/`ignore`) and computes the same
//! partition arrays `Block.java` builds in `init()`:
//!
//! * `consumers` = declaration order;
//! * `optionalConsumers` = `optional && !ignore`;
//! * `nonOptionalConsumers` = `!optional && !ignore`;
//! * `updateConsumers` = `update && !ignore`;
//! * `consPower` = the single `ConsumePower` (a second removes the first).
//!
//! ## Extended consumer kinds (GAPS #58)
//!
//! Only `ConsumeItems`/`ConsumeLiquid`/`ConsumeLiquids`/`ConsumePower`/
//! `ConsumeCoolant` are on the content [`crate::content::Consume`] enum today.
//! The remaining upstream classes (`ConsumeItemFilter` and its trait subclasses,
//! `ConsumeItemList`, `ConsumeItemExplode`, `ConsumeItemDynamic`,
//! `ConsumeLiquidFilter`/`ConsumeLiquidFlammable`, `ConsumeLiquidsDynamic`,
//! `ConsumePayloads`/`ConsumePayloadDynamic`/`ConsumePayloadFilter`,
//! `ConsumePowerDynamic`, `ConsumePowerCondition`) are modelled here as
//! [`ConsumeInstanceKind`] variants and lowered through [`ExtraConsume`] /
//! [`Consumers::build_with_extras`]. The content half only has to append the
//! matching `Consume` variants and call that constructor (see the GAPS #58
//! report); the runtime below is the single dispatch used by
//! [`crate::world::update::update_consumption`].
//!
//! The generic Java bodies are ported in
//! [`runtime_efficiency`]/[`runtime_update`]/[`runtime_trigger`] and the
//! per-instance [`ConsumeInstance`] helpers. `Consume.multiplier` is a
//! per-building function upstream; the port stores the resolved scalar
//! ([`ConsumeInstance::multiplier`]).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::{Consume, ConsumeSpec, ItemId, ItemStack, LiquidId, LiquidStack};
use crate::world::modules::LiquidTraits;

/// `ConsumeItemFilter.filter` (`Boolf<Item>`), resolved to its test data.
///
/// `ConsumeItemEfficiency`/`ConsumeItemList` are the same shape: the explicit
/// item list carries the per-item efficiency multiplier
/// (`ConsumeItemList.itemMultipliers`, default `1`).
#[derive(Debug, Clone, PartialEq)]
pub enum ItemFilter {
    /// `ConsumeItemList` (`itemMultipliers` map) / `ConsumeItemEfficiency` with
    /// an explicit item set, `(item, efficiencyMultiplier)`.
    Items(Vec<(ItemId, f32)>),
    /// `ConsumeItemFlammable.minFlammability`.
    Flammable {
        /// `Item.flammability` threshold.
        min: f32,
    },
    /// `ConsumeItemRadioactive.minRadioactivity`.
    Radioactive {
        /// `Item.radioactivity` threshold.
        min: f32,
    },
    /// `ConsumeItemExplosive.minExplosiveness`.
    Explosive {
        /// `Item.explosiveness` threshold.
        min: f32,
    },
    /// `ConsumeItemCharged.minCharge`.
    Charged {
        /// `Item.charge` threshold.
        min: f32,
    },
}

impl ItemFilter {
    /// `Boolf<Item>.get(item)`; needs the [`ItemTraits`] resource for the trait
    /// filters (a missing resource rejects every item, mirroring
    /// `LiquidTraits`' rejecting default).
    pub fn matches(&self, item: ItemId, traits: Option<&ItemTraits>) -> bool {
        match self {
            ItemFilter::Items(items) => items.iter().any(|(id, _)| *id == item),
            ItemFilter::Flammable { min } => {
                traits.is_some_and(|t| t.flammability_of(item) >= *min)
            }
            ItemFilter::Radioactive { min } => {
                traits.is_some_and(|t| t.radioactivity_of(item) >= *min)
            }
            ItemFilter::Explosive { min } => {
                traits.is_some_and(|t| t.explosiveness_of(item) >= *min)
            }
            ItemFilter::Charged { min } => traits.is_some_and(|t| t.charge_of(item) >= *min),
        }
    }

    /// `ConsumeItemEfficiency.itemEfficiencyMultiplier(item)`.
    pub fn efficiency_multiplier(&self, item: ItemId, traits: Option<&ItemTraits>) -> f32 {
        match self {
            ItemFilter::Items(items) => items
                .iter()
                .find(|(id, _)| *id == item)
                .map(|(_, mult)| *mult)
                .unwrap_or(1.0),
            ItemFilter::Flammable { .. } => traits.map(|t| t.flammability_of(item)).unwrap_or(0.0),
            ItemFilter::Radioactive { .. } => {
                traits.map(|t| t.radioactivity_of(item)).unwrap_or(0.0)
            }
            ItemFilter::Explosive { .. } => traits.map(|t| t.explosiveness_of(item)).unwrap_or(0.0),
            ItemFilter::Charged { .. } => traits.map(|t| t.charge_of(item)).unwrap_or(0.0),
        }
    }
}

/// `ConsumeLiquidBase.consumes(liquid)` filter data (`ConsumeLiquidFilter`,
/// `ConsumeLiquidFlammable`).
#[derive(Debug, Clone, PartialEq)]
pub enum LiquidFilter {
    /// `ConsumeLiquidFilter` with an explicit accepted set.
    Liquids(Vec<LiquidId>),
    /// `ConsumeLiquidFlammable.minFlammability`.
    Flammable {
        /// `Liquid.flammability` threshold.
        min: f32,
    },
}

impl LiquidFilter {
    /// `Boolf<Liquid>.get(liquid)`.
    pub fn matches(&self, liquid: LiquidId, traits: Option<&LiquidTraits>) -> bool {
        match self {
            LiquidFilter::Liquids(liquids) => liquids.contains(&liquid),
            LiquidFilter::Flammable { min } => {
                traits.is_some_and(|t| liquid_flammability(t, liquid) >= *min)
            }
        }
    }

    /// `ConsumeLiquidFlammable.liquidEfficiencyMultiplier(liquid)`.
    pub fn efficiency_multiplier(&self, liquid: LiquidId, traits: Option<&LiquidTraits>) -> f32 {
        match self {
            LiquidFilter::Liquids(_) => 1.0,
            LiquidFilter::Flammable { .. } => traits
                .map(|t| liquid_flammability(t, liquid))
                .unwrap_or(0.0),
        }
    }
}

/// One payload stack (`PayloadStack`: `UnlockableContent` + `int amount`).
///
/// The port's payload inventory is entity-based (each carried payload is an
/// entity with [`crate::world::blocks::payloads::PayloadCarried`]), so the
/// content reference is `(content id, is_block)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadStack {
    /// Block id when `is_block`, unit type id otherwise.
    pub content: u16,
    /// Whether `content` is a block.
    pub is_block: bool,
    /// Required/held count.
    pub count: i32,
}

impl PayloadStack {
    /// A one-payload required stack (`StatValues.stack(content, 1)`).
    pub const fn one(content: u16, is_block: bool) -> Self {
        Self {
            content,
            is_block,
            count: 1,
        }
    }
}

/// Runtime consumer payload (mirrors the `Consume` subclasses).
#[derive(Debug, Clone, PartialEq)]
pub enum ConsumeInstanceKind {
    /// `ConsumeItems`.
    Items(Vec<ItemStack>),
    /// `ConsumeLiquid`/`ConsumeLiquidFilter`.
    Liquid {
        /// Liquid consumed.
        liquid: crate::content::LiquidId,
        /// Amount per tick.
        amount: f32,
    },
    /// `ConsumeLiquids`.
    Liquids(Vec<LiquidStack>),
    /// `ConsumePower` (`buffered > 0` = buffered).
    Power {
        /// Power per tick.
        usage: f32,
        /// Buffered capacity.
        buffered: f32,
    },
    /// `ConsumeCoolant`.
    Coolant {
        /// Amount per tick.
        amount: f32,
        /// Liquid allowed.
        allow_liquid: bool,
        /// Gas allowed.
        allow_gas: bool,
    },
    // --- extended kinds (GAPS #58) ---
    /// `ConsumeItemFilter` + `ConsumeItemEfficiency`/`ConsumeItemList`.
    ItemFilter {
        /// Filter + per-item efficiency multiplier.
        filter: ItemFilter,
    },
    /// `ConsumeItemExplode` (never accepts items; damages the block on intake).
    ItemExplode {
        /// `ConsumeItemExplode.threshold`.
        threshold: f32,
        /// `ConsumeItemExplode.damage`.
        damage: f32,
        /// `ConsumeItemExplode.baseChance`.
        base_chance: f32,
    },
    /// `ConsumeItemDynamic` (`Func<Building, ItemStack[]>`).
    ItemDynamic,
    /// `ConsumeLiquidFilter`/`ConsumeLiquidFlammable`.
    LiquidFilter {
        /// Filter + efficiency multiplier.
        filter: LiquidFilter,
        /// Amount per tick.
        amount: f32,
    },
    /// `ConsumeLiquidsDynamic` (`Func<Building, LiquidStack[]>`).
    LiquidsDynamic,
    /// `ConsumePayloads`.
    Payloads(Vec<PayloadStack>),
    /// `ConsumePayloadDynamic` (`Func<Building, Seq<PayloadStack>>`).
    PayloadDynamic,
    /// `ConsumePayloadFilter` (accepted content set, count ignored).
    PayloadFilter(Vec<PayloadStack>),
    /// `ConsumePowerDynamic` (`Floatf<Building> usage`).
    PowerDynamic {
        /// `displayedPowerUsage` (stats only).
        displayed_usage: f32,
    },
    /// `ConsumePowerCondition` (`Boolf<Building> consume`).
    PowerCondition {
        /// Power per tick while the condition holds.
        usage: f32,
    },
}

/// One lowered consumer (`Consume` + flags).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsumeInstance {
    /// Payload.
    pub kind: ConsumeInstanceKind,
    /// `Consume.optional`.
    pub optional: bool,
    /// `Consume.update`.
    pub update: bool,
    /// `Consume.ignore`.
    pub ignore: bool,
    /// `Consume.boost` (booster consumers scale production).
    pub booster: bool,
    /// `Consume.multiplier` resolved to a scalar (`1` for vanilla consumers).
    pub multiplier: f32,
}

impl ConsumeInstance {
    /// A consumer with the Java `Consume` flag defaults.
    pub fn new(kind: ConsumeInstanceKind) -> Self {
        Self {
            kind,
            optional: false,
            update: true,
            ignore: false,
            booster: false,
            multiplier: 1.0,
        }
    }

    /// `Consume.optional` effective for the partition arrays.
    pub fn partition_optional(&self) -> bool {
        self.optional && !self.ignore
    }

    /// `Consume.ignore()`.
    pub fn is_ignored(&self) -> bool {
        self.ignore
    }

    /// Whether this is a power consumer (`ConsumePower` subclasses included).
    pub fn is_power(&self) -> bool {
        matches!(
            self.kind,
            ConsumeInstanceKind::Power { .. }
                | ConsumeInstanceKind::PowerDynamic { .. }
                | ConsumeInstanceKind::PowerCondition { .. }
        )
    }

    /// Power usage for the power consumer (0 otherwise).
    pub fn power_usage(&self) -> f32 {
        match self.kind {
            ConsumeInstanceKind::Power { usage, .. } => usage,
            ConsumeInstanceKind::PowerCondition { usage } => usage,
            _ => 0.0,
        }
    }

    /// Power buffered capacity (0 otherwise).
    pub fn power_buffered(&self) -> f32 {
        match self.kind {
            ConsumeInstanceKind::Power { buffered, .. } => buffered,
            _ => 0.0,
        }
    }

    /// Whether this consumer's filter accepts `item` for `Block.consumesItem`
    /// (`Consume.apply` sets `Block.itemFilter`).
    ///
    /// `ConsumeItemExplode.apply` is overridden empty upstream, so it never
    /// marks an item as accepted. `ConsumeItemDynamic.apply` only sets
    /// `hasItems`/`acceptsItems`; its per-building stack list is checked by
    /// [`crate::world::behavior::helpers::block_consumes_item`] through the
    /// dynamic source instead.
    pub fn accepts_item(&self, item: ItemId, traits: Option<&ItemTraits>) -> bool {
        match &self.kind {
            ConsumeInstanceKind::Items(stacks) => stacks.iter().any(|stack| stack.item == item),
            ConsumeInstanceKind::ItemFilter { filter } => filter.matches(item, traits),
            _ => false,
        }
    }

    /// Whether this consumer's filter accepts `liquid` for `Block.consumesLiquid`.
    pub fn accepts_liquid(&self, liquid: LiquidId, traits: Option<&LiquidTraits>) -> bool {
        match &self.kind {
            ConsumeInstanceKind::Liquid { liquid: wanted, .. } => *wanted == liquid,
            ConsumeInstanceKind::Liquids(stacks) => {
                stacks.iter().any(|stack| stack.liquid == liquid)
            }
            ConsumeInstanceKind::Coolant {
                allow_liquid,
                allow_gas,
                ..
            } => traits.is_some_and(|t| t.coolant_accepts(liquid, *allow_liquid, *allow_gas)),
            ConsumeInstanceKind::LiquidFilter { filter, .. } => filter.matches(liquid, traits),
            _ => false,
        }
    }

    /// `Consume.efficiencyMultiplier(build)`.
    pub fn efficiency_multiplier(&self, world: &World, entity: Entity) -> f32 {
        match &self.kind {
            ConsumeInstanceKind::ItemFilter { filter } => {
                let Some(item) = consumed_item(world, entity, &|item| {
                    filter.matches(item, world.get_resource::<ItemTraits>())
                }) else {
                    return 0.0;
                };
                filter.efficiency_multiplier(item, world.get_resource::<ItemTraits>())
            }
            ConsumeInstanceKind::LiquidFilter { filter, .. } => {
                let traits = world.get_resource::<LiquidTraits>();
                let Some(liquid) =
                    liquid_consumed(world, entity, &|liquid| filter.matches(liquid, traits))
                else {
                    return 0.0;
                };
                filter.efficiency_multiplier(liquid, traits)
            }
            ConsumeInstanceKind::PowerDynamic { .. }
            | ConsumeInstanceKind::PowerCondition { .. } => 1.0,
            _ => 1.0,
        }
    }
}

/// Per-building data source for the Java `Func<Building, …>` dynamic consumers.
///
/// The port has no content-side closures: families that use a dynamic consumer
/// (`UnitFactory`'s plan items, `PayloadDeconstructor`'s payload list, …)
/// implement this trait over their own state and pass it to the runtime.
pub trait DynamicConsume {
    /// `ConsumeItemDynamic.items.get(build)`.
    fn items(&self, _world: &World, _entity: Entity) -> Vec<ItemStack> {
        Vec::new()
    }
    /// `ConsumeLiquidsDynamic.liquids.get(build)`.
    fn liquids(&self, _world: &World, _entity: Entity) -> Vec<LiquidStack> {
        Vec::new()
    }
    /// `ConsumePayloadDynamic.payloads.get(build)`.
    fn payloads(&self, _world: &World, _entity: Entity) -> Vec<PayloadStack> {
        Vec::new()
    }
    /// `ConsumePowerDynamic.usage.get(build)`.
    fn power(&self, _world: &World, _entity: Entity) -> f32 {
        0.0
    }
    /// `ConsumePowerCondition.consume.get(build)`.
    fn power_condition(&self, _world: &World, _entity: Entity) -> bool {
        true
    }
}

/// Per-item trait table (`Item.flammability`/`radioactivity`/`explosiveness`/
/// `charge`), built once from content (`ItemTraits` mirrors
/// [`LiquidTraits`]).
///
/// A missing resource rejects every trait-filtered item, matching
/// `LiquidTraits`' rejecting default; the live runtime builds it in
/// `Sim::install_block_runtime_from` (GAPS #58 cross-file note).
#[derive(bevy_ecs::prelude::Resource, Debug, Default, Clone, PartialEq)]
pub struct ItemTraits {
    /// `Item.flammability`.
    pub flammability: Vec<f32>,
    /// `Item.radioactivity`.
    pub radioactivity: Vec<f32>,
    /// `Item.explosiveness`.
    pub explosiveness: Vec<f32>,
    /// `Item.charge`.
    pub charge: Vec<f32>,
}

impl ItemTraits {
    /// Builds the per-item vectors from the content registry order.
    pub fn from_content(content: &crate::content::ContentRegistry) -> Self {
        let mut traits = Self::default();
        for item in content.items() {
            traits.flammability.push(item.flammability);
            traits.radioactivity.push(item.radioactivity);
            traits.explosiveness.push(item.explosiveness);
            traits.charge.push(item.charge);
        }
        traits
    }

    /// `Item.flammability` indexed by id (`0` when absent).
    pub fn flammability_of(&self, item: ItemId) -> f32 {
        self.flammability.get(item.index()).copied().unwrap_or(0.0)
    }
    /// `Item.radioactivity` indexed by id (`0` when absent).
    pub fn radioactivity_of(&self, item: ItemId) -> f32 {
        self.radioactivity.get(item.index()).copied().unwrap_or(0.0)
    }
    /// `Item.explosiveness` indexed by id (`0` when absent).
    pub fn explosiveness_of(&self, item: ItemId) -> f32 {
        self.explosiveness.get(item.index()).copied().unwrap_or(0.0)
    }
    /// `Item.charge` indexed by id (`0` when absent).
    pub fn charge_of(&self, item: ItemId) -> f32 {
        self.charge.get(item.index()).copied().unwrap_or(0.0)
    }
}

/// `ConsumeItemExplode` damage events (`Building.damage`), drained by the
/// combat pass (the [`crate::world::behavior::PendingShockMineTriggers`]
/// pattern: `world` never links combat).
#[derive(Debug, Default, bevy_ecs::prelude::Resource)]
pub struct PendingConsumerDamage(pub Vec<(Entity, f32)>);

/// `Liquid.flammability` indexed by id (`0` when absent).
fn liquid_flammability(traits: &LiquidTraits, liquid: LiquidId) -> f32 {
    traits
        .flammability
        .get(liquid.index())
        .copied()
        .unwrap_or(0.0)
}

/// `ConsumeItemFilter.getConsumed(build)` over the item module, in content id
/// order (`ConsumeItemFilter.java:68-77`).
pub fn consumed_item(
    world: &World,
    entity: Entity,
    matches: &dyn Fn(ItemId) -> bool,
) -> Option<ItemId> {
    let module = world.get::<crate::world::modules::ItemModule>(entity)?;
    module
        .items
        .iter()
        .enumerate()
        .find(|(index, amount)| **amount > 0 && matches(ItemId::new(*index as u16)))
        .map(|(index, _)| ItemId::new(index as u16))
}

/// `ConsumeLiquidFilter.getConsumed(build)`: the module's **current** liquid
/// when it matches and has an amount, else the first matching liquid in content
/// order (`ConsumeLiquidFilter.java:57-71`).
pub fn liquid_consumed(
    world: &World,
    entity: Entity,
    matches: &dyn Fn(LiquidId) -> bool,
) -> Option<LiquidId> {
    let module = world.get::<crate::world::modules::LiquidModule>(entity)?;
    super::update::consumed_liquid(module, matches)
}

/// `Building.getPayloads()` contents, as one [`PayloadStack`] per content.
pub fn payload_inventory(world: &World, holder: Entity) -> Vec<PayloadStack> {
    use crate::world::blocks::payloads::PayloadCarried;
    let mut out: Vec<PayloadStack> = Vec::new();
    for entity in world.iter_entities() {
        let Some(carried) = entity.get::<PayloadCarried>() else {
            continue;
        };
        if carried.holder != holder {
            continue;
        }
        let content = payload_content(world, entity.id(), carried.kind);
        let Some(content) = content else {
            continue;
        };
        if let Some(row) = out
            .iter_mut()
            .find(|row| row.content == content.0 && row.is_block == content.1)
        {
            row.count += 1;
        } else {
            out.push(PayloadStack {
                content: content.0,
                is_block: content.1,
                count: 1,
            });
        }
    }
    out
}

/// Content id + block flag of one carried payload entity.
fn payload_content(
    world: &World,
    entity: Entity,
    kind: crate::world::blocks::payloads::PayloadKind,
) -> Option<(u16, bool)> {
    match kind {
        crate::world::blocks::payloads::PayloadKind::Build => world
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| (building.block.get(), true)),
        crate::world::blocks::payloads::PayloadKind::Unit => world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
            .map(|unit| (unit.type_id.get(), false)),
    }
}

/// Whether every `required` stack is present (`PayloadStack[]` contains check).
fn inventory_contains(held: &[PayloadStack], required: &[PayloadStack], multiplier: f32) -> bool {
    required.iter().all(|stack| {
        let amount = (stack.count as f32 * multiplier).round() as i32;
        held.iter().any(|row| {
            row.content == stack.content && row.is_block == stack.is_block && row.count >= amount
        })
    })
}

/// `ConsumePayloads.trigger`/`ConsumePayloadDynamic.trigger`: destroys the
/// required payload entities (`Payload.getPayloads().remove`).
fn remove_payloads(world: &mut World, holder: Entity, required: &[PayloadStack], multiplier: f32) {
    use crate::world::blocks::payloads::{PayloadCarried, PayloadRef, payload_destroyed};
    let mut victims: Vec<PayloadRef> = Vec::new();
    for entity in world.iter_entities() {
        let Some(carried) = entity.get::<PayloadCarried>() else {
            continue;
        };
        if carried.holder != holder {
            continue;
        }
        let Some((content, is_block)) = payload_content(world, entity.id(), carried.kind) else {
            continue;
        };
        let wanted = required
            .iter()
            .find(|stack| stack.content == content && stack.is_block == is_block);
        let Some(wanted) = wanted else {
            continue;
        };
        let amount_so_far = victims
            .iter()
            .filter(|payload| payload.content == content && payload.is_block == is_block)
            .count() as i32;
        if amount_so_far < (wanted.count as f32 * multiplier).round() as i32 {
            victims.push(PayloadRef {
                entity: Some(entity.id()),
                content,
                is_block,
            });
        }
    }
    for payload in victims {
        payload_destroyed(world, payload);
    }
}

/// `Consume.efficiency(build)` for one lowered consumer (GAPS #58 generic
/// dispatch; called by `update_consumption`).
pub fn runtime_efficiency(
    world: &World,
    entity: Entity,
    consumer: &ConsumeInstance,
    pass_delta: f32,
    dynamic: Option<&dyn DynamicConsume>,
) -> f32 {
    use ConsumeInstanceKind as K;
    let mult = consumer.multiplier;
    match &consumer.kind {
        K::Items(stacks) => {
            let Some(module) = world.get::<crate::world::modules::ItemModule>(entity) else {
                return 0.0;
            };
            for stack in stacks {
                if module.get(stack.item) < stack.amount {
                    return 0.0;
                }
            }
            1.0
        }
        K::Liquid { liquid, amount } => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
                return 0.0;
            };
            (module.get(*liquid) / (amount * pass_delta * mult)).min(1.0)
        }
        K::Liquids(stacks) => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
                return 0.0;
            };
            let mut min = 1.0f32;
            for stack in stacks {
                min = min.min(module.get(stack.liquid) / (stack.amount * pass_delta * mult));
            }
            min.min(1.0)
        }
        K::Power { .. } | K::PowerDynamic { .. } | K::PowerCondition { .. } => world
            .get::<crate::world::modules::PowerModule>(entity)
            .map(|module| module.status)
            .unwrap_or(0.0),
        K::Coolant {
            amount,
            allow_liquid,
            allow_gas,
        } => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
                return 0.0;
            };
            let traits = world.get_resource::<LiquidTraits>();
            let consumed = super::update::consumed_liquid(module, &|liquid| {
                traits
                    .is_some_and(|traits| traits.coolant_accepts(liquid, *allow_liquid, *allow_gas))
            });
            match consumed {
                Some(liquid) => (module.get(liquid) / (amount * pass_delta)).min(1.0),
                None => 0.0,
            }
        }
        K::ItemFilter { filter } => {
            let traits = world.get_resource::<ItemTraits>();
            if consumed_item(world, entity, &|item| filter.matches(item, traits)).is_some() {
                1.0
            } else {
                0.0
            }
        }
        // `ConsumeItemExplode.efficiency` returns 1 (`ConsumeItemExplode.java:63-66`).
        K::ItemExplode { .. } => 1.0,
        K::ItemDynamic => {
            let Some(dynamic) = dynamic else {
                return 0.0;
            };
            let Some(module) = world.get::<crate::world::modules::ItemModule>(entity) else {
                return 0.0;
            };
            for stack in dynamic.items(world, entity) {
                if module.get(stack.item) < (stack.amount as f32 * mult).round() as i32 {
                    return 0.0;
                }
            }
            1.0
        }
        K::LiquidFilter { filter, amount } => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let traits = world.get_resource::<LiquidTraits>();
            let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
                return 0.0;
            };
            match liquid_consumed(world, entity, &|liquid| filter.matches(liquid, traits)) {
                Some(liquid) => (module.get(liquid) / (amount * pass_delta * mult)).min(1.0),
                None => 0.0,
            }
        }
        K::LiquidsDynamic => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(dynamic) = dynamic else {
                return 0.0;
            };
            let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
                return 0.0;
            };
            let mut min = 1.0f32;
            for stack in dynamic.liquids(world, entity) {
                min = min.min(module.get(stack.liquid) / (stack.amount * pass_delta * mult));
            }
            min.min(1.0)
        }
        K::Payloads(stacks) => {
            if inventory_contains(&payload_inventory(world, entity), stacks, mult) {
                1.0
            } else {
                0.0
            }
        }
        K::PayloadDynamic => {
            let Some(dynamic) = dynamic else {
                return 0.0;
            };
            if inventory_contains(
                &payload_inventory(world, entity),
                &dynamic.payloads(world, entity),
                mult,
            ) {
                1.0
            } else {
                0.0
            }
        }
        K::PayloadFilter(accepted) => {
            let held = payload_inventory(world, entity);
            if held.iter().any(|row| {
                accepted
                    .iter()
                    .any(|stack| stack.content == row.content && stack.is_block == row.is_block)
            }) {
                1.0
            } else {
                0.0
            }
        }
    }
}

/// `Consume.update(build)` for one lowered consumer (`update_consumption`'s
/// `updateConsumers` pass; `edelta` already includes efficiency).
pub fn runtime_update(
    world: &mut World,
    entity: Entity,
    consumer: &ConsumeInstance,
    edelta: f32,
    dynamic: Option<&dyn DynamicConsume>,
) {
    use ConsumeInstanceKind as K;
    let mult = consumer.multiplier;
    match &consumer.kind {
        // Stack consumers trigger on craft, not per tick (`ConsumeItems.update`
        // is empty; `ConsumeItemDynamic`/payload consumers are trigger-only).
        K::Items(_)
        | K::ItemFilter { .. }
        | K::ItemDynamic
        | K::Payloads(_)
        | K::PayloadDynamic
        | K::PayloadFilter(_)
        | K::Power { .. }
        | K::PowerDynamic { .. }
        | K::PowerCondition { .. } => {}
        K::Liquid { liquid, amount } => {
            if let Some(mut module) = world.get_mut::<crate::world::modules::LiquidModule>(entity) {
                module.remove(*liquid, amount * edelta * mult);
            }
        }
        K::Liquids(stacks) => {
            if let Some(mut module) = world.get_mut::<crate::world::modules::LiquidModule>(entity) {
                for stack in stacks {
                    module.remove(stack.liquid, stack.amount * edelta * mult);
                }
            }
        }
        K::Coolant {
            amount,
            allow_liquid,
            allow_gas,
        } => {
            // Pick under an immutable borrow first (the traits resource and
            // the module cannot be borrowed at once), then remove.
            let consumed = {
                let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
                    return;
                };
                let traits = world.get_resource::<crate::world::modules::LiquidTraits>();
                super::update::consumed_liquid(module, &|liquid| {
                    traits.is_some_and(|traits| {
                        traits.coolant_accepts(liquid, *allow_liquid, *allow_gas)
                    })
                })
            };
            if let (Some(liquid), Some(mut module)) = (
                consumed,
                world.get_mut::<crate::world::modules::LiquidModule>(entity),
            ) {
                module.remove(liquid, amount * edelta);
            }
        }
        K::ItemExplode {
            threshold,
            damage,
            base_chance,
        } => {
            // `ConsumeItemExplode.update` (`ConsumeItemExplode.java:47-58`):
            // `if(rules.reactorExplosions && Mathf.chance(delta * baseChance *
            // clamp(item.explosiveness - threshold))) { build.damage(damage);
            // explodeEffect.at(...); }`
            let traits = world.get_resource::<ItemTraits>();
            let Some(item) = consumed_item(world, entity, &|item| {
                traits.is_some_and(|t| t.explosiveness_of(item) >= *threshold)
            }) else {
                return;
            };
            let explosiveness = traits.map(|t| t.explosiveness_of(item)).unwrap_or(0.0);
            let chance = edelta * base_chance * (explosiveness - threshold).clamp(0.0, 1.0);
            if chance <= 0.0 {
                return;
            }
            let tick = world
                .get_resource::<super::update::BuildClock>()
                .map(|clock| clock.time as u64)
                .unwrap_or(0);
            let seed = entity.to_bits() ^ (tick.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            if crate::math::ArcRand::new(seed).chance(chance as f64) {
                if !world.contains_resource::<PendingConsumerDamage>() {
                    world.insert_resource(PendingConsumerDamage::default());
                }
                if let Some(mut pending) = world.get_resource_mut::<PendingConsumerDamage>() {
                    pending.0.push((entity, *damage));
                }
            }
        }
        K::LiquidFilter { filter, amount } => {
            let traits = world
                .get_resource::<crate::world::modules::LiquidTraits>()
                .cloned();
            let consumed = world
                .get::<crate::world::modules::LiquidModule>(entity)
                .and_then(|module| {
                    super::update::consumed_liquid(module, &|liquid| {
                        filter.matches(liquid, traits.as_ref())
                    })
                });
            if let (Some(liquid), Some(mut module)) = (
                consumed,
                world.get_mut::<crate::world::modules::LiquidModule>(entity),
            ) {
                module.remove(liquid, amount * edelta * mult);
            }
        }
        K::LiquidsDynamic => {
            let Some(dynamic) = dynamic else {
                return;
            };
            let stacks = dynamic.liquids(world, entity);
            if let Some(mut module) = world.get_mut::<crate::world::modules::LiquidModule>(entity) {
                for stack in stacks {
                    module.remove(stack.liquid, stack.amount * edelta * mult);
                }
            }
        }
    }
}

/// `BuildingComp.consume()`'s trigger half for one consumer (`Consume.trigger`).
pub fn runtime_trigger(
    world: &mut World,
    entity: Entity,
    consumer: &ConsumeInstance,
    dynamic: Option<&dyn DynamicConsume>,
) {
    use ConsumeInstanceKind as K;
    let mult = consumer.multiplier;
    match &consumer.kind {
        K::Items(stacks) => {
            if let Some(mut module) = world.get_mut::<crate::world::modules::ItemModule>(entity) {
                for stack in stacks {
                    module.remove(stack.item, (stack.amount as f32 * mult).round() as i32);
                }
            }
        }
        // `ConsumeItemFilter.trigger`: remove one consumed item.
        K::ItemFilter { filter } => {
            let traits = world.get_resource::<ItemTraits>();
            let item = consumed_item(world, entity, &|item| filter.matches(item, traits));
            if let (Some(item), Some(mut module)) = (
                item,
                world.get_mut::<crate::world::modules::ItemModule>(entity),
            ) {
                module.remove(item, 1);
            }
        }
        K::ItemDynamic => {
            let Some(dynamic) = dynamic else {
                return;
            };
            let stacks = dynamic.items(world, entity);
            if let Some(mut module) = world.get_mut::<crate::world::modules::ItemModule>(entity) {
                for stack in stacks {
                    module.remove(stack.item, (stack.amount as f32 * mult).round() as i32);
                }
            }
        }
        K::Payloads(stacks) => remove_payloads(world, entity, stacks, mult),
        K::PayloadDynamic => {
            let Some(dynamic) = dynamic else {
                return;
            };
            let stacks = dynamic.payloads(world, entity);
            remove_payloads(world, entity, &stacks, mult);
        }
        // `ConsumePayloadFilter.trigger`: remove the first fitting payload.
        K::PayloadFilter(accepted) => {
            use crate::world::blocks::payloads::{PayloadCarried, PayloadRef, payload_destroyed};
            let mut victim = None;
            for entity_ref in world.iter_entities() {
                let Some(carried) = entity_ref.get::<PayloadCarried>() else {
                    continue;
                };
                if carried.holder != entity {
                    continue;
                }
                let Some((content, is_block)) =
                    payload_content(world, entity_ref.id(), carried.kind)
                else {
                    continue;
                };
                if accepted
                    .iter()
                    .any(|stack| stack.content == content && stack.is_block == is_block)
                {
                    victim = Some(PayloadRef {
                        entity: Some(entity_ref.id()),
                        content,
                        is_block,
                    });
                    break;
                }
            }
            if let Some(payload) = victim {
                payload_destroyed(world, payload);
            }
        }
        // `ConsumeItemExplode.trigger` is an empty override.
        K::ItemExplode { .. }
        | K::Liquid { .. }
        | K::Liquids(_)
        | K::Coolant { .. }
        | K::LiquidFilter { .. }
        | K::LiquidsDynamic
        | K::Power { .. }
        | K::PowerDynamic { .. }
        | K::PowerCondition { .. } => {}
    }
}

/// Requested power (`ConsumePower.requestedPower` and its subclasses).
pub fn runtime_requested_power(
    world: &World,
    entity: Entity,
    consumer: &ConsumeInstance,
    dynamic: Option<&dyn DynamicConsume>,
) -> f32 {
    use ConsumeInstanceKind as K;
    match &consumer.kind {
        K::Power { usage, buffered } => {
            if *buffered > 0.0 {
                let status = world
                    .get::<crate::world::modules::PowerModule>(entity)
                    .map(|module| module.status)
                    .unwrap_or(0.0);
                (1.0 - status) * buffered
            } else {
                let should = world
                    .get::<crate::entities::comp::Building>(entity)
                    .map(|building| building.should_consume)
                    .unwrap_or(true);
                usage * if should { 1.0 } else { 0.0 }
            }
        }
        K::PowerDynamic { .. } => dynamic
            .map(|dynamic| dynamic.power(world, entity))
            .unwrap_or(0.0),
        K::PowerCondition { usage }
            if dynamic.is_some_and(|dynamic| dynamic.power_condition(world, entity)) =>
        {
            *usage
        }
        _ => 0.0,
    }
}

/// Per-block consumer partition (`Block.java` `init()` consumer arrays).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Consumers {
    /// All consumers in declaration order.
    pub all: Vec<ConsumeInstance>,
    /// Indices of optional, non-ignored consumers.
    pub optional: Vec<usize>,
    /// Indices of non-optional, non-ignored consumers.
    pub non_optional: Vec<usize>,
    /// Indices of updating, non-ignored consumers.
    pub update: Vec<usize>,
    /// Index of the power consumer, if any.
    pub cons_power: Option<usize>,
}

/// One extended consumer payload plus its `Consume` flags (GAPS #58 lowering
/// input; the content half appends its missing `Consume` variants and fills
/// this struct).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtraConsume {
    /// Lowered payload.
    pub kind: ConsumeInstanceKind,
    /// `Consume.optional`.
    pub optional: bool,
    /// `Consume.update`.
    pub update: bool,
    /// `Consume.ignore`.
    pub ignore: bool,
    /// `Consume.boost`.
    pub booster: bool,
    /// `Consume.multiplier` scalar.
    pub multiplier: f32,
}

impl ExtraConsume {
    /// A required, updating extra consumer with the Java flag defaults.
    pub fn new(kind: ConsumeInstanceKind) -> Self {
        Self {
            kind,
            optional: false,
            update: true,
            ignore: false,
            booster: false,
            multiplier: 1.0,
        }
    }

    /// The lowered instance.
    pub fn instance(&self) -> ConsumeInstance {
        ConsumeInstance {
            kind: self.kind.clone(),
            optional: self.optional,
            update: self.update,
            ignore: self.ignore,
            booster: self.booster,
            multiplier: self.multiplier,
        }
    }
}

impl Consumers {
    /// Whether the block has any consumer.
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// Number of consumers.
    pub fn len(&self) -> usize {
        self.all.len()
    }

    /// The single power consumer, if any.
    pub fn power(&self) -> Option<&ConsumeInstance> {
        self.cons_power.and_then(|index| self.all.get(index))
    }

    /// `ConsumePower.usage` (`0` when absent).
    pub fn power_usage(&self) -> f32 {
        self.power()
            .map(ConsumeInstance::power_usage)
            .unwrap_or(0.0)
    }

    /// `ConsumePower.buffered` (`0` when absent).
    pub fn power_buffered(&self) -> f32 {
        self.power()
            .map(ConsumeInstance::power_buffered)
            .unwrap_or(0.0)
    }

    /// `ConsumePower.ignore() == buffered`.
    pub fn buffered_power(&self) -> bool {
        self.power()
            .is_some_and(|consumer| consumer.power_buffered() > 0.0)
    }

    /// Lowers a metadata consumer spec.
    pub fn from_spec(spec: &ConsumeSpec) -> ConsumeInstance {
        let kind = match &spec.consume {
            Consume::Items(stacks) => ConsumeInstanceKind::Items(stacks.clone()),
            Consume::Liquid { liquid, amount } => ConsumeInstanceKind::Liquid {
                liquid: *liquid,
                amount: *amount,
            },
            Consume::Liquids(stacks) => ConsumeInstanceKind::Liquids(stacks.clone()),
            Consume::Power { usage, buffered } => ConsumeInstanceKind::Power {
                usage: *usage,
                buffered: *buffered,
            },
            Consume::Coolant {
                amount,
                allow_liquid,
                allow_gas,
            } => ConsumeInstanceKind::Coolant {
                amount: *amount,
                allow_liquid: *allow_liquid,
                allow_gas: *allow_gas,
            },
        };
        ConsumeInstance {
            kind,
            optional: spec.optional,
            update: spec.update,
            ignore: spec.ignore,
            booster: false,
            multiplier: 1.0,
        }
    }

    /// Builds the partition arrays in the exact `Block.java` order.
    pub fn build(specs: &[ConsumeSpec]) -> Self {
        let all: Vec<ConsumeInstance> = specs.iter().map(Self::from_spec).collect();
        Self::partition(all)
    }

    /// Builds the partition arrays from the content specs **plus** the extended
    /// consumer kinds (GAPS #58). Declaration order is `specs` then `extras`;
    /// the content half must keep the upstream `consume(...)` call order when it
    /// mixes the two.
    pub fn build_with_extras(specs: &[ConsumeSpec], extras: &[ExtraConsume]) -> Self {
        let mut all: Vec<ConsumeInstance> = specs.iter().map(Self::from_spec).collect();
        all.extend(extras.iter().map(ExtraConsume::instance));
        Self::partition(all)
    }

    fn partition(all: Vec<ConsumeInstance>) -> Self {
        let mut optional = Vec::new();
        let mut non_optional = Vec::new();
        let mut update = Vec::new();
        let mut cons_power = None;
        for (index, consumer) in all.iter().enumerate() {
            if consumer.partition_optional() {
                optional.push(index);
            }
            if !consumer.optional && !consumer.ignore {
                non_optional.push(index);
            }
            if consumer.update && !consumer.ignore {
                update.push(index);
            }
            if consumer.is_power() {
                // Registering a second `ConsumePower` removes the first.
                cons_power = Some(index);
            }
        }
        Self {
            all,
            optional,
            non_optional,
            update,
            cons_power,
        }
    }

    /// Effective consumer amount list used by `update_consumption` (all
    /// non-ignored consumers), as `SmallVec` scratch.
    pub fn active_indices(&self) -> SmallVec<[usize; 8]> {
        self.all
            .iter()
            .enumerate()
            .filter(|(_, consumer)| !consumer.ignore)
            .map(|(index, _)| index)
            .collect()
    }

    /// `Block.consumesItem(item)` over the lowered consumers (GAPS #58).
    pub fn consumes_item(&self, item: ItemId, traits: Option<&ItemTraits>) -> bool {
        self.all
            .iter()
            .any(|consumer| consumer.accepts_item(item, traits))
    }

    /// `Block.consumesLiquid(liquid)` over the lowered consumers (GAPS #58).
    pub fn consumes_liquid(&self, liquid: LiquidId, traits: Option<&LiquidTraits>) -> bool {
        self.all
            .iter()
            .any(|consumer| consumer.accepts_liquid(liquid, traits))
    }

    /// Runs the trigger half of every non-ignored consumer (`consume()`).
    pub fn trigger_all(
        &self,
        world: &mut World,
        entity: Entity,
        dynamic: Option<&dyn DynamicConsume>,
    ) {
        for consumer in &self.all {
            if consumer.ignore {
                continue;
            }
            runtime_trigger(world, entity, consumer, dynamic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{BlockId, ContentRegistry};

    fn consumers_of(registry: &ContentRegistry, name: &str) -> Consumers {
        let def = registry
            .block_id(name)
            .and_then(|id| registry.block(id))
            .expect("block");
        Consumers::build(&def.consumes)
    }

    fn test_traits(registry: &ContentRegistry) -> (ItemTraits, LiquidTraits) {
        (
            ItemTraits::from_content(registry),
            LiquidTraits::from_content(registry),
        )
    }

    #[test]
    fn partition_arrays_follow_java_order() {
        let registry = crate::content::test_support::test_registry();
        // `silicon-smelter` consumes coal + sand + power.
        let consumers = consumers_of(&registry, "silicon-smelter");
        assert!(!consumers.all.is_empty());
        assert!(
            consumers
                .all
                .iter()
                .any(|c| matches!(c.kind, ConsumeInstanceKind::Items(_)))
        );
        assert!(consumers.cons_power.is_some());
        assert_eq!(
            consumers.optional.len() + consumers.non_optional.len(),
            consumers.all.len()
        );
    }

    #[test]
    fn second_power_consumer_replaces_first() {
        let specs = vec![
            ConsumeSpec {
                consume: Consume::Power {
                    usage: 1.0,
                    buffered: 0.0,
                },
                optional: false,
                update: true,
                ignore: false,
            },
            ConsumeSpec {
                consume: Consume::Power {
                    usage: 2.0,
                    buffered: 0.0,
                },
                optional: false,
                update: true,
                ignore: false,
            },
        ];
        let consumers = Consumers::build(&specs);
        assert_eq!(consumers.cons_power, Some(1));
        assert_eq!(consumers.power_usage(), 2.0);
        let _ = BlockId::AIR;
    }

    #[test]
    fn extended_power_consumers_join_the_partition() {
        let extras = vec![
            ExtraConsume::new(ConsumeInstanceKind::PowerDynamic {
                displayed_usage: 0.0,
            }),
            ExtraConsume::new(ConsumeInstanceKind::PowerCondition { usage: 2.0 }),
        ];
        let consumers = Consumers::build_with_extras(&[], &extras);
        assert_eq!(consumers.len(), 2);
        // The later power consumer replaces the earlier one (`Block.java`).
        assert_eq!(consumers.cons_power, Some(1));
        assert!(!consumers.buffered_power());
        assert_eq!(consumers.power_usage(), 2.0);
    }

    #[test]
    fn item_filters_match_traits_and_multiply_efficiency() {
        let registry = crate::content::test_support::test_registry();
        let (items, _) = test_traits(&registry);
        let flour = registry.item_id("sand").expect("sand");
        let coal = registry.item_id("coal").expect("coal");
        let filter = ItemFilter::Flammable { min: 0.2 };
        assert!(filter.matches(coal, Some(&items)));
        assert!(!filter.matches(flour, Some(&items)));
        assert_eq!(
            filter.efficiency_multiplier(coal, Some(&items)),
            items.flammability_of(coal)
        );
        // A missing trait table rejects every item (rejecting default).
        assert!(!filter.matches(coal, None));

        let list = ItemFilter::Items(vec![(coal, 1.5)]);
        assert!(list.matches(coal, None));
        assert_eq!(list.efficiency_multiplier(coal, None), 1.5);
        assert!(!list.matches(flour, None));
    }

    #[test]
    fn liquid_flammable_filter_matches() {
        let registry = crate::content::test_support::test_registry();
        let (_, liquids) = test_traits(&registry);
        let oil = registry.liquid_id("oil").expect("oil");
        let water = registry.liquid_id("water").expect("water");
        let filter = LiquidFilter::Flammable { min: 0.2 };
        assert!(filter.matches(oil, Some(&liquids)));
        assert!(!filter.matches(water, Some(&liquids)));
        assert_eq!(
            filter.efficiency_multiplier(oil, Some(&liquids)),
            liquid_flammability(&liquids, oil)
        );
    }
}
