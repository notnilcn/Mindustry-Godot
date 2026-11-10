// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `block` content kind of the JSON content parser.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    // ---- block ----

    #[allow(clippy::too_many_lines)]
    pub(super) fn parse_block(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        object: &Map<String, Value>,
    ) -> Result<ContentRef, ContentParseError> {
        let name = self.locate_name(registry, ContentType::Block, stem, object);
        self.read_bundle(object, ContentType::Block, &name);
        let leaked: &'static str = Box::leak(name.into_boxed_str());

        let mut spec = BlockSpec {
            name: leaked,
            ..BlockSpec::default()
        };
        if let Some(kind) = object.get("type").and_then(Value::as_str) {
            spec.kind = self.resolve_block_kind(file, kind)?;
        }
        if let Some(size) = field_i32(object, "size") {
            spec.size = Some(size);
        }
        if let Some(health) = field_i32(object, "health") {
            spec.health = Some(health);
        }
        if let Some(armor) = field_f32(object, "armor") {
            spec.armor = Some(armor);
        }
        if let Some(value) = field_i32(object, "itemCapacity") {
            spec.item_capacity = Some(value);
        }
        if let Some(value) = field_f32(object, "liquidCapacity") {
            spec.liquid_capacity = Some(value);
        }
        set_opt_bools(
            object,
            &mut [
                ("solid", &mut spec.solid),
                ("floating", &mut spec.floating),
                ("update", &mut spec.update),
                ("destructible", &mut spec.destructible),
                ("saveData", &mut spec.save_data),
                ("saveConfig", &mut spec.save_config),
                ("configurable", &mut spec.configurable),
                ("inEditor", &mut spec.in_editor),
                ("hasItems", &mut spec.has_items),
                ("hasLiquids", &mut spec.has_liquids),
                ("hasPower", &mut spec.has_power),
                ("placeablePlayer", &mut spec.placeable_player),
                ("placeableLiquid", &mut spec.placeable_liquid),
                ("placeableOn", &mut spec.placeable_on),
                // `Floor.drawEdgeIn`/`drawEdgeOut` (`Floor.java:92-94`; J02 T1).
                ("drawEdgeIn", &mut spec.draw_edge_in),
                ("drawEdgeOut", &mut spec.draw_edge_out),
                // `Floor.autotile` (`Floor.java:82-90`; J02 T2).
                ("autotile", &mut spec.autotile),
                // `Turret.playerControllable` (`Turret.java:103`; L06 T2).
                ("playerControllable", &mut spec.player_controllable),
            ],
        );
        // `Floor.autotileVariants`/`autotileMidVariants`/`tilingVariants`
        // (J02 T2).
        if let Some(value) = field_i32(object, "autotileVariants") {
            spec.autotile_variants = Some(value);
        }
        if let Some(value) = field_i32(object, "autotileMidVariants") {
            spec.autotile_mid_variants = Some(value);
        }
        if let Some(value) = field_i32(object, "tilingVariants") {
            spec.tiling_variants = Some(value);
        }
        if let Some(value) = field_f32(object, "overlayAlpha") {
            spec.overlay_alpha = Some(value);
        }
        // Remaining `Block`/`Floor`/`Wall` scalar fields
        // (`Block.java:80-220`, `Floor.java:60-100`).
        set_opt_f32s(
            object,
            &mut [
                ("scaledHealth", &mut spec.scaled_health),
                ("buildTime", &mut spec.build_time),
                ("buildCostMultiplier", &mut spec.build_cost_multiplier),
                ("researchCostMultiplier", &mut spec.research_cost_multiplier),
                ("priority", &mut spec.priority),
                ("oreThreshold", &mut spec.ore_threshold),
                ("oreScale", &mut spec.ore_scale),
                ("albedo", &mut spec.albedo),
                ("speedMultiplier", &mut spec.speed_multiplier),
                ("drownTime", &mut spec.drown_time),
                ("statusDuration", &mut spec.floor_status_duration),
                ("dragMultiplier", &mut spec.floor_drag_multiplier),
                ("damageTaken", &mut spec.floor_damage_taken),
                ("liquidMultiplier", &mut spec.liquid_multiplier),
                ("crushDamageMultiplier", &mut spec.crush_damage_multiplier),
                ("depositCooldown", &mut spec.deposit_cooldown),
                ("constructTime", &mut spec.construct_time),
                ("craftTime", &mut spec.craft_time),
            ],
        );
        set_opt_i32s(
            object,
            &mut [
                ("unitCapModifier", &mut spec.unit_cap_modifier),
                ("fogRadius", &mut spec.fog_radius),
            ],
        );
        set_opt_bools(
            object,
            &mut [
                ("solidifies", &mut spec.solidifies),
                ("incinerateNonBuildable", &mut spec.incinerate_non_buildable),
                ("requiresCoreZone", &mut spec.requires_core_zone),
                ("ignoreBuildDarkness", &mut spec.ignore_build_darkness),
                ("insulated", &mut spec.insulated),
                ("canOverdrive", &mut spec.can_overdrive),
                ("absorbLasers", &mut spec.absorb_lasers),
                ("allowCorePlacement", &mut spec.allow_core_placement),
                ("playerUnmineable", &mut spec.player_unmineable),
                ("wallOre", &mut spec.wall_ore),
                ("oreDefault", &mut spec.ore_default),
                ("generateIcons", &mut spec.generate_icons),
                ("hasColor", &mut spec.has_color),
                ("squareSprite", &mut spec.square_sprite),
                ("drawCached", &mut spec.draw_cached),
                ("drawDynamic", &mut spec.draw_dynamic),
                ("fillsTile", &mut spec.fills_tile),
                ("hasShadow", &mut spec.has_shadow),
                ("customShadow", &mut spec.custom_shadow),
                ("obstructsLight", &mut spec.obstructs_light),
                ("emitLight", &mut spec.emit_light),
                ("drawTeamOverlay", &mut spec.draw_team_overlay),
                ("shallow", &mut spec.shallow),
                ("crushFragile", &mut spec.crush_fragile),
                ("unitMoveBreakable", &mut spec.unit_move_breakable),
                ("teamPassable", &mut spec.team_passable),
                ("alwaysReplace", &mut spec.always_replace),
                ("privileged", &mut spec.privileged),
                ("replaceable", &mut spec.replaceable),
                ("quickRotate", &mut spec.quick_rotate),
                ("rotate", &mut spec.rotate),
                ("rotateDraw", &mut spec.rotate_draw),
                ("rotateDrawEditor", &mut spec.rotate_draw_editor),
            ],
        );
        if let Some(name) = object.get("group").and_then(Value::as_str) {
            spec.group = Some(resolve_block_group(name));
        }
        if let Some(name) = object.get("cacheLayer").and_then(Value::as_str) {
            if let Some(layer) = resolve_cache_layer(name) {
                spec.cache_layer = Some(layer);
            } else {
                self.warn(file, format!("unknown cacheLayer `{name}`"));
            }
        }
        for (field, target) in [
            ("itemDrop", &mut spec.item_drop),
            ("liquidDrop", &mut spec.liquid_drop),
            ("region", &mut spec.region),
        ] {
            if let Some(name) = object.get(field).and_then(Value::as_str) {
                *target = Some(Box::leak(name.to_owned().into_boxed_str()));
            }
        }
        if let Some(status) = object.get("status").and_then(Value::as_str) {
            spec.floor_status = Some(Box::leak(status.to_owned().into_boxed_str()));
        }
        if let Some(color) = object.get("mapColor").and_then(parse_color) {
            spec.map_color = Some(color);
        }
        if let Some(requirements) = object.get("researchCost") {
            spec.research_cost = Some(self.parse_stacks(registry, file, requirements)?);
        }
        // `Floor.blendGroup` (`Floor.java:62`; J02 T1). A self reference keeps
        // the `None` (self) default — the block is not registered yet, so
        // `from_spec` could not resolve it either.
        if let Some(group) = object.get("blendGroup").and_then(Value::as_str)
            && group != leaked
        {
            let resolved = self
                .resolve_prefixed(registry, ContentType::Block, group)
                .and_then(|reference| {
                    registry
                        .block(crate::content::BlockId::new(reference.id))
                        .map(|def| def.name.clone())
                })
                .unwrap_or_else(|| group.to_owned());
            spec.blend_group = Some(Box::leak(resolved.into_boxed_str()));
        }
        if let Some(visibility) = object.get("buildVisibility").and_then(Value::as_str) {
            spec.build_visibility = resolve_build_visibility(visibility);
        }
        if let Some(category) = object.get("category").and_then(Value::as_str) {
            spec.category = resolve_category(category);
        }
        if let Some(requirements) = object.get("requirements") {
            spec.requirements = self.parse_stacks(registry, file, requirements)?;
            if spec
                .build_visibility
                .is_none_or(|visibility| visibility == BuildVisibility::Hidden)
            {
                spec.build_visibility = Some(BuildVisibility::Shown);
            }
        }
        if let Some(consumes) = object.get("consumes") {
            spec.consumes = self.parse_consumes(registry, file, consumes)?;
        }
        // `UnitFactory.plans` (`Seq<UnitPlan>`) and `UnitAssembler.plans`
        // (`Seq<AssemblerUnitPlan>`): both are plain `plans` arrays upstream;
        // the field type comes from the declared block class
        // (`UnitFactory.java:206`, `UnitAssembler.java:52`).
        if let Some(plans) = object.get("plans") {
            let Value::Array(entries) = plans else {
                parse_err!("{file}: `plans` must be an array");
            };
            match spec.kind {
                BlockKind::UnitFactory | BlockKind::LegacyUnitFactory => {
                    spec.unit_plans = self.parse_unit_plans(registry, file, entries)?;
                }
                BlockKind::UnitAssembler => {
                    spec.assembler_plans = self.parse_assembler_plans(registry, file, entries)?;
                }
                kind => self.warn(
                    file,
                    format!("`plans` is not a field of `{}`; ignored", kind.name()),
                ),
            }
        }
        // `Block.drawer` (`ContentParser.java:182-198`): string class, array
        // shorthand for `DrawMulti`, or an object with `type`.
        if let Some(drawer) = object.get("drawer") {
            spec.drawer = self.parse_drawer(file, drawer);
        }
        // `Drill.drillMultipliers` (an `ObjectFloatMap<Item>` in Java's JSON
        // content format).
        if let Some(multipliers) = object.get("drillMultipliers").and_then(Value::as_object) {
            for (item, value) in multipliers {
                let Some(value) = value.as_f64() else {
                    continue;
                };
                let resolved = self
                    .resolve_prefixed(registry, crate::content::ContentType::Item, item)
                    .and_then(|reference| {
                        registry
                            .item(crate::content::ItemId::new(reference.id))
                            .map(|def| def.name.clone())
                    })
                    .unwrap_or_else(|| item.clone());
                spec.drill_multipliers
                    .push((Box::leak(resolved.into_boxed_str()), value as f32));
            }
        }
        // ---- production family fields (GAPS #56 follow-up) ----
        //
        // The per-class content fields the production building classes read
        // (`GenericCrafter.java:22-37`, `Separator.java:20`, `Drill.java:26-42`,
        // `BeamDrill.java:38-40`, `BurstDrill.java:26`, `Pump.java:20`,
        // `SolidPump.java:22-26`, `Fracker.java:8`, `WallCrafter.java:30-46`,
        // `AttributeCrafter.java:17-31`). They are reflective field reads
        // upstream (`ContentParser` reads every public field), so a value the
        // JSON does not set keeps the class initializer.
        if let Some(value) = object.get("outputItems") {
            spec.output_items = self.parse_stacks(registry, file, value)?;
        }
        if spec.output_items.is_empty()
            && let Some(value) = object.get("outputItem")
        {
            // `GenericCrafter.init` (`:125-126`): the single form writes the
            // array when the array is null.
            spec.output_items =
                self.parse_stacks(registry, file, &Value::Array(vec![value.clone()]))?;
        }
        if let Some(value) = object.get("outputLiquids") {
            spec.output_liquids = self.parse_liquid_stacks(registry, file, value)?;
        }
        if spec.output_liquids.is_empty()
            && let Some(value) = object.get("outputLiquid")
        {
            // `GenericCrafter.init` (`:129-130`): same single-form write-back.
            spec.output_liquids =
                self.parse_liquid_stacks(registry, file, &Value::Array(vec![value.clone()]))?;
        }
        // `GenericCrafter.liquidOutputDirections` (`:35`): an `int[]` of
        // relative directions, `-1` = dump in every direction.
        if let Some(directions) = object.get("liquidOutputDirections") {
            let Value::Array(entries) = directions else {
                parse_err!("{file}: `liquidOutputDirections` must be an array");
            };
            spec.liquid_output_directions = entries
                .iter()
                .filter_map(|entry| entry.as_i64().map(|value| value as i8))
                .collect();
        }
        if let Some(results) = object.get("results") {
            spec.separator_results = self.parse_stacks(registry, file, results)?;
        }
        set_opt_f32s(
            object,
            &mut [
                ("drillTime", &mut spec.drill_time),
                (
                    "hardnessDrillMultiplier",
                    &mut spec.hardness_drill_multiplier,
                ),
                ("liquidBoostIntensity", &mut spec.liquid_boost_intensity),
                ("warmupSpeed", &mut spec.warmup_speed),
                ("invertedTime", &mut spec.inverted_time),
                ("pumpAmount", &mut spec.pump_amount),
                ("itemUseTime", &mut spec.item_use_time),
                ("baseEfficiency", &mut spec.base_efficiency),
                ("itemBoostIntensity", &mut spec.item_boost_intensity),
                ("boostItemUseTime", &mut spec.boost_item_use_time),
                ("boostScale", &mut spec.boost_scale),
                ("minEfficiency", &mut spec.min_efficiency),
                ("maxBoost", &mut spec.max_boost),
                ("outputScale", &mut spec.output_scale),
            ],
        );
        set_opt_i32s(
            object,
            &mut [
                ("tier", &mut spec.drill_tier),
                ("range", &mut spec.beam_range),
                ("variants", &mut spec.variants),
                ("maxSize", &mut spec.max_size),
            ],
        );
        set_opt_bools(
            object,
            &mut [
                ("ignoreLiquidFullness", &mut spec.ignore_liquid_fullness),
                ("scaleLiquidConsumption", &mut spec.scale_liquid_consumption),
                ("displayEfficiency", &mut spec.display_efficiency),
                ("displayScaledOutput", &mut spec.display_scaled_output),
            ],
        );
        // `Drill.blockedItems` (`:42`) is an `ObjectSet<Item>`; the single
        // `blockedItem` field (`:40`) folds into it like upstream.
        if let Some(Value::Array(entries)) = object.get("blockedItems") {
            spec.blocked_items = entries
                .iter()
                .filter_map(Value::as_str)
                .map(|name| Box::leak(name.to_owned().into_boxed_str()) as &'static str)
                .collect();
        }
        if let Some(name) = object.get("blockedItem").and_then(Value::as_str)
            && !spec.blocked_items.contains(&name)
        {
            spec.blocked_items
                .push(Box::leak(name.to_owned().into_boxed_str()));
        }
        // `SolidPump.result` (`:22`) — the pumped liquid (`Fracker` inherits).
        if let Some(name) = object.get("result").and_then(Value::as_str) {
            spec.pump_liquid = Some(Box::leak(name.to_owned().into_boxed_str()));
        }
        // `WallCrafter.output` (`:40`) — the mined item.
        if let Some(name) = object.get("output").and_then(Value::as_str) {
            spec.output_item = Some(Box::leak(name.to_owned().into_boxed_str()));
        }
        // `SolidPump.attribute`/`Fracker`/`WallCrafter`/`AttributeCrafter`
        // (`Attribute` enum; the content field is its lowercase name,
        // `AttributeCrafter.java:15`).
        if let Some(name) = object.get("attribute").and_then(Value::as_str) {
            if crate::world::attributes::attribute_from_name(name).is_none() {
                self.warn(
                    file,
                    format!("unknown attribute `{name}`; the block keeps no boost attribute"),
                );
            }
            spec.attribute = Some(Box::leak(name.to_owned().into_boxed_str()));
        }
        // ---- environment family fields (TiledFloor/TiledWall/WobbleProp) ----
        set_opt_f32s(
            object,
            &mut [
                ("wscl", &mut spec.wobble_scale),
                ("wmag", &mut spec.wobble_magnitude),
                ("wtscl", &mut spec.wobble_time_scale),
                ("wmag2", &mut spec.wobble_magnitude2),
            ],
        );

        let mut def = BlockDef::from_spec(spec, registry, &self.bundle, self.store)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        self.apply_unlock_fields(registry, file, object, &mut def.unlock);
        let id = registry
            .add_block(def)
            .map_err(|error| ContentParseError::new(format!("{file}: {error}")))?;
        // `Floor.decoration` (`Floor.java:70`): a block reference read in the
        // deferred pass (`finish_parsing`), as Java's reflective field read.
        if let Some(decoration) = object.get("decoration").and_then(Value::as_str) {
            self.pending_decorations.push(PendingDecoration {
                file: file.to_owned(),
                mod_id: registry.current_mod().cloned(),
                floor: id,
                name: decoration.to_owned(),
            });
        }
        Ok(ContentRef::block(id))
    }
}
