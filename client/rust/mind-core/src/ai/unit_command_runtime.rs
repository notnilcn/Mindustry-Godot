// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `UnitCommand` runtime helpers (plan 11 §3.8).
//!
//! The command *metadata* (`UnitCommandDef`) lives in plan 02's
//! `content::registries::commands`; this module is the runtime side consumed by
//! `CommandAI`, `UnitGroup` and plan 15's UI: capability checks, default
//! command selection and the per-unit stance list. Upstream `UnitCommand`
//! exposes `localized`/`getIcon`/`getEmoji`, which are view concerns and stay in
//! plan 15/16.

use crate::content::ContentRegistry;
use crate::content::id::{UnitCommandId, UnitStanceId};
use crate::content::registries::commands::{ControllerKind, UnitCommandDef};
use crate::content::registries::units::UnitTypeDef;

/// Whether `unit` can be assigned `command` (`unit.type.commands.contains`).
pub fn allows_command(unit: &UnitTypeDef, command: UnitCommandId) -> bool {
    unit.commands.contains(&command)
}

/// The command selected when a `CommandAI` initializes (`UnitType.defaultCommand`,
/// falling back to the first capability).
///
/// Mirrors `CommandAI.init()`: `defaultCommand` wins, else the first command in
/// the derived list.
pub fn default_command<'a>(
    unit: &'a UnitTypeDef,
    content: &'a ContentRegistry,
) -> Option<&'a UnitCommandDef> {
    let id = unit
        .default_command
        .or_else(|| unit.commands.first().copied())?;
    content.unit_command(id)
}

/// The controller kind a command installs (`UnitCommand.controller`).
pub fn command_controller(command: &UnitCommandDef) -> ControllerKind {
    command.controller
}

/// Extra stances granted while `command` is selected (`UnitCommand.extraStances`).
pub fn extra_stances(command: &UnitCommandDef) -> &[UnitStanceId] {
    &command.extra_stances
}

/// Fills `out` with the stances a unit can use (`UnitType.getUnitStances`).
///
/// Vanilla order: the unit's declared stances in id order, then `mineAuto` for
/// mining units. Item stances (one per ore present on the map) are added by the
/// caller once plan 06's ore index is available.
pub fn get_unit_stances(
    unit: &UnitTypeDef,
    content: &ContentRegistry,
    out: &mut Vec<UnitStanceId>,
) {
    out.clear();
    out.extend_from_slice(&unit.stances);
    out.sort_unstable();
    out.dedup();
    if unit.mine_tier > 0
        && let Some(mine_auto) = content.unit_stance_by_name("mineauto")
    {
        let id = mine_auto.id;
        if let Err(index) = out.binary_search(&id) {
            out.insert(index, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn default_command_falls_back_to_first_capability() {
        let content = content();
        let unit = content.unit_by_name("dagger").expect("dagger");
        let command = default_command(unit, &content).expect("default command");
        assert!(allows_command(unit, command.id));
    }

    #[test]
    fn unit_stances_include_mine_auto_for_miners() {
        let content = content();
        let unit = content.unit_by_name("mono").expect("mono");
        let mut out = Vec::new();
        get_unit_stances(unit, &content, &mut out);
        let mine_auto = content.unit_stance_by_name("mineauto").expect("mineauto");
        if unit.mine_tier > 0 {
            assert!(out.contains(&mine_auto.id));
        }
        // No duplicates and sorted.
        let mut sorted = out.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, out);
    }

    #[test]
    fn command_controller_matches_upstream_table() {
        let content = content();
        let mine = content.unit_command_by_name("mine").expect("mine");
        assert_eq!(command_controller(mine), ControllerKind::Miner);
        let rebuild = content.unit_command_by_name("rebuild").expect("rebuild");
        assert_eq!(command_controller(rebuild), ControllerKind::Builder);
        // The builder commands expose `holdPosition` as an extra stance.
        assert_eq!(extra_stances(rebuild).len(), 1);
    }
}
