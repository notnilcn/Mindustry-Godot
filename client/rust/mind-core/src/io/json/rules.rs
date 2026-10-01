// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Rules` serde shape (plan 04 §3.6/§6.7; the runtime type is plan 12's).
//!
//! Ported from `core/src/mindustry/game/Rules.java`. Field names stay exactly
//! upstream camelCase (OD9) and every field carries `#[serde(default)]`, so
//! old and new JSON both load (forward/backward tolerance). M2 has the
//! `TypeIO.writeRules` subset; **M6 expands this to the full upstream field
//! set** — expansion is backward compatible by construction.

use serde::{Deserialize, Serialize};

use super::super::StringMap;

/// Match configuration (`mindustry.game.Rules`).
///
/// M2 subset: the fields `ApplicationTests.writeRules` exercises. Plan 12 owns
/// runtime behavior (`mode()`, team rules application, waves).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rules {
    /// Whether the waves team attacks player buildings (`attackMode`).
    pub attack_mode: bool,
    /// Build speed multiplier (`buildSpeedMultiplier`).
    pub build_speed_multiplier: f32,
    /// Arbitrary map rule tags (`tags`; `Map.tags` merge target).
    #[serde(skip_serializing_if = "StringMap::is_empty")]
    pub tags: StringMap,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            attack_mode: false,
            build_speed_multiplier: 1.0,
            tags: StringMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_defaults_and_unknown_field_tolerance() {
        // Missing fields → upstream defaults.
        let rules: Rules = serde_json::from_str("{}").unwrap();
        assert!(!rules.attack_mode);
        assert_eq!(rules.build_speed_multiplier, 1.0);
        // Unknown fields are ignored (forward tolerance).
        let rules: Rules = serde_json::from_str("{\"futureField\":5,\"attackMode\":true}").unwrap();
        assert!(rules.attack_mode);
    }
}
