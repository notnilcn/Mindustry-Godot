// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet parsing and `register_bullet_spec`.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    /// Parses an inline `bullet` object into a [`BulletSpec`] (M2 subset).
    pub(super) fn parse_bullet(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        object: &Map<String, Value>,
    ) -> Result<BulletSpec, ContentParseError> {
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .map(|class| {
                self.classmap
                    .resolve(class, ClassScope::BulletType)
                    .and_then(resolve_bullet_kind)
                    .ok_or_else(|| {
                        ContentParseError::new(format!("{file}: unknown bullet type `{class}`"))
                    })
            })
            .transpose()?
            .unwrap_or(BulletKind::Plain);
        let mut spec = BulletSpec {
            kind,
            ..BulletSpec::default()
        };
        set_opt_f32s(
            object,
            &mut [
                ("damage", &mut spec.damage),
                ("speed", &mut spec.speed),
                ("lifetime", &mut spec.lifetime),
                ("hitSize", &mut spec.hit_size),
                ("drawSize", &mut spec.draw_size),
                ("splashDamage", &mut spec.splash_damage),
                ("splashDamageRadius", &mut spec.splash_damage_radius),
            ],
        );
        set_opt_i32s(
            object,
            &mut [
                ("lightning", &mut spec.lightning),
                ("lightningLength", &mut spec.lightning_length),
                ("lightningLengthRand", &mut spec.lightning_length_rand),
            ],
        );
        set_opt_bools(
            object,
            &mut [
                ("pierce", &mut spec.pierce),
                ("pierceBuilding", &mut spec.pierce_building),
                ("keepVelocity", &mut spec.keep_velocity),
                ("collides", &mut spec.collides),
            ],
        );
        set_opt_f32s(
            object,
            &mut [
                ("width", &mut spec.width),
                ("height", &mut spec.height),
                ("length", &mut spec.length),
                ("ammoMultiplier", &mut spec.ammo_multiplier),
                ("reloadMultiplier", &mut spec.reload_multiplier),
                ("pierceDamageFactor", &mut spec.pierce_damage_factor),
            ],
        );
        if let Some(value) = field_i32(object, "pierceCap") {
            spec.pierce_cap = Some(value);
        }
        if let Some(color) = object.get("frontColor").and_then(parse_color) {
            spec.front_color = Some(color);
        }
        if let Some(color) = object.get("backColor").and_then(parse_color) {
            spec.back_color = Some(color);
        }
        if let Some(Value::Array(entries)) = object.get("colors") {
            let mut colors = Vec::with_capacity(entries.len());
            for entry in entries {
                if let Some(color) = parse_color(entry) {
                    colors.push(color);
                }
            }
            spec.colors = Some(colors);
        }
        if let Some(status) = object.get("status").and_then(Value::as_str) {
            // Bullet status is a name resolved during the unit load pass; keep raw.
            spec.status = Some(Box::leak(status.to_owned().into_boxed_str()));
        }
        // Bullet visual/audio fields (`BulletType.hitEffect`/`shootEffect`/…,
        // `BulletType.java:95-160`).
        for (field, target) in [
            ("hitEffect", &mut spec.hit_effect),
            ("despawnEffect", &mut spec.despawn_effect),
            ("shootEffect", &mut spec.shoot_effect),
            ("smokeEffect", &mut spec.smoke_effect),
            ("chargeEffect", &mut spec.charge_effect),
            ("trailEffect", &mut spec.trail_effect),
        ] {
            if let Some(value) = object.get(field)
                && let Some(effect) = self.parse_effect_ref(file, value)
            {
                *target = Some(effect);
            }
        }
        for (field, target) in [
            ("shootSound", &mut spec.shoot_sound),
            ("hitSound", &mut spec.hit_sound),
            ("despawnSound", &mut spec.despawn_sound),
        ] {
            if let Some(sound) = self.parse_sound(file, object.get(field)) {
                *target = Some(sound);
            }
        }
        set_opt_f32s(
            object,
            &mut [
                ("hitSoundVolume", &mut spec.hit_sound_volume),
                ("hitSoundPitch", &mut spec.hit_sound_pitch),
                ("hitSoundPitchRange", &mut spec.hit_sound_pitch_range),
            ],
        );
        // Remaining `BulletType` scalar fields (`BulletType.java:55-230`).
        set_opt_f32s(
            object,
            &mut [
                ("rangeOverride", &mut spec.range_override),
                ("maxRange", &mut spec.max_range),
                ("homingPower", &mut spec.homing_power),
                ("homingRange", &mut spec.homing_range),
                ("homingDelay", &mut spec.homing_delay),
                ("followAimSpeed", &mut spec.follow_aim_speed),
                ("weaveScale", &mut spec.weave_scale),
                ("weaveMag", &mut spec.weave_mag),
                ("optimalLifeFract", &mut spec.optimal_life_fract),
                ("maxDamageFraction", &mut spec.max_damage_fraction),
                ("armorMultiplier", &mut spec.armor_multiplier),
                ("blockArmorMultiplier", &mut spec.block_armor_multiplier),
                (
                    "buildingDamageMultiplier",
                    &mut spec.building_damage_multiplier,
                ),
                ("shieldDamageMultiplier", &mut spec.shield_damage_multiplier),
                ("statusDuration", &mut spec.status_duration),
                ("statusChance", &mut spec.status_chance),
                ("stickyExtraLifetime", &mut spec.sticky_extra_lifetime),
                ("inaccuracy", &mut spec.inaccuracy),
                ("hitShake", &mut spec.hit_shake),
                ("despawnShake", &mut spec.despawn_shake),
                ("trailWidth", &mut spec.trail_width),
                ("trailChance", &mut spec.trail_chance),
                ("trailInterval", &mut spec.trail_interval),
                ("trailParam", &mut spec.trail_param),
                ("trailSpread", &mut spec.trail_spread),
                ("fragRandomSpread", &mut spec.frag_random_spread),
                ("fragSpread", &mut spec.frag_spread),
                ("fragAngle", &mut spec.frag_angle),
                ("fragVelocityMin", &mut spec.frag_velocity_min),
                ("fragVelocityMax", &mut spec.frag_velocity_max),
                ("fragLifeMin", &mut spec.frag_life_min),
                ("fragLifeMax", &mut spec.frag_life_max),
                ("fragOffsetMin", &mut spec.frag_offset_min),
                ("fragOffsetMax", &mut spec.frag_offset_max),
                ("bulletInterval", &mut spec.bullet_interval),
                ("intervalRandomSpread", &mut spec.interval_random_spread),
                ("intervalSpread", &mut spec.interval_spread),
                ("intervalAngle", &mut spec.interval_angle),
                ("intervalDelay", &mut spec.interval_delay),
                ("lightningDamage", &mut spec.lightning_damage),
                ("lightningCone", &mut spec.lightning_cone),
                ("lightningAngle", &mut spec.lightning_angle),
                (
                    "spawnBulletRandomSpread",
                    &mut spec.spawn_bullet_random_spread,
                ),
                ("puddleRange", &mut spec.puddle_range),
                ("puddleAmount", &mut spec.puddle_amount),
                ("suppressionRange", &mut spec.suppression_range),
                ("suppressionDuration", &mut spec.suppression_duration),
                (
                    "suppressionEffectChance",
                    &mut spec.suppression_effect_chance,
                ),
                ("shrinkX", &mut spec.shrink_x),
                ("shrinkY", &mut spec.shrink_y),
                ("spin", &mut spec.spin),
                ("rotationOffset", &mut spec.rotation_offset),
                ("damageInterval", &mut spec.damage_interval),
                ("shake", &mut spec.shake),
                ("sideLength", &mut spec.side_length),
                ("sideWidth", &mut spec.side_width),
                ("sideAngle", &mut spec.side_angle),
                ("lightningSpacing", &mut spec.lightning_spacing),
                ("lightningDelay", &mut spec.lightning_delay),
                ("lightningAngleRand", &mut spec.lightning_angle_rand),
                ("serrationLenScl", &mut spec.serration_len_scl),
                ("serrationWidth", &mut spec.serration_width),
                ("serrationSpacing", &mut spec.serration_spacing),
                ("serrationSpaceOffset", &mut spec.serration_space_offset),
                ("serrationFadeOffset", &mut spec.serration_fade_offset),
                ("sapStrength", &mut spec.sap_strength),
                ("lengthRand", &mut spec.length_rand),
                ("empRadius", &mut spec.emp_radius),
                ("timeIncrease", &mut spec.time_increase),
                ("timeDuration", &mut spec.time_duration),
                ("powerDamageScl", &mut spec.power_damage_scl),
                ("powerSclDecrease", &mut spec.power_scl_decrease),
                ("unitDamageScl", &mut spec.unit_damage_scl),
                ("pointEffectSpace", &mut spec.point_effect_space),
                ("layer", &mut spec.layer),
                ("trailMult", &mut spec.trail_mult),
                ("trailSize", &mut spec.trail_size),
                ("explodeRange", &mut spec.explode_range),
                ("explodeDelay", &mut spec.explode_delay),
                ("flakDelay", &mut spec.flak_delay),
                ("flakInterval", &mut spec.flak_interval),
                ("recoil", &mut spec.recoil),
                ("rangeChange", &mut spec.range_change),
                ("minRangeChange", &mut spec.min_range_change),
            ],
        );
        set_opt_i32s(
            object,
            &mut [
                ("fragBullets", &mut spec.frag_bullets),
                ("pierceFragCap", &mut spec.pierce_frag_cap),
                ("intervalBullets", &mut spec.interval_bullets),
                ("multiRepeat", &mut spec.multi_repeat),
                ("puddles", &mut spec.puddles),
                ("trailLength", &mut spec.trail_length),
                ("serrations", &mut spec.serrations),
                ("despawnUnitCount", &mut spec.despawn_unit_count),
            ],
        );
        set_opt_bools(
            object,
            &mut [
                ("removeAfterPierce", &mut spec.remove_after_pierce),
                ("laserAbsorb", &mut spec.laser_absorb),
                ("laserBullet", &mut spec.laser_bullet),
                ("displayAmmoMultiplier", &mut spec.display_ammo_multiplier),
                ("killShooter", &mut spec.kill_shooter),
                ("instantDisappear", &mut spec.instant_disappear),
                ("splashDamagePierce", &mut spec.splash_damage_pierce),
                ("scaledSplashDamage", &mut spec.scaled_splash_damage),
                ("impact", &mut spec.impact),
                ("pierceArmor", &mut spec.pierce_armor),
                ("hittable", &mut spec.hittable),
                ("targetBlocks", &mut spec.target_blocks),
                ("targetMissiles", &mut spec.target_missiles),
                ("reflectable", &mut spec.reflectable),
                ("absorbable", &mut spec.absorbable),
                ("collidesAir", &mut spec.collides_air),
                ("collidesGround", &mut spec.collides_ground),
                ("collidesTiles", &mut spec.collides_tiles),
                ("collidesTeam", &mut spec.collides_team),
                ("collideFloor", &mut spec.collide_floor),
                ("collideTerrain", &mut spec.collide_terrain),
                ("sticky", &mut spec.sticky),
                ("despawnHit", &mut spec.despawn_hit),
                ("fragOnHit", &mut spec.frag_on_hit),
                ("fragOnDespawn", &mut spec.frag_on_despawn),
                ("fragOnAbsorb", &mut spec.frag_on_absorb),
                ("delayFrags", &mut spec.delay_frags),
                ("trailRotation", &mut spec.trail_rotation),
                ("underwater", &mut spec.underwater),
                ("weaveRandom", &mut spec.weave_random),
                ("largeHit", &mut spec.large_hit),
                ("hitUnits", &mut spec.hit_units),
                ("faceOutwards", &mut spec.face_outwards),
                ("hitUnder", &mut spec.hit_under),
                ("ignoreSpawnAngle", &mut spec.ignore_spawn_angle),
                ("scaleKeepVelocity", &mut spec.scale_keep_velocity),
                ("scaleLife", &mut spec.scale_life),
            ],
        );
        for (field, target) in [
            ("trailColor", &mut spec.trail_color),
            ("lightningColor", &mut spec.lightning_color),
            ("suppressColor", &mut spec.suppress_color),
            ("backColor", &mut spec.back_color),
            ("frontColor", &mut spec.front_color),
            ("mixColorFrom", &mut spec.mix_color_from),
            ("mixColorTo", &mut spec.mix_color_to),
            ("hitColor", &mut spec.hit_color),
            ("fromColor", &mut spec.from_color),
            ("toColor", &mut spec.to_color),
            ("beamColor", &mut spec.beam_color),
        ] {
            if let Some(color) = object.get(field).and_then(parse_color) {
                *target = Some(color);
            }
        }
        for (field, target) in [
            ("trailInterp", &mut spec.trail_interp),
            ("shrinkInterp", &mut spec.shrink_interp),
        ] {
            if let Some(name) = object.get(field).and_then(Value::as_str) {
                *target = Some(parse_interp(name));
            }
        }
        if let Some(value) = object.get("sprite") {
            spec.sprite = Some(value.as_str().map(str::to_owned));
        }
        if let Some(value) = object.get("backSprite") {
            spec.back_sprite = Some(value.as_str().map(str::to_owned));
        }
        // `BulletType.spawnUnit`/`despawnUnit` name a registered unit (upstream
        // assigns a constructed `UnitType`; JSON mods reference one by name).
        for (field, target) in [
            ("spawnUnit", &mut spec.spawn_unit_name),
            ("despawnUnit", &mut spec.despawn_unit_name),
        ] {
            match object.get(field) {
                Some(Value::String(raw)) => {
                    let reference = self
                        .resolve_prefixed(registry, ContentType::Unit, raw)
                        .ok_or_else(|| {
                            ContentParseError::new(format!(
                                "{file}: unknown unit `{raw}` for bullet `{field}`"
                            ))
                        })?;
                    let name = registry
                        .unit(crate::content::UnitTypeId::new(reference.id))
                        .map(|unit| unit.name.clone())
                        .unwrap_or_else(|| raw.clone());
                    *target = Some(Box::leak(name.into_boxed_str()));
                }
                Some(Value::Object(_)) => {
                    self.warn(
                        file,
                        format!(
                            "bullet `{field}` inline unit objects are not supported; name a unit instead"
                        ),
                    );
                }
                _ => {}
            }
        }
        if let Some(value) = field_f32(object, "despawnUnitChance") {
            spec.despawn_unit_chance = Some(value);
        }
        if let Some(value) = field_f32(object, "despawnUnitRadius") {
            spec.despawn_unit_radius = Some(value);
        }
        // Nested bullets (`frag`/`intervalBullet`/`spawnBullets`); plan 20 M2b.
        if let Some(Value::Object(frag)) = object.get("frag") {
            spec.frag_bullet = Some(Box::new(self.parse_bullet(registry, file, frag)?));
        }
        if let Some(Value::Object(interval)) = object.get("intervalBullet") {
            spec.interval_bullet = Some(Box::new(self.parse_bullet(registry, file, interval)?));
        }
        if let Some(Value::Array(entries)) = object.get("spawnBullets") {
            let mut bullets = Vec::with_capacity(entries.len());
            for entry in entries {
                if let Some(map) = entry.as_object() {
                    bullets.push(self.parse_bullet(registry, file, map)?);
                }
            }
            spec.spawn_bullets = bullets;
        }
        Ok(spec)
    }

    /// Registers one [`BulletSpec`] and its nested `frag`/`intervalBullet`/
    /// `spawnBullets` recursively, patching the resolved ids into the record
    /// (upstream initializer order; plan 20 M2b). `lightning` nested types are
    /// registered the same way. `spawnUnit` is plan 11 and warns.
    pub(super) fn register_bullet_spec(
        &mut self,
        registry: &mut ContentRegistry,
        spec: BulletSpec,
    ) -> Result<crate::content::BulletId, ContentParseError> {
        let mut spec = spec;
        let frag = spec.frag_bullet.take();
        let interval = spec.interval_bullet.take();
        let lightning = spec.lightning_type.take();
        let spawns = std::mem::take(&mut spec.spawn_bullets);
        if spec.spawn_unit.is_some() {
            self.warn("bullet", "bullet `spawnUnit` is plan 11 and ignored");
            spec.spawn_unit = None;
        }
        let def = BulletDef::from_spec(&spec, registry)
            .map_err(|error| ContentParseError::new(format!("bullet: {error}")))?;
        let id = registry
            .add_bullet(def)
            .map_err(|error| ContentParseError::new(format!("bullet: {error}")))?;
        let frag_id = match frag {
            Some(nested) => Some(self.register_bullet_spec(registry, *nested)?),
            None => None,
        };
        let interval_id = match interval {
            Some(nested) => Some(self.register_bullet_spec(registry, *nested)?),
            None => None,
        };
        let lightning_id = match lightning {
            Some(nested) => Some(self.register_bullet_spec(registry, *nested)?),
            None => None,
        };
        let mut spawned = Vec::with_capacity(spawns.len());
        for nested in spawns {
            spawned.push(self.register_bullet_spec(registry, nested)?);
        }
        if let Some(bullet) = registry.bullet_mut(id) {
            bullet.frag_bullet = frag_id;
            bullet.interval_bullet = interval_id;
            bullet.lightning_type = lightning_id;
            bullet.spawn_bullets = spawned;
        }
        Ok(id)
    }
}
