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

use smallvec::SmallVec;

use crate::content::{Consume, ConsumeSpec, ItemStack, LiquidStack};

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
}

impl ConsumeInstance {
    /// `Consume.optional` effective for the partition arrays.
    pub fn partition_optional(&self) -> bool {
        self.optional && !self.ignore
    }

    /// `Consume.ignore()`.
    pub fn is_ignored(&self) -> bool {
        self.ignore
    }

    /// Whether this is the power consumer.
    pub fn is_power(&self) -> bool {
        matches!(self.kind, ConsumeInstanceKind::Power { .. })
    }

    /// Power usage for the power consumer (0 otherwise).
    pub fn power_usage(&self) -> f32 {
        match self.kind {
            ConsumeInstanceKind::Power { usage, .. } => usage,
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
        }
    }

    /// Builds the partition arrays in the exact `Block.java` order.
    pub fn build(specs: &[ConsumeSpec]) -> Self {
        let all: Vec<ConsumeInstance> = specs.iter().map(Self::from_spec).collect();
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
            if matches!(consumer.kind, ConsumeInstanceKind::Power { .. }) {
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
}
