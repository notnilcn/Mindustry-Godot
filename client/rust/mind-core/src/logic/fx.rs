// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicFx` effect table.
//!
//! Ported from `core/src/mindustry/logic/LogicFx.java`. The ordered table is
//! append-only; `EffectId` resolution is plan 17's registry (by name).

/// One `LogicFx.EffectEntry`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectEntry {
    /// Effect name.
    pub name: &'static str,
    /// Has a size parameter.
    pub size: bool,
    /// Has a rotation parameter.
    pub rotate: bool,
    /// Has a color parameter.
    pub color: bool,
    /// Data parameter class (`"block"`), if any.
    pub data: Option<&'static str>,
    /// Cached bounds, negative if unset.
    pub bounds: f32,
}

impl EffectEntry {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            size: false,
            rotate: false,
            color: false,
            data: None,
            bounds: -1.0,
        }
    }
    const fn size(mut self) -> Self {
        self.size = true;
        self
    }
    const fn rotate(mut self) -> Self {
        self.rotate = true;
        self
    }
    const fn color(mut self) -> Self {
        self.color = true;
        self
    }
    const fn data(mut self, data: &'static str) -> Self {
        self.data = Some(data);
        self
    }
    const fn bounds(mut self, bounds: f32) -> Self {
        self.bounds = bounds;
        self
    }
}

/// The 35 vanilla entries in upstream `LogicFx` order.
#[rustfmt::skip]
pub const EFFECTS: &[EffectEntry] = &[
    EffectEntry::new("warn"),
    EffectEntry::new("cross"),
    EffectEntry::new("blockFall").data("block").bounds(100.0),
    EffectEntry::new("placeBlock").size(),
    EffectEntry::new("placeBlockSpark").size(),
    EffectEntry::new("breakBlock").size(),
    EffectEntry::new("spawn"),
    EffectEntry::new("trail").size().color(),
    EffectEntry::new("breakProp").size().color(),
    EffectEntry::new("smokeCloud").color(),
    EffectEntry::new("vapor").color(),
    EffectEntry::new("hit").color(),
    EffectEntry::new("hitSquare").color(),
    EffectEntry::new("shootSmall").color().rotate(),
    EffectEntry::new("shootBig").color().rotate(),
    EffectEntry::new("smokeSmall").rotate(),
    EffectEntry::new("smokeBig").rotate(),
    EffectEntry::new("smokeColor").rotate().color(),
    EffectEntry::new("smokeSquare").rotate().color(),
    EffectEntry::new("smokeSquareBig").rotate().color(),
    EffectEntry::new("spark").color(),
    EffectEntry::new("sparkBig").color(),
    EffectEntry::new("sparkShoot").rotate().color(),
    EffectEntry::new("sparkShootBig").rotate().color(),
    EffectEntry::new("drill").color(),
    EffectEntry::new("drillBig").color(),
    EffectEntry::new("lightBlock").size().color(),
    EffectEntry::new("explosion").size(),
    EffectEntry::new("smokePuff").color(),
    EffectEntry::new("sparkExplosion").color(),
    EffectEntry::new("crossExplosion").size().color(),
    EffectEntry::new("wave").size().color(),
    EffectEntry::new("bubble"),
];

/// `LogicFx.get(name)`.
pub fn get(name: &str) -> Option<&'static EffectEntry> {
    EFFECTS.iter().find(|entry| entry.name == name)
}

/// `LogicFx.all()`.
pub fn all() -> impl Iterator<Item = &'static EffectEntry> {
    EFFECTS.iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_shape_matches_upstream() {
        // `LogicFx.java` declares 33 entries (the plan text's "35" is a miscount).
        assert_eq!(EFFECTS.len(), 33);
        let block_fall = get("blockFall").unwrap();
        assert_eq!(block_fall.data, Some("block"));
        assert_eq!(block_fall.bounds, 100.0);
        assert!(get("trail").unwrap().color);
        assert!(get("shootBig").unwrap().rotate);
        assert!(get("localeprint").is_none());
    }
}
