// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret and ammo family `setStats` bodies (I04 T6, first commit).
//!
//! Java sources: `defense/turrets/BaseTurret.java:93` (shoot range +
//! activation), `Turret.java:185` (inaccuracy, reload, targets, ammo use,
//! heat), `ReloadTurret.java:17`, `ItemTurret.java:49` (removes item capacity,
//! `StatValues.ammo`, ammo capacity), `LiquidTurret.java:37`,
//! `PowerTurret.java:17`, `LaserTurret.java:26`, `ContinuousTurret.java:29`,
//! `ContinuousLiquidTurret.java:32`, `PayloadAmmoTurret.java:51`,
//! `PointDefenseTurret.java:51`, `TractorBeamTurret.java:57`, and
//! `StatValues.ammo` (`StatValues.java:701-955`) with the `bullet.*` bundle
//! keys.
//!
//! Deviations (also in the task summary):
//! * `ReloadTurret`/`LaserTurret` coolant booster panels are not ported (the
//!   `StatValues.boosters` reload math); the ammo and base rows are.
//! * `type.showStats`/`spawnBullets` collapse buttons are not modeled; nested
//!   frag/interval bullets render expanded inside sub-panels.
//! * Java's `statusText` emoji is skipped (the port's status content has no
//!   emoji field); chance/name/duration remain.
//! * `Turret.reloadWhileCharging` has no port field; its Java default is
//!   `true`, so the `firstShotDelay` reload term is absent.

use super::values;
use super::{Cell, Stat, StatCtx, StatUnit, Stats};
use crate::content::registries::blocks::{BlockDef, BlockKind};
use crate::content::{BlockId, BulletId};
use crate::ui::stat_display::{ammo_stat, fix_value};
use crate::world::blocks::defense::turrets::TurretAmmo;
use crate::world::blocks::defense::turrets::behavior::TurretBehavior;
use crate::world::blocks::payloads::PayloadKind;

/// The behavior as `dyn Any` for a family downcast.
fn behavior_any<'a>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a dyn std::any::Any> {
    let instance = ctx.blocks?.get(id)?;
    Some(instance.behavior.as_ref() as &dyn std::any::Any)
}

/// A behavior downcast.
fn behavior<'a, T: std::any::Any>(ctx: &'a StatCtx<'a>, id: BlockId) -> Option<&'a T> {
    behavior_any(ctx, id)?.downcast_ref::<T>()
}

/// `Turret`-core rows (`Turret.java:185-197`).
fn turret_core_rows(ctx: &StatCtx, stats: &mut Stats, turret: &TurretBehavior) {
    let config = &turret.config;
    stats.add_number(
        ctx,
        Stat::Inaccuracy,
        config.inaccuracy as i32 as f32,
        StatUnit::Degrees,
    );
    let delay = if config.shoot.first_shot_delay > 0.0 {
        // `!reloadWhileCharging` term (Java default true keeps it at 0).
        0.0
    } else {
        0.0
    };
    let cycle = config.reload + delay;
    if cycle > 0.0 {
        stats.add_number(
            ctx,
            Stat::Reload,
            60.0 / cycle * config.shoot.shots as f32,
            StatUnit::PerSecond,
        );
    }
    stats.add(Stat::TargetsAir, values::bool(ctx, config.target_air));
    stats.add(Stat::TargetsGround, values::bool(ctx, config.target_ground));
    if config.ammo_per_shot != 1 {
        stats.add_number(
            ctx,
            Stat::AmmoUse,
            config.ammo_per_shot as f32,
            StatUnit::PerShot,
        );
    }
    if config.heat_requirement > 0.0 {
        stats.add_number(
            ctx,
            Stat::Input,
            config.heat_requirement,
            StatUnit::HeatUnits,
        );
        if config.max_heat_efficiency > 0.0 {
            stats.add_number(
                ctx,
                Stat::MaxEfficiency,
                (config.max_heat_efficiency * 100.0) as i32 as f32,
                StatUnit::Percent,
            );
        }
    }
}

/// `BaseTurret` rows (`BaseTurret.java:93-97`).
fn base_turret_rows(config_range: f32, activation_time: f32, ctx: &StatCtx, stats: &mut Stats) {
    stats.add_number(ctx, Stat::ShootRange, config_range / 8.0, StatUnit::Blocks);
    if activation_time > 0.0 {
        stats.add_number(
            ctx,
            Stat::ActivationTime,
            activation_time / 60.0,
            StatUnit::Seconds,
        );
    }
}

/// One `[lightgray]`-prefixed separator row inside an ammo panel
/// (`StatValues.sep`).
pub(super) fn sep(rows: &mut Vec<Cell>, text: impl Into<String>) {
    rows.push(Cell::Row);
    rows.push(values::text(format!(
        "[lightgray]{}[lightgray]",
        text.into()
    )));
}

/// `StatValues.statusText(status, duration, chance)` (`StatValues.java:229`).
fn status_text(
    ctx: &StatCtx,
    status: crate::content::StatusId,
    duration: f32,
    chance: f32,
) -> String {
    let name = ctx
        .registry
        .status(status)
        .map(|def| ctx.localized_name("status", &def.name))
        .unwrap_or_default();
    let chance_text = if chance < 1.0 {
        ctx.bundle
            .format("stat.chance", &[&fix_value(chance * 100.0)])
    } else {
        String::new()
    };
    format!(
        "{chance_text}[stat]{name}[lightgray] ~ [stat]{}[lightgray] {}",
        crate::ui::stat_display::auto_fixed(duration / 60.0, 1),
        ctx.unit_label(StatUnit::Seconds)
    )
}

/// One bullet's rows inside its gray panel (`StatValues.ammo` body).
pub(super) fn bullet_rows(ctx: &StatCtx, bullet: BulletId, nested: bool) -> Vec<Cell> {
    let Some(def) = ctx.registry.bullet(bullet) else {
        return Vec::new();
    };
    let mut rows: Vec<Cell> = Vec::new();
    // Damage (with the continuous-pierce `~ dps` form).
    if def.damage > 0.0 && (def.collides || def.splash_damage <= 0.0) {
        let mut text = ctx
            .bundle
            .format("bullet.damage", &[&format!("{}", def.damage)]);
        let continuous = def.damage * 60.0;
        if continuous > 0.0 && def.interval_bullet.is_some() {
            text.push_str(&format!(
                "[lightgray] ~ [stat]{}[lightgray]{}",
                ctx.bundle
                    .format("bullet.damage", &[&format!("{}", continuous)]),
                ctx.unit_label(StatUnit::PerSecond)
            ));
        }
        sep(&mut rows, text);
    }
    if def.building_damage_multiplier != 1.0 {
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.buildingdamage",
                &[&ammo_stat(def.building_damage_multiplier * 100.0 - 100.0)],
            ),
        );
    }
    if def.range_change != 0.0 && !nested {
        sep(
            &mut rows,
            ctx.bundle
                .format("bullet.range", &[&ammo_stat(def.range_change / 8.0)]),
        );
    }
    if def.shield_damage_multiplier != 1.0 {
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.shielddamage",
                &[&ammo_stat(def.shield_damage_multiplier * 100.0 - 100.0)],
            ),
        );
    }
    if def.splash_damage > 0.0 {
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.splashdamage",
                &[
                    &format!("{}", def.splash_damage as i32),
                    &format!("{:.1}", def.splash_damage_radius / 8.0),
                ],
            ),
        );
    }
    if !nested && (def.ammo_multiplier - 1.0).abs() > f32::EPSILON && def.display_ammo_multiplier {
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.multiplier",
                &[&format!("{}", def.ammo_multiplier as i32)],
            ),
        );
    }
    if !nested && (def.reload_multiplier - 1.0).abs() > f32::EPSILON {
        let val = def.reload_multiplier * 100.0 - 100.0;
        sep(
            &mut rows,
            ctx.bundle.format("bullet.reload", &[&ammo_stat(val)]),
        );
    }
    if def.knockback > 0.0 {
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.knockback",
                &[&crate::ui::stat_display::auto_fixed(def.knockback, 2)],
            ),
        );
    }
    if def.pierce || def.pierce_cap != -1 {
        let text = if def.pierce_cap == -1 {
            ctx.bundle.get("bullet.infinitepierce").to_string()
        } else {
            ctx.bundle
                .format("bullet.pierce", &[&format!("{}", def.pierce_cap)])
        };
        sep(&mut rows, text);
    }
    if def.homing_power > 0.01 {
        sep(&mut rows, ctx.bundle.get("bullet.homing"));
    }
    if def.lightning > 0 {
        let damage = if def.lightning_damage < 0.0 {
            def.damage
        } else {
            def.lightning_damage
        };
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.lightning",
                &[&format!("{}", def.lightning), &format!("{damage}")],
            ),
        );
    }
    if def.max_damage_fraction > 0.0 {
        sep(
            &mut rows,
            ctx.bundle.format(
                "bullet.maxdamagefraction",
                &[&format!("{}", (def.max_damage_fraction * 100.0) as i32)],
            ),
        );
    }
    if def.status != crate::content::StatusId::NONE && def.status_chance > 0.0 {
        sep(
            &mut rows,
            status_text(ctx, def.status, def.status_duration, def.status_chance),
        );
    }
    if !def.target_missiles {
        sep(&mut rows, ctx.bundle.get("bullet.notargetsmissiles"));
    }
    if !def.target_blocks {
        sep(&mut rows, ctx.bundle.get("bullet.notargetsbuildings"));
    }
    // Nested interval/frag bullets render expanded inside their own panel.
    if let Some(interval) = def.interval_bullet {
        rows.push(Cell::Row);
        rows.push(values::text(ctx.bundle.format(
            "bullet.interval",
            &[&crate::ui::stat_display::auto_fixed(
                def.interval_bullets as f32 / def.bullet_interval * 60.0,
                2,
            )],
        )));
        rows.push(Cell::Row);
        rows.push(Cell::Panel {
            cells: bullet_rows(ctx, interval, true),
        });
    }
    if let Some(frag) = def.frag_bullet {
        rows.push(Cell::Row);
        rows.push(values::text(
            ctx.bundle
                .format("bullet.frags", &[&format!("{}", def.frag_bullets)]),
        ));
        rows.push(Cell::Row);
        rows.push(Cell::Panel {
            cells: bullet_rows(ctx, frag, true),
        });
    }
    rows
}

/// `StatValues.ammo(map, blockName)` for item turrets: keys sorted by content
/// id (`orderedKeys.sort()`), copper -> graphite -> silicon for duo.
fn add_item_ammo(
    ctx: &StatCtx,
    stats: &mut Stats,
    ammo: &[crate::world::blocks::defense::turrets::ItemAmmo],
    block_name: &str,
) {
    let mut entries: Vec<&crate::world::blocks::defense::turrets::ItemAmmo> = ammo.iter().collect();
    entries.sort_by_key(|entry| entry.item.raw());
    let mut panel = vec![Cell::Row];
    for entry in entries {
        let Some(name) = ctx.registry.item(entry.item).map(|def| def.name.clone()) else {
            continue;
        };
        let mut cells = vec![
            Cell::Icon {
                region: format!("item-{name}-ui"),
                amount: None,
                size: 24,
                content: Some(name.clone()),
            },
            Cell::Space { w: 8.0 },
            values::text(ctx.item_name(&name)),
        ];
        let _ = block_name;
        cells.push(Cell::Row);
        cells.extend(bullet_rows(ctx, entry.bullet, false));
        panel.push(Cell::Panel { cells });
        panel.push(Cell::Row);
    }
    stats.add(Stat::Ammo, panel);
}

/// `StatValues.ammo(map)` for liquid turrets/power turrets (no title for
/// turret keys; liquid keys show their icon + name).
fn add_liquid_ammo(
    ctx: &StatCtx,
    stats: &mut Stats,
    ammo: &[crate::world::blocks::defense::turrets::LiquidAmmo],
) {
    let mut entries = ammo.to_vec();
    entries.sort_by_key(|entry| entry.liquid.raw());
    let mut panel = vec![Cell::Row];
    for entry in entries {
        let Some(name) = ctx
            .registry
            .liquid(entry.liquid)
            .map(|def| def.name.clone())
        else {
            continue;
        };
        let mut cells = vec![
            Cell::Icon {
                region: format!("liquid-{name}-ui"),
                amount: None,
                size: 24,
                content: Some(name.clone()),
            },
            Cell::Space { w: 8.0 },
            values::text(ctx.liquid_name(&name)),
        ];
        cells.push(Cell::Row);
        cells.extend(bullet_rows(ctx, entry.bullet, false));
        panel.push(Cell::Panel { cells });
        panel.push(Cell::Row);
    }
    stats.add(Stat::Ammo, panel);
}

/// `StatValues.ammo(ObjectMap.of(this, shootType))` (no title for a turret
/// key).
fn add_power_ammo(ctx: &StatCtx, stats: &mut Stats, bullet: BulletId, block_name: &str) {
    let _ = block_name;
    let panel = vec![
        Cell::Row,
        Cell::Panel {
            cells: bullet_rows(ctx, bullet, false),
        },
        Cell::Row,
    ];
    stats.add(Stat::Ammo, panel);
}

/// `StatValues.ammo(map, true)` for payload turrets: payload icons with the
/// content name as the title.
fn add_payload_ammo(
    ctx: &StatCtx,
    stats: &mut Stats,
    ammo: &[crate::world::blocks::defense::turrets::PayloadAmmo],
) {
    let mut entries = ammo.to_vec();
    entries.sort_by_key(|entry| (entry.content.kind as u8, entry.content.id));
    let mut panel = vec![Cell::Row];
    for entry in entries {
        let content = match entry.content.kind {
            PayloadKind::Build => crate::content::ContentRef::new(
                crate::content::ContentType::Block,
                entry.content.id,
            ),
            PayloadKind::Unit => {
                crate::content::ContentRef::new(crate::content::ContentType::Unit, entry.content.id)
            }
        };
        let Some(name) = crate::game::tech_tree::content_name(ctx.registry, content) else {
            continue;
        };
        let region = format!("{}-{name}-ui", content.type_.name());
        panel.push(Cell::Panel {
            cells: vec![
                Cell::Icon {
                    region,
                    amount: None,
                    size: 24,
                    content: Some(name.clone()),
                },
                Cell::Space { w: 8.0 },
                values::text(name),
                Cell::Row,
                // nested bullet rows
                Cell::Panel {
                    cells: bullet_rows(ctx, entry.bullet, true),
                },
            ],
        });
        panel.push(Cell::Row);
    }
    stats.add(Stat::Ammo, panel);
}

/// Adds every turret family row for this block's kind.
pub fn turret_block_stats(ctx: &StatCtx, id: BlockId, def: &BlockDef, stats: &mut Stats) {
    if !matches!(
        def.kind,
        BlockKind::ItemTurret
            | BlockKind::LiquidTurret
            | BlockKind::PowerTurret
            | BlockKind::LaserTurret
            | BlockKind::ContinuousTurret
            | BlockKind::ContinuousLiquidTurret
            | BlockKind::PointDefenseTurret
            | BlockKind::TractorBeamTurret
    ) {
        return;
    }
    let Some(turret) = behavior::<TurretBehavior>(ctx, id) else {
        return;
    };
    let config = &turret.config;
    base_turret_rows(config.range, config.activation_time, ctx, stats);

    match def.kind {
        // `PointDefenseTurret extends ReloadTurret` (`PointDefenseTurret.java:51`).
        BlockKind::PointDefenseTurret => {
            if config.reload > 0.0 {
                stats.add_number(ctx, Stat::Reload, 60.0 / config.reload, StatUnit::PerSecond);
            }
            stats.add_number(ctx, Stat::Damage, config.bullet_damage, StatUnit::None);
        }
        // `TractorBeamTurret extends BaseTurret` (`TractorBeamTurret.java:57`).
        BlockKind::TractorBeamTurret => {
            stats.add(Stat::TargetsAir, values::bool(ctx, config.target_air));
            stats.add(Stat::TargetsGround, values::bool(ctx, config.target_ground));
            if config.beam_damage > 0.0 {
                stats.add_number(
                    ctx,
                    Stat::Damage,
                    config.beam_damage * 60.0,
                    StatUnit::PerSecond,
                );
            }
            if config.status != crate::content::StatusId::NONE && config.status_chance > 0.0 {
                stats.add(
                    Stat::Status,
                    vec![values::text(status_text(
                        ctx,
                        config.status,
                        config.status_duration,
                        config.status_chance,
                    ))],
                );
            }
        }
        // Turret-core kinds (`Turret.java:185`).
        _ => {
            turret_core_rows(ctx, stats, turret);
            match (&config.ammo, def.kind) {
                (TurretAmmo::Item(items), BlockKind::ItemTurret) => {
                    stats.remove(Stat::ItemCapacity);
                    add_item_ammo(ctx, stats, items, &def.name);
                    stats.add_number(
                        ctx,
                        Stat::AmmoCapacity,
                        config.max_ammo as f32 / config.ammo_per_shot.max(1) as f32,
                        StatUnit::Shots,
                    );
                }
                (TurretAmmo::Payload(items), _) => {
                    stats.remove(Stat::ItemCapacity);
                    add_payload_ammo(ctx, stats, items);
                }
                (TurretAmmo::Liquid(liquids), _) => {
                    add_liquid_ammo(ctx, stats, liquids);
                }
                (TurretAmmo::Power(bullet), _) => {
                    add_power_ammo(ctx, stats, *bullet, &def.name);
                }
                // Continuous turrets remove the reload/inaccuracy rows after
                // the ammo (`ContinuousTurret.java:32-35`).
                (TurretAmmo::Item(_), _) => {}
            }
            if def.kind == BlockKind::ContinuousTurret
                || def.kind == BlockKind::ContinuousLiquidTurret
            {
                stats.remove(Stat::Reload);
                stats.remove(Stat::Inaccuracy);
            }
        }
    }
    // `LaserTurret extends PowerTurret` (`LaserTurret.java:26`): remove the
    // coolant booster (not ported; see the module docs).
    if def.kind == BlockKind::LaserTurret {
        stats.remove(Stat::Booster);
    }
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
        // The anonymous turret ammo bullets (`boot.rs` order: after
        // `post_init`); without them the turret behaviors are not registered.
        crate::world::blocks::defense::turrets::install_turret_ammo(&mut registry);
        registry
    }

    fn bundle() -> crate::assets::bundle::Bundle {
        crate::assets::bundle::Bundle::from_layers(vec![parse_properties(
            "bullet.damage=[stat]{0}[lightgray] damage\n\
             bullet.multiplier=[stat]{0}[lightgray] ammo/item\n\
             item.copper.name=Copper\nitem.graphite.name=Graphite\nitem.silicon.name=Silicon\n\
             stat.ammo=Ammo\nstat.ammocapacity=Ammo Capacity\nstat.reload=Reload\n\
             stat.inaccuracy=Inaccuracy\nstat.targetsair=Targets Air\nstat.targetsground=Targets Ground\n\
             stat.ammouse=Ammo Use\nstat.shootrange=Shoot Range\nunit.shots=shots\n\
             unit.persecond=/sec\nunit.blocks=blocks\nunit.degrees=degrees\nunit.pershot=/shot\n\
             category.general=General\ncategory.function=Function\ncategory.items=Items\n",
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

    /// The text of every `Cell::Text` in the first value row of `stat`.
    fn texts(stats: &Stats, stat: Stat) -> Vec<String> {
        row(stats, stat)
            .first()
            .map(|cells| {
                cells
                    .iter()
                    .filter_map(|cell| match cell {
                        Cell::Text { t } => Some(t.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// I04 T6: duo's ammo lists copper -> graphite -> silicon (sorted by
    /// content id, `StatValues.ammo`'s `orderedKeys.sort()`), and copper's
    /// damage is the Java `9 damage` (`Blocks.java` duo copper ammo).
    #[test]
    fn duo_ammo_lists_copper_graphite_silicon() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry.block_id("duo").expect("duo");
        let stats = content_stats(&ctx, crate::content::ContentRef::block(id));
        let ammo = row(&stats, Stat::Ammo);
        assert_eq!(ammo.len(), 1, "one ammo value");
        let cells = &ammo[0];
        // Panels in order: copper, graphite, silicon.
        let names: Vec<String> = cells
            .iter()
            .filter_map(|cell| match cell {
                Cell::Panel { cells } => cells.iter().find_map(|inner| match inner {
                    Cell::Text { t } => Some(t.clone()),
                    _ => None,
                }),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["Copper", "Graphite", "Silicon"]);
        // Copper's first bullet row is the damage line: `[stat]9[lightgray]
        // damage` -> after render_markup the raw text still carries the tags.
        let copper_damage = cells.iter().find_map(|cell| match cell {
            Cell::Panel { cells } => cells.iter().find_map(|inner| match inner {
                Cell::Text { t } if t.contains("damage") => Some(t.clone()),
                _ => None,
            }),
            _ => None,
        });

        // Ammo capacity: the Java defaults `maxAmmo = 30`, `ammoPerShot = 1`
        // (`Turret.java:47`) -> 30 shots; the copper entry also carries the
        // `2 ammo/item` multiplier row (`bullet.multiplier`).
        assert_eq!(
            texts(&stats, Stat::AmmoCapacity),
            vec!["30", " shots"],
            "maxAmmo / max(ammoPerShot, 1)"
        );
        assert_eq!(
            copper_damage.as_deref(),
            Some("[lightgray][stat]9[lightgray] damage[lightgray]")
        );
        let copper_multiplier = cells.iter().find_map(|cell| match cell {
            Cell::Panel { cells } => cells.iter().find_map(|inner| match inner {
                Cell::Text { t } if t.contains("ammo/item") => Some(t.clone()),
                _ => None,
            }),
            _ => None,
        });
        assert_eq!(
            copper_multiplier.as_deref(),
            Some("[lightgray][stat]2[lightgray] ammo/item[lightgray]"),
            "copper gives 2 ammo per item"
        );
    }
}
