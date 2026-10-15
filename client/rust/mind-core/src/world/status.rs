// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block status computation (`core/src/mindustry/world/meta/BlockStatus.java`).
//!
//! `BlockStatus` drives the "active" pulse on building icons. The pulse uses
//! `state.tick`; this module keeps the computation Godot-free.

use crate::world::block::BlockView;

/// Display status of a placed block (`BlockStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
    /// No status.
    None,
    /// Active (pulsing).
    Active,
    /// No input (red).
    NoInput,
    /// No output (yellow).
    NoOutput,
    /// Disabled by logic (`logicDisable`).
    LogicDisable,
    /// Inactive (`inactive`).
    Inactive,
}

impl BlockStatus {
    /// Selection color for the status (`BlockStatus.color`); 14 reads it.
    pub const fn color(self) -> u32 {
        match self {
            BlockStatus::None => 0xffff_ffff,
            BlockStatus::Active => 0xffff_ffff,
            BlockStatus::NoInput => 0xff1d1d,
            BlockStatus::NoOutput => 0xffd37f,
            BlockStatus::LogicDisable => 0x8a73c6,
            BlockStatus::Inactive => 0xbfbfbf,
        }
    }

    /// The Java `BlockStatus` F6-marker colours (`world/meta/BlockStatus.java`)
    /// as `0xRRGGBBAA`: active `#5ce677`, noOutput `Color.orange` `#ffa500`,
    /// noInput `Pal.remove` `#e55454`, logicDisable `#8a73c6`, inactive and
    /// absent status `Color.lightGray` `#bfbfbf`.
    pub const fn java_color(self) -> u32 {
        match self {
            BlockStatus::Active => 0x5ce6_77ff,
            BlockStatus::NoOutput => 0xffa5_00ff,
            BlockStatus::NoInput => 0xe554_54ff,
            BlockStatus::LogicDisable => 0x8a73_c6ff,
            BlockStatus::Inactive | BlockStatus::None => 0xbfbf_bfff,
        }
    }
}

/// Average of the active/valid draw branches (`BlockStatus.forBlock`).
pub fn block_status(
    block: &BlockView<'_>,
    enabled: bool,
    has_items: bool,
    has_liquids: bool,
    power_ok: bool,
) -> BlockStatus {
    if !enabled {
        return BlockStatus::None;
    }
    // `BlockStatus.forBlock` checks item/liquid/power availability.
    if block.has_items() && !has_items {
        return BlockStatus::NoInput;
    }
    if block.has_liquids() && !has_liquids {
        return BlockStatus::NoInput;
    }
    if block.has_power() && !power_ok {
        return BlockStatus::NoInput;
    }
    if block.is_active() {
        BlockStatus::Active
    } else {
        BlockStatus::None
    }
}

/// `((state.tick / 30f) % 1f) < efficiency` (`BuildingComp.java:651`): the
/// active/noInput pulse of a running building. `state.tick` is a double, so the
/// expression is the fractional position inside the current 30-tick window; a
/// building at 40 % efficiency is `Active` for the first 40 % of every window
/// and `NoInput` for the rest.
pub fn status_pulse(tick: u64, efficiency: f32) -> BlockStatus {
    if ((tick as f32 / 30.0) % 1.0) < efficiency {
        BlockStatus::Active
    } else {
        BlockStatus::NoInput
    }
}

/// `BuildingComp.status()` (`BuildingComp.java:638-652`) with the subclass
/// overrides that matter for `AIController.targetFlagActive`'s flag set
/// (`generator`, `factory`, `core`, `launchPad`, `storage`, `battery`,
/// `turret`, `repair`, `reactor`):
///
/// ```java
/// if(!enabled) return BlockStatus.logicDisable;
/// if(!shouldConsume()) return BlockStatus.noOutput;
/// if(efficiency <= 0 || !productionValid()) return BlockStatus.noInput;
/// return ((state.tick / 30f) % 1f) < efficiency ? BlockStatus.active : BlockStatus.noInput;
/// ```
///
/// Overrides transcribed: `Turret.status()` (no ammo -> noInput,
/// Turret.java:375-379), `Battery.status()` (Battery.java:99-103),
/// `UnitFactory`/`Reconstructor`/`UnitAssembler.status()`
/// (`!team.activateUnitFactories()` -> inactive, mapped to the port's
/// `Inactive`; the enum has no `inactiveUnitFactory` and the target predicate
/// only tests `!= noInput`) and `RepairTurret.status()`
/// (potentialEfficiency 0 -> noInput, RepairTurret.java:226-228).
///
/// Documented deviations: `productionValid()` is always `true` in the vanilla
/// Java block set (BuildingComp.java:765-767, no override) and
/// `block.targetable` has no port field, so both terms are omitted.
pub fn java_block_status(
    world: &bevy_ecs::world::World,
    entity: bevy_ecs::entity::Entity,
    tick: u64,
) -> BlockStatus {
    use crate::entities::comp::Building;
    use crate::entities::comp::TeamComp;

    let Some(building) = world.get::<Building>(entity) else {
        return BlockStatus::None;
    };
    let kind = world
        .get_resource::<crate::world::BlockTable>()
        .and_then(|table| table.instance(building.block))
        .map(|instance| instance.def.kind);
    // `Battery.status()` replaces the base method (Battery.java:99-103):
    // `power.status` is the stored fraction for buffered power modules
    // (`PowerGraph.update`).
    if kind == Some(crate::content::BlockKind::Battery)
        && let Some(power) = world.get::<crate::world::modules::PowerModule>(entity)
    {
        if power.status.abs() <= 0.001 {
            return BlockStatus::NoInput;
        }
        if (power.status - 1.0).abs() <= 0.001 {
            return BlockStatus::Active;
        }
        return BlockStatus::NoOutput;
    }
    // `UnitFactory.status()`/`Reconstructor.status()`/`UnitAssembler.status()`
    // (UnitFactory.java:460-463, Reconstructor.java:269-272,
    // UnitAssembler.java:702-705): factories wait for the team's activation
    // delay (`Team.activateUnitFactories`, Team.java:126-128).
    if matches!(
        kind,
        Some(crate::content::BlockKind::UnitFactory)
            | Some(crate::content::BlockKind::Reconstructor)
            | Some(crate::content::BlockKind::UnitAssembler)
    ) {
        let rules = crate::game::runtime::rules_of(world);
        let team = world.get::<TeamComp>(entity).map_or(0, |comp| comp.team);
        let active = rules.is_none_or(|rules| {
            crate::game::team::Team::get(team).activate_unit_factories(rules, tick)
        });
        if !active {
            return BlockStatus::Inactive;
        }
    }
    // `RepairTurret.status()` (RepairTurret.java:226-228).
    if kind == Some(crate::content::BlockKind::RepairTurret)
        && (building.potential_efficiency - 0.0).abs() <= 0.01
    {
        return BlockStatus::NoInput;
    }
    if !building.enabled {
        return BlockStatus::LogicDisable;
    }
    if !building.should_consume {
        return BlockStatus::NoOutput;
    }
    // `Turret.status()`: `if(enabled && !hasAmmo()) return noInput;`
    // (Turret.java:375-379) — only the `Turret` subclasses; `BaseTurret`
    // (`BuildTurret`, `TractorBeamTurret`) keeps its `activationTimer` rule and
    // `RepairTurret` its own (above).
    if is_ammo_turret(kind) && !crate::world::blocks::defense::turrets::has_ammo(world, entity) {
        return BlockStatus::NoInput;
    }
    if building.efficiency <= 0.0 {
        return BlockStatus::NoInput;
    }
    status_pulse(tick, building.efficiency)
}

/// The port's turret kinds that extend Java `Turret` (and therefore inherit
/// `TurretBuild.status()`'s no-ammo rule, Turret.java:375-379). `BuildTurret`
/// and `TractorBeamTurret` extend `BaseTurret` and `RepairTurret` its own
/// class; all three keep their own status.
fn is_ammo_turret(kind: Option<crate::content::BlockKind>) -> bool {
    matches!(
        kind,
        Some(crate::content::BlockKind::ItemTurret)
            | Some(crate::content::BlockKind::LiquidTurret)
            | Some(crate::content::BlockKind::PowerTurret)
            | Some(crate::content::BlockKind::LaserTurret)
            | Some(crate::content::BlockKind::ContinuousLiquidTurret)
            | Some(crate::content::BlockKind::ContinuousTurret)
            | Some(crate::content::BlockKind::PointDefenseTurret)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_colors_match_upstream() {
        assert_eq!(BlockStatus::NoInput.color(), 0xff1d1d);
        assert_eq!(BlockStatus::NoOutput.color(), 0xffd37f);
    }

    /// F6 marker colours are the Java `BlockStatus` values (T14/PAR-0097).
    #[test]
    fn java_marker_colors_match_blockstatus() {
        assert_eq!(BlockStatus::Active.java_color(), 0x5ce6_77ff);
        assert_eq!(BlockStatus::NoOutput.java_color(), 0xffa5_00ff);
        assert_eq!(BlockStatus::NoInput.java_color(), 0xe554_54ff);
        assert_eq!(BlockStatus::LogicDisable.java_color(), 0x8a73_c6ff);
        assert_eq!(BlockStatus::Inactive.java_color(), 0xbfbf_bfff);
        assert_eq!(BlockStatus::None.java_color(), 0xbfbf_bfff);
    }

    /// I02 T3 / `BuildingComp.status()`: the marker pulses per 30-tick window
    /// (`((state.tick / 30f) % 1f) < efficiency`), it is not always `Active`.
    #[test]
    fn status_pulses_with_efficiency() {
        // 40 % efficiency: active for the first 40 % of the 30-tick window.
        assert_eq!(status_pulse(0, 0.4), BlockStatus::Active);
        assert_eq!(status_pulse(6, 0.4), BlockStatus::Active); // 0.2
        assert_eq!(status_pulse(15, 0.4), BlockStatus::NoInput); // 0.5
        assert_eq!(status_pulse(29, 0.4), BlockStatus::NoInput);
        // The window restarts every 30 ticks.
        assert_eq!(status_pulse(30, 0.4), BlockStatus::Active);
        assert_eq!(status_pulse(45, 0.4), BlockStatus::NoInput);
        assert_eq!(status_pulse(60, 0.4), BlockStatus::Active);
        // Full efficiency never leaves the active arm.
        assert_eq!(status_pulse(17, 1.0), BlockStatus::Active);
        assert_eq!(status_pulse(999, 1.0), BlockStatus::Active);
    }

    fn place(
        harness: &mut crate::world::harness::BuildHarness,
        x: i32,
        y: i32,
        name: &str,
    ) -> bevy_ecs::entity::Entity {
        let block = harness.content.block_id(name).expect("block");
        assert!(harness.place(x, y, block, 0, true), "placed {name}");
        harness.build_at(x, y).expect("build")
    }

    /// H01 T8: `BuildingComp.status()`'s base rule (`BuildingComp.java:638-652`)
    /// including the `((tick / 30) % 1) < efficiency` pulse.
    #[test]
    fn java_status_follows_building_comp() {
        use crate::entities::comp::Building;
        use crate::world::harness::BuildHarness;

        let mut harness = BuildHarness::new(48, 32, 5);
        let press = place(&mut harness, 10, 10, "graphite-press");
        assert_eq!(
            java_block_status(&harness.world, press, 0),
            BlockStatus::Active
        );
        // Pulse: `(10 / 30) % 1 = 0.333 < 0.5` active; `(20 / 30) = 0.666` noInput.
        harness
            .world
            .get_mut::<Building>(press)
            .expect("building")
            .efficiency = 0.5;
        assert_eq!(
            java_block_status(&harness.world, press, 10),
            BlockStatus::Active
        );
        assert_eq!(
            java_block_status(&harness.world, press, 20),
            BlockStatus::NoInput
        );
        // `efficiency <= 0` -> noInput.
        harness
            .world
            .get_mut::<Building>(press)
            .expect("building")
            .efficiency = 0.0;
        assert_eq!(
            java_block_status(&harness.world, press, 0),
            BlockStatus::NoInput
        );
        // `!shouldConsume()` -> noOutput.
        {
            let mut building = harness.world.get_mut::<Building>(press).expect("building");
            building.efficiency = 1.0;
            building.should_consume = false;
        }
        assert_eq!(
            java_block_status(&harness.world, press, 0),
            BlockStatus::NoOutput
        );
        // `!enabled` -> logicDisable.
        {
            let mut building = harness.world.get_mut::<Building>(press).expect("building");
            building.should_consume = true;
            building.enabled = false;
        }
        assert_eq!(
            java_block_status(&harness.world, press, 0),
            BlockStatus::LogicDisable
        );
    }

    /// H01 T8: `Turret.status()` (`Turret.java:375-379`): no ammo -> noInput.
    #[test]
    fn java_status_empty_turret_is_no_input() {
        use crate::world::harness::BuildHarness;

        let mut harness = BuildHarness::new(48, 32, 5);
        let duo = place(&mut harness, 10, 10, "duo");
        assert_eq!(
            java_block_status(&harness.world, duo, 0),
            BlockStatus::NoInput
        );
    }

    /// H01 T8: `Battery.status()` (`Battery.java:99-103`): the stored fraction
    /// decides (`PowerModule.status` for buffered modules).
    #[test]
    fn java_status_battery_uses_charge() {
        use crate::world::harness::BuildHarness;
        use crate::world::modules::PowerModule;

        let mut harness = BuildHarness::new(48, 32, 5);
        let battery = place(&mut harness, 10, 10, "battery");
        if harness.world.get::<PowerModule>(battery).is_none() {
            harness.world.entity_mut(battery).insert(PowerModule::new());
        }
        let set = |harness: &mut BuildHarness, status: f32| {
            harness
                .world
                .get_mut::<PowerModule>(battery)
                .expect("power")
                .status = status;
        };
        set(&mut harness, 0.0);
        assert_eq!(
            java_block_status(&harness.world, battery, 0),
            BlockStatus::NoInput
        );
        set(&mut harness, 0.5);
        assert_eq!(
            java_block_status(&harness.world, battery, 0),
            BlockStatus::NoOutput
        );
        set(&mut harness, 1.0);
        assert_eq!(
            java_block_status(&harness.world, battery, 0),
            BlockStatus::Active
        );
    }
}
