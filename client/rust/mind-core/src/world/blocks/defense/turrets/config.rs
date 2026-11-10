// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret ammo registration, Erekir rebalance and per-name config resolution.
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/Turret.java, core/src/mindustry/content/Blocks.java (turret ammo tables)

use std::collections::BTreeMap;

use crate::combat::targeting::UnitSort;
use crate::content::registries::bullets::BulletDef;
use crate::content::registries::sound_meta::SoundId;
use crate::content::registries::units::weapon::ShootPatternSpec;
use crate::content::{BulletId, BulletKind, ContentRegistry, LiquidId};
use crate::world::blocks::defense::turrets::advanced;

use super::{ItemAmmo, LiquidAmmo, PowerAmmo, TurretAmmo, TurretConfig, TurretKind};

/// Registers the plan-10 turret-ammo fixture bullets (`Blocks.java`
/// `ammoTypes`) used by the M5 oracle. Only the `duo` ammo set is ported so
/// far; the remaining vanilla turret ammo is a tracked M5/M6 content delta.
pub fn register_bullets(content: &mut ContentRegistry, names: &mut BTreeMap<String, BulletId>) {
    let mut add = |name: &str, kind: BulletKind, configure: &dyn Fn(&mut BulletDef)| {
        let mut def = BulletDef::new(kind);
        configure(&mut def);
        if let Ok(id) = content.add_bullet(def) {
            names.insert(name.to_owned(), id);
        }
    };

    // `duo` ammo (Blocks.java `duo`): copper/graphite/silicon.
    let shoot = crate::content::registries::sound_meta::SoundId::SHOOT_DUO;
    add("duo_copper", BulletKind::Basic, &|def| {
        def.speed = 2.5;
        def.lifetime = 60.0;
        def.damage = 9.0;
        def.ammo_multiplier = 2.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
        def.shoot_sound = shoot;
    });
    add("duo_graphite", BulletKind::Basic, &|def| {
        def.speed = 3.5;
        def.lifetime = 60.0;
        def.damage = 18.0;
        // `Blocks.java:3295`: `rangeChange = 16f`.
        def.range_change = 16.0;
        def.ammo_multiplier = 4.0;
        def.reload_multiplier = 0.8;
        def.hit_size = 4.0;
        def.drag = 0.0;
        def.shoot_sound = shoot;
    });
    add("duo_silicon", BulletKind::Basic, &|def| {
        def.speed = 3.0;
        def.lifetime = 60.0;
        def.damage = 12.0;
        def.ammo_multiplier = 5.0;
        def.reload_multiplier = 1.5;
        def.homing_power = 0.2;
        def.hit_size = 4.0;
        def.drag = 0.0;
        def.shoot_sound = shoot;
    });

    // M5/M6 remainder: the rest of the vanilla turret ammo tables (`Blocks.java`).
    advanced::register_bullets(content, names);

    // The ammo bullets are anonymous upstream; name them in the registry so
    // `defense::register` (and the live `BlockTable`) can resolve each turret's
    // ammo table against *this* registry's ids (PAR-0086).
    for (name, id) in names.iter() {
        content.name_bullet(name, *id);
    }

    // `Blocks.java` calls `limitRange(...)` in the turret's double-brace init;
    // upstream mutates the shared `BulletType` instances, so the port mutates
    // the (per-turret) ammo `BulletDef`s after all of them exist.
    limit_range(content, names);
    rebalance_erekir_turret_ammo(content, names);
}

/// Every vanilla turret the port resolves a config for (`config_for`).
pub(super) const VANILLA_TURRETS: &[&str] = &[
    "duo",
    "scatter",
    "scorch",
    "hail",
    "salvo",
    "swarmer",
    "fuse",
    "ripple",
    "lancer",
    "arc",
    "meltdown",
    "cyclone",
    "foreshadow",
    "spectre",
    "parallax",
    "segment",
    "wave",
    "tsunami",
    "breach",
    "diffuse",
    "sublimate",
    "titan",
    "disperse",
    "afflict",
    "lustre",
    "scathe",
    "smite",
    "malign",
    "build-tower",
];

/// `ErekirTechTree.rebalance()` part (b) (`ErekirTechTree.java:37-47`).
///
/// Every `Turret` whose requirements include an item outside
/// `Items.serpuloItems` scales its **ammo** bullet damage by `* 0.75`, once per
/// bullet id. Java's `instanceof` chain admits `ItemTurret`,
/// `ContinuousLiquidTurret` and `ContinuousTurret` only — `PowerTurret`
/// (`afflict`, `malign`) is deliberately excluded.
///
/// Ordering note: upstream runs this from `ErekirTechTree.load()` while the
/// turret ammo is created inline in `Blocks.load()` (`ItemTurret.ammo`), so it
/// precedes tech-tree construction. The port's turret ammo is installed after
/// content boot (`install_turret_ammo`, GAPS #11), so the equivalent pass runs
/// at that point; `content::tech::ekir::rebalance` keeps owning part (a).
pub(super) fn rebalance_erekir_turret_ammo(
    content: &mut ContentRegistry,
    names: &BTreeMap<String, BulletId>,
) {
    let serpulo = content.serpulo_items().to_vec();
    let mut balanced: std::collections::BTreeSet<BulletId> = std::collections::BTreeSet::new();
    let mut updates: Vec<(BulletId, f32)> = Vec::new();
    for &name in VANILLA_TURRETS {
        let Some(block) = content.block_by_name(name) else {
            continue;
        };
        // `Structs.contains(block.requirements, i -> !Items.serpuloItems.contains(i.item))`.
        if !block
            .requirements
            .iter()
            .any(|stack| !serpulo.contains(&stack.item))
        {
            continue;
        }
        let Some(config) = config_for(content, name, names) else {
            continue;
        };
        // Java's family gate: ItemTurret | ContinuousLiquidTurret | ContinuousTurret.
        let bullets: Vec<BulletId> = match config.kind {
            TurretKind::Item => match &config.ammo {
                TurretAmmo::Item(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
                _ => Vec::new(),
            },
            TurretKind::Continuous => match &config.ammo {
                TurretAmmo::Liquid(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
                TurretAmmo::Power(ammo) => vec![ammo.bullet],
                TurretAmmo::Item(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
                TurretAmmo::Payload(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
            },
            // `PowerTurret`/`LaserTurret`/`LiquidTurret`/… are not in the Java chain.
            _ => continue,
        };
        for id in bullets {
            if !balanced.insert(id) {
                continue;
            }
            if let Some(def) = content.bullet(id) {
                updates.push((id, def.damage * 0.75));
            }
        }
    }
    for (id, damage) in updates {
        if let Some(def) = content.bullet_mut(id) {
            def.damage = damage;
        }
    }
}

/// `Blocks.java` `limitRange(margin)` calls (`Turret.limitRange`,
/// `Turret.java:246-253`). Absent names do not limit their ammo.
///
/// Serpulo: `duo` 5, `scatter` 2, `hail` 0, `swarmer` 5, `salvo`/`cyclone`/
/// `spectre` default 9. Erekir: `breach` 12, `diffuse` 25, `disperse` 16,
/// `afflict` -55 (`PowerTurret.limitRange`), `scathe`/`smite` default 9.
pub(super) const LIMIT_RANGE_MARGINS: &[(&str, f32)] = &[
    // `Blocks.java:3348`, `:3434`, `:3544`, `:3748`, `:3853`, `:4225`, `:4338`.
    ("duo", 5.0),
    ("scatter", 2.0),
    ("hail", 0.0),
    ("swarmer", 5.0),
    ("salvo", 9.0),
    ("cyclone", 9.0),
    ("spectre", 9.0),
    // `Blocks.java:4487`, `:4577`, `:5054`, `:5177`, `:5686`, `:5953`.
    ("breach", 12.0),
    ("diffuse", 25.0),
    ("disperse", 16.0),
    ("afflict", -55.0),
    ("scathe", 9.0),
    ("smite", 9.0),
];

/// `BulletType.extraRangeMargin` (`BulletType.java:166-167`): only `disperse`'s
/// thorium/silicon ammo set it (`Blocks.java:4922`, `:4950` = 32).
pub(super) const EXTRA_RANGE_MARGINS: &[(&str, f32)] =
    &[("disperse_thorium", 32.0), ("disperse_silicon", 32.0)];

/// `Turret.limitRange(BulletType, margin)` (`Turret.java:246-253`):
/// `bullet.lifetime = (rangeChange + range + margin + extraRangeMargin + 10) / speed`.
pub(super) fn limit_range(content: &mut ContentRegistry, names: &BTreeMap<String, BulletId>) {
    let mut updates: Vec<(BulletId, f32)> = Vec::new();
    for &(turret, margin) in LIMIT_RANGE_MARGINS {
        let Some(config) = config_for(content, turret, names) else {
            continue;
        };
        let bullets: Vec<BulletId> = match &config.ammo {
            TurretAmmo::Item(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
            TurretAmmo::Liquid(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
            TurretAmmo::Power(ammo) => vec![ammo.bullet],
            TurretAmmo::Payload(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
        };
        for id in bullets {
            let Some(def) = content.bullet(id) else {
                continue;
            };
            // `lifetime = (…) / speed`: a zero-speed bullet (LightningBulletType)
            // has no travel time to limit; Java would divide by zero.
            if def.speed <= 0.0 {
                continue;
            }
            let extra = names
                .iter()
                .find_map(|(name, other)| (*other == id).then_some(name.as_str()))
                .and_then(|name| {
                    EXTRA_RANGE_MARGINS
                        .iter()
                        .find_map(|&(other, extra)| (other == name).then_some(extra))
                })
                .unwrap_or(0.0);
            let real_range = def.range_change + config.range;
            updates.push((id, (real_range + margin + extra + 10.0) / def.speed));
        }
    }
    for (id, lifetime) in updates {
        if let Some(def) = content.bullet_mut(id) {
            def.lifetime = lifetime;
        }
    }
}

/// Registers the vanilla turret ammo bullets into a booted registry once
/// (idempotent). The live sim's content boot calls this so placed turrets get
/// a real [`behavior::TurretBehavior`] instead of the inert default (PAR-0086).
pub fn install_turret_ammo(content: &mut ContentRegistry) {
    if content.bullet_by_name("duo_copper").is_some() {
        return;
    }
    let mut names = BTreeMap::new();
    register_bullets(content, &mut names);
    // The anonymous ammo bullets are appended after the registry's `link`
    // pass, so patch their `BulletType.estimateDPS()` too (`BulletType.java:
    // 424-441`; `Turret.estimateDps` reads it for the RTS `estimateStats`).
    crate::content::registries::units::compute_bullet_dps(content);
}

/// Builds the resolved [`TurretConfig`] for a supported block name.
///
/// `names` is the harness bullet-name map (vanilla bullets are anonymous, so
/// the plan-10 fixtures are addressed by name). `None` for unported turrets.
pub fn config_for(
    content: &ContentRegistry,
    name: &str,
    names: &BTreeMap<String, BulletId>,
) -> Option<TurretConfig> {
    let mut config = config_for_inner(content, name, names)?;
    // `Turret.init` (`Turret.java:223`): `trackingRange = Math.max(range,
    // trackingRange)`.
    config.tracking_range = config.tracking_range.max(config.range);
    apply_block_coolant(content, name, &mut config);
    apply_turret_sounds(name, &mut config);
    // Harness fixtures (`test-item`, ...) carry a coolant amount but no block
    // consumer: default to the `consumeCoolant` liquid filter.
    if config.uses_coolant() && config.coolant_liquids.is_empty() {
        config.coolant_liquids = coolant_filter(content, true, false);
    }
    Some(config)
}

/// `ConsumeCoolant.filter`: `coolant && (allowLiquid && !gas || allowGas &&
/// gas) && temperature <= maxTemp(0.5) && flammability < maxFlammability(0.1)`.
pub(super) fn coolant_filter(
    content: &ContentRegistry,
    allow_liquid: bool,
    allow_gas: bool,
) -> Vec<LiquidId> {
    content
        .liquids()
        .iter()
        .filter(|l| {
            l.coolant
                && ((allow_liquid && !l.gas) || (allow_gas && l.gas))
                && l.temperature <= 0.5
                && l.flammability < 0.1
        })
        .map(|l| l.id)
        .collect()
}

/// `Blocks.java` `coolantMultiplier` for the vanilla turrets that declare a
/// coolant consumer (absent = the `BaseTurret` default 5, or the class
/// default: `LaserTurret`/`ContinuousTurret`/`TractorBeamTurret` 1,
/// `PointDefenseTurret` 2).
pub(super) const COOLANT_MULTIPLIERS: &[(&str, f32)] = &[
    ("duo", 10.0),
    ("scorch", 1.5),
    ("hail", 10.0),
    ("arc", 30.0),
    ("foreshadow", 0.4),
    ("spectre", 0.5),
    ("meltdown", 1.0),
    ("breach", 15.0),
    ("diffuse", 15.0),
    ("titan", 3.75),
    ("disperse", 6.25),
    ("scathe", 15.0),
    ("smite", 15.0),
];

/// `Blocks.java` turret sound fields (name, `shootSound`, `shootSoundVolume`,
/// `chargeSound`, `loopSound`, `loopSoundVolume`). Absent names keep the
/// `Turret` class defaults (`Turret.java:127-138`).
pub(super) const TURRET_SOUNDS: &[(&str, &str, f32, &str, &str, f32)] = &[
    ("duo", "shootDuo", 1.0, "none", "none", 0.5),
    ("scatter", "shootScatter", 1.0, "none", "none", 0.5),
    ("scorch", "shootFlame", 1.0, "none", "none", 0.5),
    ("hail", "shootArtillerySmall", 1.0, "none", "none", 0.5),
    ("wave", "none", 1.0, "none", "loopSpray", 0.5),
    ("tsunami", "none", 1.0, "none", "loopSpray", 0.5),
    ("lancer", "shootLancer", 1.0, "chargeLancer", "none", 0.5),
    ("arc", "shootArc", 1.0, "none", "none", 0.5),
    ("swarmer", "shootMissile", 1.0, "none", "none", 0.5),
    ("salvo", "shootSalvo", 1.0, "none", "none", 0.5),
    ("fuse", "shootFuse", 0.9, "none", "none", 0.5),
    ("ripple", "shootRipple", 1.0, "none", "none", 0.5),
    ("cyclone", "shootCyclone", 1.0, "none", "none", 0.5),
    ("foreshadow", "shootForeshadow", 1.0, "none", "none", 0.5),
    ("spectre", "shootSpectre", 1.0, "none", "none", 0.5),
    (
        "meltdown",
        "shootMeltdown",
        1.0,
        "none",
        "beamMeltdown",
        2.0,
    ),
    ("breach", "shootBreach", 1.0, "none", "none", 0.5),
    ("diffuse", "shootDiffuse", 1.0, "none", "none", 0.5),
    ("sublimate", "none", 1.0, "none", "shootSublimate", 1.0),
    ("titan", "shootTank", 1.0, "none", "none", 0.5),
    ("disperse", "shootDisperse", 1.0, "none", "none", 0.5),
    ("lustre", "none", 1.0, "none", "beamLustre", 1.0),
    ("scathe", "shootScathe", 1.0, "none", "none", 0.5),
    ("smite", "shootSmite", 1.0, "none", "loopGlow", 0.8),
    ("malign", "shootMalign", 1.0, "none", "loopMalign", 1.3),
    ("segment", "shootSegment", 1.0, "none", "none", 0.5),
];

/// Applies the `Blocks.java` sound overrides for one turret name.
pub(super) fn apply_turret_sounds(name: &str, config: &mut TurretConfig) {
    let Some(row) = TURRET_SOUNDS.iter().find(|row| row.0 == name) else {
        return;
    };
    let id = |n: &str| SoundId::by_name(n).unwrap_or(SoundId::NONE);
    config.shoot_sound = id(row.1);
    config.shoot_sound_volume = row.2;
    config.charge_sound = id(row.3);
    config.loop_sound = id(row.4);
    config.loop_sound_volume = row.5;
}

/// `BaseTurret.init`: the coolant consumer comes from the block's content
/// spec (`coolant = consumeCoolant(x)` / `consume(new ConsumeLiquid(water,
/// x))`, both made optional), so the config mirrors the content data
/// (PAR-0087).
pub(super) fn apply_block_coolant(
    content: &ContentRegistry,
    name: &str,
    config: &mut TurretConfig,
) {
    use crate::content::Consume;
    let Some(def) = content.block_id(name).and_then(|id| content.block(id)) else {
        return;
    };
    let mut found = false;
    for spec in &def.consumes {
        match &spec.consume {
            Consume::Coolant {
                amount,
                allow_liquid,
                allow_gas,
            } => {
                config.coolant_amount = *amount;
                // `ConsumeCoolant` filter (maxTemp 0.5, maxFlammability 0.1).
                config.coolant_liquids = coolant_filter(content, *allow_liquid, *allow_gas);
                found = true;
            }
            Consume::Liquid { liquid, amount } if spec.optional => {
                config.coolant_amount = *amount;
                config.coolant_liquids = vec![*liquid];
                found = true;
            }
            _ => {}
        }
    }
    if !found {
        return;
    }
    config.coolant_multiplier = COOLANT_MULTIPLIERS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, m)| *m)
        .unwrap_or(match config.kind {
            TurretKind::Laser | TurretKind::Continuous | TurretKind::TractorBeam => 1.0,
            TurretKind::PointDefense => 2.0,
            _ => 5.0,
        });
}

pub(super) fn config_for_inner(
    content: &ContentRegistry,
    name: &str,
    names: &BTreeMap<String, BulletId>,
) -> Option<TurretConfig> {
    let item = |n: &str| content.item_id(n);
    let liquid = |n: &str| content.liquid_id(n);
    let bullet = |n: &str| names.get(n).copied();
    let item_ammo = |item_name: &str, bullet_name: &str| -> Option<ItemAmmo> {
        let id = bullet(bullet_name)?;
        let def = content.bullet(id);
        let mult = def.map(|d| d.ammo_multiplier as i32).unwrap_or(1);
        Some(ItemAmmo {
            item: item(item_name)?,
            bullet: id,
            ammo_multiplier: mult.max(1),
            range_change: def.map(|d| d.range_change).unwrap_or(0.0),
            min_range_change: def.map(|d| d.min_range_change).unwrap_or(0.0),
        })
    };
    let liquid_ammo = |liquid_name: &str, bullet_name: &str| -> Option<LiquidAmmo> {
        let id = bullet(bullet_name)?;
        let def = content.bullet(id);
        let mult = def.map(|d| d.ammo_multiplier).unwrap_or(1.0);
        Some(LiquidAmmo {
            liquid: liquid(liquid_name)?,
            bullet: id,
            ammo_multiplier: mult.max(0.001),
            range_change: def.map(|d| d.range_change).unwrap_or(0.0),
            min_range_change: def.map(|d| d.min_range_change).unwrap_or(0.0),
        })
    };

    match name {
        "duo" => {
            let copper = item_ammo("copper", "duo_copper")?;
            let graphite = item_ammo("graphite", "duo_graphite")?;
            let silicon = item_ammo("silicon", "duo_silicon")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Item(vec![copper, graphite, silicon]),
                liquid_consumed: 1.0 / 60.0,
                range: 160.0,
                reload: 20.0,
                max_ammo: 30,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 10.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.1,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 20.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 2.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 0.5,
                recoils: 2,
                recoil_time: 20.0,
                recoil_pow: 1.8,
                cooldown_time: 20.0,
                shoot_x: 0.0,
                shoot_y: 3.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::alternate(1, 0.0, 3.5, 2),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 10.0,
                coolant_amount: 0.1,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        "test-item" => {
            let copper = item_ammo("copper", "duo_copper")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Item(vec![copper]),
                liquid_consumed: 1.0 / 60.0,
                range: 120.0,
                reload: 10.0,
                max_ammo: 30,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 20.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.5,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 5.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 1.0,
                recoils: -1,
                recoil_time: 10.0,
                recoil_pow: 1.8,
                cooldown_time: 10.0,
                shoot_x: 0.0,
                shoot_y: 0.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 10.0,
                coolant_amount: 0.1,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        "test-liquid" => {
            let water = liquid_ammo("water", "fuse")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Liquid(vec![water]),
                liquid_consumed: 1.0 / 60.0,
                range: 120.0,
                reload: 5.0,
                max_ammo: 0,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 20.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.5,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 5.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 1.0,
                recoils: -1,
                recoil_time: 5.0,
                recoil_pow: 1.8,
                cooldown_time: 5.0,
                shoot_x: 0.0,
                shoot_y: 0.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 0.0,
                coolant_amount: 0.0,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        "test-power" => {
            let bullet = bullet("fuse")?;
            let def = content.bullet(bullet);
            Some(TurretConfig {
                ammo: TurretAmmo::Power(PowerAmmo {
                    bullet,
                    range_change: def.map(|d| d.range_change).unwrap_or(0.0),
                    min_range_change: def.map(|d| d.min_range_change).unwrap_or(0.0),
                }),
                liquid_consumed: 1.0 / 60.0,
                range: 120.0,
                reload: 10.0,
                max_ammo: 0,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 20.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.5,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 5.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 1.0,
                recoils: -1,
                recoil_time: 10.0,
                recoil_pow: 1.8,
                cooldown_time: 10.0,
                shoot_x: 0.0,
                shoot_y: 0.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 0.0,
                coolant_amount: 0.0,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        _ => advanced::config_for(content, name, names),
    }
}
