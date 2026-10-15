// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Logistics, storage, logic and defense family `setStats` bodies (I04 T4).
//!
//! One match arm per Java `setStats` site, each called after the base
//! [`super::block_stats_timed`] body — the same order as the Java
//! `super.setStats()` chain. Java sources:
//! * distribution: `Conveyor.java:56`, `StackConveyor.java:64`, `Duct.java:59`,
//!   `DuctRouter.java:48`, `DuctBridge.java:21`, `DuctJunction.java:41`,
//!   `OverflowDuct.java:41`, `Junction.java:30`, `ItemBridge.java:73`,
//!   `BufferedItemBridge.java:24`, `MassDriver.java:69`,
//!   `DirectionalUnloader.java:59`.
//! * storage: `Unloader.java:61`, `CoreBlock.java:125`.
//! * campaign: `LaunchPad.java:59`, `LandingPad.java:119`.
//! * liquid: `LiquidJunction.java:21`.
//! * logic: `LogicBlock.java:175`, `MemoryBlock.java:30`, `LogicDisplay.java:72`.
//! * defense: `Wall.java:53`, `ShieldWall.java:31`, `MendProjector.java:58`,
//!   `OverdriveProjector.java:63`, `ForceProjector.java:108`,
//!   `RegenProjector.java:91`, `ShockMine.java:41`, `ShockwaveTower.java:51`,
//!   `BuildTurret.java:81`.
//!
//! Deviations (also in the task summary):
//! * `CoreBlock`'s unit panel drops the `?` button and the `console`-settings
//!   name row (the port's `Cell` model has neither a button nor a settings
//!   read); the unit icon + localized name remain.
//! * `ForceProjector`'s coolant booster panel is not ported (it needs the
//!   `consumesLiquid` filter and per-liquid heat math); the shield/range/regen
//!   rows and the phase-fabric item booster are.
//! * `StatValues.itemBoosters` is a local helper here (the port's `values`
//!   module only carries the liquid `speedBoosters` form).

use std::any::Any;

use super::values;
use super::{Cell, Stat, StatCtx, StatUnit, Stats};
use crate::content::BlockId;
use crate::content::registries::blocks::{BlockDef, BlockKind, Consume};
use crate::content::stacks::ItemStack;
use crate::ui::stat_display::auto_fixed;
use crate::world::block_kind_data::BlockKindData;
use crate::world::blocks::defense::behaviors::{
    ForceProjectorBehavior, MendProjectorBehavior, OverdriveProjectorBehavior,
    RegenProjectorBehavior, ShieldWallBehavior, ShockMineBehavior, ShockwaveTowerBehavior,
};
use crate::world::blocks::defense::turrets::behavior::TurretBehavior;
use crate::world::blocks::distribution::{
    ConveyorBehavior, DirectionalUnloaderBehavior, DuctBehavior, DuctBridgeBehavior,
    DuctRouterBehavior, ItemBridgeBehavior, JunctionBehavior, MassDriverBehavior,
    OverflowDuctBehavior, StackConveyorBehavior,
};
use crate::world::blocks::storage::unloader::UnloaderBehavior;

/// The registered behavior as `dyn Any` for a family downcast.
fn behavior_any<'a>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a dyn Any> {
    let instance = ctx.blocks?.get(id)?;
    Some(instance.behavior.as_ref() as &dyn Any)
}

/// A behavior downcast.
fn behavior<'a, T: Any>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a T> {
    behavior_any(ctx, id)?.downcast_ref::<T>()
}

/// The kind data (`BlockKindData`) for the family knobs.
fn kind_data<'a>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a BlockKindData> {
    Some(&ctx.blocks?.get(id)?.kind_data)
}

/// `stats.add(Stat.itemsMoved, value, StatUnit.itemsSecond)`.
fn items_moved(ctx: &StatCtx, stats: &mut Stats, value: f32) {
    stats.add_number(ctx, Stat::ItemsMoved, value, StatUnit::ItemsSecond);
}

/// The projector families' `stats.timePeriod = …` pre-`super.setStats()`
/// assignments (Java: `MendProjector.java:58` `useTime`, `OverdriveProjector
/// .java:63` `useTime`, `ForceProjector.java:108` `phaseUseTime` only with an
/// item consumer, `RegenProjector.java:91` `optionalUseTime`). `None` for
/// every other kind.
pub fn time_period_before_super(ctx: &StatCtx, id: BlockId, def: &BlockDef) -> Option<f32> {
    Some(match def.kind {
        BlockKind::MendProjector => behavior::<MendProjectorBehavior>(ctx, id)?.state.use_time,
        BlockKind::OverdriveProjector => {
            behavior::<OverdriveProjectorBehavior>(ctx, id)?
                .state
                .use_time
        }
        BlockKind::ForceProjector => {
            let projector = behavior::<ForceProjectorBehavior>(ctx, id)?;
            if projector.phase_item.is_some() {
                projector.phase_use_time
            } else {
                -1.0
            }
        }
        BlockKind::RegenProjector => {
            behavior::<RegenProjectorBehavior>(ctx, id)?
                .state
                .optional_use_time
        }
        _ => return None,
    })
}

/// Adds every family row for this block's kind; unknown kinds add nothing.
pub fn logistics_block_stats(ctx: &StatCtx, id: BlockId, def: &BlockDef, stats: &mut Stats) {
    match def.kind {
        // ------------------------------------------------------------- items
        // `Conveyor.setStats` (`Conveyor.java:56`; `ArmoredConveyor extends
        // Conveyor`).
        BlockKind::Conveyor | BlockKind::ArmoredConveyor => {
            if let Some(conveyor) = behavior::<ConveyorBehavior>(ctx, id) {
                items_moved(ctx, stats, conveyor.displayed_speed);
            }
        }
        // `StackConveyor.setStats` (`StackConveyor.java:64`).
        BlockKind::StackConveyor => {
            if let Some(stack) = behavior::<StackConveyorBehavior>(ctx, id) {
                items_moved(
                    ctx,
                    stats,
                    (def.item_capacity as f32 * stack.speed * 60.0).round(),
                );
            }
        }
        // `Duct.setStats` (`Duct.java:59`).
        BlockKind::Duct => {
            if let Some(duct) = behavior::<DuctBehavior>(ctx, id) {
                items_moved(ctx, stats, 60.0 / duct.speed);
            }
        }
        // `DuctRouter.setStats` (`DuctRouter.java:48`).
        BlockKind::DuctRouter => {
            if let Some(router) = behavior::<DuctRouterBehavior>(ctx, id) {
                items_moved(ctx, stats, 60.0 / router.speed * def.item_capacity as f32);
            }
        }
        // `DuctBridge.setStats` (`DuctBridge.java:21`).
        BlockKind::DuctBridge => {
            if let Some(bridge) = behavior::<DuctBridgeBehavior>(ctx, id) {
                items_moved(ctx, stats, 60.0 / bridge.speed);
            }
        }
        // `DuctJunction.setStats` (`DuctJunction.java:41-45`) removes the item
        // capacity, but the class has no vanilla block (mod ClassMap only, like
        // Java); nothing dispatches here. `LiquidJunction` covers the vanilla
        // `remove()` case below.
        // `OverflowDuct.setStats` (`OverflowDuct.java:41`).
        BlockKind::OverflowDuct => {
            if let Some(duct) = behavior::<OverflowDuctBehavior>(ctx, id) {
                items_moved(ctx, stats, 60.0 / duct.speed);
            }
        }
        // `Junction.setStats` (`Junction.java:30`): displayed speed plus the
        // custom `capacity items per side` text.
        BlockKind::Junction => {
            if let Some(junction) = behavior::<JunctionBehavior>(ctx, id) {
                items_moved(ctx, stats, junction.displayed_speed);
            }
            stats.add(
                Stat::ItemCapacity,
                vec![values::text(format!(
                    "{} {} {}",
                    auto_fixed(junction_capacity(ctx, id), 2),
                    ctx.unit_label(StatUnit::Items),
                    ctx.unit_label(StatUnit::PerSide)
                ))],
            );
        }
        // `ItemBridge.setStats` (`ItemBridge.java:73`): `transportTime != 0`.
        BlockKind::ItemBridge | BlockKind::BufferedItemBridge => {
            if let Some(bridge) = behavior::<ItemBridgeBehavior>(ctx, id) {
                if bridge.transport_time != 0.0 {
                    items_moved(ctx, stats, 60.0 / bridge.transport_time);
                }
                // `BufferedItemBridge.setStats` (`:24`): `displayedSpeed`.
                if bridge.displayed_speed > 0.0 {
                    items_moved(ctx, stats, bridge.displayed_speed);
                }
            }
        }
        // `MassDriver.setStats` (`MassDriver.java:69`).
        BlockKind::MassDriver => {
            if let Some(driver) = behavior::<MassDriverBehavior>(ctx, id) {
                stats.add_number(ctx, Stat::ShootRange, driver.range / 8.0, StatUnit::Blocks);
                if driver.reload > 0.0 {
                    stats.add(
                        Stat::Reload,
                        vec![values::text(format!(
                            "{}{} ~ {} {}",
                            auto_fixed(60.0 / driver.reload, 2),
                            ctx.unit_label(StatUnit::PerSecond),
                            auto_fixed(def.item_capacity as f32 * (60.0 / driver.reload), 2),
                            ctx.unit_label(StatUnit::ItemsSecond)
                        ))],
                    );
                }
                stats.add_number(ctx, Stat::ReceiveRate, 60.0, StatUnit::ItemsSecond);
            }
        }
        // `DirectionalUnloader.setStats` (`DirectionalUnloader.java:59`).
        BlockKind::DirectionalUnloader => {
            if let Some(unloader) = behavior::<DirectionalUnloaderBehavior>(ctx, id) {
                stats.add_number(
                    ctx,
                    Stat::Speed,
                    60.0 / unloader.speed,
                    StatUnit::ItemsSecond,
                );
            }
        }
        // ----------------------------------------------------------- storage
        // `Unloader.setStats` (`Unloader.java:61`).
        BlockKind::StorageBlock => {
            if def.name == "unloader"
                && let Some(unloader) = behavior::<UnloaderBehavior>(ctx, id)
            {
                stats.add_number(
                    ctx,
                    Stat::Speed,
                    60.0 / unloader.speed,
                    StatUnit::ItemsSecond,
                );
            }
        }
        // `CoreBlock.setStats` (`CoreBlock.java:125`): the spawned unit panel.
        BlockKind::CoreBlock => {
            if let Some(unit) = def.core_unit {
                stats.add(
                    Stat::UnitType,
                    vec![
                        Cell::Row,
                        Cell::Panel {
                            cells: vec![
                                Cell::Space { w: 10.0 },
                                Cell::Icon {
                                    region: format!("unit-{unit}-ui"),
                                    amount: None,
                                    size: 40,
                                    content: Some(unit.to_owned()),
                                },
                                Cell::Space { w: 10.0 },
                                values::text(format!("  {}", ctx.localized_name("unit", unit))),
                                Cell::Space { w: 10.0 },
                            ],
                        },
                        Cell::Row,
                    ],
                );
            }
        }
        // ---------------------------------------------------------- campaign
        // `LaunchPad.setStats` (`LaunchPad.java:59`).
        BlockKind::LaunchPad => {
            if let Some(BlockKindData::LaunchPad(pad)) = kind_data(ctx, id) {
                stats.add_number(
                    ctx,
                    Stat::LaunchTime,
                    pad.launch_time / 60.0,
                    StatUnit::Seconds,
                );
            }
        }
        // `LandingPad.setStats` (`LandingPad.java:119`).
        BlockKind::LandingPad => {
            if let Some(BlockKindData::LandingPad(pad)) = kind_data(ctx, id) {
                stats.add_number(
                    ctx,
                    Stat::CooldownTime,
                    (pad.cooldown_time + pad.arrival_duration) / 60.0,
                    StatUnit::Seconds,
                );
            }
        }
        // ------------------------------------------------------------ liquid
        // `LiquidJunction.setStats` (`LiquidJunction.java:21`).
        BlockKind::LiquidJunction => stats.remove(Stat::LiquidCapacity),
        // ------------------------------------------------------------- logic
        // `LogicBlock.setStats` (`LogicBlock.java:175`): non-privileged only.
        BlockKind::LogicBlock => {
            if let Some(BlockKindData::Logic(logic)) = kind_data(ctx, id)
                && !logic.privileged
            {
                stats.add_number(ctx, Stat::LinkRange, logic.range / 8.0, StatUnit::Blocks);
                stats.add_number(
                    ctx,
                    Stat::Instructions,
                    logic.ipt as f32 * 60.0,
                    StatUnit::PerSecond,
                );
            }
        }
        // `MemoryBlock.setStats` (`MemoryBlock.java:30`).
        BlockKind::MemoryBlock => {
            if let Some(BlockKindData::Memory(memory)) = kind_data(ctx, id) {
                stats.add_number(
                    ctx,
                    Stat::MemoryCapacity,
                    memory.capacity as f32,
                    StatUnit::None,
                );
            }
        }
        // `LogicDisplay.setStats` (`LogicDisplay.java:72`; the tileable
        // variant extends it).
        BlockKind::LogicDisplay | BlockKind::TileableLogicDisplay => {
            let size = match kind_data(ctx, id) {
                Some(BlockKindData::Display(display)) => Some(display.display_size),
                Some(BlockKindData::TileableDisplay(display)) => Some(display.display_size),
                _ => None,
            };
            if let Some(size) = size {
                let text = size.to_string();
                stats.add(
                    Stat::DisplaySize,
                    vec![values::text(values::format_args(
                        "@x@",
                        &[text.clone(), text],
                    ))],
                );
            }
        }
        // ----------------------------------------------------------- defense
        // `Wall.setStats` (`Wall.java:53`).
        BlockKind::Wall => {
            if let Some(BlockKindData::Wall(wall)) = kind_data(ctx, id) {
                if wall.chance_deflect > 0.0 {
                    stats.add_number(
                        ctx,
                        Stat::BaseDeflectChance,
                        wall.chance_deflect,
                        StatUnit::None,
                    );
                }
                if wall.lightning_chance > 0.0 {
                    stats.add_number(
                        ctx,
                        Stat::LightningChance,
                        wall.lightning_chance * 100.0,
                        StatUnit::Percent,
                    );
                    stats.add_number(
                        ctx,
                        Stat::LightningDamage,
                        wall.lightning_damage,
                        StatUnit::None,
                    );
                }
            }
        }
        // `ShieldWall.setStats` (`ShieldWall.java:31`).
        BlockKind::ShieldWall => {
            if let Some(shield) = behavior::<ShieldWallBehavior>(ctx, id) {
                stats.add_number(
                    ctx,
                    Stat::ShieldHealth,
                    shield.state.shield_health,
                    StatUnit::None,
                );
            }
        }
        // `MendProjector.setStats` (`MendProjector.java:58`).
        BlockKind::MendProjector => {
            if let Some(mender) = behavior::<MendProjectorBehavior>(ctx, id) {
                let state = &mender.state;
                if state.heal_percent > 0.0 {
                    stats.add_number(
                        ctx,
                        Stat::RepairTime,
                        (100.0 / state.heal_percent * state.reload / 60.0) as i32 as f32,
                        StatUnit::Seconds,
                    );
                }
                stats.add_number(ctx, Stat::Range, state.range / 8.0, StatUnit::Blocks);
                if let Some(items) = booster_items(def) {
                    let boost = (state.phase_boost + state.heal_percent)
                        / state.heal_percent.max(f32::EPSILON);
                    stats.remove(Stat::Booster);
                    add_item_boosters(
                        ctx,
                        stats,
                        &format!("{{0}}{}", ctx.unit_label(StatUnit::TimesSpeed)),
                        state.use_time,
                        boost,
                        state.phase_range_boost,
                        items,
                    );
                }
            }
        }
        // `OverdriveProjector.setStats` (`OverdriveProjector.java:63`).
        BlockKind::OverdriveProjector => {
            if let Some(projector) = behavior::<OverdriveProjectorBehavior>(ctx, id) {
                let state = &projector.state;
                stats.add(
                    Stat::SpeedIncrease,
                    vec![values::text(format!(
                        "+{}%",
                        (state.speed_boost * 100.0 - 100.0) as i32
                    ))],
                );
                stats.add_number(ctx, Stat::Range, state.range / 8.0, StatUnit::Blocks);
                stats.add_number(
                    ctx,
                    Stat::ProductionTime,
                    state.use_time / 60.0,
                    StatUnit::Seconds,
                );
                if state.speed_boost_phase > 0.0
                    && let Some(items) = booster_items(def)
                {
                    stats.remove(Stat::Booster);
                    add_item_boosters(
                        ctx,
                        stats,
                        "+{0}%",
                        state.use_time,
                        state.speed_boost_phase * 100.0,
                        state.phase_range_boost,
                        items,
                    );
                }
            }
        }
        // `ForceProjector.setStats` (`ForceProjector.java:108`).
        BlockKind::ForceProjector => {
            if let Some(projector) = behavior::<ForceProjectorBehavior>(ctx, id) {
                let state = &projector.state;
                stats.add_number(ctx, Stat::ShieldHealth, state.shield_health, StatUnit::None);
                stats.add_number(ctx, Stat::Range, state.radius / 8.0, StatUnit::Blocks);
                stats.add_number(
                    ctx,
                    Stat::RegenerationRate,
                    state.cooldown_normal * 60.0,
                    StatUnit::PerSecond,
                );
                stats.add_number(
                    ctx,
                    Stat::CooldownTime,
                    (state.shield_health / state.cooldown_broken_base / 60.0) as i32 as f32,
                    StatUnit::Seconds,
                );
                // The coolant panel is not ported (see the module docs); the
                // phase-fabric item booster is.
                if projector.phase_item.is_some()
                    && let Some(items) = booster_items(def)
                {
                    stats.remove(Stat::Booster);
                    add_item_boosters(
                        ctx,
                        stats,
                        &format!("+{{0}} {}", ctx.unit_label(StatUnit::ShieldHealth)),
                        projector.phase_use_time,
                        state.phase_shield_boost,
                        state.phase_radius_boost,
                        items,
                    );
                }
            }
        }
        // `RegenProjector.setStats` (`RegenProjector.java:91`).
        BlockKind::RegenProjector => {
            if let Some(regen) = behavior::<RegenProjectorBehavior>(ctx, id) {
                let state = &regen.state;
                if state.heal_percent > 0.0 {
                    stats.add_number(
                        ctx,
                        Stat::RepairTime,
                        (1.0 / (state.heal_percent / 100.0) / 60.0) as i32 as f32,
                        StatUnit::Seconds,
                    );
                }
                stats.add_number(ctx, Stat::Range, state.range as f32, StatUnit::Blocks);
                if let Some(items) = booster_items(def) {
                    stats.remove(Stat::Booster);
                    add_item_boosters(
                        ctx,
                        stats,
                        &format!("{{0}}{}", ctx.unit_label(StatUnit::TimesSpeed)),
                        state.optional_use_time,
                        state.optional_multiplier,
                        0.0,
                        items,
                    );
                }
            }
        }
        // `ShockMine.setStats` (`ShockMine.java:41`).
        BlockKind::ShockMine => {
            if let Some(mine) = behavior::<ShockMineBehavior>(ctx, id) {
                let state = &mine.state;
                let text = ctx
                    .bundle
                    .format(
                        "bullet.lightning",
                        &[
                            state.tendrils.to_string().as_str(),
                            auto_fixed(state.damage, 2).as_str(),
                        ],
                    )
                    .replace("[stat]", "[white]");
                stats.add(Stat::Damage, vec![values::text(text)]);
            }
        }
        // `ShockwaveTower.setStats` (`ShockwaveTower.java:51`).
        BlockKind::ShockwaveTower => {
            if let Some(tower) = behavior::<ShockwaveTowerBehavior>(ctx, id) {
                let state = &tower.state;
                stats.add_number(ctx, Stat::Damage, state.bullet_damage, StatUnit::None);
                stats.add_number(ctx, Stat::Range, state.range / 8.0, StatUnit::Blocks);
                if state.reload > 0.0 {
                    stats.add_number(ctx, Stat::Reload, 60.0 / state.reload, StatUnit::PerSecond);
                }
            }
        }
        // `BuildTurret.setStats` (`BuildTurret.java:81`).
        BlockKind::BuildTurret => {
            if let Some(turret) = behavior::<TurretBehavior>(ctx, id) {
                stats.add_percent(ctx, Stat::BuildSpeed, turret.config.build_speed);
            }
        }
        _ => {}
    }
}

/// `Junction.capacity` for the custom item-capacity text.
fn junction_capacity(ctx: &StatCtx, id: BlockId) -> f32 {
    ctx.blocks
        .and_then(|table| table.get(id))
        .map(|instance| instance.def.item_capacity as f32)
        .unwrap_or(0.0)
}

/// The `ConsumeItems` stacks `findConsumer(c -> c instanceof ConsumeItems)`
/// returns (the optional boost consumers on the projectors).
fn booster_items(def: &BlockDef) -> Option<&[ItemStack]> {
    def.consumes.iter().find_map(|spec| match &spec.consume {
        Consume::Items(stacks) if !stacks.is_empty() => Some(stacks.as_slice()),
        _ => None,
    })
}

/// `StatValues.itemBoosters(unit, timePeriod, speedBoost, rangeBoost, items)`
/// (`StatValues.java:628-655`): the gray panel with one `displayItem` row per
/// consumable plus the range/speed boost text.
fn add_item_boosters(
    ctx: &StatCtx,
    stats: &mut Stats,
    unit: &str,
    time_period: f32,
    speed_boost: f32,
    range_boost: f32,
    items: &[ItemStack],
) {
    let mut panel = Vec::new();
    for stack in items {
        panel.push(Cell::Row);
        if time_period < 0.0 {
            panel.extend(values::items(ctx, std::slice::from_ref(stack), true));
        } else {
            panel.extend(values::display_item_timed(
                ctx,
                stack.item,
                stack.amount as f32,
                time_period,
                true,
            ));
        }
    }
    if range_boost != 0.0 {
        panel.push(Cell::Row);
        panel.push(values::text(format!(
            "[lightgray]+[stat]{}[lightgray] {}",
            auto_fixed(range_boost / 8.0, 2),
            ctx.unit_label(StatUnit::Blocks)
        )));
    }
    if speed_boost != 0.0 {
        panel.push(Cell::Row);
        panel.push(values::text(unit.replace(
            "{0}",
            &format!("[stat]{}[lightgray]", auto_fixed(speed_boost, 2)),
        )));
    }
    stats.add(Stat::Booster, vec![Cell::Panel { cells: panel }]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::bundle::parse_properties;
    use crate::content::ContentRef;
    use crate::content::bundle::MemoryBundle;
    use crate::content::registries::create_base_content;
    use crate::content::settings_store::MemoryUnlockStore;
    use crate::ui::content_stats::content_stats;
    use crate::world::block::BlockTable;

    fn registry() -> crate::content::ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        registry.init().expect("init");
        registry.post_init().expect("post_init");
        registry
    }

    /// Minimal bundle (the family labels the tests assert on).
    fn bundle() -> crate::assets::bundle::Bundle {
        crate::assets::bundle::Bundle::from_layers(vec![parse_properties(
            "unit.itemssecond=items/second\nunit.items=items\nunit.perside=per side\n\
             unit.seconds=seconds\nunit.percent=%\nunit.perminute=/min\nunit.timesspeed=x\n\
             unit.blocks=blocks\nstat.itemcapacity=Item Capacity\nstat.itemsmoved=Items Moved\n\
             stat.repairtime=Repair Time\nstat.range=Range\nstat.booster=Booster\n\
             category.items=Items\ncategory.general=General\n",
        )])
    }

    fn ctx<'a>(
        registry: &'a crate::content::ContentRegistry,
        bundle: &'a crate::assets::bundle::Bundle,
        blocks: &'a BlockTable,
    ) -> StatCtx<'a> {
        StatCtx {
            registry,
            bundle,
            blocks: Some(blocks),
            in_game: true,
            planet: Some("serpulo"),
            campaign: false,
        }
    }

    fn row(stats: &Stats, stat: Stat) -> Vec<Vec<Cell>> {
        stats
            .to_map()
            .iter()
            .flat_map(|(_, rows)| rows.iter())
            .find(|(existing, _)| *existing == stat)
            .map(|(_, values)| values.clone())
            .unwrap_or_default()
    }

    /// The `Items Moved` cells' text form (the markup-free value).
    fn items_moved_texts(stats: &Stats) -> Vec<String> {
        row(stats, Stat::ItemsMoved)
            .iter()
            .map(|cells| {
                cells
                    .iter()
                    .map(|cell| match cell {
                        Cell::Text { t } => t.clone(),
                        _ => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect()
    }

    /// The first `Text` cell of the first `Items Moved` row (`values::number`'s
    /// `Strings.autoFixed` number).
    fn items_moved_numbers(stats: &Stats) -> Vec<String> {
        row(stats, Stat::ItemsMoved)
            .iter()
            .filter_map(|cells| match cells.first() {
                Some(Cell::Text { t }) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    /// I04 T4: `Conveyor.setStats` shows `displayedSpeed`
    /// (`Conveyor.java:56`): conveyor 5, titanium-conveyor 10
    /// (`Blocks.java:2074,2082`), formatted through `values::number`
    /// (`Strings.autoFixed`) -> `"5"` / `"10"`.
    #[test]
    fn conveyor_stats() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        for (name, expected) in [("conveyor", "5"), ("titanium-conveyor", "10")] {
            let id = registry.block_id(name).expect(name);
            let stats = content_stats(&ctx, ContentRef::block(id));
            let numbers = items_moved_numbers(&stats);
            assert_eq!(numbers, vec![expected.to_string()], "{name} speed cell");
            assert_eq!(
                items_moved_texts(&stats),
                vec![format!("{expected} items/second")],
                "{name} unit cell"
            );
        }
    }

    /// I04 T4: `MendProjector.setStats` (`MendProjector.java:58-62`):
    /// `repairTime = (int)(100f / healPercent * reload / 60f)` and
    /// `range / tilesize`, for the vanilla mender (healPercent 4, reload 200,
    /// range 40 -> 83 s / 5 blocks).
    #[test]
    fn mender_stats() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry.block_id("mender").expect("mender");
        let stats = content_stats(&ctx, ContentRef::block(id));
        assert_eq!(
            row(&stats, Stat::RepairTime),
            vec![vec![
                Cell::Text { t: "83".into() },
                Cell::Text {
                    t: " seconds".into()
                }
            ]]
        );
        assert_eq!(
            row(&stats, Stat::Range),
            vec![vec![
                Cell::Text { t: "5".into() },
                Cell::Text {
                    t: " blocks".into()
                }
            ]]
        );
    }

    /// `LiquidJunction.setStats` (`LiquidJunction.java:21`) removes the liquid
    /// capacity. (The plan's `duct_junction_has_no_item_capacity` test is not
    /// buildable: `DuctJunction` has no vanilla block in Java or the port —
    /// mod `ClassMap` only — so no content id exists to query; the vanilla
    /// `remove()` case is the liquid junction.)
    #[test]
    fn liquid_junction_has_no_liquid_capacity() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry
            .block_id("liquid-junction")
            .expect("liquid-junction");
        let stats = content_stats(&ctx, ContentRef::block(id));
        assert!(
            row(&stats, Stat::LiquidCapacity).is_empty(),
            "liquid-junction hides the liquid capacity"
        );
    }
}
