// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit-factory and payload-block family `setStats` bodies (I04 T5).
//!
//! Java sources: `units/UnitFactory.java:146` (output plans),
//! `units/Reconstructor.java:92` (production time + upgrade pairs),
//! `units/UnitAssembler.java:206` (plans with time, module tier and payload
//! requirements), `units/UnitAssemblerModule.java:32` (`moduleTier`),
//! `units/RepairTurret.java:71` (`range`/`repairSpeed` + coolant boosters),
//! `units/RepairTower.java:36` (`range`/`repairSpeed`),
//! `payloads/PayloadConveyor.java:62` (`payloadCapacity` squared),
//! `payloads/PayloadMassDriver.java:81` (capacity + reload + shoot range),
//! `payloads/Constructor.java:42` (`output "@x@ ~ @x@"` + build speed).
//!
//! Deviations (also in the task summary):
//! * `isBanned()` has no port field; the banned branch (cancel icon) is not
//!   emitted.
//! * Locked plans render the `lock` icon glyph via `[icon name="lock"]` (the
//!   port's `Cell` model has no icon-font cell).
//! * The assembler's per-payload `StatValues.stack` cells use the content icon
//!   by payload type (unit/block/item), not `Item.emoji()`.

use super::values;
use super::{Cell, Stat, StatCtx, StatUnit, Stats};
use crate::content::registries::blocks::{BlockDef, BlockKind};
use crate::content::{BlockId, ContentRef, ContentType};
use crate::game::tech_tree;
use crate::ui::stat_display::auto_fixed;
use crate::world::blocks::payloads::constructor::ConstructorBehavior;
use crate::world::blocks::payloads::payload_conveyor::PayloadConveyorBehavior;
use crate::world::blocks::payloads::payload_mass_driver::PayloadMassDriverBehavior;
use crate::world::blocks::units::behavior::{
    AssemblerModuleBehavior, RepairTowerBehavior, RepairTurretBehavior,
};

/// The behavior as `dyn Any` for a family downcast.
fn behavior_any<'a>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a dyn std::any::Any> {
    let instance = ctx.blocks?.get(id)?;
    Some(instance.behavior.as_ref() as &dyn std::any::Any)
}

/// A behavior downcast.
fn behavior<'a, T: std::any::Any>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a T> {
    behavior_any(ctx, id)?.downcast_ref::<T>()
}

/// `UnlockableContent.unlockedNow()`: outside a campaign everything is
/// unlocked; in one the `<name>-unlocked` bit (or `alwaysUnlocked`) decides.
fn unlocked_now(ctx: &StatCtx, content: ContentRef) -> bool {
    if !ctx.campaign {
        return true;
    }
    tech_tree::unlock_fields(ctx.registry, content)
        .is_some_and(|fields| fields.unlocked || fields.always_unlocked)
}

/// The `lock` glyph cell for a locked plan (`t.image(Icon.lock)`): the dialog
/// draws an icon-font glyph for `<type>-<name>-ui` regions it cannot resolve
/// (`MindIconEffect`), so `lock`/`right` use the `Cell::Icon` path.
fn lock_cell() -> Cell {
    Cell::Icon {
        region: "lock".into(),
        amount: None,
        size: 40,
        content: None,
    }
}

/// The `Icon.right` arrow between an upgrade pair's two boxes.
fn arrow_cell() -> Cell {
    Cell::Icon {
        region: "right".into(),
        amount: None,
        size: 40,
        content: None,
    }
}

/// A unit's localized name.
fn unit_name<'a>(ctx: &'a StatCtx<'a>, id: crate::content::UnitTypeId) -> Option<&'a str> {
    ctx.registry.unit(id).map(|def| def.name.as_str())
}

/// One unit-factory plan box (`UnitFactory.java:150-185`): 40 px icon, name,
/// `time s` and the item requirements.
fn factory_plan_cells(
    ctx: &StatCtx,
    plan: &crate::content::registries::blocks::UnitPlanDef,
) -> Vec<Cell> {
    let Some(name) = unit_name(ctx, plan.unit) else {
        return vec![lock_cell()];
    };
    let mut cells = vec![
        Cell::Icon {
            region: format!("unit-{name}-ui"),
            amount: None,
            size: 40,
            content: Some(name.to_owned()),
        },
        Cell::Space { w: 10.0 },
        values::text(format!(
            "{}\n[lightgray]{} {}",
            ctx.localized_name("unit", name),
            auto_fixed(plan.time / 60.0, 1),
            ctx.unit_label(StatUnit::Seconds)
        )),
    ];
    for stack in &plan.requirements {
        cells.push(Cell::Space { w: 5.0 });
        cells.extend(values::display_item_timed(
            ctx,
            stack.item,
            stack.amount as f32,
            plan.time,
            true,
        ));
    }
    cells
}

/// One upgrade pair's unit box (`Reconstructor.java:97-121`): 40 px icon +
/// name, left- or right-aligned per side.
fn upgrade_unit_cells(ctx: &StatCtx, id: crate::content::UnitTypeId) -> Vec<Cell> {
    let Some(name) = unit_name(ctx, id) else {
        return vec![lock_cell()];
    };
    vec![
        Cell::Icon {
            region: format!("unit-{name}-ui"),
            amount: None,
            size: 40,
            content: Some(name.to_owned()),
        },
        Cell::Space { w: 10.0 },
        values::text(ctx.localized_name("unit", name)),
    ]
}

/// `StatValues.stack(ItemStack)`-style payload cell: the content icon + amount.
fn payload_cell(ctx: &StatCtx, stack: &crate::content::stacks::PayloadStack) -> Option<Cell> {
    let name = tech_tree::content_name(ctx.registry, stack.item)?;
    let type_name = stack.item.type_.name();
    Some(Cell::Icon {
        region: format!("{type_name}-{name}-ui"),
        amount: Some(stack.amount.to_string()),
        size: 32,
        content: Some(name),
    })
}

/// Adds every unit-factory/payload family row for this block's kind.
pub fn unit_block_stats(ctx: &StatCtx, id: BlockId, def: &BlockDef, stats: &mut Stats) {
    match def.kind {
        // `UnitFactory.setStats` (`UnitFactory.java:146`).
        BlockKind::UnitFactory => {
            stats.remove(Stat::ItemCapacity);
            let mut panel = vec![Cell::Row];
            for plan in &def.unit_plans {
                panel.push(Cell::Row);
                if unlocked_now(ctx, ContentRef::new(ContentType::Unit, plan.unit.raw())) {
                    panel.push(Cell::Panel {
                        cells: factory_plan_cells(ctx, plan),
                    });
                } else {
                    panel.push(lock_cell());
                }
            }
            stats.add(Stat::Output, panel);
        }
        // `Reconstructor.setStats` (`Reconstructor.java:92`).
        BlockKind::Reconstructor => {
            stats.add_number(
                ctx,
                Stat::ProductionTime,
                def.construct_time / 60.0,
                StatUnit::Seconds,
            );
            let mut panel = vec![Cell::Row];
            for (from, to) in &def.reconstructor_upgrades {
                if unlocked_now(ctx, ContentRef::new(ContentType::Unit, from.raw()))
                    && unlocked_now(ctx, ContentRef::new(ContentType::Unit, to.raw()))
                {
                    panel.push(Cell::Row);
                    panel.push(Cell::Panel {
                        cells: upgrade_unit_cells(ctx, *from),
                    });
                    panel.push(Cell::Panel {
                        cells: vec![arrow_cell()],
                    });
                    panel.push(Cell::Panel {
                        cells: upgrade_unit_cells(ctx, *to),
                    });
                }
            }
            stats.add(Stat::Output, panel);
        }
        // `UnitAssembler.setStats` (`UnitAssembler.java:206`).
        BlockKind::UnitAssembler => {
            let mut panel = vec![Cell::Row];
            let mut tier = 0;
            for plan in &def.assembler_plans {
                panel.push(Cell::Row);
                if unlocked_now(ctx, ContentRef::new(ContentType::Unit, plan.unit.raw())) {
                    let Some(name) = unit_name(ctx, plan.unit) else {
                        continue;
                    };
                    let mut cells = vec![
                        Cell::Icon {
                            region: format!("unit-{name}-ui"),
                            amount: None,
                            size: 40,
                            content: Some(name.to_owned()),
                        },
                        Cell::Space { w: 10.0 },
                        values::text(format!(
                            "{}\n[lightgray]{} {}",
                            ctx.localized_name("unit", name),
                            auto_fixed(plan.time / 60.0, 1),
                            ctx.unit_label(StatUnit::Seconds)
                        )),
                    ];
                    if tier > 0 {
                        cells.push(values::text(format!(
                            "\n[lightgray]{}: {tier}",
                            ctx.stat_label(Stat::ModuleTier)
                        )));
                    }
                    for stack in &plan.payloads {
                        if let Some(cell) = payload_cell(ctx, stack) {
                            cells.push(Cell::Space { w: 5.0 });
                            cells.push(cell);
                        }
                    }
                    panel.push(Cell::Panel { cells });
                } else {
                    panel.push(lock_cell());
                }
                tier += 1;
            }
            stats.add(Stat::Output, panel);
        }
        // `UnitAssemblerModule.setStats` (`UnitAssemblerModule.java:32`).
        BlockKind::UnitAssemblerModule => {
            stats.add_number(
                ctx,
                Stat::ModuleTier,
                AssemblerModuleBehavior::TIER as f32,
                StatUnit::None,
            );
        }
        // `RepairTurret.setStats` (`RepairTurret.java:71`).
        BlockKind::RepairTurret => {
            if let Some(repair) = behavior::<RepairTurretBehavior>(ctx, id) {
                let turret = &repair.turret;
                stats.add_number(
                    ctx,
                    Stat::Range,
                    turret.repair_radius / 8.0,
                    StatUnit::Blocks,
                );
                stats.add_number(
                    ctx,
                    Stat::RepairSpeed,
                    turret.repair_speed * 60.0,
                    StatUnit::PerSecond,
                );
                if turret.accept_coolant
                    && let Some(amount) = coolant_amount(def)
                {
                    let liquids: Vec<(crate::content::LiquidId, f32)> = ctx
                        .registry
                        .liquids()
                        .iter()
                        .filter(|liquid| coolant_accepted(liquid, def))
                        .map(|liquid| (liquid.id, amount))
                        .collect();
                    stats.remove(Stat::Booster);
                    stats.add(
                        Stat::Booster,
                        values::speed_boosters(
                            ctx,
                            ctx.bundle.get("bar.strength"),
                            &liquids,
                            turret.coolant_multiplier,
                            true,
                        ),
                    );
                }
            }
        }
        // `RepairTower.setStats` (`RepairTower.java:36`).
        BlockKind::RepairTower => {
            if let Some(tower) = behavior::<RepairTowerBehavior>(ctx, id) {
                let tower = &tower.tower;
                stats.add_number(ctx, Stat::Range, tower.range / 8.0, StatUnit::Blocks);
                stats.add_number(
                    ctx,
                    Stat::RepairSpeed,
                    tower.heal_amount * 60.0,
                    StatUnit::PerSecond,
                );
            }
        }
        // `PayloadConveyor.setStats` (`PayloadConveyor.java:62`; the router and
        // reinforced conveyor extend it).
        BlockKind::PayloadConveyor | BlockKind::PayloadRouter => {
            if let Some(conveyor) = behavior::<PayloadConveyorBehavior>(ctx, id) {
                stats.add(
                    Stat::PayloadCapacity,
                    values::squared(ctx, conveyor.payload_limit, StatUnit::BlocksSquared),
                );
            }
        }
        // `PayloadMassDriver.setStats` (`PayloadMassDriver.java:81`).
        BlockKind::PayloadMassDriver => {
            if let Some(driver) = behavior::<PayloadMassDriverBehavior>(ctx, id) {
                stats.add(
                    Stat::PayloadCapacity,
                    values::squared(ctx, driver.max_payload_size, StatUnit::BlocksSquared),
                );
                let cycle = driver.charge_time + driver.reload;
                if cycle > 0.0 {
                    stats.add_number(ctx, Stat::Reload, 60.0 / cycle, StatUnit::PerSecond);
                }
                stats.add_number(ctx, Stat::ShootRange, driver.range / 8.0, StatUnit::Blocks);
            }
        }
        // `Constructor.setStats` (`Constructor.java:42`).
        BlockKind::Constructor => {
            if let Some(constructor) = behavior::<ConstructorBehavior>(ctx, id) {
                let min = constructor.min_block_size.to_string();
                let max = constructor.max_block_size.to_string();
                stats.add(
                    Stat::Output,
                    vec![values::text(values::format_args(
                        "@x@ ~ @x@",
                        &[min.clone(), min, max.clone(), max],
                    ))],
                );
                stats.add_percent(ctx, Stat::BuildSpeed, constructor.build_speed);
            }
        }
        _ => {}
    }
}

/// The per-tick `ConsumeCoolant.amount` of the block, if any.
fn coolant_amount(def: &BlockDef) -> Option<f32> {
    use crate::content::registries::blocks::Consume;
    def.consumes.iter().find_map(|spec| match &spec.consume {
        Consume::Coolant { amount, .. } => Some(*amount),
        _ => None,
    })
}

/// `ConsumeCoolant`'s default filter (`ConsumeCoolant.java:9-13`):
/// `liquid.coolant && (allowLiquid && !gas || allowGas && gas) &&
/// temperature <= 0.5 && flammability < 0.1`.
fn coolant_accepted(liquid: &crate::content::registries::liquids::Liquid, def: &BlockDef) -> bool {
    use crate::content::registries::blocks::Consume;
    let Some(spec) = def
        .consumes
        .iter()
        .find(|spec| matches!(spec.consume, Consume::Coolant { .. }))
    else {
        return false;
    };
    let Consume::Coolant {
        allow_liquid,
        allow_gas,
        ..
    } = &spec.consume
    else {
        return false;
    };
    liquid.coolant
        && ((*allow_liquid && !liquid.gas) || (*allow_gas && liquid.gas))
        && liquid.temperature <= 0.5
        && liquid.flammability < 0.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::bundle::parse_properties;
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

    fn bundle() -> crate::assets::bundle::Bundle {
        crate::assets::bundle::Bundle::from_layers(vec![parse_properties(
            "unit.seconds=seconds\nunit.persecond=/sec\nunit.blocks=blocks\n\
             unit.blockssquared=blocks squared\nunit.percent=%\n\
             stat.output=Output\nstat.productiontime=Production Time\n\
             stat.moduletier=Module Tier\nstat.range=Range\nstat.repairspeed=Repair Speed\n\
             stat.payloadcapacity=Payload Capacity\nstat.reload=Reload\nstat.shootrange=Shoot Range\n\
             stat.buildspeed=Build Speed\n\
             item.copper.name=Copper\nitem.lead.name=Lead\nitem.silicon.name=Silicon\n\
             unit.dagger.name=Dagger\nunit.mace.name=Mace\n\
             category.general=General\ncategory.function=Function\ncategory.crafting=Crafting\n\
             category.items=Items\nbar.strength=Strength\n",
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

    /// I04 T5: `Reconstructor.setStats` lists upgrade pairs where both sides
    /// are unlocked (`Reconstructor.java:97-121`); with everything unlocked the
    /// additive reconstructor's first pair is nova -> pulsar
    /// (`Blocks.java:6345-6352` — the plan's "dagger -> mace first" was
    /// reversed; Java lists nova -> pulsar first, dagger -> mace second).
    #[test]
    fn additive_reconstructor_lists_unlocked_upgrades() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry
            .block_id("additive-reconstructor")
            .expect("additive-reconstructor");
        let stats = content_stats(&ctx, crate::content::ContentRef::block(id));
        let output = row(&stats, Stat::Output);
        assert_eq!(output.len(), 1, "one output value");
        let cells = &output[0];
        // Collect the panels in order: the first pair is nova -> pulsar
        // (icon, arrow, icon), the second dagger -> mace.
        let panels: Vec<Vec<Cell>> = cells
            .iter()
            .filter_map(|cell| match cell {
                Cell::Panel { cells } => Some(cells.clone()),
                _ => None,
            })
            .collect();
        let nova = registry.unit_id("nova").expect("nova");
        let pulsar = registry.unit_id("pulsar").expect("pulsar");
        let dagger = registry.unit_id("dagger").expect("dagger");
        let mace = registry.unit_id("mace").expect("mace");
        assert_eq!(
            panels.first(),
            Some(&upgrade_unit_cells(&ctx, nova)),
            "first panel is the nova upgrade source"
        );
        assert_eq!(
            panels.get(2),
            Some(&upgrade_unit_cells(&ctx, pulsar)),
            "third panel is the pulsar upgrade target"
        );
        assert_eq!(
            panels.get(3),
            Some(&upgrade_unit_cells(&ctx, dagger)),
            "fourth panel is the dagger upgrade source"
        );
        assert_eq!(
            panels.get(5),
            Some(&upgrade_unit_cells(&ctx, mace)),
            "sixth panel is the mace upgrade target"
        );
    }
}
