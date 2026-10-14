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
        cells.push(text(format!(
            "[lightgray]{}{}",
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
