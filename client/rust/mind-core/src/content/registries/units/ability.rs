// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/entities/abilities/*.java (fields only; update/draw
//         behavior is plan 11).

//! Unit ability metadata (plan 02 M5).
//!
//! Abilities are data records: the class tag plus the constructor arguments and
//! field assignments used by vanilla `UnitTypes.java` and the `NeoplasmUnitType`
//! preset. Only the kinds used by vanilla content are modelled (append-only).

use crate::content::color::Rgba;
use crate::content::registries::fx_meta::EffectId;
use crate::content::registries::pal;
use crate::content::registries::sound_meta::SoundId;

/// Ability class tag (`AbilityKind` content ABI; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbilityKind {
    /// `ShieldRegenFieldAbility`.
    ShieldRegenField = 0,
    /// `RepairFieldAbility`.
    RepairField = 1,
    /// `ForceFieldAbility`.
    ForceField = 2,
    /// `StatusFieldAbility`.
    StatusField = 3,
    /// `EnergyFieldAbility`.
    EnergyField = 4,
    /// `SuppressionFieldAbility`.
    SuppressionField = 5,
    /// `ShieldArcAbility`.
    ShieldArc = 6,
    /// `MoveEffectAbility`.
    MoveEffect = 7,
    /// `SpawnDeathAbility`.
    SpawnDeath = 8,
    /// `RegenAbility` (Neoplasm preset).
    Regen = 9,
    /// `LiquidExplodeAbility` (Neoplasm preset).
    LiquidExplode = 10,
    /// `LiquidRegenAbility` (Neoplasm preset).
    LiquidRegen = 11,
}

impl AbilityKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            AbilityKind::ShieldRegenField => "ShieldRegenFieldAbility",
            AbilityKind::RepairField => "RepairFieldAbility",
            AbilityKind::ForceField => "ForceFieldAbility",
            AbilityKind::StatusField => "StatusFieldAbility",
            AbilityKind::EnergyField => "EnergyFieldAbility",
            AbilityKind::SuppressionField => "SuppressionFieldAbility",
            AbilityKind::ShieldArc => "ShieldArcAbility",
            AbilityKind::MoveEffect => "MoveEffectAbility",
            AbilityKind::SpawnDeath => "SpawnDeathAbility",
            AbilityKind::Regen => "RegenAbility",
            AbilityKind::LiquidExplode => "LiquidExplodeAbility",
            AbilityKind::LiquidRegen => "LiquidRegenAbility",
        }
    }
}

/// Ability metadata record (union of the used kinds' fields).
///
/// Constructor arguments map to the same-named fields; the defaults below are
/// the class field initializers from `abilities/*.java`. Content references
/// (status, liquid, unit) stay name-based and resolve in the load pass.
#[derive(Debug, Clone, PartialEq)]
pub struct AbilitySpec {
    /// Ability class tag.
    pub kind: AbilityKind,
    /// Amount per tick (`ShieldRegenField`/`RepairField`/`EnergyField damage`,
    /// `RegenAbility.amount`, `SpawnDeathAbility.amount` as f32 when whole).
    pub amount: f32,
    /// `ShieldRegenFieldAbility.max` / `ForceFieldAbility.max` /
    /// `ShieldArcAbility.max` (max shield).
    pub max: f32,
    /// Reload ticks.
    pub reload: f32,
    /// Range in world units.
    pub range: f32,
    /// `RepairFieldAbility.healPercent` / `EnergyFieldAbility.healPercent`.
    pub heal_percent: f32,
    /// `*FieldAbility.sameTypeHealMult`.
    pub same_type_heal_mult: f32,
    /// `RepairFieldAbility.maxTargets` / `EnergyFieldAbility.maxTargets`.
    pub max_targets: i32,
    /// `RepairFieldAbility.smartHeal`.
    pub smart_heal: bool,
    /// `RepairFieldAbility.smartDowntime`.
    pub smart_downtime: f32,
    /// `ForceFieldAbility.sides` / `ShieldArcAbility` polygon sides.
    pub sides: i32,
    /// `ForceFieldAbility.rotation`.
    pub rotation: f32,
    /// `ForceFieldAbility.regen` / `ShieldArcAbility.regen`.
    pub regen: f32,
    /// `ForceFieldAbility.cooldown` / `ShieldArcAbility.cooldown`.
    pub cooldown: f32,
    /// `ForceFieldAbility.breakSound`.
    pub break_sound: SoundId,
    /// `StatusFieldAbility.effect` (status name).
    pub effect: Option<&'static str>,
    /// `StatusFieldAbility.duration` / `EnergyFieldAbility.statusDuration`.
    pub duration: f32,
    /// `EnergyFieldAbility.status` (status name).
    pub status: Option<&'static str>,
    /// `SuppressionFieldAbility.orbRadius`.
    pub orb_radius: f32,
    /// `SuppressionFieldAbility.particleSize`.
    pub particle_size: f32,
    /// `SuppressionFieldAbility.particles`.
    pub particles: i32,
    /// `SuppressionFieldAbility.color` (nullable: `Pal.suppress` default).
    pub color: Option<Rgba>,
    /// `SuppressionFieldAbility.effectColor`.
    pub effect_color: Option<Rgba>,
    /// `SuppressionFieldAbility.particleColor` (defaults to `color`).
    pub particle_color: Option<Rgba>,
    /// `SuppressionFieldAbility.active`.
    pub active: bool,
    /// `SuppressionFieldAbility.x`.
    pub x: f32,
    /// `SuppressionFieldAbility.y`.
    pub y: f32,
    /// `ShieldArcAbility.angle`.
    pub angle: f32,
    /// `ShieldArcAbility.width`.
    pub width: f32,
    /// `ShieldArcAbility.chanceDeflect`.
    pub chance_deflect: f32,
    /// `ShieldArcAbility.whenShooting`.
    pub when_shooting: bool,
    /// `ShieldArcAbility.region` (custom region override; empty = default).
    pub region: String,
    /// `MoveEffectAbility.minVelocity`.
    pub min_velocity: f32,
    /// `MoveEffectAbility.interval`.
    pub interval: f32,
    /// `MoveEffectAbility.effect`.
    pub move_effect: EffectId,
    /// `MoveEffectAbility.teamColor`.
    pub team_color: bool,
    /// `SpawnDeathAbility.unit` (unit name).
    pub unit: Option<&'static str>,
    /// `SpawnDeathAbility.randAmount`.
    pub rand_amount: i32,
    /// `SpawnDeathAbility.spread`.
    pub spread: f32,
    /// `RegenAbility.percentAmount`.
    pub percent_amount: f32,
    /// `LiquidExplodeAbility`/`LiquidRegenAbility.liquid` (liquid name).
    pub liquid: Option<&'static str>,
    /// `LiquidRegenAbility.slurpEffect`.
    pub slurp_effect: EffectId,
}

impl AbilitySpec {
    /// Class defaults for `kind` (`abilities/*.java` field initializers).
    pub fn for_kind(kind: AbilityKind) -> Self {
        let mut spec = Self {
            kind,
            amount: 1.0,
            max: 100.0,
            reload: 100.0,
            range: 60.0,
            heal_percent: 0.0,
            same_type_heal_mult: 1.0,
            max_targets: -1,
            smart_heal: false,
            smart_downtime: 60.0 * 8.0,
            sides: 6,
            rotation: 0.0,
            regen: 0.1,
            cooldown: 60.0 * 5.0,
            break_sound: SoundId::SHIELD_BREAK_SMALL,
            effect: None,
            duration: 60.0,
            status: None,
            orb_radius: 4.1,
            particle_size: 4.0,
            particles: 15,
            color: None,
            effect_color: None,
            particle_color: None,
            active: true,
            x: 0.0,
            y: 0.0,
            angle: 80.0,
            width: 6.0,
            chance_deflect: -1.0,
            when_shooting: true,
            region: String::new(),
            min_velocity: 0.08,
            interval: 3.0,
            move_effect: EffectId::MISSILE_TRAIL,
            team_color: false,
            unit: None,
            rand_amount: 0,
            spread: 8.0,
            percent_amount: 0.0,
            liquid: None,
            slurp_effect: EffectId::HEAL,
        };
        match kind {
            AbilityKind::ShieldRegenField | AbilityKind::RepairField => {}
            AbilityKind::ForceField | AbilityKind::ShieldArc => {
                spec.max = 200.0;
            }
            AbilityKind::StatusField => {}
            AbilityKind::EnergyField => {
                spec.max_targets = 25;
                spec.heal_percent = 3.0;
                spec.duration = 60.0 * 6.0;
                spec.status = Some("electrified");
            }
            AbilityKind::SuppressionField => {
                spec.reload = 60.0 * 1.5;
                spec.range = 200.0;
                spec.color = Some(pal::SUPPRESS);
                spec.effect_color = Some(pal::SAP_BULLET);
            }
            AbilityKind::MoveEffect => {}
            AbilityKind::SpawnDeath => {}
            AbilityKind::Regen => {
                // `RegenAbility.amount` defaults to `0` (the union default is
                // the `ShieldRegenField`/`RepairField` `1`).
                spec.amount = 0.0;
            }
            AbilityKind::LiquidExplode => {
                spec.liquid = Some("water");
            }
            AbilityKind::LiquidRegen => {}
        }
        spec
    }

    /// `new ShieldRegenFieldAbility(amount, max, reload, range)`.
    pub fn shield_regen_field(amount: f32, max: f32, reload: f32, range: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::ShieldRegenField);
        spec.amount = amount;
        spec.max = max;
        spec.reload = reload;
        spec.range = range;
        spec
    }

    /// `new RepairFieldAbility(amount, reload, range)`.
    pub fn repair_field(amount: f32, reload: f32, range: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::RepairField);
        spec.amount = amount;
        spec.reload = reload;
        spec.range = range;
        spec
    }

    /// `new ForceFieldAbility(radius, regen, max, cooldown[, sides, rotation])`.
    pub fn force_field(
        radius: f32,
        regen: f32,
        max: f32,
        cooldown: f32,
        sides: Option<i32>,
        rotation: Option<f32>,
    ) -> Self {
        let mut spec = Self::for_kind(AbilityKind::ForceField);
        spec.range = radius;
        spec.regen = regen;
        spec.max = max;
        spec.cooldown = cooldown;
        if let Some(sides) = sides {
            spec.sides = sides;
        }
        if let Some(rotation) = rotation {
            spec.rotation = rotation;
        }
        spec
    }

    /// `new StatusFieldAbility(effect, duration, reload, range)`.
    pub fn status_field(effect: &'static str, duration: f32, reload: f32, range: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::StatusField);
        spec.effect = Some(effect);
        spec.duration = duration;
        spec.reload = reload;
        spec.range = range;
        spec
    }

    /// `new EnergyFieldAbility(damage, reload, range)`.
    pub fn energy_field(damage: f32, reload: f32, range: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::EnergyField);
        spec.amount = damage;
        spec.reload = reload;
        spec.range = range;
        spec
    }

    /// `new SuppressionFieldAbility()`.
    pub fn suppression_field() -> Self {
        Self::for_kind(AbilityKind::SuppressionField)
    }

    /// `new ShieldArcAbility()`.
    pub fn shield_arc() -> Self {
        Self::for_kind(AbilityKind::ShieldArc)
    }

    /// `new MoveEffectAbility(x, y, color, effect, interval)`.
    pub fn move_effect(x: f32, y: f32, color: Rgba, effect: EffectId, interval: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::MoveEffect);
        spec.x = x;
        spec.y = y;
        spec.color = Some(color);
        spec.move_effect = effect;
        spec.interval = interval;
        spec
    }

    /// `new SpawnDeathAbility(unit, amount, spread)`.
    pub fn spawn_death(unit: &'static str, amount: i32, spread: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::SpawnDeath);
        spec.unit = Some(unit);
        spec.amount = amount as f32;
        spec.spread = spread;
        spec
    }

    /// `new RegenAbility()` with `percentAmount`.
    pub fn regen(percent_amount: f32) -> Self {
        let mut spec = Self::for_kind(AbilityKind::Regen);
        spec.percent_amount = percent_amount;
        spec
    }

    /// `new LiquidExplodeAbility()` with `liquid`.
    pub fn liquid_explode(liquid: &'static str) -> Self {
        let mut spec = Self::for_kind(AbilityKind::LiquidExplode);
        spec.liquid = Some(liquid);
        spec
    }

    /// `new LiquidRegenAbility()` with `liquid`/`slurpEffect`.
    pub fn liquid_regen(liquid: &'static str, slurp_effect: EffectId) -> Self {
        let mut spec = Self::for_kind(AbilityKind::LiquidRegen);
        spec.liquid = Some(liquid);
        spec.slurp_effect = slurp_effect;
        spec
    }
}
