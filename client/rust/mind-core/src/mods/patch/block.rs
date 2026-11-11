// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block patching: `apply_block`, its scalar edits and the block edit helpers.
//!
//! Upstream: core/src/mindustry/mod/DataPatcher.java

use super::load::parse_liquid_stacks;
use super::*;

impl DataPatcher {
    // ---- block ----

    #[allow(clippy::too_many_lines)]
    pub(super) fn apply_block(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.block_id(name) else {
            self.warn(format!("unknown block `{name}`"));
            return;
        };
        let reference = ContentRef::block(id);
        match field {
            "requirements" => {
                self.edit_item_requirements(registry, reference, id.raw(), mode, value)
            }
            "plans" => self.edit_unit_plans(registry, reference, id.raw(), mode, value),
            field if field == "upgrades" || field.starts_with("upgrades.") => {
                self.edit_reconstructor_upgrades(registry, reference, id.raw(), field, mode, value)
            }
            "consumes" => self.edit_consumes(registry, reference, id, mode, value),
            field if field == "attributes" || field.starts_with("attributes.") => {
                self.edit_attributes(registry, reference, id.raw(), field, value)
            }
            field if field == "drillMultipliers" || field.starts_with("drillMultipliers.") => {
                self.edit_drill_multipliers(registry, reference, id.raw(), field, value)
            }
            "health" => self.set_i32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.health),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.health = v;
                    }
                },
            ),
            "armor" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.armor),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.armor = v;
                    }
                },
            ),
            "itemCapacity" => self.set_i32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.item_capacity),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.item_capacity = v;
                    }
                },
            ),
            "liquidCapacity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.liquid_capacity),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.liquid_capacity = v;
                    }
                },
            ),
            "solid" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.solid),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.solid = v;
                    }
                },
            ),
            "floating" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.floating),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.floating = v;
                    }
                },
            ),
            "update" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.update),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.update = v;
                    }
                },
            ),
            "destructible" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.destructible),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.destructible = v;
                    }
                },
            ),
            "region" => {
                if mode != FieldMode::Set {
                    return;
                }
                if let Some(region) = value.as_str() {
                    self.set_block_field(
                        registry,
                        reference,
                        field,
                        |block| block.region.clone(),
                        move |block, v: String| block.region = v,
                        region.to_owned(),
                    );
                }
            }
            "itemDrop" => {
                if mode != FieldMode::Set {
                    return;
                }
                if let Some(name) = value.as_str() {
                    let resolved = registry
                        .item_id(name)
                        .or_else(|| registry.item_id(&registry.transform_name(name)));
                    match resolved {
                        Some(item) => self.set_block_field(
                            registry,
                            reference,
                            field,
                            |block| block.item_drop,
                            move |block, v| block.item_drop = v,
                            Some(item),
                        ),
                        None => {
                            self.warn(format!("unknown item `{name}` for `block.{name}.itemDrop`"))
                        }
                    }
                }
            }
            "liquidDrop" => {
                if mode != FieldMode::Set {
                    return;
                }
                if let Some(name) = value.as_str() {
                    let resolved = registry
                        .liquid_id(name)
                        .or_else(|| registry.liquid_id(&registry.transform_name(name)));
                    match resolved {
                        Some(liquid) => self.set_block_field(
                            registry,
                            reference,
                            field,
                            |block| block.liquid_drop,
                            move |block, v| block.liquid_drop = v,
                            Some(liquid),
                        ),
                        None => self.warn(format!(
                            "unknown liquid `{name}` for `block.{name}.liquidDrop`"
                        )),
                    }
                }
            }
            _ => {
                if !self.apply_block_scalar(registry, reference, id, field, mode, value) {
                    self.warn(format!("unknown field `block.{name}.{field}`"));
                }
            }
        }
    }

    /// Applies one scalar `block.<name>.<field>` edit; `false` = unknown field.
    ///
    /// Generalises the `DataPatcher` block surface over every scalar
    /// [`BlockDef`](crate::content::BlockDef) field the JSON content format can
    /// address (`DataPatcher.java` field reflection).
    pub(super) fn apply_block_scalar(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        id: BlockId,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) -> bool {
        macro_rules! int {
            ($field:ident, $json:literal) => {
                if field == $json {
                    self.set_i32(
                        registry,
                        reference,
                        field,
                        value,
                        mode,
                        |r| r.block(id).map(|b| b.$field),
                        move |r, v| {
                            if let Some(b) = r.block_mut(id) {
                                b.$field = v;
                            }
                        },
                    );
                    return true;
                }
            };
        }
        macro_rules! float {
            ($field:ident, $json:literal) => {
                if field == $json {
                    self.set_f32(
                        registry,
                        reference,
                        field,
                        value,
                        mode,
                        |r| r.block(id).map(|b| b.$field),
                        move |r, v| {
                            if let Some(b) = r.block_mut(id) {
                                b.$field = v;
                            }
                        },
                    );
                    return true;
                }
            };
        }
        macro_rules! boolean {
            ($field:ident, $json:literal) => {
                if field == $json {
                    self.set_bool(
                        registry,
                        reference,
                        field,
                        value,
                        mode,
                        |r| r.block(id).map(|b| b.$field),
                        move |r, v| {
                            if let Some(b) = r.block_mut(id) {
                                b.$field = v;
                            }
                        },
                    );
                    return true;
                }
            };
        }

        // Numeric fields (`Block.java`/`Floor.java`/subclass field surface).
        int!(fog_radius, "fogRadius");
        int!(unit_cap_modifier, "unitCapModifier");
        int!(item_capacity, "itemCapacity");
        int!(autotile_variants, "autotileVariants");
        int!(autotile_mid_variants, "autotileMidVariants");
        int!(tiling_variants, "tilingVariants");
        int!(size_offset, "sizeOffset");
        float!(scaled_health, "scaledHealth");
        float!(build_time, "buildTime");
        float!(build_cost_multiplier, "buildCostMultiplier");
        float!(research_cost_multiplier, "researchCostMultiplier");
        float!(priority, "priority");
        float!(liquid_capacity, "liquidCapacity");
        float!(ore_threshold, "oreThreshold");
        float!(ore_scale, "oreScale");
        float!(albedo, "albedo");
        float!(visual_rotation_offset, "visualRotationOffset");
        float!(base_explosiveness, "baseExplosiveness");
        float!(explosiveness_scale, "explosivenessScale");
        float!(flammability_scale, "flammabilityScale");
        float!(speed_multiplier, "speedMultiplier");
        float!(drown_time, "drownTime");
        float!(overlay_alpha, "overlayAlpha");
        float!(floor_status_duration, "statusDuration");
        float!(floor_drag_multiplier, "dragMultiplier");
        float!(floor_damage_taken, "damageTaken");
        float!(liquid_multiplier, "liquidMultiplier");
        float!(crush_damage_multiplier, "crushDamageMultiplier");
        float!(deposit_cooldown, "depositCooldown");
        float!(construct_time, "constructTime");
        float!(craft_time, "craftTime");
        float!(offset, "offset");
        float!(building_cache_layer, "buildingCacheLayer");
        boolean!(under_bullets, "underBullets");
        boolean!(rebuildable, "rebuildable");
        boolean!(deconstruct_drop_all_liquid, "deconstructDropAllLiquid");
        boolean!(incinerate_non_buildable, "incinerateNonBuildable");
        boolean!(requires_core_zone, "requiresCoreZone");
        boolean!(ignore_build_darkness, "ignoreBuildDarkness");
        boolean!(solidifies, "solidifies");
        boolean!(save_data, "saveData");
        boolean!(save_config, "saveConfig");
        boolean!(configurable, "configurable");
        boolean!(in_editor, "inEditor");
        boolean!(placeable_player, "placeablePlayer");
        boolean!(placeable_liquid, "placeableLiquid");
        boolean!(placeable_on, "placeableOn");
        boolean!(insulated, "insulated");
        boolean!(can_overdrive, "canOverdrive");
        boolean!(absorb_lasers, "absorbLasers");
        boolean!(allow_core_placement, "allowCorePlacement");
        boolean!(player_unmineable, "playerUnmineable");
        boolean!(wall_ore, "wallOre");
        boolean!(ore_default, "oreDefault");
        boolean!(rotate, "rotate");
        boolean!(rotate_draw, "rotateDraw");
        boolean!(rotate_draw_editor, "rotateDrawEditor");
        boolean!(square_sprite, "squareSprite");
        boolean!(has_color, "hasColor");
        boolean!(draw_cached, "drawCached");
        boolean!(draw_dynamic, "drawDynamic");
        boolean!(fills_tile, "fillsTile");
        boolean!(has_shadow, "hasShadow");
        boolean!(custom_shadow, "customShadow");
        boolean!(obstructs_light, "obstructsLight");
        boolean!(emit_light, "emitLight");
        boolean!(draw_team_overlay, "drawTeamOverlay");
        boolean!(autotile, "autotile");
        boolean!(draw_edge_in, "drawEdgeIn");
        boolean!(draw_edge_out, "drawEdgeOut");
        boolean!(shallow, "shallow");
        boolean!(is_liquid, "isLiquid");
        boolean!(crush_fragile, "crushFragile");
        boolean!(unit_move_breakable, "unitMoveBreakable");
        boolean!(team_passable, "teamPassable");
        boolean!(always_replace, "alwaysReplace");
        boolean!(privileged, "privileged");
        boolean!(replaceable, "replaceable");
        boolean!(quick_rotate, "quickRotate");
        boolean!(commandable, "commandable");
        boolean!(player_controllable, "playerControllable");
        boolean!(use_color, "useColor");
        false
    }

    /// Generic `BlockDef` field setter with the reset-on-unapply bookkeeping.
    pub(super) fn set_block_field<T: Clone + 'static>(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        get: impl FnOnce(&crate::content::BlockDef) -> T,
        set: impl Fn(&mut crate::content::BlockDef, T) + Clone + 'static,
        new: T,
    ) {
        if self.mark_used(reference, field) {
            let Some(original) = registry.block(BlockId::new(reference.id)).map(get) else {
                return;
            };
            let reset = set.clone();
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(reference.id)) {
                    reset(block, original);
                }
            });
        }
        if let Some(block) = registry.block_mut(BlockId::new(reference.id)) {
            set(block, new);
        }
    }

    /// Edits `block.requirements` (`Seq<ItemStack>`).
    pub(super) fn edit_item_requirements(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        // Object-index form (`PatcherTests.specificArrayRequirements`):
        // `{"0": "surge-alloy/10"}` assigns by index.
        if mode == FieldMode::Set
            && let Value::Object(map) = value
            && !map.is_empty()
            && map.keys().all(|key| key.parse::<usize>().is_ok())
        {
            let mut edits: Vec<(usize, ItemStack)> = Vec::new();
            for (key, entry) in map {
                let Ok(index) = key.parse::<usize>() else {
                    continue;
                };
                match self.parse_item_stacks(registry, entry) {
                    Ok(stacks) if !stacks.is_empty() => edits.push((index, stacks[0])),
                    Ok(_) => {}
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                }
            }
            let Some(original) = registry
                .block(BlockId::new(raw))
                .map(|b| b.requirements.clone())
            else {
                return;
            };
            if self.mark_used(reference, "requirements") {
                self.push_reset(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.requirements = original;
                    }
                });
            }
            if let Some(block) = registry.block_mut(BlockId::new(raw)) {
                for (index, stack) in edits {
                    if index < block.requirements.len() {
                        block.requirements[index] = stack;
                    }
                }
            }
            return;
        }
        let stacks = match self.parse_item_stacks(registry, value) {
            Ok(stacks) => stacks,
            Err(message) => {
                self.warn(message);
                return;
            }
        };
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|b| b.requirements.clone())
        else {
            return;
        };
        if self.mark_used(reference, "requirements") {
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                    block.requirements = original;
                }
            });
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            match mode {
                FieldMode::Set => block.requirements = stacks,
                FieldMode::Append => block.requirements.extend(stacks),
                FieldMode::Index(index) => {
                    if index < block.requirements.len() {
                        block.requirements[index] = stacks[0];
                    }
                }
            }
        }
    }

    /// Edits `block.<name>.plans` (`Seq<UnitPlan>`): whole replace, `+` append
    /// (single or array) and numeric index (`PatcherTests.unitFactoryPlans`).
    pub(super) fn edit_unit_plans(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        use crate::content::registries::blocks::UnitPlanDef;
        let entries: Vec<&Map<String, Value>> = match value {
            Value::Object(map) => vec![map],
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match item.as_object() {
                        Some(map) => out.push(map),
                        None => {
                            self.warn("plan entries must be objects");
                            return;
                        }
                    }
                }
                out
            }
            _ => {
                self.warn("`plans` must be an object or array");
                return;
            }
        };
        let mut created = Vec::with_capacity(entries.len());
        for map in entries {
            let Some(unit_name) = map.get("unit").and_then(Value::as_str) else {
                self.warn("plan is missing `unit`");
                return;
            };
            let Some(unit) = registry
                .unit_id(unit_name)
                .or_else(|| registry.unit_id(&registry.transform_name(unit_name)))
            else {
                self.warn(format!("unknown unit `{unit_name}`"));
                return;
            };
            let requirements = match map.get("requirements") {
                Some(value) => match self.parse_item_stacks(registry, value) {
                    Ok(stacks) => stacks,
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
                None => Vec::new(),
            };
            let time = map
                .get("time")
                .and_then(Value::as_f64)
                .map(|v| v as f32)
                .unwrap_or(0.0);
            created.push(UnitPlanDef {
                unit,
                time,
                requirements,
            });
        }
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|b| b.unit_plans.clone())
        else {
            return;
        };
        if self.mark_used(reference, "plans") {
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                    block.unit_plans = original;
                }
            });
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            match mode {
                FieldMode::Set => block.unit_plans = created,
                FieldMode::Append => block.unit_plans.extend(created),
                FieldMode::Index(index) => {
                    if index < block.unit_plans.len() && !created.is_empty() {
                        block.unit_plans[index] = created.swap_remove(0);
                    }
                }
            }
        }
    }

    /// Edits `block.<name>.upgrades` (`Seq<UnitType[]>`: reconstructor upgrade
    /// pairs; `PatcherTests.reconstructorPlans`, `reconstructorPlansEditSpecific`,
    /// `reconstructorPlansAdd`, `nestedArrays`, `nestedArrays2`).
    ///
    /// The trailing `.N` / `.+` was peeled into `mode` by [`Self::apply_path`];
    /// `field`'s suffix after `upgrades` is the optional outer index used by
    /// inner-pair edits (`upgrades.0.1`).
    pub(super) fn edit_reconstructor_upgrades(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let suffix = field.strip_prefix("upgrades").unwrap_or(field);
        let suffix = suffix.strip_prefix('.').unwrap_or(suffix);

        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|block| block.reconstructor_upgrades.clone())
        else {
            return;
        };

        let mut replace: Option<Vec<(UnitTypeId, UnitTypeId)>> = None;
        let mut append: Option<Vec<(UnitTypeId, UnitTypeId)>> = None;
        let mut replace_index: Option<(usize, (UnitTypeId, UnitTypeId))> = None;
        let mut set_component: Option<(usize, usize, UnitTypeId)> = None;

        if suffix.is_empty() {
            match mode {
                FieldMode::Set => match self.parse_upgrade_pairs(registry, value) {
                    Ok(pairs) => replace = Some(pairs),
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
                FieldMode::Append => match self.parse_upgrade_pairs(registry, value) {
                    Ok(pairs) => append = Some(pairs),
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
                FieldMode::Index(index) => match self.parse_upgrade_pair(registry, value) {
                    Ok(pair) => replace_index = Some((index, pair)),
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
            }
        } else {
            let Ok(outer) = suffix.parse::<usize>() else {
                self.warn(format!("invalid upgrades index `{suffix}`"));
                return;
            };
            match mode {
                FieldMode::Index(inner) => {
                    let Some(name) = value.as_str() else {
                        self.warn("upgrade component must be a unit name");
                        return;
                    };
                    let Some(unit) = self.resolve_unit(registry, name) else {
                        self.warn(format!("unknown unit `{name}`"));
                        return;
                    };
                    set_component = Some((outer, inner, unit));
                }
                _ => {
                    self.warn(format!("invalid upgrades path `{field}`"));
                    return;
                }
            }
        }

        if self.mark_used(reference, "upgrades") {
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                    block.reconstructor_upgrades = original;
                }
            });
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            if let Some(pairs) = replace {
                block.reconstructor_upgrades = pairs;
            }
            if let Some(pairs) = append {
                block.reconstructor_upgrades.extend(pairs);
            }
            if let Some((index, pair)) = replace_index
                && index < block.reconstructor_upgrades.len()
            {
                block.reconstructor_upgrades[index] = pair;
            }
            if let Some((outer, inner, unit)) = set_component
                && let Some(pair) = block.reconstructor_upgrades.get_mut(outer)
            {
                match inner {
                    0 => pair.0 = unit,
                    1 => pair.1 = unit,
                    _ => {}
                }
            }
        }
    }

    /// Resolves a unit name (raw, then `<mod>-`-prefixed).
    pub(super) fn resolve_unit(
        &self,
        registry: &ContentRegistry,
        name: &str,
    ) -> Option<UnitTypeId> {
        registry
            .unit_id(name)
            .or_else(|| registry.unit_id(&registry.transform_name(name)))
    }

    /// Parses one `[from, to]` upgrade pair (array or `{"0":…,"1":…}` object).
    pub(super) fn parse_upgrade_pair(
        &self,
        registry: &ContentRegistry,
        value: &Value,
    ) -> Result<(UnitTypeId, UnitTypeId), String> {
        let units = match value {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let name = item
                        .as_str()
                        .ok_or("upgrade pair entries must be strings")?;
                    out.push(
                        self.resolve_unit(registry, name)
                            .ok_or_else(|| format!("unknown unit `{name}`"))?,
                    );
                }
                out
            }
            Value::Object(map) => {
                let mut out = Vec::with_capacity(2);
                for key in ["0", "1"] {
                    let Some(name) = map.get(key).and_then(Value::as_str) else {
                        return Err(format!("upgrade pair is missing index `{key}`"));
                    };
                    out.push(
                        self.resolve_unit(registry, name)
                            .ok_or_else(|| format!("unknown unit `{name}`"))?,
                    );
                }
                out
            }
            _ => return Err("upgrade pair must be an array or object".to_owned()),
        };
        if units.len() < 2 {
            return Err("upgrade pair needs exactly two units".to_owned());
        }
        Ok((units[0], units[1]))
    }

    /// Parses a `Seq<UnitType[]>` value: a list of pairs, a single flat pair,
    /// or an empty array (clear).
    pub(super) fn parse_upgrade_pairs(
        &self,
        registry: &ContentRegistry,
        value: &Value,
    ) -> Result<Vec<(UnitTypeId, UnitTypeId)>, String> {
        let Value::Array(items) = value else {
            if value.is_object() {
                return Ok(vec![self.parse_upgrade_pair(registry, value)?]);
            }
            return Err("`upgrades` must be an array of pairs".to_owned());
        };
        if items.is_empty() {
            return Ok(Vec::new());
        }
        if items.iter().all(Value::is_string) {
            return Ok(vec![self.parse_upgrade_pair(registry, value)?]);
        }
        let mut out = Vec::with_capacity(items.len());
        for entry in items {
            out.push(self.parse_upgrade_pair(registry, entry)?);
        }
        Ok(out)
    }

    /// Merges a `consumes` object (typed adds + `remove` semantics; M3 subset).
    pub(super) fn edit_consumes(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        id: BlockId,
        _mode: FieldMode,
        value: &Value,
    ) {
        let Some(map) = value.as_object() else {
            self.warn("`consumes` must be an object");
            return;
        };
        let Some(original) = registry.block(id).map(|b| {
            (
                b.consumes.clone(),
                b.has_power,
                b.has_items,
                b.accepts_items,
                b.has_liquids,
            )
        }) else {
            return;
        };
        let raw = id.raw();
        if self.mark_used(reference, "consumes") {
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                    block.consumes = original.0.clone();
                    block.has_power = original.1;
                    block.has_items = original.2;
                    block.accepts_items = original.3;
                    block.has_liquids = original.4;
                }
            });
        }
        let remove = map.get("remove").and_then(Value::as_str);
        let power = map.get("power").and_then(Value::as_f64).map(|v| v as f32);
        let buffered = map
            .get("powerBuffered")
            .and_then(Value::as_f64)
            .map(|v| v as f32);
        let coolant = map.get("coolant").and_then(Value::as_f64).map(|v| v as f32);
        // `readBlockConsumers` typed entries (`item`/`items`, `liquid`/`liquids`).
        let items = match map.get("items").or_else(|| map.get("item")) {
            Some(value) => match self.parse_item_stacks(registry, value) {
                Ok(stacks) => Some(stacks),
                Err(message) => {
                    self.warn(message);
                    None
                }
            },
            None => None,
        };
        let liquids = match map.get("liquids").or_else(|| map.get("liquid")) {
            Some(value) => match parse_liquid_stacks(registry, value) {
                Ok(stacks) => Some(stacks),
                Err(message) => {
                    self.warn(message);
                    None
                }
            },
            None => None,
        };
        if let Some(block) = registry.block_mut(id) {
            match remove {
                Some("all") => block.consumes.clear(),
                Some("items" | "item") => block
                    .consumes
                    .retain(|spec| !matches!(spec.consume, Consume::Items(_))),
                Some("liquids" | "liquid") => block.consumes.retain(|spec| {
                    !matches!(spec.consume, Consume::Liquid { .. } | Consume::Liquids(_))
                }),
                Some("power") => block
                    .consumes
                    .retain(|spec| !matches!(spec.consume, Consume::Power { .. })),
                Some("coolant") => block
                    .consumes
                    .retain(|spec| !matches!(spec.consume, Consume::Coolant { .. })),
                _ => {}
            }
            let spec = |consume| ConsumeSpec {
                consume,
                optional: false,
                update: false,
                ignore: false,
            };
            if let Some(items) = items {
                block.has_items = true;
                block.accepts_items = true;
                block.consumes.push(spec(Consume::Items(items)));
            }
            if let Some(liquids) = liquids {
                block.has_liquids = true;
                for stack in liquids {
                    block.consumes.push(spec(Consume::Liquid {
                        liquid: stack.liquid,
                        amount: stack.amount,
                    }));
                }
            }
            if let Some(amount) = coolant {
                block.consumes.push(spec(Consume::Coolant {
                    amount,
                    allow_liquid: true,
                    allow_gas: true,
                }));
            }
            if power.is_some() || buffered.is_some() {
                block.consumes.push(spec(Consume::Power {
                    usage: power.unwrap_or(0.0),
                    buffered: buffered.unwrap_or(0.0),
                }));
                block.has_power = true;
            }
        }
    }

    /// Edits `block.<name>.attributes` (`Attributes` map; plan 20 M3b). Accepts
    /// an object form (`{oil: 99}`) and the dotted field form
    /// (`block.grass.attributes.heat: 77`), including custom names.
    pub(super) fn edit_attributes(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        field: &str,
        value: &Value,
    ) {
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|block| block.attributes.clone())
        else {
            return;
        };
        if self.mark_used(reference, "attributes") {
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                    block.attributes = original;
                }
            });
        }
        let mut updates: Vec<(String, f32)> = Vec::new();
        if let Some(key) = field.strip_prefix("attributes.") {
            if let Some(value) = value.as_f64() {
                updates.push((key.to_owned(), value as f32));
            }
        } else if let Some(map) = value.as_object() {
            for (key, entry) in map {
                if let Some(value) = entry.as_f64() {
                    updates.push((key.clone(), value as f32));
                }
            }
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            for (key, value) in updates {
                match block.attributes.iter_mut().find(|(name, _)| *name == key) {
                    Some(entry) => entry.1 = value,
                    None => block.attributes.push((key, value)),
                }
            }
        }
    }

    /// Edits `block.<name>.drillMultipliers` (`ObjectFloatMap<Item>`; plan 20
    /// M3b). Object and dotted forms, `"-"` removes a key.
    pub(super) fn edit_drill_multipliers(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        field: &str,
        value: &Value,
    ) {
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|block| block.drill_multipliers.clone())
        else {
            return;
        };
        if self.mark_used(reference, "drillMultipliers") {
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                    block.drill_multipliers = original;
                }
            });
        }
        let mut updates: Vec<(ItemId, f32)> = Vec::new();
        let mut removals: Vec<ItemId> = Vec::new();
        let mut apply = |name: &str,
                         value: &Value,
                         updates: &mut Vec<(ItemId, f32)>,
                         removals: &mut Vec<ItemId>| {
            let Some(item) = registry
                .item_id(name)
                .or_else(|| registry.item_id(&registry.transform_name(name)))
            else {
                self.warn(format!("unknown item `{name}`"));
                return;
            };
            if value.as_str() == Some("-") {
                removals.push(item);
            } else if let Some(value) = value.as_f64() {
                updates.push((item, value as f32));
            } else {
                self.warn(format!(
                    "drillMultipliers `{name}` must be a number or \"-\""
                ));
            }
        };
        if let Some(key) = field.strip_prefix("drillMultipliers.") {
            apply(key, value, &mut updates, &mut removals);
        } else if let Some(map) = value.as_object() {
            for (key, entry) in map {
                apply(key, entry, &mut updates, &mut removals);
            }
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            for item in removals {
                block
                    .drill_multipliers
                    .retain(|(existing, _)| *existing != item);
            }
            for (item, value) in updates {
                match block
                    .drill_multipliers
                    .iter_mut()
                    .find(|(existing, _)| *existing == item)
                {
                    Some(entry) => entry.1 = value,
                    None => block.drill_multipliers.push((item, value)),
                }
            }
        }
    }
}
