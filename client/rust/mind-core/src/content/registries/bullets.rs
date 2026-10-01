// SPDX-License-Identifier: GPL-3.0-only

//! Internal bullet registry (metadata half).
//!
//! Ported from `core/src/mindustry/content/Bullets.java` (6 shared internal
//! bullets), `core/src/mindustry/entities/bullet/BulletType.java`,
//! `BasicBulletType.java`, `FireBulletType.java` and `SpaceLiquidBulletType.java`
//! (field defaults only; behavior in plan 10).

use super::super::color::Rgba;
use super::super::ctype::{Content, ModContentInfo};
use super::super::id::{BulletId, LiquidId, StatusId, UnitTypeId};
use super::super::load::ContentRegistry;
use super::super::{ContentError, ContentType};
use super::fx_meta::EffectId;

/// Bullet class tag (`BulletType` hierarchy), used by plan 10 behavior dispatch
/// and by audit output. Names match the Java classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BulletKind {
    /// Plain `BulletType`.
    Plain,
    /// `BasicBulletType`.
    Basic,
    /// `FireBulletType`.
    Fire,
    /// `SpaceLiquidBulletType`.
    SpaceLiquid,
}

impl BulletKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            BulletKind::Plain => "BulletType",
            BulletKind::Basic => "BasicBulletType",
            BulletKind::Fire => "FireBulletType",
            BulletKind::SpaceLiquid => "SpaceLiquidBulletType",
        }
    }
}

/// Bullet content record (`mindustry.entities.bullet.BulletType` metadata).
#[derive(Debug, Clone, PartialEq)]
pub struct BulletDef {
    /// Dense id in the bullet content space.
    pub id: BulletId,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Class-kind tag.
    pub kind: BulletKind,
    /// Lifetime in ticks.
    pub lifetime: f32,
    /// Speed in units/tick.
    pub speed: f32,
    /// Direct damage on hit.
    pub damage: f32,
    /// Hitbox size.
    pub hit_size: f32,
    /// Clipping hitbox.
    pub draw_size: f32,
    /// Drag as a fraction of velocity.
    pub drag: f32,
    /// Acceleration per frame.
    pub accel: f32,
    /// Whether velocity is inherited from the shooter.
    pub keep_velocity: bool,
    /// Whether lifetime is scaled to disappear at the target (artillery).
    pub scale_life: bool,
    /// Whether to pierce units.
    pub pierce: bool,
    /// Whether to pierce buildings.
    pub pierce_building: bool,
    /// Max number of pierced objects (`-1` unlimited).
    pub pierce_cap: i32,
    /// Multiplier of damage decreased per health pierced.
    pub pierce_damage_factor: f32,
    /// Multiplied by turret reload speed to get the final shoot speed.
    pub reload_multiplier: f32,
    /// Ammo created per item/liquid.
    pub ammo_multiplier: f32,
    /// Splash damage (`0` disables).
    pub splash_damage: f32,
    /// Splash damage radius (`<0` disables).
    pub splash_damage_radius: f32,
    /// Whether splash damage is scaled by hitbox size.
    pub scaled_splash_damage: bool,
    /// Damage multiplier against tiles.
    pub building_damage_multiplier: f32,
    /// Damage multiplier against force shields.
    pub shield_damage_multiplier: f32,
    /// Status effect applied on hit.
    pub status: StatusId,
    /// Applied status duration.
    pub status_duration: f32,
    /// Chance to apply the status.
    pub status_chance: f32,
    /// Whether unit armor is ignored.
    pub pierce_armor: bool,
    /// Multiplier of unit/building armor used in damage calculations.
    pub armor_multiplier: f32,
    /// Multiplier of building armor only.
    pub block_armor_multiplier: f32,
    /// Whether the bullet can be hit by point defense.
    pub hittable: bool,
    /// Whether the bullet can be reflected.
    pub reflectable: bool,
    /// Whether the projectile can be absorbed by shields.
    pub absorbable: bool,
    /// Whether the bullet collides with anything at all.
    pub collides: bool,
    /// Whether the bullet collides with air units.
    pub collides_air: bool,
    /// Whether the bullet collides with ground units.
    pub collides_ground: bool,
    /// Whether the bullet collides with tiles.
    pub collides_tiles: bool,
    /// Whether the bullet collides with friendly tiles.
    pub collides_team: bool,
    /// Effect shown on direct hit.
    pub hit_effect: EffectId,
    /// Effect shown when the bullet despawns.
    pub despawn_effect: EffectId,
    /// Effect created when shooting.
    pub shoot_effect: EffectId,
    /// Extra smoke effect created when shooting.
    pub smoke_effect: EffectId,
    /// Trail effect spawned behind the bullet.
    pub trail_effect: EffectId,
    /// Trail length (`<=0` disables the trail).
    pub trail_length: i32,
    /// Frag bullet created on hit/despawn.
    pub frag_bullet: Option<BulletId>,
    /// Number of frag bullets created.
    pub frag_bullets: i32,
    /// Whether frags are created on hit.
    pub frag_on_hit: bool,
    /// Whether frags are created on despawn.
    pub frag_on_despawn: bool,
    /// Bullet created at a fixed interval.
    pub interval_bullet: Option<BulletId>,
    /// Interval in ticks between interval bullets.
    pub bullet_interval: f32,
    /// Lightning root count.
    pub lightning: i32,
    /// Lightning strand length.
    pub lightning_length: i32,
    /// Lightning damage (`<0` uses bullet damage).
    pub lightning_damage: f32,
    /// Lightning spread cone.
    pub lightning_cone: f32,
    /// Lightning angle offset.
    pub lightning_angle: f32,
    /// Number of puddles created.
    pub puddles: i32,
    /// Range of puddles around the bullet.
    pub puddle_range: f32,
    /// Liquid count per puddle.
    pub puddle_amount: f32,
    /// Liquid the puddles consist of.
    pub puddle_liquid: LiquidId,
    /// Unit spawned instead of the bullet.
    pub spawn_unit: Option<UnitTypeId>,
    /// Unit spawned on despawn.
    pub despawn_unit: Option<UnitTypeId>,
    /// Front sprite region name.
    pub sprite: Option<String>,
    /// Back sprite region name.
    pub back_sprite: Option<String>,
    /// Sprite width.
    pub width: f32,
    /// Sprite height.
    pub height: f32,
    /// Knockback in velocity.
    pub knockback: f32,
    /// Color used for hit/despawn effects.
    pub hit_color: Rgba,
    /// Color of light emitted by the bullet.
    pub light_color: Rgba,
    /// Whether the bullet creates fires on impact.
    pub make_fire: bool,
    /// Number of fires attempted around the bullet.
    pub incend_amount: i32,
    /// Spread of fires around the bullet.
    pub incend_spread: f32,
    /// Chance of fire creation.
    pub incend_chance: f32,
    /// Lifesteal fraction of dealt damage.
    pub lifesteal: f32,
    /// Flat block healing on hit.
    pub heal_amount: f32,
    /// Percent block healing on hit.
    pub heal_percent: f32,
}

impl BulletDef {
    /// Creates a bullet with `BulletType` defaults; kind-specific subclass
    /// initializer defaults are applied here (Basic/Fire/SpaceLiquid).
    pub fn new(kind: BulletKind) -> Self {
        let mut bullet = Self {
            id: BulletId::new(0),
            minfo: ModContentInfo::default(),
            removed: false,
            kind,
            lifetime: 40.0,
            speed: 1.0,
            damage: 1.0,
            hit_size: 4.0,
            draw_size: 40.0,
            drag: 0.0,
            accel: 0.0,
            keep_velocity: true,
            scale_life: false,
            pierce: false,
            pierce_building: false,
            pierce_cap: -1,
            pierce_damage_factor: 0.0,
            reload_multiplier: 1.0,
            ammo_multiplier: 2.0,
            splash_damage: 0.0,
            splash_damage_radius: -1.0,
            scaled_splash_damage: false,
            building_damage_multiplier: 1.0,
            shield_damage_multiplier: 1.0,
            status: StatusId::NONE,
            status_duration: 60.0 * 8.0,
            status_chance: 1.0,
            pierce_armor: false,
            armor_multiplier: 1.0,
            block_armor_multiplier: 1.0,
            hittable: true,
            reflectable: true,
            absorbable: true,
            collides: true,
            collides_air: true,
            collides_ground: true,
            collides_tiles: true,
            collides_team: false,
            hit_effect: EffectId::HIT_BULLET_SMALL,
            despawn_effect: EffectId::HIT_BULLET_SMALL,
            shoot_effect: EffectId::SHOOT_SMALL,
            smoke_effect: EffectId::SHOOT_SMALL_SMOKE,
            trail_effect: EffectId::MISSILE_TRAIL,
            trail_length: -1,
            frag_bullet: None,
            frag_bullets: 9,
            frag_on_hit: true,
            frag_on_despawn: true,
            interval_bullet: None,
            bullet_interval: 20.0,
            lightning: 0,
            lightning_length: 5,
            lightning_damage: -1.0,
            lightning_cone: 360.0,
            lightning_angle: 0.0,
            puddles: 0,
            puddle_range: 0.0,
            puddle_amount: 5.0,
            puddle_liquid: LiquidId::WATER,
            spawn_unit: None,
            despawn_unit: None,
            sprite: None,
            back_sprite: None,
            width: 5.0,
            height: 7.0,
            knockback: 0.0,
            hit_color: Rgba::WHITE,
            light_color: Rgba::from_rgba8888(0xfbd367ff),
            make_fire: false,
            incend_amount: 0,
            incend_spread: 8.0,
            incend_chance: 1.0,
            lifesteal: 0.0,
            heal_amount: 0.0,
            heal_percent: 0.0,
        };
        match kind {
            BulletKind::Plain => {}
            BulletKind::Basic => {
                bullet.sprite = Some(String::from("bullet"));
            }
            BulletKind::Fire => {
                // `FireBulletType` instance initializer.
                bullet.pierce = true;
                bullet.collides_tiles = false;
                bullet.collides = false;
                bullet.drag = 0.03;
                bullet.hit_effect = EffectId::NONE;
                bullet.despawn_effect = EffectId::NONE;
                bullet.trail_effect = EffectId::FIREBALLSMOKE;
            }
            BulletKind::SpaceLiquid => {
                // `SpaceLiquidBulletType()` constructor.
                bullet.speed = 3.5;
                bullet.damage = 0.0;
                bullet.collides = false;
                bullet.lifetime = 90.0;
                bullet.despawn_effect = EffectId::NONE;
                bullet.hit_effect = EffectId::NONE;
                bullet.smoke_effect = EffectId::NONE;
                bullet.shoot_effect = EffectId::NONE;
                bullet.drag = 0.002;
                bullet.hittable = false;
            }
        }
        bullet
    }
}

impl Content for BulletDef {
    const TYPE: ContentType = ContentType::Bullet;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = BulletId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        self.kind.name()
    }
}

/// Loads the 6 shared internal bullets in `Bullets.load()` order.
pub fn load(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    // Status effects load before bullets (upstream order).
    let shocked = registry
        .status_id("shocked")
        .ok_or_else(|| ContentError::UnknownName(String::from("shocked")))?;

    // Not allowed in weapons - used only to prevent NullPointerExceptions.
    registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::Basic);
        bullet.speed = 2.5;
        bullet.damage = 9.0;
        bullet.sprite = Some(String::from("ohno"));
        bullet.width = 7.0;
        bullet.height = 9.0;
        bullet.lifetime = 60.0;
        bullet.ammo_multiplier = 2.0;
        bullet
    })?;

    // Lightning bullets need to be initialized first.
    let damage_lightning = registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::Plain);
        bullet.speed = 0.0001;
        bullet.damage = 0.0;
        bullet.lifetime = EffectId::LIGHTNING.meta().lifetime;
        bullet.hit_effect = EffectId::HIT_LANCER;
        bullet.despawn_effect = EffectId::NONE;
        bullet.status = shocked;
        bullet.status_duration = 10.0;
        bullet.hittable = false;
        bullet.light_color = Rgba::WHITE;
        bullet
    })?;

    // Copy that does not damage air units.
    let base = registry
        .bullet(damage_lightning)
        .ok_or(ContentError::UnknownId(damage_lightning.raw()))?
        .clone();
    let mut ground = base.clone();
    ground.collides_air = false;
    registry.add_bullet(ground)?;

    // Copy that does not damage ground units or tiles.
    let mut air = base;
    air.collides_ground = false;
    air.collides_tiles = false;
    registry.add_bullet(air)?;

    registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::Fire);
        bullet.speed = 1.0;
        bullet.damage = 4.0;
        bullet.hittable = false;
        bullet
    })?;

    registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::SpaceLiquid);
        bullet.knockback = 0.7;
        bullet.drag = 0.01;
        bullet
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `bullets::damage_lightning_copy_flags` (plan 02 §5 M1) plus the internal
    /// bullet ids/values.
    #[test]
    fn damage_lightning_copy_flags() {
        let registry = test_registry();
        assert_eq!(registry.bullets().len(), 6, "bullet count");

        let placeholder = registry.bullet(BulletId::new(0)).unwrap();
        assert_eq!(placeholder.speed, 2.5);
        assert_eq!(placeholder.damage, 9.0);
        assert_eq!(placeholder.sprite.as_deref(), Some("ohno"));
        assert_eq!((placeholder.width, placeholder.height), (7.0, 9.0));
        assert_eq!(placeholder.lifetime, 60.0);
        assert_eq!(placeholder.ammo_multiplier, 2.0);

        let lightning = registry.bullet(BulletId::new(1)).unwrap();
        assert_eq!(lightning.lifetime, EffectId::LIGHTNING.meta().lifetime);
        assert_eq!(lightning.hit_effect, EffectId::HIT_LANCER);
        assert_eq!(lightning.despawn_effect, EffectId::NONE);
        assert_eq!(lightning.status, registry.status_id("shocked").unwrap());
        assert_eq!(lightning.status_duration, 10.0);
        assert!(!lightning.hittable);

        let ground = registry.bullet(BulletId::new(2)).unwrap();
        assert!(!ground.collides_air);
        assert!(ground.collides_ground);
        assert!(ground.collides_tiles);
        assert_eq!(ground.lifetime, lightning.lifetime);
        assert_eq!(ground.hit_effect, lightning.hit_effect);

        let air = registry.bullet(BulletId::new(3)).unwrap();
        assert!(air.collides_air);
        assert!(!air.collides_ground);
        assert!(!air.collides_tiles);

        let fireball = registry.bullet(BulletId::new(4)).unwrap();
        assert_eq!(fireball.speed, 1.0);
        assert_eq!(fireball.damage, 4.0);
        assert!(!fireball.hittable);
        assert!(fireball.pierce);
        assert!(!fireball.collides);
        assert_eq!(fireball.drag, 0.03);

        let space = registry.bullet(BulletId::new(5)).unwrap();
        assert_eq!(space.knockback, 0.7);
        assert_eq!(space.drag, 0.01);
        assert_eq!(space.speed, 3.5);
        assert_eq!(space.lifetime, 90.0);
        assert!(!space.hittable);
    }
}
