// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Runtime stats container and bar evaluation
//! (`core/src/mindustry/world/meta/{Stats,StatValues}.java`).
//!
//! Plan 02 owns the stat *data* (`StatSpec`); plan 14 owns display formatting.
//! This module owns the runtime accumulation order and the bar fraction math the
//! renderer/inspector consumes.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BarSpec, ItemStack, LiquidId, StatSpec};

/// Stat category (`StatCat`), declaration order = UI tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum StatCat {
    /// General.
    #[default]
    General,
    /// Power.
    Power,
    /// Liquids.
    Liquids,
    /// Items.
    Items,
    /// Production.
    Production,
    /// Defense.
    Defense,
    /// Function.
    Function,
    /// Turrets.
    Turrets,
}

impl StatCat {
    /// Bundle key suffix (`stat.category.<key>`).
    pub const fn key(self) -> &'static str {
        match self {
            StatCat::General => "general",
            StatCat::Power => "power",
            StatCat::Liquids => "liquids",
            StatCat::Items => "items",
            StatCat::Production => "production",
            StatCat::Defense => "defense",
            StatCat::Function => "function",
            StatCat::Turrets => "turret",
        }
    }
}

/// Runtime stat value.
#[derive(Debug, Clone, PartialEq)]
pub enum StatValue {
    /// Boolean stat.
    Bool(bool),
    /// Numeric stat with a unit key.
    Number {
        /// Value.
        value: f32,
        /// Display unit key.
        unit: &'static str,
    },
    /// Percentage (0..1), optionally per-second.
    Percent {
        /// Value.
        value: f32,
        /// Whether `value` is per second.
        per_second: bool,
    },
    /// Item stacks.
    Items(Vec<ItemStack>),
    /// A liquid amount.
    Liquid {
        /// Liquid.
        liquid: LiquidId,
        /// Amount.
        amount: f32,
        /// Whether per second.
        per_second: bool,
    },
}

/// One runtime stat row.
#[derive(Debug, Clone, PartialEq)]
pub struct StatEntry {
    /// Category tab.
    pub category: StatCat,
    /// Bundle/stat name.
    pub name: String,
    /// Value.
    pub value: StatValue,
}

/// Runtime `Stats` accumulator (`Block.setStats` data lifted to rows).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Rows in insertion order.
    pub entries: Vec<StatEntry>,
    /// Whether category headers are emitted.
    pub use_categories: bool,
    /// Time period in seconds used for per-second formatting (`60`).
    pub time_period: f32,
}

impl Stats {
    /// Creates an empty container with the upstream defaults.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            use_categories: false,
            time_period: 60.0,
        }
    }

    /// Adds a row.
    pub fn add(&mut self, category: StatCat, name: impl Into<String>, value: StatValue) {
        self.entries.push(StatEntry {
            category,
            name: name.into(),
            value,
        });
    }

    /// Lifts plan-02 `StatSpec`s into runtime rows.
    pub fn from_specs(specs: &[StatSpec]) -> Self {
        let mut stats = Self::new();
        for spec in specs {
            match spec {
                StatSpec::Bool { name, value } => {
                    stats.add(StatCat::General, *name, StatValue::Bool(*value));
                }
                StatSpec::Number { name, value, unit } => {
                    stats.add(
                        StatCat::General,
                        *name,
                        StatValue::Number {
                            value: *value,
                            unit,
                        },
                    );
                }
                StatSpec::Percent {
                    name,
                    value,
                    per_second,
                } => {
                    stats.add(
                        StatCat::General,
                        *name,
                        StatValue::Percent {
                            value: *value,
                            per_second: *per_second,
                        },
                    );
                }
                StatSpec::Items { name, stacks } => {
                    stats.add(StatCat::Items, *name, StatValue::Items(stacks.clone()));
                }
                StatSpec::Liquid {
                    name,
                    liquid,
                    amount,
                    per_second,
                } => {
                    stats.add(
                        StatCat::Liquids,
                        *name,
                        StatValue::Liquid {
                            liquid: *liquid,
                            amount: *amount,
                            per_second: *per_second,
                        },
                    );
                }
            }
        }
        stats
    }
}

/// A bar display request (`Block.setBars` → render/UI contract).
#[derive(Debug, Clone, PartialEq)]
pub struct BarDisplay {
    /// Bar identity (parity with `content::BarSpec`).
    pub spec: BarSpec,
    /// Fill fraction `0..=1`.
    pub fraction: f32,
}

/// Evaluates a `BarSpec` against a building entity.
///
/// Bars whose backing module is absent return `None` (Java skips them). The
/// per-kind dynamic bars (craft/progress/drill speed) are supplied by the
/// behavior through [`crate::world::behavior::BuildingBehavior`].
pub fn evaluate_bar(
    world: &World,
    entity: Entity,
    spec: &BarSpec,
    block: &crate::world::block::BlockView<'_>,
) -> Option<f32> {
    use crate::world::modules::{ItemModule, LiquidModule, PowerModule};
    match spec {
        BarSpec::Health => {
            let health = world.get::<crate::entities::comp::Health>(entity)?;
            if health.max_health <= 0.0 {
                return Some(0.0);
            }
            Some((health.health / health.max_health).clamp(0.0, 1.0))
        }
        BarSpec::Power => {
            if !block.has_power() {
                return None;
            }
            world
                .get::<PowerModule>(entity)
                .map(|module| module.status.clamp(0.0, 1.0))
        }
        BarSpec::Items => {
            if !block.has_items() {
                return None;
            }
            world.get::<ItemModule>(entity).map(|module| {
                let capacity = block.item_capacity().max(1) as f32;
                (module.total as f32 / capacity).clamp(0.0, 1.0)
            })
        }
        BarSpec::Liquid => {
            let module = world.get::<LiquidModule>(entity)?;
            let capacity = block.liquid_capacity();
            if capacity <= 0.0 {
                return None;
            }
            Some((module.current() / capacity).clamp(0.0, 1.0))
        }
        BarSpec::Heat | BarSpec::Shield | BarSpec::Custom { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_lift_specs_in_order() {
        let specs = vec![
            StatSpec::Bool {
                name: "full",
                value: true,
            },
            StatSpec::Number {
                name: "range",
                value: 5.0,
                unit: "blocks",
            },
        ];
        let stats = Stats::from_specs(&specs);
        assert_eq!(stats.entries.len(), 2);
        assert_eq!(stats.entries[0].name, "full");
        assert_eq!(
            stats.entries[1].value,
            StatValue::Number {
                value: 5.0,
                unit: "blocks"
            }
        );
    }
}
