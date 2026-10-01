// SPDX-License-Identifier: GPL-3.0-only

//! Starting loadouts (raw schematic base64).
//!
//! Ported from `core/src/mindustry/content/Loadouts.java`. Upstream stores
//! `Schematic` objects, not `Content` (`ContentType.loadout_UNUSED` is a
//! historical slot), so this table is **not** part of the content ID spaces —
//! decode/validation is plan 12 (`Schematics`), and the base64 is preserved
//! byte-for-byte.

use super::super::ContentError;
use super::super::load::ContentRegistry;

/// One campaign starting loadout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadoutDef {
    /// Loadout name (parity ABI).
    pub name: String,
    /// Raw `Schematics.readBase64` payload, byte-for-byte.
    pub schematic_base64: String,
    /// Decoded schematic handle (plan 12).
    pub schematic: Option<()>,
}

impl LoadoutDef {
    /// Creates a raw loadout entry.
    pub fn new(name: &str, schematic_base64: &str) -> Self {
        Self {
            name: name.to_owned(),
            schematic_base64: schematic_base64.to_owned(),
            schematic: None,
        }
    }
}

/// Loads the 4 vanilla starting loadouts in `Loadouts.load()` order.
pub fn load(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    // `LoadoutDef` has no `Content` id; it is stored as a named side table.
    registry.set_loadouts(vec![
        LoadoutDef::new(
            "basicShard",
            "bXNjaAF4nGNgZmBmZmDJS8xNZZDJKCkpKLbS16/MLy0p1UtK1XcNi/Q3cKwwyqkyYOBOSS1OLsosKMnMz2NgYGDLSUxKzSlmYIqOZWTgSs4vStUtzkgsSgFKMYIQkAAAhSEXTA==",
        ),
        LoadoutDef::new(
            "basicFoundation",
            "bXNjaAF4nGNgYWBhZmDJS8xNZWBNSk3MK2bgTkktTi7KLCjJzM9jYGBgy0lMSs0pZmCKjmVk4E/OL0rVTcsvzUtJhMozghCQAACx6RHB",
        ),
        LoadoutDef::new(
            "basicNucleus",
            "bXNjaAF4nA3CwQ2AIBAEwAXFjxRBA1ZkfCDcgwh3BiTG7iUzMDATZvaFYGOK7pPuLpYXa6QWarqfJAxVsGR/Um7Q+6Fgg1TauIdMvQFQgB7wAza8E4M=",
        ),
        LoadoutDef::new(
            "basicBastion",
            "bXNjaAF4nGNgYWBhZmDJS8xNZWBNzMsEUtwpqcXJRZkFJZn5eQyClfmlCin5Cnn5JQqpFZnFJVwMbDmJSak5xQxM0bGMDDzJ+UWpukmJxWDVDAyMIAQkACMdFqE=",
        ),
    ]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;

    #[test]
    fn loadouts_are_opaque_and_ordered() {
        let registry = test_registry();
        let names: Vec<&str> = registry
            .loadouts()
            .iter()
            .map(|loadout| loadout.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "basicShard",
                "basicFoundation",
                "basicNucleus",
                "basicBastion"
            ]
        );
        for loadout in registry.loadouts() {
            assert!(loadout.schematic_base64.starts_with("bXNjaA"));
            assert!(loadout.schematic.is_none(), "plan 12 decodes");
        }
    }
}
