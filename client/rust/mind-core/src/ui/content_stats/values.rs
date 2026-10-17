// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `world/meta/StatValues.java` display helpers (H05 T10a): the value builders
//! whose `StatValue.display(table)` bodies add cells to the info table. Each
//! helper returns the cells of one *value*; the dialog adds a 10 px gap after
//! each value (`ContentInfoDialog.java` `inset.add().size(10f)`).

use super::{Cell, StatCtx, StatUnit};
use crate::content::ItemId;
use crate::content::LiquidId;
use crate::content::stacks::{ItemStack, LiquidStack};
use crate::ui::stat_display::{auto_fixed, fix_value};

/// Arc `Strings.format(text, args)`: each `@` is replaced by the next argument
/// (`Strings.java:305-321`), e.g. `"@x@"` + `["1", "1"]` -> `"1x1"`.
pub fn format_args(text: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(text.len() + args.len() * 2);
    let mut next = 0;
    for ch in text.chars() {
        if ch == '@' && next < args.len() {
            out.push_str(&args[next]);
            next += 1;
        } else {
            out.push(ch);
        }
    }
    out
}

/// A plain text cell.
pub fn text(value: impl Into<String>) -> Cell {
    Cell::Text { t: value.into() }
}

/// `StatValues.bool` (`@yes`/`@no`).
pub fn bool(ctx: &StatCtx, value: bool) -> Vec<Cell> {
    let key = if value { "yes" } else { "no" };
    vec![text(ctx.bundle.get(key).to_owned())]
}

/// `StatValues.number(value, unit, merge = false)`: value cell + unit cell.
///
/// `unit == none` collapses to the value cell (Java adds an empty unit cell).
/// Units with an `icon` (power/liquid/heat) get a leading icon cell; Java embeds
/// `Iconc.power` etc. inline, the port uses the row's icon cell instead.
pub fn number(ctx: &StatCtx, value: f32, unit: StatUnit) -> Vec<Cell> {
    let mut cells = unit_icon_cells(unit);
    cells.push(text(fix_value(value)));
    if unit == StatUnit::None {
        return cells;
    }
    cells.push(text(format!(
        "{}{}",
        if unit.space() { " " } else { "" },
        unit.localized(ctx)
    )));
    cells
}

/// `StatValues.number(value, unit, merge = true)`: one merged cell.
pub fn number_merged(ctx: &StatCtx, value: f32, unit: StatUnit) -> Vec<Cell> {
    number_merged_text(ctx, fix_value(value), unit)
}

/// `StatValues` unit icon (16 px `Iconc` glyph, e.g. `power`/`liquid`/`waves`).
fn unit_icon_cells(unit: StatUnit) -> Vec<Cell> {
    match unit.icon_name() {
        Some(region) => vec![Cell::Icon {
            region: region.to_owned(),
            amount: None,
            size: 16,
            content: None,
        }],
        None => Vec::new(),
    }
}

/// `StatValues.squared` (`"2x2"` + unit).
pub fn squared(ctx: &StatCtx, value: f32, unit: StatUnit) -> Vec<Cell> {
    let fixed = fix_value(value);
    number_merged_text(ctx, format!("{fixed}x{fixed}"), unit)
}

/// `StatValues.multiplierModifier`: `[stat]1.5` + unit (merged text cell).
pub fn multiplier_modifier(ctx: &StatCtx, value: f32, unit: StatUnit) -> Vec<Cell> {
    number_merged_text(ctx, crate::ui::stat_display::mult_stat(value), unit)
}

/// `StatValues.percentModifier`: `(value - 1) * 100` with `ammoStat` colors.
pub fn percent_modifier(ctx: &StatCtx, value: f32, unit: StatUnit) -> Vec<Cell> {
    number_merged_text(
        ctx,
        crate::ui::stat_display::ammo_stat((value - 1.0) * 100.0),
        unit,
    )
}

fn number_merged_text(ctx: &StatCtx, value: String, unit: StatUnit) -> Vec<Cell> {
    let mut cells = unit_icon_cells(unit);
    if unit == StatUnit::None {
        cells.push(text(value));
        return cells;
    }
    let l2 = format!(
        "{}{}",
        if unit.space() { " " } else { "" },
        unit.localized(ctx)
    );
    cells.push(text(format!("{value}{l2}")));
    cells
}

/// `StatValues.items(false, stacks)` — one icon cell per stack (`displayItem`).
pub fn items(ctx: &StatCtx, stacks: &[ItemStack], display_name: bool) -> Vec<Cell> {
    let mut cells = Vec::new();
    for stack in stacks {
        let Some(name) = ctx.registry.item(stack.item).map(|item| item.name.clone()) else {
            continue;
        };
        cells.push(item_icon(&name, stack.amount, 32));
        if display_name {
            cells.push(Cell::Space { w: 4.0 });
            cells.push(text(ctx.item_name(&name)));
        }
        // `table.add(displayItem(...)).padRight(5)`.
        cells.push(Cell::Space { w: 5.0 });
    }
    cells
}

/// `StatValues.items(timePeriod, stacks)`: icon + a `[lightgray]<amount>/sec`
/// label (`displayItem(item, amount, timePeriod, showName)`).
pub fn items_per_second(ctx: &StatCtx, time_period: f32, stacks: &[ItemStack]) -> Vec<Cell> {
    let mut cells = Vec::new();
    for stack in stacks {
        let Some(name) = ctx.registry.item(stack.item).map(|item| item.name.clone()) else {
            continue;
        };
        cells.push(item_icon(&name, stack.amount, 32));
        let rate = if time_period > 0.0 {
            stack.amount as f32 / (time_period / 60.0)
        } else {
            stack.amount as f32
        };
        cells.push(Cell::Space { w: 2.0 });
        // `displayItem(item, amount, timePeriod, showName = true)`
        // (`StatValues.java:157-163, 310-315`): the localized name above the
        // rate (L05 T3: java/info-cultivator.png "Spore Pod / 0.6/sec").
        cells.push(text(format!(
            "{}\n[lightgray]{}{}",
            ctx.item_name(&name),
            auto_fixed(rate, 3),
            ctx.unit_label(StatUnit::PerSecond)
        )));
        cells.push(Cell::Space { w: 5.0 });
    }
    cells
}

/// One item icon cell (`StatValues.stack`: 32 px icon + outline amount).
pub fn item_icon(name: &str, amount: i32, size: u8) -> Cell {
    Cell::Icon {
        region: format!("item-{name}-ui"),
        amount: (amount != 0).then(|| crate::ui::text::format_amount_default(i64::from(amount))),
        size,
        content: Some(name.to_owned()),
    }
}

/// `StatValues.liquid(liquid, amount, perSecond)` (`displayLiquid`).
pub fn liquid(ctx: &StatCtx, liquid: LiquidId, amount: f32, per_second: bool) -> Vec<Cell> {
    let mut cells = Vec::new();
    let Some(name) = ctx.registry.liquid(liquid).map(|l| l.name.clone()) else {
        return cells;
    };
    cells.push(Cell::Icon {
        region: format!("liquid-{name}-ui"),
        amount: (amount != 0.0).then(|| fix_value(amount)),
        size: 32,
        content: Some(name.clone()),
    });
    cells.push(Cell::Space { w: 5.0 });
    if per_second && amount != 0.0 {
        cells.push(text(format!(
            "[lightgray]{}",
            ctx.unit_label(StatUnit::PerSecond)
        )));
        cells.push(Cell::Space { w: 5.0 });
    }
    cells.push(text(ctx.liquid_name(&name)));
    cells
}

/// `StatValues.liquids(timePeriod, perSecond, stacks)`: amounts scaled by
/// `60 / timePeriod`.
pub fn liquids(
    ctx: &StatCtx,
    stacks: &[LiquidStack],
    time_period: f32,
    per_second: bool,
) -> Vec<Cell> {
    let mut cells = Vec::new();
    for stack in stacks {
        let amount = if time_period > 0.0 {
            stack.amount * (60.0 / time_period)
        } else {
            stack.amount
        };
        cells.extend(liquid(ctx, stack.liquid, amount, per_second));
        cells.push(Cell::Space { w: 5.0 });
    }
    cells
}

/// Item icon cells for a set of item ids (used by item-filter displays).
pub fn item_ids(ctx: &StatCtx, ids: &[ItemId], display_name: bool) -> Vec<Cell> {
    let stacks: Vec<ItemStack> = ids
        .iter()
        .map(|item| ItemStack {
            item: *item,
            amount: 0,
        })
        .collect();
    items(ctx, &stacks, display_name)
}

/// `StatValues.displayItemPercent(item, percent, showName)`.
pub fn display_item_percent(
    ctx: &StatCtx,
    item: ItemId,
    percent: i32,
    show_name: bool,
) -> Vec<Cell> {
    let mut cells = Vec::new();
    let Some(name) = ctx.registry.item(item).map(|def| def.name.clone()) else {
        return cells;
    };
    cells.push(Cell::Icon {
        region: format!("item-{name}-ui"),
        amount: None,
        size: 32,
        content: Some(name.clone()),
    });
    cells.push(Cell::Space { w: 2.0 });
    let label = if show_name {
        format!("{}\n[lightgray]{percent}%", ctx.item_name(&name))
    } else {
        format!("[lightgray]{percent}%")
    };
    cells.push(text(label));
    cells.push(Cell::Space { w: 5.0 });
    cells
}

/// `StatValues.displayItem(item, amount, timePeriod, showName)` (the timed
/// overload, `StatValues.java:310-315`): 32 px item icon with the raw amount
/// outline, then `name\n[lightgray]<amount / (timePeriod / 60)><perSecond>`
/// (`Strings.autoFixed(..., 3)`).
pub fn display_item_timed(
    ctx: &StatCtx,
    item: ItemId,
    amount: f32,
    time_period: f32,
    show_name: bool,
) -> Vec<Cell> {
    let mut cells = Vec::new();
    let Some(name) = ctx.registry.item(item).map(|def| def.name.clone()) else {
        return cells;
    };
    cells.push(item_icon(&name, amount as i32, 32));
    let rate = crate::ui::stat_display::auto_fixed(amount / (time_period / 60.0), 3);
    let name_prefix = if show_name {
        format!("{}\n", ctx.item_name(&name))
    } else {
        String::new()
    };
    let label = format!(
        "{name_prefix}[lightgray]{rate}{}",
        ctx.unit_label(StatUnit::PerSecond)
    );
    cells.push(text(label));
    cells.push(Cell::Space { w: 5.0 });
    cells
}

/// Which floors `StatValues.drillables` accepts: `Drill.setStats` filters
/// `Floor && !wallOre`, `BeamDrill.setStats` `(Floor && wallOre) || StaticWall`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrillablesScope {
    /// `Drill.setStats` (`Drill.java:178-179`).
    Floor,
    /// `BeamDrill.setStats` (`BeamDrill.java:122-125`).
    WallOre,
}

/// `UnitType.setStats` mine-tier list (`UnitType.java:824-833`):
/// `StatValues.drillables(mineSpeed, 1f, 1, null, filter)` — `multipliers` is
/// null, so each 40 px panel has only the block icon, its localized name and
/// the item emoji (no rate). Filter: an item drop, floors by `wallOre` vs
/// `mineWalls`/`mineFloor`, non-floors only with `mineWalls`, `hardness <=
/// mineTier` and not `playerUnmineable` (`doubletapmine` defaults off).
pub fn unit_drillables(
    ctx: &StatCtx,
    mine_tier: i32,
    mine_walls: bool,
    mine_floor: bool,
) -> Vec<Cell> {
    use crate::content::registries::blocks::BlockKind;

    let is_floor = |kind: BlockKind| {
        matches!(
            kind,
            BlockKind::Floor
                | BlockKind::EmptyFloor
                | BlockKind::ColoredFloor
                | BlockKind::OreBlock
                | BlockKind::ShallowLiquid
                | BlockKind::SteamVent
        )
    };
    let mut cells = vec![Cell::Row];
    let mut index = 0usize;
    for def in ctx.registry.blocks() {
        let Some(drop) = def.item_drop else {
            continue;
        };
        let minable = if is_floor(def.kind) {
            (def.wall_ore && mine_walls) || (!def.wall_ore && mine_floor)
        } else {
            mine_walls
        };
        if !minable || def.player_unmineable {
            continue;
        }
        let Some(item) = ctx.registry.item(drop) else {
            continue;
        };
        if item.hardness > mine_tier {
            continue;
        }
        cells.push(Cell::Panel {
            cells: vec![
                Cell::Icon {
                    region: format!("block-{}-ui", def.name),
                    amount: None,
                    size: 40,
                    content: Some(def.name.clone()),
                },
                Cell::Space { w: 10.0 },
                text(block_display_name(ctx, def)),
                Cell::Row,
                Cell::Space { w: 50.0 },
                Cell::Icon {
                    region: format!("item-{}-ui", item.name),
                    amount: None,
                    size: 16,
                    content: Some(item.name.clone()),
                },
            ],
        });
        index += 1;
        if index.is_multiple_of(2) {
            cells.push(Cell::Row);
        }
    }
    cells
}

/// `StatValues.drillables(drillTime, drillMultiplier, size, multipliers, filter)`:
/// one 40 px panel per drillable block with the item and its per-second rate.
///
/// Deviation: the port lists every drillable block in the registry (Java also
/// requires `indexer.isBlockPresent(f)` in game); the item glyph is the 16 px
/// item icon instead of `Item.emoji()`.
pub fn drillables(
    ctx: &StatCtx,
    drill: &crate::world::block_kind_data::DrillDef,
    size: f32,
    scope: DrillablesScope,
) -> Vec<Cell> {
    use crate::content::registries::blocks::BlockKind;

    let is_floor = |kind: BlockKind| {
        matches!(
            kind,
            BlockKind::Floor
                | BlockKind::EmptyFloor
                | BlockKind::ColoredFloor
                | BlockKind::OreBlock
                | BlockKind::ShallowLiquid
                | BlockKind::SteamVent
        )
    };
    let mut cells = Vec::new();
    let mut index = 0usize;
    for def in ctx.registry.blocks() {
        let Some(drop) = def.item_drop else {
            continue;
        };
        let in_scope = match scope {
            DrillablesScope::Floor => is_floor(def.kind) && !def.wall_ore,
            DrillablesScope::WallOre => {
                (is_floor(def.kind) && def.wall_ore)
                    || (def.kind == BlockKind::StaticWall && def.item_drop.is_some())
            }
        };
        if !in_scope {
            continue;
        }
        let Some(item) = ctx.registry.item(drop) else {
            continue;
        };
        if item.hardness > drill.tier || drill.blocked_items.contains(&drop.raw()) {
            continue;
        }
        let multiplier = drill
            .drill_multipliers
            .iter()
            .find(|(id, _)| *id == drop.raw())
            .map(|(_, value)| *value)
            .unwrap_or(1.0);
        let time = (drill.drill_time + drill.hardness_drill_multiplier * item.hardness as f32)
            .max(drill.drill_time);
        let rate = 60.0 / (time / multiplier) * size;
        cells.push(Cell::Panel {
            cells: vec![
                Cell::Icon {
                    region: format!("block-{}-ui", def.name),
                    amount: None,
                    size: 40,
                    content: Some(def.name.clone()),
                },
                text(block_display_name(ctx, def)),
                Cell::Space { w: 4.0 },
                Cell::Icon {
                    region: format!("item-{}-ui", item.name),
                    amount: None,
                    size: 16,
                    content: Some(item.name.clone()),
                },
                Cell::Space { w: 4.0 },
                text(format!(
                    "[lightgray]{}{}",
                    auto_fixed(rate, 2),
                    ctx.unit_label(StatUnit::PerSecond)
                )),
            ],
        });
        index += 1;
        if index.is_multiple_of(2) {
            cells.push(Cell::Row);
        }
    }
    cells
}

/// `StatValues.speedBoosters(unit, amount, speed, strength, filter)`: one panel
/// per boosting liquid (`unit` has its `{0}` replaced by the coloured speed).
pub fn speed_boosters(
    ctx: &StatCtx,
    unit: &str,
    liquids: &[(LiquidId, f32)],
    speed: f32,
    strength: bool,
) -> Vec<Cell> {
    let mut cells = Vec::new();
    for (liquid, amount) in liquids {
        let Some(def) = ctx.registry.liquid(*liquid) else {
            continue;
        };
        let name = def.name.clone();
        let mut panel = vec![
            Cell::Icon {
                region: format!("liquid-{name}-ui"),
                amount: None,
                size: 40,
                content: Some(name.clone()),
            },
            text(ctx.localized_name("liquid", &name)),
            Cell::Space { w: 4.0 },
            text(format!(
                "[lightgray]{} {}",
                auto_fixed(amount * 60.0, 2),
                ctx.unit_label(StatUnit::PerSecond)
            )),
        ];
        if speed != f32::MAX {
            let value = speed * (if strength { def.heat_capacity } else { 1.0 })
                + if strength { 1.0 } else { 0.0 };
            panel.push(Cell::Space { w: 6.0 });
            panel.push(text(unit.replace(
                "{0}",
                &format!("[stat]{}[lightgray]", auto_fixed(value, 2)),
            )));
        }
        cells.push(Cell::Panel { cells: panel });
        cells.push(Cell::Row);
    }
    cells
}

/// `Block.localizedName` for the drillables list: an `OreBlock` copies its ore
/// item's name (`OreBlock.java:17`, `this.localizedName = ore.localizedName`);
/// a wall ore appends the `wallore` bundle suffix (`OreBlock.setup`).
fn block_display_name(ctx: &StatCtx, def: &crate::content::registries::blocks::BlockDef) -> String {
    if let Some(drop) = def.item_drop
        && def.kind == crate::content::registries::blocks::BlockKind::OreBlock
    {
        let name = ctx
            .registry
            .item(drop)
            .map(|item| ctx.item_name(&item.name))
            .unwrap_or_else(|| def.name.clone());
        if def.wall_ore {
            return format!("{} {}", name, ctx.bundle.get("wallore"));
        }
        return name;
    }
    ctx.localized_name("block", &def.name)
}
