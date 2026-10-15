// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit `setStats` bodies (I04 T6, second commit).
//!
//! Java sources: `type/UnitType.java:790` (health, armor, speed, size, item
//! capacity, range, crush damage, leg splash, target flags, abilities, flying,
//! boost, mine, build speed, payload capacity, weapons, immunities),
//! `type/Weapon.java:181` (`addStats`: inaccuracy, reload, ammo table) and
//! `world/meta/StatValues.java:655` (`weapons`) / `:682` (`abilities`).
//!
//! Deviations (also in the task summary):
//! * The `abilities` table lists one panel per displayed ability with its
//!   localized name and its knobs; Java calls each ability's own
//!   `display(Table)` (repair fields, shields, spawn callouts) which the
//!   port's `AbilitySpec` does not model per kind.
//! * `mineTier`'s `StatValues.drillables` list is not emitted for units (the
//!   port's helper is drill-def based); `mineSpeed`/`mineTier` are.
//! * Weapon preview sprites (`showStatSprite`) are not drawn (the panel title
//!   row carries the weapon's bundle info key when present).

use super::turret_ammo::bullet_rows;
use super::{Cell, Stat, StatCtx, StatUnit, Stats, values};
use crate::content::UnitTypeId;
use crate::content::registries::units::weapon::WeaponDef;
use crate::content::registries::units::{AbilityKind, UnitTypeDef};
use crate::ui::stat_display::{auto_fixed, fix_value};

/// `UnitType.setStats` (`UnitType.java:790`) plus `StatValues.weapons`.
pub fn unit_stats(ctx: &StatCtx, id: UnitTypeId) -> Stats {
    let Some(def) = ctx.registry.unit(id) else {
        return Stats::new();
    };
    let mut stats = Stats::new();

    stats.add_number(ctx, Stat::Health, def.health, StatUnit::None);
    stats.add_number(ctx, Stat::Armor, def.armor, StatUnit::None);
    stats.add_number(
        ctx,
        Stat::Speed,
        def.speed * 60.0 / 8.0,
        StatUnit::TilesSecond,
    );
    stats.add(
        Stat::Size,
        values::squared(ctx, def.hit_size / 8.0, StatUnit::Blocks),
    );
    stats.add_number(
        ctx,
        Stat::ItemCapacity,
        def.item_capacity as f32,
        StatUnit::Items,
    );
    stats.add(
        Stat::Range,
        vec![
            values::text(auto_fixed(def.max_range / 8.0, 1)),
            values::text(format!(" {}", ctx.unit_label(StatUnit::Blocks))),
        ],
    );

    if def.crush_damage > 0.0 {
        stats.add_number(
            ctx,
            Stat::CrushDamage,
            def.crush_damage * 60.0 * 5.0,
            StatUnit::PerSecond,
        );
    }
    if def.leg_splash_damage > 0.0 && def.leg_splash_range > 0.0 {
        let text = ctx
            .bundle
            .format(
                "bullet.splashdamage",
                &[
                    &auto_fixed(def.leg_splash_damage, 2),
                    &auto_fixed(def.leg_splash_range / 8.0, 2),
                ],
            )
            .replace("[stat]", "[white]");
        stats.add(
            Stat::LegSplashDamage,
            vec![values::text(format!(
                "{text} {}",
                ctx.unit_label(StatUnit::PerLeg)
            ))],
        );
    }

    stats.add(Stat::TargetsAir, values::bool(ctx, def.target_air));
    stats.add(Stat::TargetsGround, values::bool(ctx, def.target_ground));

    if !def.abilities.is_empty() {
        stats.add(Stat::Abilities, abilities_cells(ctx, def));
    }

    stats.add(Stat::Flying, values::bool(ctx, def.flying));
    if !def.flying {
        stats.add(Stat::CanBoost, values::bool(ctx, def.can_boost));
        if def.can_boost {
            stats.add_number(
                ctx,
                Stat::BoostingSpeed,
                def.boost_multiplier * def.speed * 60.0 / 8.0,
                StatUnit::TilesSecond,
            );
        }
    }

    if def.mine_tier >= 1 {
        stats.add_percent(ctx, Stat::MineSpeed, def.mine_speed);
        stats.add_number(ctx, Stat::MineTier, def.mine_tier as f32, StatUnit::None);
    }
    if def.build_speed > 0.0 {
        stats.add_percent(ctx, Stat::BuildSpeed, def.build_speed);
    }
    if def.payload_capacity > 0.0 {
        stats.add(
            Stat::PayloadCapacity,
            values::squared(
                ctx,
                (def.payload_capacity / (8.0 * 8.0)).sqrt(),
                StatUnit::Blocks,
            ),
        );
    }

    if !def.weapons.is_empty() {
        stats.add(Stat::Weapons, weapon_cells(ctx, def));
    }

    if !def.immunities.is_empty() {
        let mut cells = Vec::new();
        for status in &def.immunities {
            let Some(status_def) = ctx.registry.status(*status) else {
                continue;
            };
            let name = status_def.name.clone();
            cells.push(Cell::Icon {
                region: format!("status-{name}-ui"),
                amount: None,
                size: 20,
                content: Some(name.clone()),
            });
            cells.push(values::text(ctx.localized_name("status", &name)));
            cells.push(Cell::Space { w: 6.0 });
        }
        stats.add(Stat::Immunities, cells);
    }

    stats
}

/// `StatValues.weapons(unit, weapons)` (`StatValues.java:655-678`): one gray
/// panel per displayed, non-flipped weapon with its `Weapon.addStats` rows.
fn weapon_cells(ctx: &StatCtx, def: &UnitTypeDef) -> Vec<Cell> {
    let mut cells = vec![Cell::Row];
    for (index, weapon) in def.weapons.iter().enumerate() {
        if weapon.flip_sprite || !weapon.display {
            continue;
        }
        let mut rows: Vec<Cell> = Vec::new();
        // `tableInfo(w, "unit.<name>.weapon.<index>.info")`.
        let info_key = format!("unit.{}.weapon.{index}.info", def.name);
        if ctx.bundle.has(&info_key) {
            rows.push(values::text(format!(
                "[lightgray]{}",
                ctx.bundle.get(&info_key)
            )));
            rows.push(Cell::Row);
        }
        rows.extend(weapon_add_stats(ctx, weapon));
        cells.push(Cell::Panel { cells: rows });
        cells.push(Cell::Row);
    }
    cells
}

/// `Weapon.addStats(unit, table)` (`Weapon.java:181-192`).
fn weapon_add_stats(ctx: &StatCtx, weapon: &WeaponDef) -> Vec<Cell> {
    let mut rows = Vec::new();
    if weapon.inaccuracy > 0.0 {
        rows.push(Cell::Row);
        rows.push(values::text(format!(
            "[lightgray]{}: [white]{} {}",
            ctx.stat_label(Stat::Inaccuracy),
            weapon.inaccuracy as i32,
            ctx.unit_label(StatUnit::Degrees)
        )));
    }
    if !weapon.always_continuous && weapon.reload > 0.0 && !weapon.bullet.kill_shooter {
        rows.push(Cell::Row);
        rows.push(values::text(format!(
            "[lightgray]{}: {}[white]{} {}",
            ctx.stat_label(Stat::Reload),
            if weapon.mirror { "2x " } else { "" },
            auto_fixed(60.0 / weapon.reload * weapon.shoot.shots as f32, 2),
            ctx.unit_label(StatUnit::PerSecond)
        )));
    }
    // `StatValues.ammo(ObjectMap.of(u, bullet))`: unit-keyed entries are
    // compact (no title row).
    rows.push(Cell::Row);
    rows.extend(bullet_rows(ctx, weapon.bullet.id, false));
    rows
}

/// `StatValues.abilities` (`StatValues.java:682-700`): one panel per displayed
/// ability.
fn abilities_cells(ctx: &StatCtx, def: &UnitTypeDef) -> Vec<Cell> {
    let mut cells = vec![Cell::Row];
    let mut count = 0;
    for ability in &def.abilities {
        let mut rows: Vec<Cell> = Vec::new();
        if let Some(kind_name) = ability_kind_name(&ability.kind) {
            rows.push(values::text(format!(
                "[stat]{}",
                ctx.bundle.get(&format!("ability.{kind_name}"))
            )));
            rows.push(Cell::Row);
        }
        // The knobs the port's `AbilitySpec` models, in a stable order.
        for (label, value) in [
            (ctx.stat_label(Stat::Healing), ability.amount),
            (ctx.stat_label(Stat::Range), ability.range),
            (ctx.stat_label(Stat::Reload), ability.reload),
            (ctx.stat_label(Stat::RegenerationRate), ability.regen),
            (ctx.stat_label(Stat::CooldownTime), ability.cooldown),
        ] {
            if value > 0.0 {
                rows.push(values::text(format!(
                    "[lightgray]{label}: [white]{}",
                    fix_value(value)
                )));
                rows.push(Cell::Row);
            }
        }
        cells.push(Cell::Panel { cells: rows });
        count += 1;
        if count == 2 {
            count = 0;
            cells.push(Cell::Row);
        }
    }
    cells
}

/// The bundle key half of the ability kind (`Ability.java` subclasses with an
/// `ability.*` bundle name).
fn ability_kind_name(kind: &AbilityKind) -> Option<&'static str> {
    Some(match kind {
        AbilityKind::ShieldRegenField => "shieldregenfield",
        AbilityKind::RepairField => "repairfield",
        AbilityKind::ForceField => "forcefield",
        AbilityKind::StatusField => "statusfield",
        AbilityKind::Regen => "regen",
        _ => return None,
    })
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
        crate::world::blocks::defense::turrets::install_turret_ammo(&mut registry);
        registry
    }

    fn bundle() -> crate::assets::bundle::Bundle {
        crate::assets::bundle::Bundle::from_layers(vec![parse_properties(
            "unit.tilessecond=tiles/s\nunit.blocks=blocks\nunit.items=items\nbullet.multiplier=[stat]{0}[lightgray] ammo/item\n\
             unit.perminute=/min\nunit.persecond=/sec\nunit.seconds=seconds\nunit.degrees=degrees\n\
             bullet.damage=[stat]{0}[lightgray] damage\n\
             stat.health=Health\nstat.armor=Armor\nstat.speed=Speed\nstat.size=Size\n\
             stat.itemcapacity=Item Capacity\nstat.range=Range\nstat.targetsair=Targets Air\n\
             stat.targetsground=Targets Ground\nstat.flying=Flying\nstat.canboost=Can Boost\n\
             stat.weapons=Weapons\nstat.reload=Reload\nstat.inaccuracy=Inaccuracy\n\
             stat.ammocapacity=Ammo Capacity\nstat.minespeed=Mine Speed\nstat.minetier=Mine Tier\n\
             category.general=General\ncategory.function=Function\n",
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

    /// I04 T6: dagger's `UnitType.setStats` + `StatValues.weapons`
    /// (`UnitType.java:790`, `StatValues.java:655`): health 150, speed
    /// `0.5 * 60 / 8 = 3.75` tiles/s, and one weapon panel whose reload row is
    /// `2x 60 / 26 = 2.3 /sec` — Java's `UnitType.init()` mirrors the default
    /// `Weapon.mirror = true` weapon into two copies and doubles their reload
    /// (`UnitType.java:1037-1055`), with the 9-damage bullet.
    #[test]
    fn dagger_weapon_stats() {
        let registry = registry();
        let bundle = bundle();
        let table = BlockTable::build_default(&registry).expect("table");
        let ctx = ctx(&registry, &bundle, &table);
        let id = registry.unit_id("dagger").expect("dagger");
        let stats = content_stats(
            &ctx,
            ContentRef::new(crate::content::ContentType::Unit, id.raw()),
        );
        let texts = |stat: Stat| -> Vec<String> {
            row(&stats, stat)
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
        };
        assert_eq!(texts(Stat::Health), vec!["150"], "dagger health");
        assert_eq!(texts(Stat::Speed), vec!["3.75", " tiles/s"]);
        // The weapon panel: one entry with the reload line and the damage row.
        let weapons = row(&stats, Stat::Weapons);
        assert_eq!(weapons.len(), 1, "one weapons value");
        let panels: Vec<Vec<Cell>> = weapons[0]
            .iter()
            .filter_map(|cell| match cell {
                Cell::Panel { cells } => Some(cells.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(panels.len(), 1, "one weapon panel");
        let panel_texts: Vec<String> = panels[0]
            .iter()
            .filter_map(|cell| match cell {
                Cell::Text { t } => Some(t.clone()),
                _ => None,
            })
            .collect();
        assert!(
            panel_texts
                .iter()
                .any(|t| t == "[lightgray]Reload: 2x [white]2.3 /sec"),
            "dagger reload row, got {panel_texts:?}"
        );
        assert!(
            panel_texts
                .iter()
                .any(|t| t.contains("[stat]9[lightgray] damage")),
            "dagger bullet damage row, got {panel_texts:?}"
        );
    }
}
