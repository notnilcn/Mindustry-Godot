// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Production, crafting and power family `setStats` bodies (H05 T10b).
//!
//! One function per Java class, each called from [`family_block_stats`] after
//! the base [`super::block_stats_timed`] body — the same order as the Java
//! `super.setStats()` chain. The Java sources are the
//! `world/blocks/production/*` classes plus `world/blocks/power/*`; the value
//! helpers live in [`super::values`].
//!
//! Family knobs come from the runtime [`crate::world::block::BlockTable`]
//! (`kind_data` for crafters/drills/pumps, the registered behavior structs for
//! generators/reactors via the `Any` upcast) so there is exactly one source of
//! truth per number. Recorded deviations from Java (also in the task summary):
//! * `StatValues.blocks(…)` (tiles/affinities): the port lists the registry's
//!   floors with a non-zero attribute instead of the live-map indexer set, and
//!   the detailed efficiency/output form is not ported (`AttributeCrafter`
//!   `outputScale`/`displayEfficiency` have no port fields — the short
//!   `blockEfficiency` list is used).
//! * `StatValues.drillables`: all drillable floors are listed regardless of map
//!   presence; the item glyph is the 16 px item icon, not `item.emoji()`.
//! * `StatValues.blocks` in the menu shows `@stat.showinmap` as Java does
//!   (`state.isGame()` -> [`StatCtx::in_game`]).
//! * `NuclearReactor` heat output is Erekir-only in Java; the port gates it on
//!   [`StatCtx::planet`] == `"erekir"` as well.

use std::any::Any;

use super::values;
use super::{Cell, Stat, StatCtx, StatUnit, Stats, block_stats_timed};
use crate::content::registries::blocks::{BlockDef, BlockKind, Consume};
use crate::content::stacks::{ItemStack, LiquidStack};
use crate::content::{BlockId, ItemId, LiquidId};
use crate::world::block_kind_data::{BlockKindData, CrafterDef, DrillDef};
use crate::world::blocks::heat::behavior::HeatCrafterBehavior;
use crate::world::blocks::power::behavior::{
    ConsumeGeneratorBehavior, HeaterGeneratorBehavior, ImpactReactorBehavior,
    NuclearReactorBehavior, SolarGeneratorBehavior, ThermalGeneratorBehavior,
    VariableReactorBehavior,
};
use crate::world::blocks::power::nodes::PowerNodeBehavior;

/// Full block stats: the base `Block.setStats` body plus the family override
/// (`GenericCrafter.setStats` etc.), exactly like the Java class chain.
pub fn family_block_stats(ctx: &StatCtx, id: BlockId) -> Stats {
    let Some(def) = ctx.registry.block(id) else {
        return Stats::new();
    };
    // Java sets `stats.timePeriod` BEFORE `super.setStats()` in every crafter
    // family (GenericCrafter, Separator, Fracker, ConsumeGenerator, reactors);
    // the base consumer rows depend on it.
    let time_period = time_period_before_super(ctx, def, id);
    let mut stats = block_stats_timed(ctx, id, time_period);
    family_body(ctx, id, def, &mut stats);
    stats
}

/// The `stats.timePeriod = …` assignment each family body performs before
/// `super.setStats()` (`-1` = the `Stats` default).
fn time_period_before_super(ctx: &StatCtx, def: &BlockDef, id: BlockId) -> f32 {
    match def.kind {
        BlockKind::GenericCrafter
        | BlockKind::HeatProducer
        | BlockKind::HeatCrafter
        | BlockKind::AttributeCrafter => match kind_data(ctx, id) {
            Some(BlockKindData::Crafter(c)) => c.craft_time,
            Some(BlockKindData::AttributeCrafter(c)) => c.crafter.craft_time,
            _ => -1.0,
        },
        BlockKind::Separator => match kind_data(ctx, id) {
            Some(BlockKindData::Separator(s)) => s.craft_time,
            _ => -1.0,
        },
        BlockKind::Fracker => match kind_data(ctx, id) {
            Some(BlockKindData::Fracker(f)) => f.item_use_time,
            _ => -1.0,
        },
        BlockKind::ConsumeGenerator => consume_generator(ctx, id)
            .map(|g| g.config.item_duration)
            .unwrap_or(-1.0),
        BlockKind::HeaterGenerator => heater_generator(ctx, id)
            .map(|g| g.generator.config.item_duration)
            .unwrap_or(-1.0),
        BlockKind::NuclearReactor => nuclear_reactor(ctx, id)
            .map(|r| r.config.item_duration)
            .unwrap_or(-1.0),
        BlockKind::ImpactReactor => impact_reactor(ctx, id)
            .map(|r| r.config.item_duration)
            .unwrap_or(-1.0),
        _ => -1.0,
    }
}

/// Applies the family body for `def.kind` (`<> instanceof <Class>` dispatch).
fn family_body(ctx: &StatCtx, id: BlockId, def: &BlockDef, stats: &mut Stats) {
    let Some(kind) = kind_data(ctx, id) else {
        return;
    };
    match def.kind {
        BlockKind::GenericCrafter | BlockKind::HeatProducer => {
            if let BlockKindData::Crafter(crafter) = kind {
                generic_crafter_stats(ctx, stats, def, crafter);
            }
        }
        // `HeatCrafter extends GenericCrafter` (`HeatCrafter.java:8`).
        BlockKind::HeatCrafter => {
            if let BlockKindData::Crafter(crafter) = kind {
                generic_crafter_stats(ctx, stats, def, crafter);
            }
            heat_crafter_stats(ctx, stats, id);
        }
        BlockKind::AttributeCrafter => {
            if let BlockKindData::AttributeCrafter(crafter) = kind {
                generic_crafter_stats(ctx, stats, def, &crafter.crafter);
                attribute_crafter_stats(ctx, stats, def, crafter);
            }
        }
        BlockKind::Separator => {
            if let BlockKindData::Separator(separator) = kind {
                separator_stats(ctx, stats, separator);
            }
        }
        BlockKind::Drill => {
            if let BlockKindData::Drill(drill) = kind {
                drill_stats(ctx, stats, def, drill);
                drill_booster_stats(
                    ctx,
                    stats,
                    def,
                    drill.liquid_boost_intensity * drill.liquid_boost_intensity,
                );
            }
        }
        // `BurstDrill extends Drill` (`BurstDrill.java`): the super body runs,
        // then the booster value is replaced with the (un-squared) intensity.
        BlockKind::BurstDrill => {
            if let BlockKindData::BurstDrill(burst) = kind {
                drill_stats(ctx, stats, def, &burst.drill);
                drill_booster_stats(ctx, stats, def, burst.drill.liquid_boost_intensity);
            }
        }
        BlockKind::BeamDrill => {
            if let BlockKindData::BeamDrill(beam) = kind {
                beam_drill_stats(ctx, stats, def, beam);
            }
        }
        BlockKind::Pump => {
            if let BlockKindData::Pump(pump) = kind {
                pump_stats(ctx, stats, def, pump.pump_amount);
            }
        }
        // `Fracker extends SolidPump` (`Fracker.java:12`).
        BlockKind::SolidPump | BlockKind::Fracker => {
            if let BlockKindData::SolidPump(pump) = kind {
                // A plain `SolidPump` produces water (`SolidPump.result`
                // default, `Blocks.java` water-extractor).
                solid_pump_stats(
                    ctx,
                    stats,
                    def,
                    pump.pump_amount,
                    Some(LiquidId::WATER),
                    &pump.attribute,
                    pump.base_efficiency,
                );
            } else if let BlockKindData::Fracker(fracker) = kind {
                solid_pump_stats(
                    ctx,
                    stats,
                    def,
                    fracker.pump_amount,
                    fracker.result.map(LiquidId::new),
                    &fracker.attribute,
                    fracker.base_efficiency,
                );
                // `Fracker.setStats` (`Fracker.java:19-24`).
                stats.add_number(
                    ctx,
                    Stat::ProductionTime,
                    fracker.item_use_time / 60.0,
                    StatUnit::Seconds,
                );
            }
        }
        BlockKind::WallCrafter => {
            if let BlockKindData::WallCrafter(wall) = kind {
                wall_crafter_stats(
                    ctx,
                    stats,
                    def,
                    wall.drill_time,
                    &wall.attribute,
                    wall.output,
                );
            }
        }
        BlockKind::ItemSource => {
            if let BlockKindData::ItemSource(source) = kind {
                item_source_stats(ctx, stats, source.items_per_second);
            }
        }
        BlockKind::ConsumeGenerator => {
            if let Some(generator) = consume_generator(ctx, id) {
                consume_generator_stats(ctx, stats, def, &generator.config);
            }
        }
        // `SolarGenerator extends PowerGenerator` (`SolarGenerator.java:9`):
        // remove + re-add the same row is a no-op.
        BlockKind::SolarGenerator => {
            if let Some(solar) = solar_generator(ctx, id) {
                generator_row(ctx, stats, solar.config.power_production);
            }
        }
        BlockKind::ThermalGenerator => {
            if let Some(thermal) = thermal_generator(ctx, id) {
                thermal_generator_stats(ctx, stats, def, thermal);
            }
        }
        BlockKind::NuclearReactor => {
            if let Some(reactor) = nuclear_reactor(ctx, id) {
                nuclear_reactor_stats(ctx, stats, def, reactor);
            }
        }
        BlockKind::ImpactReactor => {
            if let Some(reactor) = impact_reactor(ctx, id) {
                impact_reactor_stats(ctx, stats, def, reactor);
            }
        }
        BlockKind::VariableReactor => {
            if let Some(reactor) = variable_reactor(ctx, id) {
                // `VariableReactor extends PowerGenerator` (`VariableReactor.java`):
                // the base row, then the max-heat input (`:67-71`).
                generator_row(ctx, stats, reactor.config.power_production);
                stats.add_number(ctx, Stat::Input, reactor.max_heat, StatUnit::HeatUnits);
            }
        }
        // `HeaterGenerator extends ConsumeGenerator` (`HeaterGenerator.java:8`).
        BlockKind::HeaterGenerator => {
            if let Some(heater) = heater_generator(ctx, id) {
                consume_generator_stats(ctx, stats, def, &heater.generator.config);
                // `HeaterGenerator.setStats` (`HeaterGenerator.java:27-31`).
                stats.add_number(ctx, Stat::Output, heater.heat_output, StatUnit::HeatUnits);
            }
        }
        // `BeamNode extends PowerNode` (`BeamNode.java:14`).
        BlockKind::BeamNode => {
            beam_node_stats(ctx, stats, def);
        }
        BlockKind::PowerNode | BlockKind::LongPowerNode => {
            if let Some(node) = power_node(ctx, id) {
                power_node_rows(ctx, stats, node.laser_range, node.max_nodes);
            }
        }
        // `PowerVoid.setStats` (`PowerVoid.java:16-19`).
        BlockKind::PowerVoid => stats.remove(Stat::PowerUse),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// The runtime kind data for `id` (family knobs).
fn kind_data<'a>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a BlockKindData> {
    Some(&ctx.blocks?.get(id)?.kind_data)
}

/// The registered behavior as `dyn Any` for a family downcast.
fn behavior_any<'a>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a dyn Any> {
    let instance = ctx.blocks?.get(id)?;
    Some(instance.behavior.as_ref() as &dyn Any)
}

/// `block.consumes` power usage (`ConsumePower.usage`), when present.
fn power_usage(def: &BlockDef) -> Option<f32> {
    def.consumes.iter().find_map(|spec| match &spec.consume {
        Consume::Power { usage, .. } => Some(*usage),
        _ => None,
    })
}

/// The first plain liquid consumer (`ConsumeLiquid`), the drill/wall-crafter
/// booster. The port's `ConsumeSpec` has no `booster` flag, and the drills only
/// declare this one liquid consumer (`Blocks.java` `consumeLiquid(Liquids.water,
/// 0.1f).boost()`).
fn liquid_booster(def: &BlockDef) -> Option<(LiquidId, f32)> {
    def.consumes.iter().find_map(|spec| match &spec.consume {
        Consume::Liquid { liquid, amount } => Some((*liquid, *amount)),
        _ => None,
    })
}

fn consume_generator<'a>(
    ctx: &'a StatCtx<'_>,
    id: BlockId,
) -> Option<&'a ConsumeGeneratorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<ConsumeGeneratorBehavior>()
}

fn solar_generator<'a>(ctx: &'a StatCtx<'_>, id: BlockId) -> Option<&'a SolarGeneratorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<SolarGeneratorBehavior>()
}

fn thermal_generator<'a>(
    ctx: &'a StatCtx<'_>,
    id: BlockId,
) -> Option<&'a ThermalGeneratorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<ThermalGeneratorBehavior>()
}

fn nuclear_reactor<'a>(ctx: &'a StatCtx<'_>, id: BlockId) -> Option<&'a NuclearReactorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<NuclearReactorBehavior>()
}

fn impact_reactor<'a>(ctx: &'a StatCtx<'_>, id: BlockId) -> Option<&'a ImpactReactorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<ImpactReactorBehavior>()
}

fn variable_reactor<'a>(ctx: &'a StatCtx<'_>, id: BlockId) -> Option<&'a VariableReactorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<VariableReactorBehavior>()
}

fn heater_generator<'a>(ctx: &'a StatCtx<'_>, id: BlockId) -> Option<&'a HeaterGeneratorBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<HeaterGeneratorBehavior>()
}

fn power_node<'a>(ctx: &'a StatCtx<'_>, id: BlockId) -> Option<&'a PowerNodeBehavior> {
    behavior_any(ctx, id)?.downcast_ref::<PowerNodeBehavior>()
}

fn item_stacks(stacks: &[(u16, i32)]) -> Vec<ItemStack> {
    stacks
        .iter()
        .map(|(item, amount)| ItemStack {
            item: ItemId::new(*item),
            amount: *amount,
        })
        .collect()
}

fn liquid_stacks(stacks: &[(u16, f32)]) -> Vec<LiquidStack> {
    stacks
        .iter()
        .map(|(liquid, amount)| LiquidStack {
            liquid: LiquidId::new(*liquid),
            amount: *amount,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Crafters
// ---------------------------------------------------------------------------

/// `GenericCrafter.setStats` (`GenericCrafter.java:66-81`); also the body of
/// `HeatProducer` (`HeatProducer.java` adds no `setStats`).
fn generic_crafter_stats(ctx: &StatCtx, stats: &mut Stats, def: &BlockDef, crafter: &CrafterDef) {
    let has_items = def.has_items;
    let outputs = !crafter.output_items.is_empty();
    if (has_items && def.item_capacity > 0) || outputs {
        stats.add_number(
            ctx,
            Stat::ProductionTime,
            crafter.craft_time / 60.0,
            StatUnit::Seconds,
        );
    }
    if outputs {
        stats.add(
            Stat::Output,
            values::items_per_second(ctx, crafter.craft_time, &item_stacks(&crafter.output_items)),
        );
    }
    if !crafter.output_liquids.is_empty() {
        // `StatValues.liquids(1f, outputLiquids)` scales by `60 / 1`.
        stats.add(
            Stat::Output,
            values::liquids(ctx, &liquid_stacks(&crafter.output_liquids), 1.0, true),
        );
    }
}

/// `HeatCrafter.setStats` (`HeatCrafter.java:36-41`).
fn heat_crafter_stats(ctx: &StatCtx, stats: &mut Stats, id: BlockId) {
    let Some(crafter) = behavior_any(ctx, id).and_then(|b| b.downcast_ref::<HeatCrafterBehavior>())
    else {
        return;
    };
    stats.add_number(ctx, Stat::Input, crafter.requirement, StatUnit::HeatUnits);
    // `stats.add(Stat.maxEfficiency, (int)(maxEfficiency * 100f), percent)`:
    // the int cast (not `addPercent`, which would round differently).
    stats.add_number(
        ctx,
        Stat::MaxEfficiency,
        (crafter.max_efficiency * 100.0) as i32 as f32,
        StatUnit::Percent,
    );
}

/// `AttributeCrafter.setStats` (`AttributeCrafter.java:77-85`).
fn attribute_crafter_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    crafter: &crate::world::block_kind_data::AttributeCrafterDef,
) {
    let Some(attribute) = attribute_name(crafter.attribute) else {
        return;
    };
    let scale = crafter.boost_scale * (def.size * def.size) as f32;
    // `baseEfficiency <= 0.0001f ? Stat.tiles : Stat.affinities`; the port has
    // no `displayEfficiency`, so `startZero = false` (the Java default).
    let stat = if crafter.base_efficiency <= 0.0001 {
        Stat::Tiles
    } else {
        Stat::Affinities
    };
    stats.add(
        stat,
        attribute_block_cells(ctx, attribute, def.floating, scale, false, true),
    );
}

/// Attribute index (`AttributeCrafterDef.attribute`) -> registry name; `-1` has
/// no port representation (`Attribute.spores` is not in the port's `Attribute`).
fn attribute_name(index: i16) -> Option<&'static str> {
    const NAMES: [&str; 5] = ["heat", "spores", "water", "light", "steam"];
    usize::try_from(index)
        .ok()
        .and_then(|i| NAMES.get(i).copied())
}

/// `Separator.setStats` (`Separator.java:37-50`).
fn separator_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    separator: &crate::world::block_kind_data::SeparatorDef,
) {
    let sum: i32 = separator.results.iter().map(|(_, amount)| *amount).sum();
    let mut cells = Vec::new();
    for (item, amount) in &separator.results {
        let percent = if sum == 0 {
            0
        } else {
            ((*amount as f32 / sum as f32) * 100.0) as i32
        };
        cells.extend(values::display_item_percent(
            ctx,
            ItemId::new(*item),
            percent,
            true,
        ));
    }
    stats.add(Stat::Output, cells);
    stats.add_number(
        ctx,
        Stat::ProductionTime,
        separator.craft_time / 60.0,
        StatUnit::Seconds,
    );
}

// ---------------------------------------------------------------------------
// Drills, pumps
// ---------------------------------------------------------------------------

/// `Drill.setStats` (`Drill.java:175-191`).
fn drill_stats(ctx: &StatCtx, stats: &mut Stats, def: &BlockDef, drill: &DrillDef) {
    let size = (def.size * def.size) as f32;
    stats.add(
        Stat::DrillTier,
        values::drillables(ctx, drill, size, values::DrillablesScope::Floor),
    );
    // `60f / drillTime * size * size`.
    stats.add_number(
        ctx,
        Stat::DrillSpeed,
        60.0 / drill.drill_time * size,
        StatUnit::ItemsSecond,
    );
}

/// `BeamDrill.setStats` (`BeamDrill.java:119-135`).
fn beam_drill_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    beam: &crate::world::block_kind_data::BeamDrillDef,
) {
    let drill = DrillDef {
        drill_time: beam.drill_time,
        tier: beam.tier,
        // `drillables(drillTime, 0f, size, …)`: no hardness term.
        hardness_drill_multiplier: 0.0,
        drill_multipliers: def
            .drill_multipliers
            .iter()
            .map(|(item, value)| (item.raw(), *value))
            .collect(),
        blocked_items: beam.blocked_items.clone(),
        wall_drill: true,
        ..DrillDef::default()
    };
    stats.add(
        Stat::DrillTier,
        values::drillables(
            ctx,
            &drill,
            def.size as f32,
            values::DrillablesScope::WallOre,
        ),
    );
    // `60f / drillTime * size`.
    stats.add_number(
        ctx,
        Stat::DrillSpeed,
        60.0 / beam.drill_time * def.size as f32,
        StatUnit::ItemsSecond,
    );
    beam_drill_booster(ctx, stats, def);
}

/// `Drill.setStats` liquid-booster branch (`Drill.java:183-190`): the speed is
/// `liquidBoostIntensity²` for `Drill`, the plain intensity for `BurstDrill`.
fn drill_booster_stats(ctx: &StatCtx, stats: &mut Stats, def: &BlockDef, speed: f32) {
    let Some((liquid, amount)) = liquid_booster(def) else {
        return;
    };
    if (speed - 1.0).abs() <= f32::EPSILON {
        return;
    }
    stats.remove(Stat::Booster);
    stats.add(
        Stat::Booster,
        values::speed_boosters(ctx, "{0}x speed", &[(liquid, amount)], speed, false),
    );
}

/// `BeamDrill.setStats` booster branch (`BeamDrill.java:129-134`): the speed is
/// `optionalBoostIntensity` (the class default is 1.6; no vanilla plasma bore
/// declares a booster consumer, so this emits nothing for them).
fn beam_drill_booster(ctx: &StatCtx, stats: &mut Stats, def: &BlockDef) {
    let Some((liquid, amount)) = liquid_booster(def) else {
        return;
    };
    stats.add(
        Stat::Booster,
        values::speed_boosters(ctx, "{0}x speed", &[(liquid, amount)], 1.6, false),
    );
}

/// `Pump.setStats` (`Pump.java:34-37`): `60f * pumpAmount * size * size`.
fn pump_stats(ctx: &StatCtx, stats: &mut Stats, def: &BlockDef, pump_amount: f32) {
    stats.add_number(
        ctx,
        Stat::Output,
        60.0 * pump_amount * (def.size * def.size) as f32,
        StatUnit::LiquidSecond,
    );
}

/// `SolidPump.setStats` (`SolidPump.java:58-66`).
#[allow(clippy::too_many_arguments)]
fn solid_pump_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    pump_amount: f32,
    result: Option<LiquidId>,
    attribute: &str,
    base_efficiency: f32,
) {
    stats.remove(Stat::Output);
    if let Some(liquid) = result {
        stats.add(
            Stat::Output,
            values::liquid(ctx, liquid, 60.0 * pump_amount, true),
        );
    }
    if attribute.is_empty() {
        return;
    }
    let stat = if base_efficiency > 0.0001 {
        Stat::Affinities
    } else {
        Stat::Tiles
    };
    stats.add(
        stat,
        attribute_block_cells(
            ctx,
            attribute,
            def.floating,
            1.0,
            base_efficiency <= 0.001,
            true,
        ),
    );
}

/// `WallCrafter.setStats` (`WallCrafter.java:76-100`). The port's wall crafters
/// have no item consumer (`cliff-crusher`), so the item-booster branch is
/// inert; the liquid branch uses the same `speedBoosters` shape.
fn wall_crafter_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    drill_time: f32,
    attribute: &str,
    output: Option<u16>,
) {
    if let Some(item) = output {
        stats.add(
            Stat::Output,
            values::items(
                ctx,
                &[ItemStack {
                    item: ItemId::new(item),
                    amount: 1,
                }],
                true,
            ),
        );
    }
    if !attribute.is_empty() {
        stats.add(
            Stat::Tiles,
            attribute_block_cells(ctx, attribute, def.floating, 1.0, true, false),
        );
    }
    stats.add_number(
        ctx,
        Stat::DrillSpeed,
        60.0 / drill_time * def.size as f32,
        StatUnit::ItemsSecond,
    );
    if let Some((liquid, amount)) = liquid_booster(def) {
        stats.add(
            Stat::Booster,
            values::speed_boosters(ctx, "{0}x speed", &[(liquid, amount)], 1.6, false),
        );
    }
}

/// `ItemSource.setStats` (`ItemSource.java:45-49`).
fn item_source_stats(ctx: &StatCtx, stats: &mut Stats, items_per_second: f32) {
    stats.add_number(ctx, Stat::Output, items_per_second, StatUnit::ItemsSecond);
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

/// `PowerGenerator.setStats` (`PowerGenerator.java:80-83`): `generationType` is
/// `Stat.basePowerGeneration` for every vanilla generator.
fn generator_row(ctx: &StatCtx, stats: &mut Stats, power_production: f32) {
    stats.add_number(
        ctx,
        Stat::BasePowerGeneration,
        power_production * 60.0,
        StatUnit::PowerSecond,
    );
}

/// `ConsumeGenerator.setStats` (`ConsumeGenerator.java:83-94`).
fn consume_generator_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    config: &crate::world::blocks::power::generator::GeneratorConfig,
) {
    generator_row(ctx, stats, config.power_production);
    if def.has_items {
        stats.add_number(
            ctx,
            Stat::ProductionTime,
            config.item_duration / 60.0,
            StatUnit::Seconds,
        );
    }
    if let Some((liquid, amount)) = config.output_liquid {
        stats.add(
            Stat::Output,
            values::liquid(ctx, liquid, amount * 60.0, true),
        );
    }
}

/// `ThermalGenerator.setStats` (`ThermalGenerator.java:56-66`).
fn thermal_generator_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    generator: &ThermalGeneratorBehavior,
) {
    let scale = (def.size * def.size) as f32;
    stats.add(
        Stat::Tiles,
        attribute_block_cells(
            ctx,
            attribute_key(generator.attribute),
            def.floating,
            scale,
            false,
            true,
        ),
    );
    stats.remove(Stat::BasePowerGeneration);
    // `displayEfficiencyScale` has no port field; the Java default is 1.
    stats.add_number(
        ctx,
        Stat::BasePowerGeneration,
        generator.config.power_production * 60.0,
        StatUnit::PowerSecond,
    );
    if let Some((liquid, amount)) = generator.output_liquid {
        stats.add(
            Stat::Output,
            values::liquid(ctx, liquid, amount * scale * 60.0, true),
        );
    }
}

/// `ThermalGenerator.attribute` -> the registry name the block attributes use.
fn attribute_key(attribute: crate::content::Attribute) -> &'static str {
    match attribute {
        crate::content::Attribute::Heat => "heat",
        crate::content::Attribute::Spores => "spores",
        crate::content::Attribute::Water => "water",
        crate::content::Attribute::Light => "light",
        crate::content::Attribute::Steam => "steam",
    }
}

/// `NuclearReactor.setStats` (`NuclearReactor.java:77-98`).
fn nuclear_reactor_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    reactor: &NuclearReactorBehavior,
) {
    // `NuclearReactor extends PowerGenerator` (`NuclearReactor.java`): the base
    // `setStats` adds the generation row first.
    generator_row(ctx, stats, reactor.config.power_production);
    let item_duration = reactor.config.item_duration;
    let item_capacity = reactor.item_capacity as f32;
    // `meltdownTime` closure.
    let avg = (item_duration / 60.0) / (1.0 + reactor.heat_consume_rate / 2.0);
    let val = 30.0 * reactor.heating * item_capacity * avg;
    let time = item_capacity * avg * (1.0 - (1.0 - 1.0 / val).max(0.0).sqrt());
    let meltdown = if val > 1.0 {
        format!(
            "{} {} {}",
            crate::ui::stat_display::auto_fixed(time, 2),
            StatUnit::Seconds.localized(ctx),
            ctx.bundle.get("bar.whenfull").to_owned()
        )
    } else {
        ctx.bundle.get("bar.nevermelts").to_owned()
    };
    stats.add(Stat::MeltdownTime, vec![values::text(meltdown)]);
    if def.has_items {
        stats.add_number(
            ctx,
            Stat::ProductionTime,
            item_duration / 60.0,
            StatUnit::Seconds,
        );
    }
    // Erekir-only (`state.getPlanet() == Planets.erekir`).
    if ctx.planet == Some("erekir") && reactor.heat_output > 0.0 {
        let text = values::format_args(
            ctx.bundle.get("bar.upto"),
            &[format!(
                "{} {} {}",
                StatUnit::HeatUnits.markup(),
                crate::ui::stat_display::fixed(reactor.heat_output, 0),
                ctx.bundle.get("unit.heatunitsperside")
            )],
        );
        stats.add(Stat::Output, vec![values::text(text)]);
    }
}

/// `ImpactReactor.setStats` (`ImpactReactor.java:57-79`).
fn impact_reactor_stats(
    ctx: &StatCtx,
    stats: &mut Stats,
    def: &BlockDef,
    reactor: &ImpactReactorBehavior,
) {
    // `ImpactReactor extends PowerGenerator` (`ImpactReactor.java`).
    generator_row(ctx, stats, reactor.config.power_production);
    if def.has_items {
        stats.add_number(
            ctx,
            Stat::ProductionTime,
            reactor.config.item_duration / 60.0,
            StatUnit::Seconds,
        );
    }
    let Some(usage) = power_usage(def) else {
        return;
    };
    // `statWarmup(progress) = -ln(progress) / warmupSpeed / 60`.
    let warmup_speed = reactor.config.warmup_speed;
    let stat_warmup = |progress: f32| -> f32 { -progress.ln() / warmup_speed / 60.0 };
    let max = stat_warmup(0.001);
    let equal = stat_warmup(1.0 - (usage / reactor.config.power_production).powf(1.0 / 5.0));
    let seconds = StatUnit::Seconds.localized(ctx);
    let equilibrium = StatUnit::PowerEquilibrium.localized(ctx);
    stats.add(
        Stat::WarmupTime,
        vec![values::text(format!(
            "{} {} ~ {} {} {}",
            crate::ui::stat_display::auto_fixed(max, 2),
            seconds,
            crate::ui::stat_display::auto_fixed(equal, 2),
            seconds,
            equilibrium
        ))],
    );
    // Java also attaches `.tooltip(bar.lerpwarmuptime, t90, t99)`; the T10a
    // `Stats` model has no per-add tooltip slot (bundle `stat.<name>.info` only).
}

/// `PowerNode.setStats` (`PowerNode.java:144-149`).
fn power_node_rows(ctx: &StatCtx, stats: &mut Stats, laser_range: f32, max_nodes: u8) {
    stats.add_number(ctx, Stat::PowerRange, laser_range, StatUnit::Blocks);
    stats.add_number(
        ctx,
        Stat::PowerConnections,
        max_nodes as f32,
        StatUnit::None,
    );
}

/// `BeamNode extends PowerNode` (`BeamNode.java:14,58-62`): the `PowerNode` rows
/// with the class defaults (`laserRange` 6, `maxNodes` 3; `Blocks.java` sets
/// neither for the beam nodes) plus the beam `range`.
fn beam_node_stats(ctx: &StatCtx, stats: &mut Stats, def: &BlockDef) {
    power_node_rows(ctx, stats, 6.0, 3);
    let range = crate::input::line_rules::beam_range(&def.name);
    stats.add_number(ctx, Stat::PowerRange, range as f32, StatUnit::Blocks);
}

// ---------------------------------------------------------------------------
// `StatValues.blocks(attr, …)`
// ---------------------------------------------------------------------------

/// `StatValues.blocks(attr, floating, scale, startZero, checkFloors)`: the
/// floors whose `attributes[attr]` is non-zero, sorted by attribute value
/// (ascending); the menu shows `@stat.showinmap` like Java.
fn attribute_block_cells(
    ctx: &StatCtx,
    attribute: &str,
    floating: bool,
    scale: f32,
    start_zero: bool,
    check_floors: bool,
) -> Vec<Cell> {
    if !ctx.in_game {
        return vec![values::text(ctx.bundle.get("stat.showinmap").to_owned())];
    }
    let mut blocks: Vec<(&BlockDef, f32)> = ctx
        .registry
        .blocks()
        .iter()
        .filter(|def| {
            (!check_floors || is_floor_like(def.kind))
                && !(def.is_deep && !floating)
                && attribute_value(def, attribute) != 0.0
        })
        .map(|def| (def, attribute_value(def, attribute)))
        .collect();
    blocks.sort_by(|a, b| a.1.total_cmp(&b.1));
    if blocks.is_empty() {
        return vec![values::text(ctx.bundle.get("none.inmap").to_owned())];
    }
    let mut cells = Vec::new();
    for (index, (def, value)) in blocks.iter().enumerate() {
        let multiplier = value * scale;
        let color = if multiplier < 0.0 {
            "[scarlet]"
        } else if start_zero {
            "[accent]"
        } else {
            "[accent]+"
        };
        cells.push(Cell::Panel {
            cells: vec![
                Cell::Icon {
                    region: format!("block-{}-ui", def.name),
                    amount: None,
                    size: 40,
                    content: Some(def.name.clone()),
                },
                values::text(format!("{color}{}%", (multiplier * 100.0) as i32)),
            ],
        });
        if (index + 1).is_multiple_of(5) {
            cells.push(Cell::Row);
        }
    }
    cells
}

/// `Block.attributes.get(attr)`.
fn attribute_value(def: &BlockDef, attribute: &str) -> f32 {
    def.attributes
        .iter()
        .filter(|(name, _)| name == attribute)
        .map(|(_, value)| *value)
        .sum()
}

/// A floor-family block (`block instanceof Floor`): the port's floor kinds plus
/// `OreBlock`/`SteamVent` (both extend `Floor` upstream).
fn is_floor_like(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Floor
            | BlockKind::EmptyFloor
            | BlockKind::ColoredFloor
            | BlockKind::OreBlock
            | BlockKind::ShallowLiquid
            | BlockKind::SteamVent
    )
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

    /// Minimal bundle (the family labels the tests assert on).
    fn bundle() -> crate::assets::bundle::Bundle {
        crate::assets::bundle::Bundle::from_layers(vec![parse_properties(
            "unit.seconds=seconds\nunit.itemssecond=items/second\nunit.liquidsecond=liquid units/second\n\
             unit.powersecond=power units/second\nunit.heatunits=heat units\nunit.persecond=/sec\n\
             stat.productiontime=Production Time\nstat.output=Output\nstat.input=Input\n\
             stat.drillspeed=Drill Speed\nstat.drilltier=Drill Tier\nstat.basepowergeneration=Power Generation\n\
             stat.itemcapacity=Item Capacity\nstat.poweruse=Power Use\nstat.booster=Booster\n\
             stat.booster.drill=Drill Speed\nstat.tiles=Tiles\nstat.affinities=Affinities\n\
             stat.meltdowntime=Meltdown Time\nstat.warmuptime=Warmup Time\n\
             category.crafting=Crafting\ncategory.general=General\ncategory.power=Power\ncategory.items=Items\n\
             item.copper.name=Copper\nitem.coal.name=Coal\nitem.graphite.name=Graphite\nitem.sand.name=Sand\n\
             liquid.water.name=Water\nitem.water.name=Water\nstat.showinmap=show in map\nnone.inmap=none\n",
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

    #[test]
    fn mechanical_drill_stats() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry
            .block_id("mechanical-drill")
            .expect("mechanical-drill");
        let stats = content_stats(&ctx, crate::content::ContentRef::block(id));
        // `Drill.setStats`: `60f / drillTime * size * size` = 60/600*2*2.
        assert_eq!(
            row(&stats, Stat::DrillSpeed),
            vec![vec![
                Cell::Text { t: "0.4".into() },
                Cell::Text {
                    t: " items/second".into()
                }
            ]]
        );
        // The drillables tier list is a panel row per drillable floor.
        let tier = row(&stats, Stat::DrillTier);
        assert_eq!(tier.len(), 1);
        assert!(
            tier[0]
                .iter()
                .any(|cell| matches!(cell, Cell::Panel { .. })),
            "drillables list missing: {:?}",
            tier[0]
        );
    }

    #[test]
    fn graphite_press_stats() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry.block_id("graphite-press").expect("graphite-press");
        let stats = content_stats(&ctx, crate::content::ContentRef::block(id));
        // `GraphitePress` craftTime 90: productionTime 90/60 = 1.5 s.
        assert_eq!(
            row(&stats, Stat::ProductionTime),
            vec![vec![
                Cell::Text { t: "1.5".into() },
                Cell::Text {
                    t: " seconds".into()
                }
            ]]
        );
        // Output graphite: 1 / (90/60) = 0.666 per second (Arc truncation).
        let output = row(&stats, Stat::Output);
        assert!(
            output
                .iter()
                .flatten()
                .any(|cell| matches!(cell, Cell::Text { t } if t == "[lightgray]0.666/sec")),
            "output row: {output:?}"
        );
        // Input coal 2: 2 / (90/60) = 1.333 per second.
        let input = row(&stats, Stat::Input);
        assert!(
            input
                .iter()
                .flatten()
                .any(|cell| matches!(cell, Cell::Text { t } if t == "[lightgray]1.333/sec")),
            "input row: {input:?}"
        );
    }

    #[test]
    fn combustion_generator_reads_behavior_knobs() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry
            .block_id("combustion-generator")
            .expect("combustion-generator");
        let stats = content_stats(&ctx, crate::content::ContentRef::block(id));
        // `ConsumeGeneratorBehavior.config.power_production = 1` (`behavior.rs`
        // register, `Blocks.java` combustion-generator) -> 60 power/second.
        assert_eq!(
            row(&stats, Stat::BasePowerGeneration),
            vec![vec![
                Cell::Icon {
                    region: "power".into(),
                    amount: None,
                    size: 16,
                    content: None,
                },
                Cell::Text { t: "60".into() },
                Cell::Text {
                    t: " power units/second".into()
                },
            ]]
        );
    }
}
