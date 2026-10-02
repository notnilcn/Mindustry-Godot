// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ClassMap` replacement (plan 20 §3.5): alias + FQCN → class tag resolution.
//!
//! Rust has no runtime reflection, so upstream's generated `ClassMap.java`
//! becomes a committed alias table ([`super::classmap_gen`]) plus runtime block
//! aliases derived from `BlockKind::ALL`. `resolve` mirrors upstream's
//! `ClassMap.get(name)` first-hit behavior with a scope guard: a name registered
//! for a different base class is a mismatch, not a hit.

use super::classmap_gen;

/// Base class a `type`/`template` alias is resolved against (`ClassMap.resolve`
/// `base` argument; upstream passes `Block.class`, `BulletType.class`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassScope {
    /// `mindustry.world.Block`.
    Block,
    /// `mindustry.entities.bullet.BulletType`.
    BulletType,
    /// `mindustry.entities.Effect`.
    Effect,
    /// `mindustry.entities.abilities.Ability`.
    Ability,
    /// `mindustry.graphics.DrawPart`.
    DrawPart,
    /// `mindustry.entities.pattern.ShootPattern`.
    ShootPattern,
    /// `mindustry.entities.units.UnitController` factories.
    UnitController,
    /// `mindustry.type.UnitType`.
    UnitType,
    /// `mindustry.type.Weather`.
    Weather,
    /// `mindustry.type.Liquid`.
    Liquid,
    /// `mindustry.type.StatusEffect`.
    Status,
    /// `mindustry.type.Item`.
    Item,
    /// `mindustry.type.SectorPreset`.
    Sector,
    /// `mindustry.type.Planet`.
    Planet,
    /// `mindustry.type.TeamEntry`.
    Team,
}

/// One alias → class tag mapping.
#[derive(Debug, Clone, Copy)]
pub struct ClassTag {
    /// Alias as written in JSON (`GenericCrafter`, `mindustry.world.…`).
    pub alias: &'static str,
    /// Base class the alias resolves against.
    pub scope: ClassScope,
    /// Resolved kind tag (Java short class name).
    pub tag: &'static str,
}

impl ClassTag {
    /// Const constructor for the generated table.
    pub const fn new(alias: &'static str, scope: ClassScope, tag: &'static str) -> Self {
        Self { alias, scope, tag }
    }
}

/// Alias resolution table (`ClassMap`).
pub struct ClassTagMap {
    entries: Vec<ClassTag>,
}

impl Default for ClassTagMap {
    fn default() -> Self {
        Self::committed()
    }
}

impl ClassTagMap {
    /// Builds the committed table + runtime block aliases.
    pub fn committed() -> Self {
        let mut entries: Vec<ClassTag> = classmap_gen::ENTRIES.to_vec();
        // Block aliases derive from every ported kind (plan 20 §3.5):
        // `<ShortClass>` and `mindustry.world.…<ShortClass>` both resolve.
        for kind in crate::content::BlockKind::ALL {
            let tag = kind.name();
            entries.push(ClassTag::new(tag, ClassScope::Block, tag));
        }
        Self { entries }
    }

    /// Upstream `ClassMap.get`: first alias matching `alias`/`scope`.
    ///
    /// A fully-qualified class name resolves by its final segment, so both
    /// `GenericCrafter` and `mindustry.world.blocks.production.GenericCrafter`
    /// hit. A name registered only for a different scope is not a match.
    pub fn resolve(&self, alias: &str, scope: ClassScope) -> Option<&'static str> {
        if let Some(hit) = self
            .entries
            .iter()
            .find(|entry| entry.alias == alias && entry.scope == scope)
        {
            return Some(hit.tag);
        }
        // FQCN fallback: strip the package and retry (exact short-name match).
        let short = alias.rsplit('.').next().unwrap_or(alias);
        if short != alias
            && let Some(hit) = self
                .entries
                .iter()
                .find(|entry| entry.alias == short && entry.scope == scope)
        {
            return Some(hit.tag);
        }
        None
    }

    /// Whether `alias` resolves under any scope (for the `noClassResolution`
    /// warning: upstream only warns when the name is entirely unknown).
    pub fn is_known(&self, alias: &str) -> bool {
        let short = alias.rsplit('.').next().unwrap_or(alias);
        self.entries
            .iter()
            .any(|entry| entry.alias == alias || entry.alias == short)
    }

    /// Number of entries (test/audit helper).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_alias() {
        let map = ClassTagMap::committed();
        assert_eq!(
            map.resolve("LaserBulletType", ClassScope::BulletType),
            Some("LaserBulletType")
        );
        assert_eq!(
            map.resolve("GenericCrafter", ClassScope::Block),
            Some("GenericCrafter")
        );
    }

    #[test]
    fn fqcn_alias() {
        let map = ClassTagMap::committed();
        assert_eq!(
            map.resolve(
                "mindustry.world.blocks.production.GenericCrafter",
                ClassScope::Block
            ),
            Some("GenericCrafter")
        );
        assert_eq!(
            map.resolve(
                "mindustry.entities.bullet.LaserBulletType",
                ClassScope::BulletType
            ),
            Some("LaserBulletType")
        );
    }

    #[test]
    fn unknown_type_fails() {
        let map = ClassTagMap::committed();
        assert_eq!(map.resolve("NoSuchKind", ClassScope::Block), None);
        assert!(!map.is_known("NoSuchKind"));
    }

    #[test]
    fn scope_mismatch() {
        let map = ClassTagMap::committed();
        // `LaserBulletType` exists, but not as a block.
        assert_eq!(map.resolve("LaserBulletType", ClassScope::Block), None);
        assert!(map.is_known("LaserBulletType"));
    }
}
