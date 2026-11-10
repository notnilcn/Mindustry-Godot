// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Name/kind resolution methods and the free `resolve_*`/`parse_*`
//! helpers.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    pub(super) fn resolve_block_kind(
        &mut self,
        file: &str,
        class: &str,
    ) -> Result<BlockKind, ContentParseError> {
        if let Some(tag) = self.classmap.resolve(class, ClassScope::Block)
            && let Some(kind) = resolve_block_kind_tag(tag)
        {
            return Ok(kind);
        }
        if let Some(tag) = self.classmap.resolve(class, ClassScope::Block)
            && !resolve_block_kind_tag(tag).is_some()
        {
            // `ControlBlock`/`UnitTetherBlock` are interfaces upstream; naming
            // them as a block `type` fails there too.
            parse_err!("{file}: `{class}` is not a concrete block class");
        }
        if !self.allow_class_resolution && !self.classmap.is_known(class) {
            self.warn(
                file,
                format!("type `{class}` could not be resolved (class resolution disabled)"),
            );
        }
        Err(ContentParseError::new(format!(
            "{file}: unknown block type `{class}`"
        )))
    }

    pub(super) fn resolve_status(&self, registry: &ContentRegistry, raw: &str) -> Option<StatusId> {
        self.resolve_prefixed(registry, ContentType::Status, raw)
            .map(|reference| StatusId::new(reference.id))
    }

    pub(super) fn resolve_planet(&self, registry: &ContentRegistry, raw: &str) -> Option<PlanetId> {
        self.resolve_prefixed(registry, ContentType::Planet, raw)
            .map(|reference| PlanetId::new(reference.id))
    }

    pub(super) fn resolve_controller(&self, class: &str) -> Option<ControllerKind> {
        let tag = self.classmap.resolve(class, ClassScope::UnitController)?;
        match tag {
            "AssemblerAI" => Some(ControllerKind::Assembler),
            "BuilderAI" => Some(ControllerKind::Builder {
                core_flee_range: 400.0,
            }),
            "CargoAI" => Some(ControllerKind::Cargo),
            "NoAI" => Some(ControllerKind::No),
            "MissileAI" => Some(ControllerKind::Missile),
            _ => None,
        }
    }

    pub(super) fn resolve_ai_controller(&self, class: &str) -> Option<AiControllerKind> {
        match self.classmap.resolve(class, ClassScope::UnitController)? {
            "DefenderAI" => Some(AiControllerKind::Defender),
            "FlyingFollowAI" => Some(AiControllerKind::FlyingFollow),
            "HugAI" => Some(AiControllerKind::Hug),
            "SuicideAI" => Some(AiControllerKind::Suicide),
            // The standard AIs (`GroundAI`/`FlyingAI`/`BoostAI`/`MinerAI`/
            // `RepairAI`/`PrebuildAI`/`LogicAI`) are the port's default unit AI
            // (`AiControllerKind::Default` selects ground/flying by `flying`,
            // `registries/units/mod.rs:317-325`).
            "GroundAI" | "FlyingAI" | "BoostAI" | "MinerAI" | "RepairAI" | "PrebuildAI"
            | "LogicAI" => Some(AiControllerKind::Default),
            _ => None,
        }
    }

    /// Records a warning (unknown field/ignored value).
    pub fn warn(&mut self, file: &str, message: impl Into<String>) {
        self.warnings.push(ModWarning {
            file: file.to_owned(),
            message: message.into(),
        });
    }

    pub(super) fn warn_unknown(
        &mut self,
        file: &str,
        kind: &str,
        object: &Map<String, Value>,
        known: &[&str],
    ) {
        for key in object.keys() {
            if !known.contains(&key.as_str()) && !UNLOCK_FIELDS.contains(&key.as_str()) {
                self.warn(file, format!("unknown {kind} field `{key}`"));
            }
        }
    }

    /// Bundle entries registered from JSON `name`/`description`.
    pub fn bundle_entries(&self) -> &MemoryBundle {
        &self.bundle
    }
}

/// Splits a `<name>/<amount>` item stack string (default amount 1).
pub fn parse_stack(raw: &str) -> Option<(String, i32)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match raw.split_once('/') {
        Some((name, amount)) => Some((name.trim().to_owned(), amount.trim().parse().ok()?)),
        None => Some((raw.to_owned(), 1)),
    }
}

/// Resolves a name against the registry, applying the current mod prefix.
pub fn resolve_name(registry: &ContentRegistry, raw: &str) -> Option<String> {
    if registry.item_id(raw).is_some()
        || registry.block_id(raw).is_some()
        || registry.liquid_id(raw).is_some()
        || registry.unit_id(raw).is_some()
    {
        return Some(raw.to_owned());
    }
    let prefixed = registry.transform_name(raw);
    if registry.item_id(&prefixed).is_some()
        || registry.block_id(&prefixed).is_some()
        || registry.liquid_id(&prefixed).is_some()
        || registry.unit_id(&prefixed).is_some()
    {
        return Some(prefixed);
    }
    None
}

/// Resolves a block class tag to a [`BlockKind`].
pub fn resolve_block_kind_tag(tag: &str) -> Option<BlockKind> {
    crate::content::BlockKind::ALL
        .iter()
        .copied()
        .find(|kind| kind.name() == tag)
}

/// Resolves a block `type` class name through the committed ClassMap.
pub fn resolve_block_kind(name: &str) -> Option<BlockKind> {
    let map = ClassTagMap::committed();
    map.resolve(name, ClassScope::Block)
        .and_then(resolve_block_kind_tag)
}

/// Resolves a bullet class tag to a [`BulletKind`].
pub fn resolve_bullet_kind(tag: &str) -> Option<BulletKind> {
    Some(match tag {
        "BulletType" => BulletKind::Plain,
        "BasicBulletType" => BulletKind::Basic,
        "FireBulletType" => BulletKind::Fire,
        "SpaceLiquidBulletType" => BulletKind::SpaceLiquid,
        "ArtilleryBulletType" => BulletKind::Artillery,
        "MissileBulletType" => BulletKind::Missile,
        "LaserBoltBulletType" => BulletKind::LaserBolt,
        "SapBulletType" => BulletKind::Sap,
        "LightningBulletType" => BulletKind::Lightning,
        "LaserBulletType" => BulletKind::Laser,
        "FlakBulletType" => BulletKind::Flak,
        "ExplosionBulletType" => BulletKind::Explosion,
        "RailBulletType" => BulletKind::Rail,
        "ContinuousLaserBulletType" => BulletKind::ContinuousLaser,
        "ShrapnelBulletType" => BulletKind::Shrapnel,
        "LiquidBulletType" => BulletKind::Liquid,
        "EmpBulletType" => BulletKind::Emp,
        "BombBulletType" => BulletKind::Bomb,
        "ContinuousBulletType" => BulletKind::Continuous,
        "MultiBulletType" => BulletKind::Multi,
        "PointBulletType" => BulletKind::Point,
        "PointLaserBulletType" => BulletKind::PointLaser,
        "ContinuousFlameBulletType" => BulletKind::ContinuousFlame,
        "InterceptorBulletType" => BulletKind::Interceptor,
        "EmptyBulletType" => BulletKind::Empty,
        "MassDriverBolt" => BulletKind::MassDriver,
        _ => return None,
    })
}

/// Resolves an ability class tag to an [`AbilityKind`].
pub fn resolve_ability_kind(tag: &str) -> Option<AbilityKind> {
    Some(match tag {
        "ShieldRegenFieldAbility" => AbilityKind::ShieldRegenField,
        "RepairFieldAbility" => AbilityKind::RepairField,
        "ForceFieldAbility" => AbilityKind::ForceField,
        "StatusFieldAbility" => AbilityKind::StatusField,
        "EnergyFieldAbility" => AbilityKind::EnergyField,
        "SuppressionFieldAbility" => AbilityKind::SuppressionField,
        "ShieldArcAbility" => AbilityKind::ShieldArc,
        "MoveEffectAbility" => AbilityKind::MoveEffect,
        "SpawnDeathAbility" => AbilityKind::SpawnDeath,
        "RegenAbility" => AbilityKind::Regen,
        "LiquidExplodeAbility" => AbilityKind::LiquidExplode,
        "LiquidRegenAbility" => AbilityKind::LiquidRegen,
        // Mod-only classes with port runtime/draw (`GAPS #61`, PAR-0366).
        "ArmorPlateAbility" => AbilityKind::ArmorPlate,
        "MoveLightningAbility" => AbilityKind::MoveLightning,
        "UnitSpawnAbility" => AbilityKind::UnitSpawn,
        _ => return None,
    })
}

/// Resolves a drawer class tag to a [`DrawerKind`].
pub fn resolve_drawer_kind(tag: &str) -> Option<DrawerKind> {
    Some(match tag {
        "DrawBlock" => DrawerKind::Block,
        "DrawDefault" => DrawerKind::Default,
        "DrawMulti" => DrawerKind::Multi,
        "DrawArcSmelt" => DrawerKind::ArcSmelt,
        "DrawBubbles" => DrawerKind::Bubbles,
        "DrawCells" => DrawerKind::Cells,
        "DrawCircles" => DrawerKind::Circles,
        "DrawCrucibleFlame" => DrawerKind::CrucibleFlame,
        "DrawCultivator" => DrawerKind::Cultivator,
        "DrawFade" => DrawerKind::Fade,
        "DrawFlame" => DrawerKind::Flame,
        "DrawFrames" => DrawerKind::Frames,
        "DrawHeatInput" => DrawerKind::HeatInput,
        "DrawHeatOutput" => DrawerKind::HeatOutput,
        "DrawHeatRegion" => DrawerKind::HeatRegion,
        "DrawLiquidOutputs" => DrawerKind::LiquidOutputs,
        "DrawLiquidRegion" => DrawerKind::LiquidRegion,
        "DrawLiquidTile" => DrawerKind::LiquidTile,
        "DrawMultiWeave" => DrawerKind::MultiWeave,
        "DrawParticles" => DrawerKind::Particles,
        "DrawPistons" => DrawerKind::Pistons,
        "DrawPlasma" => DrawerKind::Plasma,
        "DrawPower" => DrawerKind::Power,
        "DrawPulseShape" => DrawerKind::PulseShape,
        "DrawPumpLiquid" => DrawerKind::PumpLiquid,
        "DrawSoftParticles" => DrawerKind::SoftParticles,
        "DrawSpikes" => DrawerKind::Spikes,
        "DrawWeave" => DrawerKind::Weave,
        "DrawBlockParts" => DrawerKind::BlockParts,
        "DrawRegion" => DrawerKind::Region,
        "DrawRegionSpin" => DrawerKind::RegionSpin,
        "DrawTurret" => DrawerKind::Turret,
        "DrawWeapon" => DrawerKind::Weapon,
        "DrawSideRegion" => DrawerKind::SideRegion,
        "DrawGlowRegion" => DrawerKind::GlowRegion,
        "DrawArc" => DrawerKind::Arc,
        "DrawBlurSpin" => DrawerKind::BlurSpin,
        "DrawParticle" => DrawerKind::Particle,
        "DrawShape" => DrawerKind::Shape,
        "DrawSoftShadow" => DrawerKind::SoftShadow,
        "DrawWarmupRegion" => DrawerKind::WarmupRegion,
        _ => return None,
    })
}

/// Resolves a draw-part class tag to a [`DrawPartKind`].
pub fn resolve_draw_part_kind(tag: &str) -> Option<DrawPartKind> {
    Some(match tag {
        "RegionPart" | "DrawPart" => DrawPartKind::RegionPart,
        "ShapePart" => DrawPartKind::ShapePart,
        "HoverPart" => DrawPartKind::HoverPart,
        "FlarePart" => DrawPartKind::FlarePart,
        "HaloPart" => DrawPartKind::HaloPart,
        _ => return None,
    })
}

/// Resolves a `PartProgress` constant name (`PartProgress.java:75-95`).
pub fn part_progress_constant(name: &str) -> Option<PartProgressSpec> {
    Some(match name {
        "reload" => PartProgressSpec::Reload,
        "smoothReload" => PartProgressSpec::SmoothReload,
        "warmup" => PartProgressSpec::Warmup,
        "charge" => PartProgressSpec::Charge,
        "recoil" => PartProgressSpec::Recoil,
        "heat" => PartProgressSpec::Heat,
        "life" => PartProgressSpec::Life,
        "time" => PartProgressSpec::Time,
        _ => return None,
    })
}

/// Resolves an `Interp` name to the ported [`InterpKind`] subset.
pub fn parse_interp(name: &str) -> InterpKind {
    match name {
        "one" => InterpKind::One,
        "slope" => InterpKind::Slope,
        "pow2In" => InterpKind::Pow2In,
        "pow5In" => InterpKind::Pow5In,
        _ => InterpKind::Linear,
    }
}

/// Resolves a shoot-pattern class tag to a [`ShootPatternKind`].
pub fn resolve_shoot_kind(tag: &str) -> Option<ShootPatternKind> {
    Some(match tag {
        "ShootPattern" => ShootPatternKind::ShootPattern,
        "ShootAlternate" => ShootPatternKind::ShootAlternate,
        "ShootSpread" => ShootPatternKind::ShootSpread,
        "ShootHelix" => ShootPatternKind::ShootHelix,
        "ShootBarrel" => ShootPatternKind::ShootBarrel,
        "ShootMulti" => ShootPatternKind::ShootMulti,
        "ShootSine" => ShootPatternKind::ShootSine,
        "ShootSummon" => ShootPatternKind::ShootSummon,
        _ => return None,
    })
}

/// Resolves a weather class tag to a [`WeatherKind`].
pub fn resolve_weather_kind(tag: &str) -> Option<WeatherKind> {
    Some(match tag {
        "Weather" => WeatherKind::Plain,
        "ParticleWeather" => WeatherKind::Particle,
        "RainWeather" => WeatherKind::Rain,
        "MagneticStorm" => WeatherKind::MagneticStorm,
        "SolarFlare" => WeatherKind::SolarFlare,
        _ => return None,
    })
}

/// Resolves a unit class tag to a [`UnitKind`].
pub fn resolve_unit_class_kind(tag: &str) -> Option<UnitKind> {
    Some(match tag {
        "UnitType" => UnitKind::UnitType,
        "ErekirUnitType" => UnitKind::ErekirUnitType,
        "TankUnitType" => UnitKind::TankUnitType,
        "MissileUnitType" => UnitKind::MissileUnitType,
        "NeoplasmUnitType" => UnitKind::NeoplasmUnitType,
        _ => return None,
    })
}

/// Resolves a unit `type`/entity keyword to an [`EntityDefSpec`].
pub fn resolve_entity_def(name: &str) -> Option<EntityDefSpec> {
    use crate::content::registries::units::entity;
    Some(match name {
        "mech" => entity::MECH,
        "legs" | "leg" => entity::LEGS,
        "flyer" | "flying" | "air" => entity::AIR,
        "hover" => entity::HOVER,
        "naval" | "water" | "ship" => entity::NAVAL,
        "payload" => entity::AIR_PAYLOAD,
        "tank" => entity::TANK,
        "missile" => entity::MISSILE,
        "crawl" => entity::CRAWL,
        "block" => entity::BLOCK,
        _ => return None,
    })
}

/// Resolves a `BuildVisibility` name.
pub fn resolve_build_visibility(name: &str) -> Option<BuildVisibility> {
    match name {
        "hidden" => Some(BuildVisibility::Hidden),
        "shown" => Some(BuildVisibility::Shown),
        "debugOnly" => Some(BuildVisibility::DebugOnly),
        "editorOnly" => Some(BuildVisibility::EditorOnly),
        "sandboxOnly" => Some(BuildVisibility::SandboxOnly),
        _ => None,
    }
}

/// Resolves a `Category` name.
pub fn resolve_category(name: &str) -> Option<Category> {
    match name {
        "turret" => Some(Category::Turret),
        "production" => Some(Category::Production),
        "distribution" => Some(Category::Distribution),
        "liquid" => Some(Category::Liquid),
        "power" => Some(Category::Power),
        "defense" => Some(Category::Defense),
        "crafting" => Some(Category::Crafting),
        "units" => Some(Category::Units),
        "effect" => Some(Category::Effect),
        "logic" => Some(Category::Logic),
        _ => None,
    }
}

/// Resolves an environment [`Attribute`] name.
pub fn resolve_attribute(name: &str) -> Option<Attribute> {
    match name {
        "heat" => Some(Attribute::Heat),
        "spores" => Some(Attribute::Spores),
        "water" => Some(Attribute::Water),
        "light" => Some(Attribute::Light),
        _ => None,
    }
}

/// Resolves a `MeshKind` name.
pub fn resolve_mesh_kind(name: &str) -> Option<MeshKind> {
    Some(match name {
        "shaderSphere" | "ShaderSphereMesh" => MeshKind::ShaderSphere,
        "sun" | "SunMesh" => MeshKind::Sun,
        "hexSphere" | "HexMesh" => MeshKind::Hex { detail: 5 },
        "asteroid" | "NoiseMesh" => MeshKind::Asteroid,
        _ => return None,
    })
}

/// Resolves a `CloudMeshKind` name.
pub fn resolve_cloud_mesh_kind(name: &str) -> Option<CloudMeshKind> {
    Some(match name {
        "cloudMesh" | "multiHex" | "MultiMesh" => CloudMeshKind::MultiHex,
        _ => return None,
    })
}

/// Resolves a `GeneratorKind` name.
pub fn resolve_generator_kind(name: &str) -> Option<GeneratorKind> {
    Some(match name {
        "serpulo" | "SerpuloPlanetGenerator" => GeneratorKind::Serpulo,
        "erekir" | "ErekirPlanetGenerator" => GeneratorKind::Erekir,
        "tantros" | "TantrosPlanetGenerator" => GeneratorKind::Tantros,
        "asteroid" | "AsteroidGenerator" => GeneratorKind::Asteroid,
        "fileMap" | "FileMapGenerator" => GeneratorKind::FileMap,
        "none" | "None" => GeneratorKind::None,
        _ => return None,
    })
}

/// `BlockGroup` name to enum (`BlockGroup.java`; unknown names fall back to
/// `none` like an unknown enum constant in the parser's lenient mode).
pub fn resolve_block_group(name: &str) -> crate::content::registries::blocks::BlockGroup {
    use crate::content::registries::blocks::BlockGroup;
    match name {
        "walls" => BlockGroup::Walls,
        "projectors" => BlockGroup::Projectors,
        "turrets" => BlockGroup::Turrets,
        "transportation" => BlockGroup::Transportation,
        "power" => BlockGroup::Power,
        "liquids" => BlockGroup::Liquids,
        "drills" => BlockGroup::Drills,
        "units" => BlockGroup::Units,
        "logic" => BlockGroup::Logic,
        "payloads" => BlockGroup::Payloads,
        "heat" => BlockGroup::Heat,
        _ => BlockGroup::None,
    }
}

/// `CacheLayer` name to enum (`CacheLayer.java` order).
pub fn resolve_cache_layer(name: &str) -> Option<crate::render::layer::CacheLayerId> {
    use crate::render::layer::CacheLayerId;
    CacheLayerId::ALL
        .iter()
        .copied()
        .find(|layer| layer.name() == name)
}

/// Parses a JSON color (hex string or `[r,g,b,a]`).
pub fn parse_color(value: &Value) -> Option<Rgba> {
    match value {
        Value::String(hex) => Rgba::from_hex(hex),
        Value::Array(components) => {
            let channel = |index: usize| {
                components
                    .get(index)
                    .and_then(Value::as_f64)
                    .map(|value| value as f32)
            };
            Some(Rgba::new(
                channel(0)?,
                channel(1)?,
                channel(2)?,
                channel(3).unwrap_or(1.0),
            ))
        }
        _ => None,
    }
}

/// Resolves a legacy/current content folder to its type.
pub fn content_type_for_folder(folder: &str) -> Option<ContentType> {
    match folder {
        "items" | "item" => Some(ContentType::Item),
        "blocks" | "block" => Some(ContentType::Block),
        "liquids" | "liquid" => Some(ContentType::Liquid),
        "statuses" | "status" => Some(ContentType::Status),
        "units" | "unit" => Some(ContentType::Unit),
        "weathers" | "weather" => Some(ContentType::Weather),
        "sectors" | "sector" => Some(ContentType::Sector),
        "planets" | "planet" => Some(ContentType::Planet),
        "teams" | "team" => Some(ContentType::Team),
        _ => None,
    }
}
