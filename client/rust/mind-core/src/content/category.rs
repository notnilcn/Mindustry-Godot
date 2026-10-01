// SPDX-License-Identifier: GPL-3.0-only

//! Block place-menu categories.
//!
//! Ported from `core/src/mindustry/type/Category.java` (10 variants, `prev`/`next`).
//! Plan 02 M3 uses these in `BlockDef`; the enum itself is metadata-only.

use serde::{Deserialize, Serialize};

/// A block place-menu category (`Category`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Offensive turrets.
    Turret = 0,
    /// Blocks that produce raw resources, such as drills.
    Production = 1,
    /// Blocks that move items around.
    Distribution = 2,
    /// Blocks that move liquids around.
    Liquid = 3,
    /// Blocks that generate or transport power.
    Power = 4,
    /// Walls and other defensive structures.
    Defense = 5,
    /// Blocks that craft things.
    Crafting = 6,
    /// Blocks that create units.
    Units = 7,
    /// Things for storage or passive effects.
    Effect = 8,
    /// Blocks related to logic.
    Logic = 9,
}

impl Category {
    /// All variants in declaration order (`Category.all`).
    pub const ALL: [Category; 10] = [
        Category::Turret,
        Category::Production,
        Category::Distribution,
        Category::Liquid,
        Category::Power,
        Category::Defense,
        Category::Crafting,
        Category::Units,
        Category::Effect,
        Category::Logic,
    ];

    /// Ordinal index.
    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// Java enum identifier.
    pub const fn name(self) -> &'static str {
        match self {
            Category::Turret => "turret",
            Category::Production => "production",
            Category::Distribution => "distribution",
            Category::Liquid => "liquid",
            Category::Power => "power",
            Category::Defense => "defense",
            Category::Crafting => "crafting",
            Category::Units => "units",
            Category::Effect => "effect",
            Category::Logic => "logic",
        }
    }

    /// `Category.prev()`.
    pub const fn prev(self) -> Category {
        let index = (self.ordinal() + Category::ALL.len() - 1) % Category::ALL.len();
        Category::ALL[index]
    }

    /// `Category.next()`.
    pub const fn next(self) -> Category {
        let index = (self.ordinal() + 1) % Category::ALL.len();
        Category::ALL[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_order_and_cycles() {
        let golden = [
            "turret",
            "production",
            "distribution",
            "liquid",
            "power",
            "defense",
            "crafting",
            "units",
            "effect",
            "logic",
        ];
        for (index, name) in golden.iter().enumerate() {
            assert_eq!(Category::ALL[index].name(), *name);
            assert_eq!(Category::ALL[index].ordinal(), index);
        }
        assert_eq!(Category::Turret.prev(), Category::Logic);
        assert_eq!(Category::Logic.next(), Category::Turret);
    }
}
