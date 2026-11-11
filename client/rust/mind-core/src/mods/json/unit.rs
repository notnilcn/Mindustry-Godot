// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `unit` content kind plus weapons, abilities and shoot patterns.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    // ---- unit ----

    #[allow(clippy::too_many_lines)]
    pub(super) fn parse_unit(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Unit, stem, object);
        self.read_bundle(object, ContentType::Unit, &name);
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());

        let template = object.get("template").and_then(Value::as_str);
        if let Some(class) = template
            && self.classmap.resolve(class, ClassScope::UnitType).is_none()
        {
            parse_err!("{file}: unknown unit template `{class}`");
        }
        let kind = template
            .and_then(resolve_unit_class_kind)
            .unwrap_or(UnitKind::UnitType);
        let entity = object
            .get("type")
            .and_then(Value::as_str)
            .and_then(resolve_entity_def)
            .unwrap_or_default();
        let mut spec = UnitSpec::for_kind(leaked, kind, entity);
        set_opt_f32s(
            object,
            &mut [
                ("health", &mut spec.health),
                ("speed", &mut spec.speed),
                ("armor", &mut spec.armor),
                ("hitSize", &mut spec.hit_size),
                ("drag", &mut spec.drag),
                ("accel", &mut spec.accel),
                ("rotateSpeed", &mut spec.rotate_speed),
                ("range", &mut spec.range),
                ("mineRange", &mut spec.mine_range),
                ("buildRange", &mut spec.build_range),
                ("buildSpeed", &mut spec.build_speed),
                ("mineSpeed", &mut spec.mine_speed),
            ],
        );
        if let Some(value) = field_i32(object, "itemCapacity") {
            spec.item_capacity = Some(value);
        }
        if let Some(value) = field_i32(object, "mineTier") {
            spec.mine_tier = Some(value);
        }
        if let Some(value) = field_bool(object, "flying") {
            spec.flying = Some(value);
        }
        set_opt_bools(
            object,
            &mut [
                ("targetAir", &mut spec.target_air),
                ("targetGround", &mut spec.target_ground),
                ("useUnitCap", &mut spec.use_unit_cap),
                ("playerControllable", &mut spec.player_controllable),
                ("logicControllable", &mut spec.logic_controllable),
                ("hidden", &mut spec.hidden),
                ("isEnemy", &mut spec.is_enemy),
            ],
        );
        // Remaining `UnitType` scalar fields (`UnitType.java:60-260`).
        set_opt_f32s(
            object,
            &mut [
                ("boostMultiplier", &mut spec.boost_multiplier),
                ("floorMultiplier", &mut spec.floor_multiplier),
                ("baseRotateSpeed", &mut spec.base_rotate_speed),
                ("clipSize", &mut spec.clip_size),
                ("deathShake", &mut spec.death_shake),
                ("stepShake", &mut spec.step_shake),
                ("rippleScale", &mut spec.ripple_scale),
                ("riseSpeed", &mut spec.rise_speed),
                ("descentSpeed", &mut spec.descent_speed),
                ("fallSpeed", &mut spec.fall_speed),
                ("missileAccelTime", &mut spec.missile_accel_time),
                ("maxRange", &mut spec.max_range),
                ("circleTargetRadius", &mut spec.circle_target_radius),
                ("crashDamageMultiplier", &mut spec.crash_damage_multiplier),
                ("wreckHealthMultiplier", &mut spec.wreck_health_multiplier),
                ("drownTimeMultiplier", &mut spec.drown_time_multiplier),
                ("strafePenalty", &mut spec.strafe_penalty),
                ("researchCostMultiplier", &mut spec.research_cost_multiplier),
                ("knockbackMultiplier", &mut spec.knockback_multiplier),
                ("groundLayer", &mut spec.ground_layer),
                ("flyingLayer", &mut spec.flying_layer),
                ("payloadCapacity", &mut spec.payload_capacity),
                ("aimDst", &mut spec.aim_dst),
                ("buildBeamOffset", &mut spec.build_beam_offset),
                ("targetPriority", &mut spec.target_priority),
                ("shadowElevation", &mut spec.shadow_elevation),
                ("shadowElevationScl", &mut spec.shadow_elevation_scl),
                ("engineOffset", &mut spec.engine_offset),
                ("engineSize", &mut spec.engine_size),
                ("engineLayer", &mut spec.engine_layer),
                ("itemOffsetY", &mut spec.item_offset_y),
                ("lightRadius", &mut spec.light_radius),
                ("lightOpacity", &mut spec.light_opacity),
                ("softShadowScl", &mut spec.soft_shadow_scl),
                ("fogRadius", &mut spec.fog_radius),
                ("waveTrailX", &mut spec.wave_trail_x),
                ("waveTrailY", &mut spec.wave_trail_y),
                ("trailScl", &mut spec.trail_scl),
                ("legLength", &mut spec.leg_length),
                ("legSpeed", &mut spec.leg_speed),
                ("legForwardScl", &mut spec.leg_forward_scl),
                ("legBaseOffset", &mut spec.leg_base_offset),
                ("legMoveSpace", &mut spec.leg_move_space),
                ("legExtension", &mut spec.leg_extension),
                ("legPairOffset", &mut spec.leg_pair_offset),
                ("legLengthScl", &mut spec.leg_length_scl),
                ("legStraightLength", &mut spec.leg_straight_length),
                ("legMaxLength", &mut spec.leg_max_length),
                ("legMinLength", &mut spec.leg_min_length),
                ("legSplashDamage", &mut spec.leg_splash_damage),
                ("legSplashRange", &mut spec.leg_splash_range),
                ("baseLegStraightness", &mut spec.base_leg_straightness),
                ("legStraightness", &mut spec.leg_straightness),
                ("mechLandShake", &mut spec.mech_land_shake),
                ("mechSideSway", &mut spec.mech_side_sway),
                ("mechFrontSway", &mut spec.mech_front_sway),
                ("mechStride", &mut spec.mech_stride),
                ("segmentMag", &mut spec.segment_mag),
                ("segmentScl", &mut spec.segment_scl),
                ("segmentPhase", &mut spec.segment_phase),
                ("segmentRotSpeed", &mut spec.segment_rot_speed),
                ("segmentMaxRot", &mut spec.segment_max_rot),
                ("segmentSpacing", &mut spec.segment_spacing),
                ("segmentRotationRange", &mut spec.segment_rotation_range),
                ("crawlSlowdown", &mut spec.crawl_slowdown),
                ("deathSoundVolume", &mut spec.death_sound_volume),
                ("wreckSoundVolume", &mut spec.wreck_sound_volume),
                ("loopSoundVolume", &mut spec.loop_sound_volume),
                ("stepSoundVolume", &mut spec.step_sound_volume),
                ("stepSoundPitch", &mut spec.step_sound_pitch),
                ("stepSoundPitchRange", &mut spec.step_sound_pitch_range),
                ("moveSoundVolume", &mut spec.move_sound_volume),
                ("moveSoundPitchMin", &mut spec.move_sound_pitch_min),
                ("moveSoundPitchMax", &mut spec.move_sound_pitch_max),
                ("tankMoveVolume", &mut spec.tank_move_volume),
                ("mineSoundVolume", &mut spec.mine_sound_volume),
            ],
        );
        set_opt_i32s(
            object,
            &mut [
                ("outlineRadius", &mut spec.outline_radius),
                ("trailLength", &mut spec.trail_length),
                ("legCount", &mut spec.leg_count),
                ("legGroupSize", &mut spec.leg_group_size),
                ("treadFrames", &mut spec.tread_frames),
                ("treadPullOffset", &mut spec.tread_pull_offset),
                ("segments", &mut spec.segments),
                ("segmentUnits", &mut spec.segment_units),
            ],
        );
        set_opt_bools(
            object,
            &mut [
                ("wobble", &mut spec.wobble),
                ("faceTarget", &mut spec.face_target),
                ("circleTarget", &mut spec.circle_target),
                ("autoDropBombs", &mut spec.auto_drop_bombs),
                ("targetBuildingsMobile", &mut spec.target_buildings_mobile),
                ("canBoost", &mut spec.can_boost),
                ("boostWhenBuilding", &mut spec.boost_when_building),
                ("boostWhenMining", &mut spec.boost_when_mining),
                ("controlSelectGlobal", &mut spec.control_select_global),
                ("allowedInPayloads", &mut spec.allowed_in_payloads),
                ("hittable", &mut spec.hittable),
                ("killable", &mut spec.killable),
                ("targetable", &mut spec.targetable),
                ("vulnerableWithPayloads", &mut spec.vulnerable_with_payloads),
                ("pickupUnits", &mut spec.pickup_units),
                ("physics", &mut spec.physics),
                ("canDrown", &mut spec.can_drown),
                ("coreUnitDock", &mut spec.core_unit_dock),
                ("createWreck", &mut spec.create_wreck),
                ("createScorch", &mut spec.create_scorch),
                ("lowAltitude", &mut spec.low_altitude),
                ("rotateToBuilding", &mut spec.rotate_to_building),
                ("allowLegStep", &mut spec.allow_leg_step),
                ("legPhysicsLayer", &mut spec.leg_physics_layer),
                ("hovering", &mut spec.hovering),
                ("omniMovement", &mut spec.omni_movement),
                ("rotateMoveFirst", &mut spec.rotate_move_first),
                ("healFlash", &mut spec.heal_flash),
                ("singleTarget", &mut spec.single_target),
                ("forceMultiTarget", &mut spec.force_multi_target),
                ("internal", &mut spec.internal),
                (
                    "internalGenerateSprites",
                    &mut spec.internal_generate_sprites,
                ),
                ("bounded", &mut spec.bounded),
                ("autoFindTarget", &mut spec.auto_find_target),
                ("targetUnderBlocks", &mut spec.target_under_blocks),
                ("alwaysShootWhenMoving", &mut spec.always_shoot_when_moving),
                ("hoverable", &mut spec.hoverable),
                ("alwaysCreateOutline", &mut spec.always_create_outline),
                ("generateFullIcon", &mut spec.generate_full_icon),
                ("squareShape", &mut spec.square_shape),
                ("drawBuildBeam", &mut spec.draw_build_beam),
                ("drawMineBeam", &mut spec.draw_mine_beam),
                ("drawCell", &mut spec.draw_cell),
                ("drawItems", &mut spec.draw_items),
                ("drawShields", &mut spec.draw_shields),
                ("drawBody", &mut spec.draw_body),
                ("drawSoftShadow", &mut spec.draw_soft_shadow),
                ("drawMinimap", &mut spec.draw_minimap),
                ("useEngineElevation", &mut spec.use_engine_elevation),
                ("allowChangeCommands", &mut spec.allow_change_commands),
                ("outlines", &mut spec.outlines),
                ("mineWalls", &mut spec.mine_walls),
                ("mineFloor", &mut spec.mine_floor),
                ("mineHardnessScaling", &mut spec.mine_hardness_scaling),
                ("legBaseUnder", &mut spec.leg_base_under),
                ("lockLegBase", &mut spec.lock_leg_base),
                ("legContinuousMove", &mut spec.leg_continuous_move),
                ("flipBackLegs", &mut spec.flip_back_legs),
                ("flipLegSide", &mut spec.flip_leg_side),
                ("emitWalkSound", &mut spec.emit_walk_sound),
                ("emitWalkEffect", &mut spec.emit_walk_effect),
                ("mechStepParticles", &mut spec.mech_step_particles),
                ("crushFragile", &mut spec.crush_fragile),
            ],
        );
        for (field, target) in [
            ("healColor", &mut spec.heal_color),
            ("lightColor", &mut spec.light_color),
            ("shieldColor", &mut spec.shield_color),
            ("engineColor", &mut spec.engine_color),
            ("engineColorInner", &mut spec.engine_color_inner),
            ("trailColor", &mut spec.trail_color),
            ("outlineColor", &mut spec.outline_color),
            ("mechLegColor", &mut spec.mech_leg_color),
        ] {
            if let Some(color) = object.get(field).and_then(parse_color) {
                *target = Some(color);
            }
        }
        for (field, target) in [
            ("deathSound", &mut spec.death_sound),
            ("wreckSound", &mut spec.wreck_sound),
            ("loopSound", &mut spec.loop_sound),
            ("stepSound", &mut spec.step_sound),
            ("tankMoveSound", &mut spec.tank_move_sound),
            ("moveSound", &mut spec.move_sound),
            ("mineSound", &mut spec.mine_sound),
        ] {
            if let Some(sound) = self.parse_sound(file, object.get(field)) {
                *target = Some(sound);
            }
        }
        for (field, target) in [
            ("fallEffect", &mut spec.fall_effect),
            ("fallEngineEffect", &mut spec.fall_engine_effect),
            ("deathExplosionEffect", &mut spec.death_explosion_effect),
        ] {
            if let Some(value) = object.get(field)
                && let Some(effect) = self.parse_effect_ref(file, value)
            {
                *target = Some(effect);
            }
        }
        if let Some(Value::Array(entries)) = object.get("targetFlags") {
            for entry in entries {
                if let Some(flag) = entry
                    .as_str()
                    .and_then(crate::mods::patch::resolve_block_flag)
                {
                    spec.target_flags.push(Some(flag));
                } else if let Some(name) = entry.as_str() {
                    self.warn(file, format!("unknown targetFlags entry `{name}`"));
                }
            }
        }
        for (field, target) in [("defaultCommand", &mut spec.default_command)] {
            if let Some(name) = object.get(field).and_then(Value::as_str) {
                let reference = self
                    .resolve_prefixed(registry, ContentType::UnitCommand, name)
                    .ok_or_else(|| {
                        ContentParseError::new(format!(
                            "{file}: unknown unit command `{name}` for `{field}`"
                        ))
                    })?;
                let leaked = registry
                    .unit_command(crate::content::UnitCommandId::new(reference.id))
                    .map(|record| record.name.clone())
                    .unwrap_or_else(|| name.to_owned());
                *target = Some(Box::leak(leaked.into_boxed_str()));
            }
        }
        for (field, target) in [
            ("commands", &mut spec.commands),
            ("stances", &mut spec.stances),
        ] {
            if let Some(Value::Array(entries)) = object.get(field) {
                let type_ = if field == "commands" {
                    ContentType::UnitCommand
                } else {
                    ContentType::UnitStance
                };
                for entry in entries {
                    let Some(name) = entry.as_str() else {
                        continue;
                    };
                    let Some(reference) = self.resolve_prefixed(registry, type_, name) else {
                        self.warn(file, format!("unknown {field} entry `{name}`"));
                        continue;
                    };
                    let leaked = match type_ {
                        ContentType::UnitCommand => registry
                            .unit_command(crate::content::UnitCommandId::new(reference.id))
                            .map(|record| record.name.clone()),
                        _ => registry
                            .unit_stance(crate::content::UnitStanceId::new(reference.id))
                            .map(|record| record.name.clone()),
                    }
                    .unwrap_or_else(|| name.to_owned());
                    target.push(Box::leak(leaked.into_boxed_str()));
                }
            }
        }
        if let Some(class) = object.get("controller").and_then(Value::as_str) {
            spec.controller = Some(self.resolve_controller(class).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown controller `{class}`"))
            })?);
        }
        if let Some(class) = object.get("aiController").and_then(Value::as_str) {
            spec.ai_controller = Some(self.resolve_ai_controller(class).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown aiController `{class}`"))
            })?);
        }
        if let Some(Value::Array(entries)) = object.get("immunities") {
            let mut names = Vec::new();
            for entry in entries {
                if let Some(raw) = entry.as_str()
                    && let Some(status) = self.resolve_status(registry, raw)
                    && let Some(record) = registry.status(status)
                {
                    names.push(record.name.clone());
                }
            }
            // `UnitSpec.immunities` holds `&'static str` names; leak the resolved set.
            let leaked: &'static [&'static str] = Box::leak(
                names
                    .into_iter()
                    .map(|name| Box::leak(name.into_boxed_str()) as &'static str)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            );
            spec.immunities = leaked.to_vec();
        }

        if let Some(Value::Array(entries)) = object.get("abilities") {
            let mut abilities = Vec::with_capacity(entries.len());
            for entry in entries {
                let Some(map) = entry.as_object() else {
                    self.warn(file, "ability entries must be objects; skipped");
                    continue;
                };
                if let Some(ability) = self.parse_ability(file, registry, map)? {
                    abilities.push(ability);
                }
            }
            spec.abilities = abilities;
        }

        let mut def = UnitTypeDef::from_spec(&spec, registry, &self.bundle, self.store)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        self.apply_unlock_fields(registry, file, object, &mut def.unlock);
        let unit_id = registry
            .add_unit(def)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;

        let weapons = self.parse_weapons(registry, file, object)?;
        if !weapons.is_empty() {
            registry
                .unit_mut(unit_id)
                .ok_or_else(|| {
                    ContentParseError::new(format!("{file}: unit lost after registration"))
                })?
                .weapons = weapons;
        }
        self.warn_unknown(file, "unit", object, UNIT_FIELDS);
        Ok(ContentRef::new(ContentType::Unit, unit_id.raw()))
    }

    /// Parses the `weapons` array into [`WeaponDef`]s, creating inline bullets.
    pub(super) fn parse_weapons(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        object: &Map<String, Value>,
    ) -> Result<Vec<WeaponDef>, ContentParseError> {
        let Some(Value::Array(entries)) = object.get("weapons") else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let Some(weapon) = entry.as_object() else {
                self.warn(file, "weapon entries must be objects; skipped");
                continue;
            };
            out.push(self.parse_weapon_object(registry, file, weapon, index)?);
        }
        Ok(out)
    }

    /// Parses one `abilities` entry (`Ability{}`) into an [`AbilitySpec`]
    /// (plan 20 M2b; `PatcherTests.unitAbilities{,Array}`).
    /// `ContentParser` ability entry. `None` marks a recognized-but-unsupported
    /// class (`EmptyDataAbility`), which warns and is left out of the unit
    /// instead of failing the content.
    ///
    /// Every runtime-modelled [`AbilityKind`] resolves here, including the
    /// mod-only `ArmorPlateAbility`/`MoveLightningAbility`/`UnitSpawnAbility`
    /// (`ContentParser.java:355-360` reads any `Ability` subclass by field).
    pub fn parse_ability(
        &mut self,
        file: &str,
        registry: &ContentRegistry,
        object: &Map<String, Value>,
    ) -> Result<Option<AbilitySpec>, ContentParseError> {
        let class = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| ContentParseError::new(format!("{file}: ability is missing `type`")))?;
        if let Some(tag) = self.classmap.resolve(class, ClassScope::Ability) {
            if let Some(kind) = resolve_ability_kind(tag) {
                return self
                    .parse_ability_fields(file, registry, object, kind)
                    .map(Some);
            }
            // `EmptyDataAbility` (the type-IO placeholder, `EmptyDataAbility.java`)
            // has no JSON-side runtime: a recognized alias warns instead of
            // failing the unit (`ContentParser.java:355-360` would register it).
            if tag == "EmptyDataAbility" {
                self.warn(
                    file,
                    format!("ability `{tag}` is unsupported in the port; ignored"),
                );
                return Ok(None);
            }
        }
        Err(ContentParseError::new(format!(
            "{file}: unknown ability `{class}`"
        )))
    }

    pub(super) fn parse_ability_fields(
        &mut self,
        _file: &str,
        registry: &ContentRegistry,
        object: &Map<String, Value>,
        kind: AbilityKind,
    ) -> Result<AbilitySpec, ContentParseError> {
        let mut spec = AbilitySpec::for_kind(kind);

        set_f32s(
            object,
            &mut [
                ("amount", &mut spec.amount),
                ("max", &mut spec.max),
                ("reload", &mut spec.reload),
                ("range", &mut spec.range),
                ("healPercent", &mut spec.heal_percent),
                ("sameTypeHealMult", &mut spec.same_type_heal_mult),
                ("smartDowntime", &mut spec.smart_downtime),
                ("rotation", &mut spec.rotation),
                ("regen", &mut spec.regen),
                ("cooldown", &mut spec.cooldown),
                ("duration", &mut spec.duration),
                ("orbRadius", &mut spec.orb_radius),
                ("particleSize", &mut spec.particle_size),
                ("x", &mut spec.x),
                ("y", &mut spec.y),
                ("angle", &mut spec.angle),
                ("width", &mut spec.width),
                ("chanceDeflect", &mut spec.chance_deflect),
                ("minVelocity", &mut spec.min_velocity),
                ("interval", &mut spec.interval),
                ("spread", &mut spec.spread),
                ("percentAmount", &mut spec.percent_amount),
            ],
        );
        set_i32s(
            object,
            &mut [
                ("particles", &mut spec.particles),
                ("maxTargets", &mut spec.max_targets),
                ("sides", &mut spec.sides),
                ("randAmount", &mut spec.rand_amount),
            ],
        );
        set_bools(
            object,
            &mut [
                ("smartHeal", &mut spec.smart_heal),
                ("active", &mut spec.active),
                ("whenShooting", &mut spec.when_shooting),
                ("teamColor", &mut spec.team_color),
            ],
        );
        // Mod-only kinds (PAR-0587 / `GAPS #61`): these classes declare fields
        // the union above names differently, so read them per class exactly as
        // `ContentParser.readFields` does (`ContentParser.java:355-360` — JSON
        // keys are the Java field names).
        match kind {
            AbilityKind::ArmorPlate => {
                // `ArmorPlateAbility.healthMultiplier = 0.2f`
                // (`ArmorPlateAbility.java:30`); added to
                // `Unit.healthMultiplier` while shooting.
                if let Some(value) = field_f32(object, "healthMultiplier") {
                    spec.health_multiplier = value;
                }
            }
            AbilityKind::MoveLightning => {
                // `MoveLightningAbility` fields (`MoveLightningAbility.java:12-25`):
                // `damage` is the union's `amount`, `y` the spawn offset.
                if let Some(value) = field_f32(object, "damage") {
                    spec.amount = value;
                }
                if let Some(value) = field_i32(object, "length") {
                    spec.length = value;
                }
                if let Some(value) = field_f32(object, "chance") {
                    spec.chance = value;
                }
                if let Some(value) = field_f32(object, "y") {
                    spec.spawn_offset = value;
                }
                if let Some(value) = field_f32(object, "minSpeed") {
                    spec.min_speed = value;
                }
                if let Some(value) = field_f32(object, "maxSpeed") {
                    spec.max_speed = value;
                }
                if let Some(value) = field_bool(object, "alternate") {
                    spec.alternate = value;
                }
            }
            AbilityKind::UnitSpawn => {
                // `UnitSpawnAbility` fields (`UnitSpawnAbility.java:13-20`);
                // `unit` is resolved by the shared reference block below.
                if let Some(value) = field_f32(object, "spawnTime") {
                    spec.spawn_time = value;
                }
                if let Some(value) = field_f32(object, "spawnX") {
                    spec.spawn_x = value;
                }
                if let Some(value) = field_f32(object, "spawnY") {
                    spec.spawn_y = value;
                }
            }
            _ => {}
        }
        if let Some(color) = object.get("color").and_then(parse_color) {
            spec.color = Some(color);
        }
        if let Some(color) = object.get("effectColor").and_then(parse_color) {
            spec.effect_color = Some(color);
        }
        if let Some(color) = object.get("particleColor").and_then(parse_color) {
            spec.particle_color = Some(color);
        }
        if let Some(status) = object.get("status").and_then(Value::as_str) {
            spec.status = Some(Box::leak(status.to_owned().into_boxed_str()));
        }
        if let Some(status) = object.get("effect").and_then(Value::as_str) {
            spec.effect = Some(Box::leak(status.to_owned().into_boxed_str()));
        }
        if let Some(unit) = object.get("unit").and_then(Value::as_str) {
            let resolved = self
                .resolve_prefixed(registry, ContentType::Unit, unit)
                .map(|reference| reference.id)
                .unwrap_or(0);
            // Keep the resolved unit name for later binding.
            let name = registry
                .unit(crate::content::UnitTypeId::new(resolved))
                .map(|record| record.name.clone())
                .unwrap_or_else(|| unit.to_owned());
            spec.unit = Some(Box::leak(name.into_boxed_str()));
        }
        if let Some(liquid) = object.get("liquid").and_then(Value::as_str) {
            let resolved = self
                .resolve_prefixed(registry, ContentType::Liquid, liquid)
                .map(|reference| reference.id)
                .unwrap_or(0);
            let name = registry
                .liquid(crate::content::LiquidId::new(resolved))
                .map(|record| record.name.clone())
                .unwrap_or_else(|| liquid.to_owned());
            spec.liquid = Some(Box::leak(name.into_boxed_str()));
        }
        Ok(spec)
    }

    /// Parses a single weapon object (`Weapon{}`), registering its inline bullet
    /// and running the created-object `init()` callback (plan 20 M3b).
    ///
    /// Shared by unit-content parsing and the `DataPatcher` created-object
    /// callback path (`PatcherTests.unitWeapons`/`addWeapon`).
    pub fn parse_weapon_object(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        weapon: &Map<String, Value>,
        index: usize,
    ) -> Result<WeaponDef, ContentParseError> {
        let weapon_name: &'static str = weapon
            .get("name")
            .and_then(Value::as_str)
            .map(|name| Box::leak(name.to_owned().into_boxed_str()) as &'static str)
            .unwrap_or_else(|| Box::leak(format!("weapon{index}").into_boxed_str()));
        let mut spec = WeaponSpec {
            name: weapon_name,
            ..WeaponSpec::default()
        };
        if let Some(value) = field_bool(weapon, "mirror") {
            spec.mirror = Some(value);
        }
        if let Some(value) = field_f32(weapon, "reload") {
            spec.reload = Some(value);
        }
        if let Some(value) = field_f32(weapon, "x") {
            spec.x = Some(value);
        }
        if let Some(value) = field_f32(weapon, "y") {
            spec.y = Some(value);
        }
        if let Some(value) = field_bool(weapon, "alternate") {
            spec.alternate = Some(value);
        }
        if let Some(value) = field_bool(weapon, "rotate") {
            spec.rotate = Some(value);
        }
        if let Some(value) = field_bool(weapon, "shootOnDeath") {
            spec.shoot_on_death = Some(value);
        }
        // Remaining `Weapon` fields (`Weapon.java:30-180`), including the
        // effect/draw-part surface: `ejectEffect`/`shootOnDeathEffect`/
        // `beamEffect`, the sound slots, `shootStatus` and `parts`.
        set_opt_bools(
            weapon,
            &mut [
                ("display", &mut spec.display),
                ("showStatSprite", &mut spec.show_stat_sprite),
                ("top", &mut spec.top),
                ("continuous", &mut spec.continuous),
                ("alwaysContinuous", &mut spec.always_continuous),
                ("controllable", &mut spec.controllable),
                ("aiControllable", &mut spec.ai_controllable),
                ("alwaysShooting", &mut spec.always_shooting),
                ("autoTarget", &mut spec.auto_target),
                ("predictTarget", &mut spec.predict_target),
                ("useAttackRange", &mut spec.use_attack_range),
                ("linearWarmup", &mut spec.linear_warmup),
                ("ignoreRotation", &mut spec.ignore_rotation),
                ("noAttack", &mut spec.no_attack),
                ("parentizeEffects", &mut spec.parentize_effects),
                ("targetBuildings", &mut spec.target_buildings),
                ("targetUnits", &mut spec.target_units),
                ("healBeamMount", &mut spec.heal_beam_mount),
            ],
        );
        set_opt_f32s(
            weapon,
            &mut [
                ("baseRotation", &mut spec.base_rotation),
                ("aimChangeSpeed", &mut spec.aim_change_speed),
                ("targetInterval", &mut spec.target_interval),
                ("targetSwitchInterval", &mut spec.target_switch_interval),
                ("rotateSpeed", &mut spec.rotate_speed),
                ("inaccuracy", &mut spec.inaccuracy),
                ("shake", &mut spec.shake),
                ("recoil", &mut spec.recoil),
                ("recoilTime", &mut spec.recoil_time),
                ("recoilPow", &mut spec.recoil_pow),
                ("cooldownTime", &mut spec.cooldown_time),
                ("shootX", &mut spec.shoot_x),
                ("shootY", &mut spec.shoot_y),
                ("xRand", &mut spec.x_rand),
                ("yRand", &mut spec.y_rand),
                ("shadow", &mut spec.shadow),
                ("velocityRnd", &mut spec.velocity_rnd),
                ("extraVelocity", &mut spec.extra_velocity),
                ("lifeRnd", &mut spec.life_rnd),
                ("extraLife", &mut spec.extra_life),
                ("shootCone", &mut spec.shoot_cone),
                ("rotationLimit", &mut spec.rotation_limit),
                ("minWarmup", &mut spec.min_warmup),
                ("shootWarmupSpeed", &mut spec.shoot_warmup_speed),
                ("smoothReloadSpeed", &mut spec.smooth_reload_speed),
                ("soundPitchMin", &mut spec.sound_pitch_min),
                ("soundPitchMax", &mut spec.sound_pitch_max),
                ("minShootVelocity", &mut spec.min_shoot_velocity),
                ("maxShootVelocity", &mut spec.max_shoot_velocity),
                ("layerOffset", &mut spec.layer_offset),
                ("activeSoundVolume", &mut spec.active_sound_volume),
                ("shootSoundVolume", &mut spec.shoot_sound_volume),
                ("shootStatusDuration", &mut spec.shoot_status_duration),
                ("damageTargetWeight", &mut spec.damage_target_weight),
                ("repairSpeed", &mut spec.repair_speed),
                ("fractionRepairSpeed", &mut spec.fraction_repair_speed),
                ("beamWidth", &mut spec.beam_width),
                ("pulseRadius", &mut spec.pulse_radius),
                ("pulseStroke", &mut spec.pulse_stroke),
                ("widthSinMag", &mut spec.width_sin_mag),
                ("widthSinScl", &mut spec.width_sin_scl),
            ],
        );
        if let Some(value) = field_i32(weapon, "recoils") {
            spec.recoils = Some(value);
        }
        for (field, target) in [
            ("heatColor", &mut spec.heat_color),
            ("beamColor", &mut spec.beam_color),
            ("laserColor", &mut spec.laser_color),
            ("healColor", &mut spec.heal_color),
        ] {
            if let Some(color) = weapon.get(field).and_then(parse_color) {
                *target = Some(color);
            }
        }
        for (field, target) in [
            ("activeSound", &mut spec.active_sound),
            ("shootSound", &mut spec.shoot_sound),
            ("initialShootSound", &mut spec.initial_shoot_sound),
            ("chargeSound", &mut spec.charge_sound),
        ] {
            if let Some(sound) = self.parse_sound(file, weapon.get(field)) {
                *target = Some(sound);
            }
        }
        for (field, target) in [
            ("ejectEffect", &mut spec.eject_effect),
            ("shootOnDeathEffect", &mut spec.shoot_on_death_effect),
        ] {
            if let Some(value) = weapon.get(field)
                && let Some(effect) = self.parse_effect_ref(file, value)
            {
                *target = Some(effect);
            }
        }
        if let Some(raw) = weapon.get("beamEffect").and_then(Value::as_str) {
            match effect_by_name(raw) {
                Some(effect) => spec.beam_effect = Some(effect),
                None => self.warn(file, format!("unknown beamEffect `{raw}`")),
            }
        }
        if let Some(status) = weapon.get("shootStatus").and_then(Value::as_str) {
            spec.shoot_status = Some(Box::leak(status.to_owned().into_boxed_str()));
        }
        if let Some(value) = weapon.get("parts") {
            spec.parts = self.parse_draw_parts(file, value);
        }
        match weapon.get("bullet") {
            Some(Value::Object(bullet)) => {
                let bullet_spec = self.parse_bullet(registry, file, bullet)?;
                spec.bullet = BulletRef::Inline(Box::new(bullet_spec));
            }
            Some(Value::String(raw)) => {
                // `ContentParser` bullet field parser: a string is
                // `field(Bullets.class, name)` (`ContentParser.java:165-170`).
                let named = registry
                    .bullet_by_name(raw)
                    .or_else(|| crate::content::registries::bullets::bullet_by_java_name(raw));
                match named {
                    Some(id) => spec.bullet = BulletRef::Named(id),
                    None => self.warn(file, format!("unknown named bullet `{raw}`")),
                }
            }
            _ => {}
        }
        if let Some(Value::Object(shoot)) = weapon.get("shoot") {
            spec.shoot = Some(self.parse_shoot_pattern(file, shoot)?);
        }
        let bullet_resolved = match &spec.bullet {
            BulletRef::Inline(bullet_spec) => {
                let id = self.register_bullet_spec(registry, (**bullet_spec).clone())?;
                let record = registry
                    .bullet(id)
                    .ok_or_else(|| ContentParseError::new(format!("{file}: bullet lost")))?;
                ResolvedBullet {
                    id,
                    range: record.compute_range(),
                    heals: record.heals(),
                    kill_shooter: record.kill_shooter,
                    dps: 0.0,
                }
            }
            BulletRef::Named(id) => {
                let record = registry.bullet(*id).ok_or_else(|| {
                    ContentParseError::new(format!("{file}: named bullet is not registered"))
                })?;
                ResolvedBullet {
                    id: *id,
                    range: record.compute_range(),
                    heals: record.heals(),
                    kill_shooter: record.kill_shooter,
                    dps: record.estimate_dps,
                }
            }
            BulletRef::Placeholder | BulletRef::Pre(_) => ResolvedBullet {
                id: crate::content::BulletId::new(0),
                range: 0.0,
                heals: false,
                kill_shooter: false,
                dps: 0.0,
            },
        };
        let mut def = WeaponDef::from_spec(spec, bullet_resolved, registry)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        // Created-object lifecycle: upstream calls `init()`/`postInit()` on the
        // freshly constructed weapon before attaching it.
        def.init();
        Ok(def)
    }

    /// Parses a weapon `shoot` pattern object (`ShootPattern` subclass)
    /// (plan 20 M2b; `PatcherTests.bigPatch` `shoot:{type:ShootAlternate}`).
    pub(super) fn parse_shoot_pattern(
        &mut self,
        file: &str,
        object: &Map<String, Value>,
    ) -> Result<ShootPatternSpec, ContentParseError> {
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .map(|class| {
                self.classmap
                    .resolve(class, ClassScope::ShootPattern)
                    .and_then(resolve_shoot_kind)
                    .ok_or_else(|| {
                        ContentParseError::new(format!("{file}: unknown shoot pattern `{class}`"))
                    })
            })
            .transpose()?
            .unwrap_or(ShootPatternKind::ShootPattern);
        let mut spec = ShootPatternSpec {
            kind,
            ..ShootPatternSpec::default()
        };
        set_i32s(
            object,
            &mut [
                ("shots", &mut spec.shots),
                ("barrels", &mut spec.barrels),
                ("barrelOffset", &mut spec.barrel_offset),
            ],
        );
        set_f32s(
            object,
            &mut [
                ("firstShotDelay", &mut spec.first_shot_delay),
                ("shotDelay", &mut spec.shot_delay),
                ("spread", &mut spec.spread),
                ("scl", &mut spec.scl),
                ("mag", &mut spec.mag),
                ("offset", &mut spec.offset),
                ("sineScl", &mut spec.sine_scl),
                ("sineMag", &mut spec.sine_mag),
                ("x", &mut spec.summon_x),
                ("y", &mut spec.summon_y),
                ("radius", &mut spec.summon_radius),
            ],
        );
        if let Some(value) = field_bool(object, "mirror") {
            spec.mirror = value;
        }
        Ok(spec)
    }
}
