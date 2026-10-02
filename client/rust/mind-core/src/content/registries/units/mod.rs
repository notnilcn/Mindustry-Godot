// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (metadata half; 65 records),
//         core/src/mindustry/type/UnitType.java (fields + `init`/`postInit`/
//         `researchRequirements` metadata derivations),
//         core/src/mindustry/type/unit/{ErekirUnitType,TankUnitType,
//         MissileUnitType,NeoplasmUnitType}.java (preset defaults),
//         core/src/mindustry/entities/UnitEngine.java (value type).

//! Unit metadata registry (plan 02 M5).
//!
//! Behavior (entity/component system, controllers, mirroring at runtime,
//! pathfinding, AI) is owned by plan 11; this module carries the data half of
//! `UnitType` plus the `@EntityDef` table (`annotations/AGENTS.md`) the codegen
//! consumed. Waves [`standard`], [`erekir`] and [`special`] are generated from
//! `UnitTypes.java` by `parity/tools/gen_units.py` in upstream order.
//!
//! ## Layout
//!
//! * [`UnitTypeDef`] — the full metadata record; derived fields are filled by
//!   [`Content::init_self`] exactly like `UnitType.init()`.
//! * [`UnitSpec`] — the generated wave input (name-based references, class
//!   defaults from [`UnitSpec::for_kind`]).
//! * [`EntityDefSpec`] — the `@EntityDef` component groups (19 declarations,
//!   verbatim) that drive `init()` derivations and plan 11's entity table.
//! * [`weapon`], [`ability`], [`parts`] — weapon/ability/draw-part metadata.

pub mod ability;
pub mod parts;
pub mod weapon;

pub mod erekir;
pub mod special;
pub mod standard;

use self::ability::AbilitySpec;
use self::parts::DrawPartSpec;
use self::weapon::{BulletRef, WeaponDef, WeaponSpec};
use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::{
    BlockId, BulletId, ItemId, StatusId, UnitCommandId, UnitStanceId, UnitTypeId,
};
use super::super::settings_store::UnlockStore;
use super::super::stacks::{ItemSeq, ItemStack, round_to_i32};
use super::super::{ContentError, ContentRef, ContentType};
use super::blocks::{BlockFlag, Consume, EnvMask, round_to, ui_round_amount};
use super::bullets::{BulletDef, BulletSpec};
use super::pal;
use super::sound_meta::SoundId;
use super::{ContentRegistry, planets::EnvFlag};

pub use super::fx_meta::{EffectId, EffectRef, EffectSpec, InlineEffectKind};
pub use ability::AbilityKind;
pub use parts::{DrawPartKind, PartProgressSpec};
pub use weapon::{ShootPatternKind, WeaponKind};

/// `Vars.buildingRange` (`core/src/mindustry/Vars.java:119`).
pub const BUILDING_RANGE: f32 = 220.0;

/// `Layer.groundUnit` (`graphics/Layer.java:54`).
pub const LAYER_GROUND_UNIT: f32 = 60.0;
/// `Layer.flyingUnitLow` (`graphics/Layer.java:69`).
pub const LAYER_FLYING_UNIT_LOW: f32 = 90.0;
/// `Layer.effect` (`graphics/Layer.java:75`).
pub const LAYER_EFFECT: f32 = 110.0;
/// `Layer.flyingUnit` (`graphics/Layer.java:78`).
pub const LAYER_FLYING_UNIT: f32 = 115.0;

/// Java unit class tag (`UnitKind` content ABI; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UnitKind {
    /// `mindustry.type.UnitType`.
    #[default]
    UnitType = 0,
    /// `mindustry.type.unit.ErekirUnitType`.
    ErekirUnitType = 1,
    /// `mindustry.type.unit.TankUnitType` (extends `ErekirUnitType`).
    TankUnitType = 2,
    /// `mindustry.type.unit.MissileUnitType`.
    MissileUnitType = 3,
    /// `mindustry.type.unit.NeoplasmUnitType`.
    NeoplasmUnitType = 4,
}

impl UnitKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            UnitKind::UnitType => "UnitType",
            UnitKind::ErekirUnitType => "ErekirUnitType",
            UnitKind::TankUnitType => "TankUnitType",
            UnitKind::MissileUnitType => "MissileUnitType",
            UnitKind::NeoplasmUnitType => "NeoplasmUnitType",
        }
    }
}

/// Entity component tag from `@EntityDef` component lists
/// (`entities/comp/*`; the `c` suffix is part of the Java interface name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnitComponent {
    /// `Unitc`.
    Unit,
    /// `Mechc`.
    Mech,
    /// `Legsc`.
    Legs,
    /// `ElevationMovec`.
    ElevationMove,
    /// `WaterMovec`.
    WaterMove,
    /// `Payloadc`.
    Payload,
    /// `BlockUnitc`.
    BlockUnit,
    /// `BuildingTetherc`.
    BuildingTether,
    /// `TargetDummyc`.
    TargetDummy,
    /// `Tankc`.
    Tank,
    /// `TimedKillc`.
    TimedKill,
    /// `Crawlc`.
    Crawl,
}

impl UnitComponent {
    /// Java interface name (`<Comp>c`).
    pub const fn name(self) -> &'static str {
        match self {
            UnitComponent::Unit => "Unitc",
            UnitComponent::Mech => "Mechc",
            UnitComponent::Legs => "Legsc",
            UnitComponent::ElevationMove => "ElevationMovec",
            UnitComponent::WaterMove => "WaterMovec",
            UnitComponent::Payload => "Payloadc",
            UnitComponent::BlockUnit => "BlockUnitc",
            UnitComponent::BuildingTether => "BuildingTetherc",
            UnitComponent::TargetDummy => "TargetDummyc",
            UnitComponent::Tank => "Tankc",
            UnitComponent::TimedKill => "TimedKillc",
            UnitComponent::Crawl => "Crawlc",
        }
    }
}

/// `@EntityDef` component group (metadata replacing the codegen input).
///
/// `class_name` is the generated `mindustry.gen` entity class for the group
/// (first declaration wins; `annotations/.../EntityProcess.java:298`):
/// alphabetical component concatenation, `Entity` appended on base-name
/// collision, `Legacy<Field>` for legacy groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityDefSpec {
    /// Component list, in declaration order.
    pub components: &'static [UnitComponent],
    /// `@EntityDef(legacy = true)`.
    pub legacy: bool,
    /// Generated entity class name (`EntityMapping` target).
    pub class_name: &'static str,
}

impl EntityDefSpec {
    /// Whether the entity is a water-move unit (`WaterMovec`).
    pub fn is_naval(&self) -> bool {
        self.components.contains(&UnitComponent::WaterMove)
    }

    /// Whether the entity steps on legs (`Legsc`/`Crawlc`).
    pub fn allow_leg_step(&self) -> bool {
        self.components.contains(&UnitComponent::Legs)
            || self.components.contains(&UnitComponent::Crawl)
    }

    /// Whether the entity carries payloads (`Payloadc`).
    pub fn is_payload(&self) -> bool {
        self.components.contains(&UnitComponent::Payload)
    }
}

impl Default for EntityDefSpec {
    fn default() -> Self {
        entity::AIR
    }
}

/// The 19 `@EntityDef` declaration groups of `UnitTypes.java` (verbatim).
pub mod entity {
    use super::{EntityDefSpec, UnitComponent};

    /// `{Unitc.class, Mechc.class}` — mace, dagger, crawler, fortress, scepter, reign, vela.
    pub static MECH: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Mech],
        legacy: false,
        class_name: "MechUnit",
    };
    /// `{Unitc.class, Mechc.class}` legacy — nova, pulsar, quasar.
    pub static MECH_LEGACY: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Mech],
        legacy: true,
        class_name: "MechUnitLegacyNova",
    };
    /// `{Unitc.class, Legsc.class}` — corvus, atrax, merui, cleroi, anthicus, tecta, collaris.
    pub static LEGS: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Legs],
        legacy: false,
        class_name: "LegsUnit",
    };
    /// `{Unitc.class, Legsc.class}` legacy — spiroct, arkyid, toxopid.
    pub static LEGS_LEGACY: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Legs],
        legacy: true,
        class_name: "LegsUnitLegacySpiroct",
    };
    /// `{Unitc.class, ElevationMovec.class}` — elude.
    pub static HOVER: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::ElevationMove],
        legacy: false,
        class_name: "ElevationMoveUnit",
    };
    /// `{Unitc.class}` — flare, eclipse, horizon, zenith, antumbra, avert, obviate.
    pub static AIR: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit],
        legacy: false,
        class_name: "UnitEntity",
    };
    /// `{Unitc.class}` legacy — mono.
    pub static AIR_LEGACY_MONO: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit],
        legacy: true,
        class_name: "UnitEntityLegacyMono",
    };
    /// `{Unitc.class}` legacy — poly.
    pub static AIR_LEGACY_POLY: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit],
        legacy: true,
        class_name: "UnitEntityLegacyPoly",
    };
    /// `{Unitc.class, Payloadc.class}` — mega, evoke, incite, emanate, quell, disrupt.
    pub static AIR_PAYLOAD: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Payload],
        legacy: false,
        class_name: "PayloadUnit",
    };
    /// `{Unitc.class, Payloadc.class}` legacy — quad.
    pub static AIR_PAYLOAD_LEGACY_QUAD: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Payload],
        legacy: true,
        class_name: "PayloadUnitLegacyQuad",
    };
    /// `{Unitc.class, Payloadc.class}` legacy — oct.
    pub static AIR_PAYLOAD_LEGACY_OCT: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Payload],
        legacy: true,
        class_name: "PayloadUnitLegacyOct",
    };
    /// `{Unitc.class}` legacy — alpha, beta, gamma.
    pub static AIR_LEGACY_ALPHA: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit],
        legacy: true,
        class_name: "UnitEntityLegacyAlpha",
    };
    /// `{Unitc.class, WaterMovec.class}` — risso .. navanax.
    pub static NAVAL: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::WaterMove],
        legacy: false,
        class_name: "UnitWaterMove",
    };
    /// `{Unitc.class, BlockUnitc.class}` — block.
    pub static BLOCK: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::BlockUnit],
        legacy: false,
        class_name: "BlockUnit",
    };
    /// `{Unitc.class, BuildingTetherc.class, Payloadc.class}` — manifold, assemblyDrone.
    pub static TETHER: EntityDefSpec = EntityDefSpec {
        components: &[
            UnitComponent::Unit,
            UnitComponent::BuildingTether,
            UnitComponent::Payload,
        ],
        legacy: false,
        class_name: "BuildingTetherPayloadUnit",
    };
    /// `{TargetDummyc.class, Unitc.class}` — dummy.
    pub static DUMMY: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::TargetDummy, UnitComponent::Unit],
        legacy: false,
        class_name: "TargetDummyUnit",
    };
    /// `{Unitc.class, Tankc.class}` — stell, locus, precept, vanquish, conquer.
    pub static TANK: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Tank],
        legacy: false,
        class_name: "TankUnit",
    };
    /// `{Unitc.class, TimedKillc.class}` — missile (codegen-only field).
    pub static MISSILE: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::TimedKill],
        legacy: false,
        class_name: "TimedKillUnit",
    };
    /// `{Unitc.class, Crawlc.class}` — latum, renale.
    pub static CRAWL: EntityDefSpec = EntityDefSpec {
        components: &[UnitComponent::Unit, UnitComponent::Crawl],
        legacy: false,
        class_name: "CrawlUnit",
    };
}

/// `aiController` kind tag (`ai/types/*`; `Default` = ground/flying by `flying`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AiControllerKind {
    /// `() -> !flying ? new GroundAI() : new FlyingAI()` (`UnitType` default).
    #[default]
    Default = 0,
    /// `DefenderAI::new`.
    Defender = 1,
    /// `FlyingFollowAI::new`.
    FlyingFollow = 2,
    /// `HugAI::new`.
    Hug = 3,
    /// `SuicideAI::new`.
    Suicide = 4,
}

impl AiControllerKind {
    /// Java class-ish name.
    pub const fn name(self) -> &'static str {
        match self {
            AiControllerKind::Default => "default",
            AiControllerKind::Defender => "DefenderAI",
            AiControllerKind::FlyingFollow => "FlyingFollowAI",
            AiControllerKind::Hug => "HugAI",
            AiControllerKind::Suicide => "SuicideAI",
        }
    }
}

/// `controller` kind tag (`Func<Unit, UnitController>`; plan 11 dispatches).
///
/// Ordinal order is declaration order (content ABI, append-only).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ControllerKind {
    /// The `UnitType` default lambda (`aiController` for AI teams, else
    /// `CommandAI`; non-player-controllable units always `aiController`).
    #[default]
    Default,
    /// `u -> new AssemblerAI()`.
    Assembler,
    /// `u -> new BuilderAI(true, coreFleeRange)`.
    Builder {
        /// `coreFleeRange` (500 for the core units).
        core_flee_range: f32,
    },
    /// `u -> u.team.isAI() ? new BuilderAI(true, 400f) : new CommandAI()`.
    BuilderOrCommand,
    /// `u -> new CargoAI()`.
    Cargo,
    /// `u -> new NoAI()`.
    No,
    /// `u -> new MissileAI()` (`MissileUnitType` preset).
    Missile,
}

impl ControllerKind {
    /// Debug/audit name.
    pub const fn name(self) -> &'static str {
        match self {
            ControllerKind::Default => "default",
            ControllerKind::Assembler => "AssemblerAI",
            ControllerKind::Builder { .. } => "BuilderAI",
            ControllerKind::BuilderOrCommand => "BuilderAI|CommandAI",
            ControllerKind::Cargo => "CargoAI",
            ControllerKind::No => "NoAI",
            ControllerKind::Missile => "MissileAI",
        }
    }
}

/// `UnitEngine` value (`entities/UnitEngine.java`: x, y, radius, rotation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineSpec {
    /// X offset.
    pub x: f32,
    /// Y offset.
    pub y: f32,
    /// Engine radius.
    pub radius: f32,
    /// Rotation in degrees.
    pub rotation: f32,
}

impl EngineSpec {
    /// Builds an engine (`new UnitEngine(x, y, radius, rotation)`).
    pub const fn new(x: f32, y: f32, radius: f32, rotation: f32) -> Self {
        Self {
            x,
            y,
            radius,
            rotation,
        }
    }

    /// The mirrored copy added by `UnitType.setEnginesMirror`.
    pub fn mirrored(&self) -> Self {
        let mut copy = *self;
        copy.x *= -1.0;
        copy.rotation = 180.0 - copy.rotation;
        if copy.rotation < 0.0 {
            copy.rotation += 360.0;
        }
        copy
    }
}

/// Tread rectangle (`Rect` in image coordinates, relative to the center).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreadRect {
    /// Left offset (image px, center-relative).
    pub x: f32,
    /// Top offset (image px, center-relative).
    pub y: f32,
    /// Width (image px).
    pub width: f32,
    /// Height (image px).
    pub height: f32,
}

/// Bullet handle resolved during the unit load pass.
///
/// Carries the values `UnitType.init()` reads from the bullet (`range`, `heals`,
/// `killShooter`) snapshotted at load time, plus the DPS estimate patched in by
/// the post-sweep [`link`] pass (after `ErekirTechTree.rebalance` has scaled
/// damages, matching upstream's lazy `cachedDps`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedBullet {
    /// Registered bullet id.
    pub id: BulletId,
    /// `BulletType.range` (computed at registration; see [`BulletDef`]).
    pub range: f32,
    /// `BulletType.heals()`.
    pub heals: bool,
    /// `BulletType.killShooter`.
    pub kill_shooter: bool,
    /// `BulletType.estimateDPS()` (post-rebalance; set by [`link`]).
    pub dps: f32,
}

impl ResolvedBullet {
    /// `Weapon.range()`.
    pub fn range(&self) -> f32 {
        self.range
    }

    /// `bullet.estimateDPS()`.
    pub fn estimate_dps(&self) -> f32 {
        self.dps
    }

    /// `bullet.heals()`.
    pub fn heals(&self) -> bool {
        self.heals
    }
}

/// Generated unit input record (name-based references).
///
/// Scalar fields default to the Java `UnitType` defaults plus the class-tag
/// preset overrides (see [`UnitSpec::for_kind`]); `None` means "class default".
/// List fields (`weapons`, `abilities`, `parts`, `engines_mirror`,
/// `pre_bullets`, `immunities`) are appended by the generated waves *after* the
/// preset entries, matching upstream constructor-then-initializer order.
#[derive(Debug, Clone, Default)]
pub struct UnitSpec {
    /// Content name (parity ABI).
    pub name: &'static str,
    /// Java class tag.
    pub kind: UnitKind,
    /// `@EntityDef` component group.
    pub entity_def: EntityDefSpec,
    /// Movement speed (world units/tick).
    pub speed: Option<f32>,
    /// Boost speed multiplier.
    pub boost_multiplier: Option<f32>,
    /// Terrain speed multiplier.
    pub floor_multiplier: Option<f32>,
    /// Body rotation speed (degrees/tick).
    pub rotate_speed: Option<f32>,
    /// Mech base rotation speed.
    pub base_rotate_speed: Option<f32>,
    /// Movement drag fraction.
    pub drag: Option<f32>,
    /// Acceleration fraction.
    pub accel: Option<f32>,
    /// Hitbox side.
    pub hit_size: Option<f32>,
    /// Clipping size (`<0` = derive).
    pub clip_size: Option<f32>,
    /// Always unlocked in the tech tree (`UnlockableContent.alwaysUnlocked`).
    pub always_unlocked: Option<bool>,
    /// Hide details in custom games when locked (`UnlockableContent.hideDetails`).
    pub hide_details: Option<bool>,
    /// Death screen shake (`<0` = derive).
    pub death_shake: Option<f32>,
    /// Step shake (`<0` = derive).
    pub step_shake: Option<f32>,
    /// Ripple/dust scale.
    pub ripple_scale: Option<f32>,
    /// Boost rise speed.
    pub rise_speed: Option<f32>,
    /// Boost descent speed.
    pub descent_speed: Option<f32>,
    /// Death fall speed.
    pub fall_speed: Option<f32>,
    /// Missile acceleration ticks.
    pub missile_accel_time: Option<f32>,
    /// Raw health.
    pub health: Option<f32>,
    /// Armor.
    pub armor: Option<f32>,
    /// Approach range override (`<0` = derive).
    pub range: Option<f32>,
    /// Max range override (`<0` = derive).
    pub max_range: Option<f32>,
    /// Mining range.
    pub mine_range: Option<f32>,
    /// Build range.
    pub build_range: Option<f32>,
    /// Circle-target radius.
    pub circle_target_radius: Option<f32>,
    /// Crash damage multiplier.
    pub crash_damage_multiplier: Option<f32>,
    /// Wreck health multiplier.
    pub wreck_health_multiplier: Option<f32>,
    /// Drown time multiplier.
    pub drown_time_multiplier: Option<f32>,
    /// Strafe penalty.
    pub strafe_penalty: Option<f32>,
    /// Research cost multiplier.
    pub research_cost_multiplier: Option<f32>,
    /// Knockback multiplier.
    pub knockback_multiplier: Option<f32>,
    /// Ground draw layer.
    pub ground_layer: Option<f32>,
    /// Flying draw layer (`<0` = derive).
    pub flying_layer: Option<f32>,
    /// Payload capacity (world units²).
    pub payload_capacity: Option<f32>,
    /// Build speed (`<0` = disabled).
    pub build_speed: Option<f32>,
    /// Minimum weapon target distance (`<0` = derive).
    pub aim_dst: Option<f32>,
    /// Build beam offset.
    pub build_beam_offset: Option<f32>,
    /// Targeting priority.
    pub target_priority: Option<f32>,
    /// Shadow elevation (`<0` = derive/none).
    pub shadow_elevation: Option<f32>,
    /// Shadow elevation scale.
    pub shadow_elevation_scl: Option<f32>,
    /// Engine offset.
    pub engine_offset: Option<f32>,
    /// Engine radius (0 disables the default engine).
    pub engine_size: Option<f32>,
    /// Engine layer (`<0` = default).
    pub engine_layer: Option<f32>,
    /// Item draw offset.
    pub item_offset_y: Option<f32>,
    /// Light radius (`<0` = derive).
    pub light_radius: Option<f32>,
    /// Light opacity.
    pub light_opacity: Option<f32>,
    /// Soft shadow scale.
    pub soft_shadow_scl: Option<f32>,
    /// Fog radius in tiles (`<0` = derive).
    pub fog_radius: Option<f32>,
    /// Wave trail X.
    pub wave_trail_x: Option<f32>,
    /// Wave trail Y.
    pub wave_trail_y: Option<f32>,
    /// Trail scale.
    pub trail_scl: Option<f32>,
    /// Counts as an enemy in the wave counter.
    pub is_enemy: Option<bool>,
    /// Always at elevation 1.
    pub flying: Option<bool>,
    /// Wobbles while flying.
    pub wobble: Option<bool>,
    /// Targets air.
    pub target_air: Option<bool>,
    /// Targets ground.
    pub target_ground: Option<bool>,
    /// Faces target when aiming.
    pub face_target: Option<bool>,
    /// Bomber circling AI flag.
    pub circle_target: Option<bool>,
    /// Carpet-bomber flag.
    pub auto_drop_bombs: Option<bool>,
    /// Mobile auto-target buildings flag.
    pub target_buildings_mobile: Option<bool>,
    /// Can boost.
    pub can_boost: Option<bool>,
    /// Always boosts when building.
    pub boost_when_building: Option<bool>,
    /// Always boosts when mining.
    pub boost_when_mining: Option<bool>,
    /// Logic-controllable.
    pub logic_controllable: Option<bool>,
    /// Player-controllable.
    pub player_controllable: Option<bool>,
    /// Global selection hotkey flag.
    pub control_select_global: Option<bool>,
    /// Can enter payloads.
    pub allowed_in_payloads: Option<bool>,
    /// Hittable.
    pub hittable: Option<bool>,
    /// Killable.
    pub killable: Option<bool>,
    /// Targetable.
    pub targetable: Option<bool>,
    /// Hittable while carrying payloads.
    pub vulnerable_with_payloads: Option<bool>,
    /// Can pick up units.
    pub pickup_units: Option<bool>,
    /// Physically collides.
    pub physics: Option<bool>,
    /// Drowns in deep liquids.
    pub can_drown: Option<bool>,
    /// Counts toward the unit cap.
    pub use_unit_cap: Option<bool>,
    /// Core-unit docking.
    pub core_unit_dock: Option<bool>,
    /// Creates a wreck on death.
    pub create_wreck: Option<bool>,
    /// Creates scorch marks on death.
    pub create_scorch: Option<bool>,
    /// Drawn under effects (low-altitude flight).
    pub low_altitude: Option<bool>,
    /// Looks at buildings while building.
    pub rotate_to_building: Option<bool>,
    /// Explicit leg-step override (derived from the entity def in `init`).
    pub allow_leg_step: Option<bool>,
    /// Leg physics layer flag.
    pub leg_physics_layer: Option<bool>,
    /// Hovers (ignores floor).
    pub hovering: Option<bool>,
    /// Omni-directional movement.
    pub omni_movement: Option<bool>,
    /// Faces move direction first.
    pub rotate_move_first: Option<bool>,
    /// Flashes when healed.
    pub heal_flash: Option<bool>,
    /// Single-target weapons.
    pub single_target: Option<bool>,
    /// Multi-target with one weapon.
    pub force_multi_target: Option<bool>,
    /// Hidden from database.
    pub hidden: Option<bool>,
    /// Internal unit (no sprite generation).
    pub internal: Option<bool>,
    /// Generate sprites despite being internal.
    pub internal_generate_sprites: Option<bool>,
    /// Pushed away from map edges.
    pub bounded: Option<bool>,
    /// RTS auto-attack while moving.
    pub auto_find_target: Option<bool>,
    /// Targets "under" blocks.
    pub target_under_blocks: Option<bool>,
    /// Always shoots while moving.
    pub always_shoot_when_moving: Option<bool>,
    /// Hover tooltip.
    pub hoverable: Option<bool>,
    /// Always create the base outline.
    pub always_create_outline: Option<bool>,
    /// Generate the full icon.
    pub generate_full_icon: Option<bool>,
    /// Square shadow.
    pub square_shape: Option<bool>,
    /// Draws the build beam.
    pub draw_build_beam: Option<bool>,
    /// Draws the mining beam.
    pub draw_mine_beam: Option<bool>,
    /// Draws the team cell.
    pub draw_cell: Option<bool>,
    /// Draws carried items.
    pub draw_items: Option<bool>,
    /// Draws the unit shield bar.
    pub draw_shields: Option<bool>,
    /// Draws the body.
    pub draw_body: Option<bool>,
    /// Draws the soft shadow.
    pub draw_soft_shadow: Option<bool>,
    /// Draws on the minimap.
    pub draw_minimap: Option<bool>,
    /// AI controller tag.
    pub ai_controller: Option<AiControllerKind>,
    /// Controller tag.
    pub controller: Option<ControllerKind>,
    /// Default command (name; `None` = first command).
    pub default_command: Option<&'static str>,
    /// Explicit command list (empty = derived).
    pub commands: Vec<&'static str>,
    /// Explicit stance list (empty = derived).
    pub stances: Vec<&'static str>,
    /// Heal flash color.
    pub heal_color: Option<Rgba>,
    /// Emitted light color.
    pub light_color: Option<Rgba>,
    /// Shield color override.
    pub shield_color: Option<Rgba>,
    /// Death (explosion) sound.
    pub death_sound: Option<SoundId>,
    /// Death sound volume.
    pub death_sound_volume: Option<f32>,
    /// Wreck sound.
    pub wreck_sound: Option<SoundId>,
    /// Wreck sound volume.
    pub wreck_sound_volume: Option<f32>,
    /// Ambient loop sound.
    pub loop_sound: Option<SoundId>,
    /// Loop sound volume.
    pub loop_sound_volume: Option<f32>,
    /// Step sound.
    pub step_sound: Option<SoundId>,
    /// Step sound volume.
    pub step_sound_volume: Option<f32>,
    /// Step sound pitch.
    pub step_sound_pitch: Option<f32>,
    /// Step sound pitch range.
    pub step_sound_pitch_range: Option<f32>,
    /// Tank move sound.
    pub tank_move_sound: Option<SoundId>,
    /// Move sound.
    pub move_sound: Option<SoundId>,
    /// Move sound volume.
    pub move_sound_volume: Option<f32>,
    /// Move pitch min.
    pub move_sound_pitch_min: Option<f32>,
    /// Move pitch max.
    pub move_sound_pitch_max: Option<f32>,
    /// Tank move volume.
    pub tank_move_volume: Option<f32>,
    /// Fall effect.
    pub fall_effect: Option<EffectRef>,
    /// Fall engine effect.
    pub fall_engine_effect: Option<EffectRef>,
    /// Death explosion effect.
    pub death_explosion_effect: Option<EffectRef>,
    /// Engine color override.
    pub engine_color: Option<Rgba>,
    /// Engine inner color.
    pub engine_color_inner: Option<Rgba>,
    /// Engine/wave trail length.
    pub trail_length: Option<i32>,
    /// Engine trail color.
    pub trail_color: Option<Rgba>,
    /// Engine elevation flag.
    pub use_engine_elevation: Option<bool>,
    /// Target priority flags (`None` entries = closest-target fallback).
    pub target_flags: Vec<Option<BlockFlag>>,
    /// Command-changing UI flag.
    pub allow_change_commands: Option<bool>,
    /// Outline color.
    pub outline_color: Option<Rgba>,
    /// Outline radius.
    pub outline_radius: Option<i32>,
    /// Sprite outlines flag.
    pub outlines: Option<bool>,
    /// Item capacity (`<0` = derive from hit size).
    pub item_capacity: Option<i32>,
    /// Max mineable ore hardness (`<0` = disabled).
    pub mine_tier: Option<i32>,
    /// Mining speed.
    pub mine_speed: Option<f32>,
    /// Mines wall ores.
    pub mine_walls: Option<bool>,
    /// Mines floor ores.
    pub mine_floor: Option<bool>,
    /// Hardness slows mining.
    pub mine_hardness_scaling: Option<bool>,
    /// Mining loop sound.
    pub mine_sound: Option<SoundId>,
    /// Mining sound volume.
    pub mine_sound_volume: Option<f32>,
    /// Leg count.
    pub leg_count: Option<i32>,
    /// Leg group size.
    pub leg_group_size: Option<i32>,
    /// Total leg length.
    pub leg_length: Option<f32>,
    /// Leg move speed.
    pub leg_speed: Option<f32>,
    /// Leg forward scale.
    pub leg_forward_scl: Option<f32>,
    /// Leg base offset.
    pub leg_base_offset: Option<f32>,
    /// Leg move spacing.
    pub leg_move_space: Option<f32>,
    /// Leg extension.
    pub leg_extension: Option<f32>,
    /// Leg pair offset.
    pub leg_pair_offset: Option<f32>,
    /// Leg length scale.
    pub leg_length_scl: Option<f32>,
    /// Leg straight length.
    pub leg_straight_length: Option<f32>,
    /// Max leg length fraction.
    pub leg_max_length: Option<f32>,
    /// Min leg length fraction.
    pub leg_min_length: Option<f32>,
    /// Leg splash damage.
    pub leg_splash_damage: Option<f32>,
    /// Leg splash range.
    pub leg_splash_range: Option<f32>,
    /// Base leg straightness.
    pub base_leg_straightness: Option<f32>,
    /// Leg straightness.
    pub leg_straightness: Option<f32>,
    /// Leg base drawn under.
    pub leg_base_under: Option<bool>,
    /// Lock legs to base.
    pub lock_leg_base: Option<bool>,
    /// Legs always move.
    pub leg_continuous_move: Option<bool>,
    /// Flip back legs.
    pub flip_back_legs: Option<bool>,
    /// Flip leg side.
    pub flip_leg_side: Option<bool>,
    /// Water walk sounds.
    pub emit_walk_sound: Option<bool>,
    /// Water walk effects.
    pub emit_walk_effect: Option<bool>,
    /// Mech landing shake.
    pub mech_land_shake: Option<f32>,
    /// Mech side sway.
    pub mech_side_sway: Option<f32>,
    /// Mech front sway.
    pub mech_front_sway: Option<f32>,
    /// Mech stride (`<0` = derive).
    pub mech_stride: Option<f32>,
    /// Mech step particles (derived when `step_shake` unset).
    pub mech_step_particles: Option<bool>,
    /// Mech leg color.
    pub mech_leg_color: Option<Rgba>,
    /// Tread rects.
    pub tread_rects: Vec<TreadRect>,
    /// Tread frames.
    pub tread_frames: Option<i32>,
    /// Tread pull offset.
    pub tread_pull_offset: Option<i32>,
    /// Crushes fragile blocks.
    pub crush_fragile: Option<bool>,
    /// Segment count.
    pub segments: Option<i32>,
    /// Independent segment units.
    pub segment_units: Option<i32>,
    /// Segment layer order.
    pub segment_layer_order: Option<bool>,
    /// Segment sine magnitude.
    pub segment_mag: Option<f32>,
    /// Segment sine scale.
    pub segment_scl: Option<f32>,
    /// Segment sine phase.
    pub segment_phase: Option<f32>,
    /// Segment rotation speed.
    pub segment_rot_speed: Option<f32>,
    /// Max segment rotation difference.
    pub segment_max_rot: Option<f32>,
    /// Segment spacing (`<0` = derive).
    pub segment_spacing: Option<f32>,
    /// Segment rotation range.
    pub segment_rotation_range: Option<f32>,
    /// Crawl slowdown multiplier.
    pub crawl_slowdown: Option<f32>,
    /// Crush damage per tick.
    pub crush_damage: Option<f32>,
    /// Crawl slowdown fraction.
    pub crawl_slowdown_frac: Option<f32>,
    /// Missile lifetime.
    pub lifetime: Option<f32>,
    /// Missile homing delay.
    pub homing_delay: Option<f32>,
    /// Environment flags required.
    pub env_required: Option<EnvMask>,
    /// Environment flags enabled.
    pub env_enabled: Option<EnvMask>,
    /// Environment flags disabled.
    pub env_disabled: Option<EnvMask>,
    /// Immunities (status names; naval preset appends `wet` in `link`).
    pub immunities: Vec<&'static str>,
    /// Local bullet variables constructed before the weapons
    /// (`BulletType x = new ...`), in construction order.
    pub pre_bullets: Vec<BulletSpec>,
    /// Weapons (with inline/pre bullet references).
    pub weapons: Vec<WeaponSpec>,
    /// Abilities (preset abilities are already present from `for_kind`).
    pub abilities: Vec<AbilitySpec>,
    /// Draw parts.
    pub parts: Vec<DrawPartSpec>,
    /// Mirrored engines (`setEnginesMirror` — expansion runs in `from_spec`).
    pub engines_mirror: Vec<EngineSpec>,
}

impl UnitSpec {
    /// Class-preset spec for `kind`, transcribed from the upstream preset
    /// constructors (`type/unit/*.java`). Fields not listed keep the Java
    /// `UnitType` base defaults.
    pub fn for_kind(name: &'static str, kind: UnitKind, entity_def: EntityDefSpec) -> Self {
        let mut spec = Self {
            name,
            kind,
            entity_def,
            ..Self::default()
        };
        match kind {
            UnitKind::UnitType => {}
            UnitKind::ErekirUnitType => {
                // `ErekirUnitType` (`type/unit/ErekirUnitType.java:11-14`).
                spec.outline_color = Some(pal::DARK_OUTLINE);
                spec.env_disabled = Some(EnvMask::of(vec![EnvFlag::Space]));
                spec.research_cost_multiplier = Some(10.0);
            }
            UnitKind::TankUnitType => {
                // `TankUnitType` extends `ErekirUnitType` (`type/unit/TankUnitType.java:8-15`).
                spec.outline_color = Some(pal::DARK_OUTLINE);
                spec.research_cost_multiplier = Some(10.0);
                spec.square_shape = Some(true);
                spec.omni_movement = Some(false);
                spec.rotate_move_first = Some(true);
                spec.rotate_speed = Some(1.3);
                spec.env_disabled = Some(EnvMask::none());
                spec.speed = Some(0.8);
            }
            UnitKind::MissileUnitType => {
                // `MissileUnitType` (`type/unit/MissileUnitType.java:15-43`).
                spec.player_controllable = Some(false);
                spec.create_wreck = Some(false);
                spec.create_scorch = Some(false);
                spec.logic_controllable = Some(false);
                spec.is_enemy = Some(false);
                spec.use_unit_cap = Some(false);
                spec.draw_cell = Some(false);
                spec.allowed_in_payloads = Some(false);
                spec.controller = Some(ControllerKind::Missile);
                spec.flying = Some(true);
                spec.env_enabled = Some(EnvMask::any());
                spec.env_disabled = Some(EnvMask::none());
                spec.physics = Some(false);
                spec.bounded = Some(false);
                spec.trail_length = Some(7);
                spec.hidden = Some(true);
                spec.hoverable = Some(false);
                spec.speed = Some(4.0);
                spec.lifetime = Some(60.0 * 1.7);
                spec.rotate_speed = Some(2.5);
                spec.range = Some(6.0);
                spec.target_priority = Some(-1.0);
                spec.outline_color = Some(pal::DARK_OUTLINE);
                spec.fog_radius = Some(2.0);
                spec.loop_sound = Some(SoundId::LOOP_MISSILE_TRAIL);
                spec.loop_sound_volume = Some(0.05);
                spec.draw_minimap = Some(false);
            }
            UnitKind::NeoplasmUnitType => {
                // `NeoplasmUnitType` (`type/unit/NeoplasmUnitType.java:14-41`).
                spec.outline_color = Some(pal::NEOPLASM_OUTLINE);
                spec.immunities = vec!["burning", "melting"];
                spec.env_disabled = Some(EnvMask::none());
                spec.draw_cell = Some(false);
                spec.abilities = vec![
                    AbilitySpec::regen(1.0 / (70.0 * 60.0) * 100.0),
                    AbilitySpec::liquid_explode("neoplasm"),
                    AbilitySpec::liquid_regen("neoplasm", EffectId::NEOPLASM_HEAL),
                ];
                spec.heal_flash = Some(true);
                spec.heal_color = Some(pal::NEOPLASM1);
            }
        }
        spec
    }
}

/// Builds a spec with the class defaults for `kind` (`UnitSpec::for_kind`).
pub fn spec(name: &'static str, kind: UnitKind, entity_def: EntityDefSpec) -> UnitSpec {
    UnitSpec::for_kind(name, kind, entity_def)
}

/// Unit metadata record (plan 02 §6.1 `UnitTypeDef`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitTypeDef {
    /// Dense id in the unit content space.
    pub id: UnitTypeId,
    /// Content name (parity ABI; already mod-prefixed).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Java class tag.
    pub kind: UnitKind,
    /// `@EntityDef` component group.
    pub entity_def: EntityDefSpec,
    /// Movement speed.
    pub speed: f32,
    /// Boost multiplier.
    pub boost_multiplier: f32,
    /// Floor multiplier.
    pub floor_multiplier: f32,
    /// Rotation speed.
    pub rotate_speed: f32,
    /// Mech base rotation speed.
    pub base_rotate_speed: f32,
    /// Drag.
    pub drag: f32,
    /// Acceleration.
    pub accel: f32,
    /// Hitbox side.
    pub hit_size: f32,
    /// Death shake.
    pub death_shake: f32,
    /// Step shake (derived when `<0`).
    pub step_shake: f32,
    /// Ripple scale.
    pub ripple_scale: f32,
    /// Rise speed.
    pub rise_speed: f32,
    /// Descent speed.
    pub descent_speed: f32,
    /// Fall speed.
    pub fall_speed: f32,
    /// Missile acceleration time.
    pub missile_accel_time: f32,
    /// Health.
    pub health: f32,
    /// Armor.
    pub armor: f32,
    /// Approach range (derived).
    pub range: f32,
    /// Max range (derived).
    pub max_range: f32,
    /// Mining range.
    pub mine_range: f32,
    /// Build range.
    pub build_range: f32,
    /// Circle-target radius.
    pub circle_target_radius: f32,
    /// Crash damage multiplier.
    pub crash_damage_multiplier: f32,
    /// Wreck health multiplier.
    pub wreck_health_multiplier: f32,
    /// DPS estimate (derived by the post-sweep [`link`] pass).
    pub dps_estimate: f32,
    /// Clipping size (derived).
    pub clip_size: f32,
    /// Drown time multiplier.
    pub drown_time_multiplier: f32,
    /// Strafe penalty.
    pub strafe_penalty: f32,
    /// Research cost multiplier.
    pub research_cost_multiplier: f32,
    /// Knockback multiplier.
    pub knockback_multiplier: f32,
    /// Ground layer.
    pub ground_layer: f32,
    /// Flying layer (derived when `<0`).
    pub flying_layer: f32,
    /// Payload capacity.
    pub payload_capacity: f32,
    /// Build speed.
    pub build_speed: f32,
    /// Minimum aim distance (derived when `<0`).
    pub aim_dst: f32,
    /// Build beam offset.
    pub build_beam_offset: f32,
    /// Mine beam offset (derived).
    pub mine_beam_offset: f32,
    /// Target priority.
    pub target_priority: f32,
    /// Shadow elevation.
    pub shadow_elevation: f32,
    /// Shadow elevation scale.
    pub shadow_elevation_scl: f32,
    /// Engine offset.
    pub engine_offset: f32,
    /// Engine size.
    pub engine_size: f32,
    /// Engine layer.
    pub engine_layer: f32,
    /// Item draw offset.
    pub item_offset_y: f32,
    /// Light radius (derived when `<0`).
    pub light_radius: f32,
    /// Light opacity.
    pub light_opacity: f32,
    /// Soft shadow scale.
    pub soft_shadow_scl: f32,
    /// Fog radius in tiles (derived when `<0`).
    pub fog_radius: f32,
    /// Wave trail X.
    pub wave_trail_x: f32,
    /// Wave trail Y.
    pub wave_trail_y: f32,
    /// Trail scale.
    pub trail_scl: f32,
    /// Wave-counter enemy flag.
    pub is_enemy: bool,
    /// Flying flag.
    pub flying: bool,
    /// Wobble flag.
    pub wobble: bool,
    /// Targets air.
    pub target_air: bool,
    /// Targets ground.
    pub target_ground: bool,
    /// Faces target.
    pub face_target: bool,
    /// Circles target.
    pub circle_target: bool,
    /// Drops bombs early.
    pub auto_drop_bombs: bool,
    /// Mobile building targeting.
    pub target_buildings_mobile: bool,
    /// Can boost.
    pub can_boost: bool,
    /// Boosts while building.
    pub boost_when_building: bool,
    /// Boosts while mining.
    pub boost_when_mining: bool,
    /// Logic-controllable.
    pub logic_controllable: bool,
    /// Player-controllable.
    pub player_controllable: bool,
    /// Global-select flag.
    pub control_select_global: bool,
    /// Allowed in payloads.
    pub allowed_in_payloads: bool,
    /// Hittable.
    pub hittable: bool,
    /// Killable.
    pub killable: bool,
    /// Targetable.
    pub targetable: bool,
    /// Vulnerable with payloads.
    pub vulnerable_with_payloads: bool,
    /// Picks up units.
    pub pickup_units: bool,
    /// Physics collisions.
    pub physics: bool,
    /// Can drown.
    pub can_drown: bool,
    /// Uses the unit cap.
    pub use_unit_cap: bool,
    /// Core-unit dock.
    pub core_unit_dock: bool,
    /// Creates a wreck.
    pub create_wreck: bool,
    /// Creates scorch.
    pub create_scorch: bool,
    /// Low altitude.
    pub low_altitude: bool,
    /// Rotates to buildings.
    pub rotate_to_building: bool,
    /// Walks over blocks (derived).
    pub allow_leg_step: bool,
    /// Leg physics layer.
    pub leg_physics_layer: bool,
    /// Hovering.
    pub hovering: bool,
    /// Omni movement.
    pub omni_movement: bool,
    /// Rotates before moving.
    pub rotate_move_first: bool,
    /// Heal flash.
    pub heal_flash: bool,
    /// Can heal blocks (derived).
    pub can_heal: bool,
    /// Single target.
    pub single_target: bool,
    /// Force multi-target.
    pub force_multi_target: bool,
    /// Can attack (derived).
    pub can_attack: bool,
    /// Hidden.
    pub hidden: bool,
    /// Internal.
    pub internal: bool,
    /// Internal sprite generation.
    pub internal_generate_sprites: bool,
    /// Bounded by map edges.
    pub bounded: bool,
    /// Naval (derived).
    pub naval: bool,
    /// RTS auto-attack.
    pub auto_find_target: bool,
    /// Targets under blocks.
    pub target_under_blocks: bool,
    /// Always shoots when moving.
    pub always_shoot_when_moving: bool,
    /// Hoverable.
    pub hoverable: bool,
    /// Always create outline.
    pub always_create_outline: bool,
    /// Generate full icon.
    pub generate_full_icon: bool,
    /// Square shadow.
    pub square_shape: bool,
    /// Draw build beam.
    pub draw_build_beam: bool,
    /// Draw mine beam.
    pub draw_mine_beam: bool,
    /// Draw cell.
    pub draw_cell: bool,
    /// Draw items.
    pub draw_items: bool,
    /// Draw shields.
    pub draw_shields: bool,
    /// Draw body.
    pub draw_body: bool,
    /// Draw soft shadow.
    pub draw_soft_shadow: bool,
    /// Draw minimap.
    pub draw_minimap: bool,
    /// AI controller tag.
    pub ai_controller: AiControllerKind,
    /// Controller tag.
    pub controller: ControllerKind,
    /// Default command (derived when unset).
    pub default_command: Option<UnitCommandId>,
    /// Available commands (derived when the spec list is empty).
    pub commands: Vec<UnitCommandId>,
    /// Available stances (derived when the spec list is empty).
    pub stances: Vec<UnitStanceId>,
    /// Heal flash color.
    pub heal_color: Rgba,
    /// Light color.
    pub light_color: Rgba,
    /// Shield color.
    pub shield_color: Option<Rgba>,
    /// Death sound (derived when `unset`).
    pub death_sound: SoundId,
    /// Death sound volume.
    pub death_sound_volume: f32,
    /// Wreck sound (derived when `unset`).
    pub wreck_sound: SoundId,
    /// Wreck sound volume.
    pub wreck_sound_volume: f32,
    /// Loop sound.
    pub loop_sound: SoundId,
    /// Loop sound volume.
    pub loop_sound_volume: f32,
    /// Step sound.
    pub step_sound: SoundId,
    /// Step volume.
    pub step_sound_volume: f32,
    /// Step pitch.
    pub step_sound_pitch: f32,
    /// Step pitch range.
    pub step_sound_pitch_range: f32,
    /// Tank move sound.
    pub tank_move_sound: SoundId,
    /// Move sound.
    pub move_sound: SoundId,
    /// Move volume.
    pub move_sound_volume: f32,
    /// Move pitch min.
    pub move_sound_pitch_min: f32,
    /// Move pitch max.
    pub move_sound_pitch_max: f32,
    /// Tank move volume.
    pub tank_move_volume: f32,
    /// Fall effect.
    pub fall_effect: EffectRef,
    /// Fall engine effect.
    pub fall_engine_effect: EffectRef,
    /// Death explosion effect.
    pub death_explosion_effect: EffectRef,
    /// Tread effect (`None` = created in `init`; body is plan 17).
    pub tread_effect: Option<EffectRef>,
    /// Engine color.
    pub engine_color: Option<Rgba>,
    /// Engine inner color.
    pub engine_color_inner: Rgba,
    /// Trail length.
    pub trail_length: i32,
    /// Trail color.
    pub trail_color: Option<Rgba>,
    /// Engine elevation flag.
    pub use_engine_elevation: bool,
    /// Target flags.
    pub target_flags: Vec<Option<BlockFlag>>,
    /// Allow command changes.
    pub allow_change_commands: bool,
    /// Outline color.
    pub outline_color: Rgba,
    /// Outline radius.
    pub outline_radius: i32,
    /// Outlines flag.
    pub outlines: bool,
    /// Item capacity (derived when `<0`).
    pub item_capacity: i32,
    /// Mine tier.
    pub mine_tier: i32,
    /// Mine speed.
    pub mine_speed: f32,
    /// Mines walls.
    pub mine_walls: bool,
    /// Mines floors.
    pub mine_floor: bool,
    /// Hardness scaling.
    pub mine_hardness_scaling: bool,
    /// Mine sound.
    pub mine_sound: SoundId,
    /// Mine sound volume.
    pub mine_sound_volume: f32,
    /// Leg count.
    pub leg_count: i32,
    /// Leg group size.
    pub leg_group_size: i32,
    /// Leg length.
    pub leg_length: f32,
    /// Leg speed.
    pub leg_speed: f32,
    /// Leg forward scale.
    pub leg_forward_scl: f32,
    /// Leg base offset.
    pub leg_base_offset: f32,
    /// Leg move space.
    pub leg_move_space: f32,
    /// Leg extension.
    pub leg_extension: f32,
    /// Leg pair offset.
    pub leg_pair_offset: f32,
    /// Leg length scale.
    pub leg_length_scl: f32,
    /// Leg straight length.
    pub leg_straight_length: f32,
    /// Leg max length.
    pub leg_max_length: f32,
    /// Leg min length.
    pub leg_min_length: f32,
    /// Leg splash damage.
    pub leg_splash_damage: f32,
    /// Leg splash range.
    pub leg_splash_range: f32,
    /// Base leg straightness.
    pub base_leg_straightness: f32,
    /// Leg straightness.
    pub leg_straightness: f32,
    /// Leg base under.
    pub leg_base_under: bool,
    /// Lock leg base.
    pub lock_leg_base: bool,
    /// Legs continuous move.
    pub leg_continuous_move: bool,
    /// Flip back legs.
    pub flip_back_legs: bool,
    /// Flip leg side.
    pub flip_leg_side: bool,
    /// Walk sounds.
    pub emit_walk_sound: bool,
    /// Walk effects.
    pub emit_walk_effect: bool,
    /// Mech land shake.
    pub mech_land_shake: f32,
    /// Mech side sway.
    pub mech_side_sway: f32,
    /// Mech front sway.
    pub mech_front_sway: f32,
    /// Mech stride (derived when `<0`).
    pub mech_stride: f32,
    /// Mech step particles (derived with `step_shake`).
    pub mech_step_particles: bool,
    /// Mech leg color.
    pub mech_leg_color: Rgba,
    /// Tread rects.
    pub tread_rects: Vec<TreadRect>,
    /// Tread frames.
    pub tread_frames: i32,
    /// Tread pull offset.
    pub tread_pull_offset: i32,
    /// Crush fragile.
    pub crush_fragile: bool,
    /// Segments.
    pub segments: i32,
    /// Segment units.
    pub segment_units: i32,
    /// Segment layer order.
    pub segment_layer_order: bool,
    /// Segment magnitude.
    pub segment_mag: f32,
    /// Segment scale.
    pub segment_scl: f32,
    /// Segment phase.
    pub segment_phase: f32,
    /// Segment rotation speed.
    pub segment_rot_speed: f32,
    /// Segment max rotation.
    pub segment_max_rot: f32,
    /// Segment spacing (derived when `<0`).
    pub segment_spacing: f32,
    /// Segment rotation range.
    pub segment_rotation_range: f32,
    /// Crawl slowdown.
    pub crawl_slowdown: f32,
    /// Crush damage.
    pub crush_damage: f32,
    /// Crawl slowdown fraction.
    pub crawl_slowdown_frac: f32,
    /// Missile lifetime.
    pub lifetime: f32,
    /// Missile homing delay.
    pub homing_delay: f32,
    /// Required environment.
    pub env_required: EnvMask,
    /// Enabled environment.
    pub env_enabled: EnvMask,
    /// Disabled environment.
    pub env_disabled: EnvMask,
    /// Immunities (naval preset appends `wet` in `link`).
    pub immunities: Vec<StatusId>,
    /// Weapons (mirror-expanded in `init`).
    pub weapons: Vec<WeaponDef>,
    /// Abilities.
    pub abilities: Vec<AbilitySpec>,
    /// Draw parts.
    pub parts: Vec<DrawPartSpec>,
    /// Engines (mirror-expanded in `from_spec`; default engine added in `init`).
    pub engines: Vec<EngineSpec>,
    /// Flowfield path type (`-1` = derived; plan 11 consumes).
    pub flowfield_path_type: i32,
    /// Path cost id (derived; plan 11 consumes).
    pub path_cost_id: i32,
}

/// Resolves a status name list to ids (generated-wave helper).
fn resolve_statuses(
    registry: &ContentRegistry,
    names: &[&'static str],
) -> Result<Vec<StatusId>, ContentError> {
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        out.push(
            registry
                .status_id(name)
                .ok_or_else(|| ContentError::UnknownName((*name).to_owned()))?,
        );
    }
    Ok(out)
}

impl UnitTypeDef {
    /// Resolves the scalar fields of a generated [`UnitSpec`] (weapons are
    /// attached by the load pass after their bullets register).
    pub fn from_spec(
        spec: &UnitSpec,
        registry: &ContentRegistry,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Result<Self, ContentError> {
        let immunities = resolve_statuses(registry, &spec.immunities)?;
        let mut engines = Vec::with_capacity(spec.engines_mirror.len() * 2);
        for engine in &spec.engines_mirror {
            engines.push(*engine);
            engines.push(engine.mirrored());
        }
        let target_flags = if spec.target_flags.is_empty() {
            vec![None]
        } else {
            spec.target_flags.clone()
        };
        let default_command = spec
            .default_command
            .map(|name| {
                registry
                    .unit_commands
                    .iter()
                    .find(|command| command.name == name)
                    .map(|command| command.id)
                    .ok_or_else(|| ContentError::UnknownName(name.to_owned()))
            })
            .transpose()?;
        let mut def = Self {
            id: UnitTypeId::new(0),
            name: spec.name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Unit, spec.name, bundle, store),
            kind: spec.kind,
            entity_def: spec.entity_def,
            speed: spec.speed.unwrap_or(1.1),
            boost_multiplier: spec.boost_multiplier.unwrap_or(1.0),
            floor_multiplier: spec.floor_multiplier.unwrap_or(1.0),
            rotate_speed: spec.rotate_speed.unwrap_or(5.0),
            base_rotate_speed: spec.base_rotate_speed.unwrap_or(5.0),
            drag: spec.drag.unwrap_or(0.3),
            accel: spec.accel.unwrap_or(0.5),
            hit_size: spec.hit_size.unwrap_or(6.0),
            death_shake: spec.death_shake.unwrap_or(-1.0),
            step_shake: spec.step_shake.unwrap_or(-1.0),
            ripple_scale: spec.ripple_scale.unwrap_or(1.0),
            rise_speed: spec.rise_speed.unwrap_or(0.08),
            descent_speed: spec.descent_speed.unwrap_or(0.08),
            fall_speed: spec.fall_speed.unwrap_or(0.018),
            missile_accel_time: spec.missile_accel_time.unwrap_or(0.0),
            health: spec.health.unwrap_or(200.0),
            armor: spec.armor.unwrap_or(0.0),
            range: spec.range.unwrap_or(-1.0),
            max_range: spec.max_range.unwrap_or(-1.0),
            mine_range: spec.mine_range.unwrap_or(70.0),
            build_range: spec.build_range.unwrap_or(BUILDING_RANGE),
            circle_target_radius: spec.circle_target_radius.unwrap_or(80.0),
            crash_damage_multiplier: spec.crash_damage_multiplier.unwrap_or(1.0),
            wreck_health_multiplier: spec.wreck_health_multiplier.unwrap_or(0.25),
            dps_estimate: -1.0,
            clip_size: spec.clip_size.unwrap_or(-1.0),
            drown_time_multiplier: spec.drown_time_multiplier.unwrap_or(1.0),
            strafe_penalty: spec.strafe_penalty.unwrap_or(0.5),
            research_cost_multiplier: spec.research_cost_multiplier.unwrap_or(50.0),
            knockback_multiplier: spec.knockback_multiplier.unwrap_or(1.0),
            ground_layer: spec.ground_layer.unwrap_or(LAYER_GROUND_UNIT),
            flying_layer: spec.flying_layer.unwrap_or(-1.0),
            payload_capacity: spec.payload_capacity.unwrap_or(8.0),
            build_speed: spec.build_speed.unwrap_or(-1.0),
            aim_dst: spec.aim_dst.unwrap_or(-1.0),
            build_beam_offset: spec.build_beam_offset.unwrap_or(3.8),
            mine_beam_offset: f32::NEG_INFINITY,
            target_priority: spec.target_priority.unwrap_or(0.0),
            shadow_elevation: spec.shadow_elevation.unwrap_or(-1.0),
            shadow_elevation_scl: spec.shadow_elevation_scl.unwrap_or(1.0),
            engine_offset: spec.engine_offset.unwrap_or(5.0),
            engine_size: spec.engine_size.unwrap_or(2.5),
            engine_layer: spec.engine_layer.unwrap_or(-1.0),
            item_offset_y: spec.item_offset_y.unwrap_or(3.0),
            light_radius: spec.light_radius.unwrap_or(-1.0),
            light_opacity: spec.light_opacity.unwrap_or(0.6),
            soft_shadow_scl: spec.soft_shadow_scl.unwrap_or(1.0),
            fog_radius: spec.fog_radius.unwrap_or(-1.0),
            wave_trail_x: spec.wave_trail_x.unwrap_or(4.0),
            wave_trail_y: spec.wave_trail_y.unwrap_or(-3.0),
            trail_scl: spec.trail_scl.unwrap_or(1.0),
            is_enemy: spec.is_enemy.unwrap_or(true),
            flying: spec.flying.unwrap_or(false),
            wobble: spec.wobble.unwrap_or(true),
            target_air: spec.target_air.unwrap_or(true),
            target_ground: spec.target_ground.unwrap_or(true),
            face_target: spec.face_target.unwrap_or(true),
            circle_target: spec.circle_target.unwrap_or(false),
            auto_drop_bombs: spec.auto_drop_bombs.unwrap_or(false),
            target_buildings_mobile: spec.target_buildings_mobile.unwrap_or(true),
            can_boost: spec.can_boost.unwrap_or(false),
            boost_when_building: spec.boost_when_building.unwrap_or(true),
            boost_when_mining: spec.boost_when_mining.unwrap_or(true),
            logic_controllable: spec.logic_controllable.unwrap_or(true),
            player_controllable: spec.player_controllable.unwrap_or(true),
            control_select_global: spec.control_select_global.unwrap_or(true),
            allowed_in_payloads: spec.allowed_in_payloads.unwrap_or(true),
            hittable: spec.hittable.unwrap_or(true),
            killable: spec.killable.unwrap_or(true),
            targetable: spec.targetable.unwrap_or(true),
            vulnerable_with_payloads: spec.vulnerable_with_payloads.unwrap_or(false),
            pickup_units: spec.pickup_units.unwrap_or(true),
            physics: spec.physics.unwrap_or(true),
            can_drown: spec.can_drown.unwrap_or(true),
            use_unit_cap: spec.use_unit_cap.unwrap_or(true),
            core_unit_dock: spec.core_unit_dock.unwrap_or(false),
            create_wreck: spec.create_wreck.unwrap_or(true),
            create_scorch: spec.create_scorch.unwrap_or(true),
            low_altitude: spec.low_altitude.unwrap_or(false),
            rotate_to_building: spec.rotate_to_building.unwrap_or(true),
            allow_leg_step: spec.allow_leg_step.unwrap_or(false),
            leg_physics_layer: spec.leg_physics_layer.unwrap_or(true),
            hovering: spec.hovering.unwrap_or(false),
            omni_movement: spec.omni_movement.unwrap_or(true),
            rotate_move_first: spec.rotate_move_first.unwrap_or(false),
            heal_flash: spec.heal_flash.unwrap_or(true),
            can_heal: false,
            single_target: spec.single_target.unwrap_or(false),
            force_multi_target: spec.force_multi_target.unwrap_or(false),
            can_attack: true,
            hidden: spec.hidden.unwrap_or(false),
            internal: spec.internal.unwrap_or(false),
            internal_generate_sprites: spec.internal_generate_sprites.unwrap_or(false),
            bounded: spec.bounded.unwrap_or(true),
            naval: false,
            auto_find_target: spec.auto_find_target.unwrap_or(true),
            target_under_blocks: spec.target_under_blocks.unwrap_or(true),
            always_shoot_when_moving: spec.always_shoot_when_moving.unwrap_or(false),
            hoverable: spec.hoverable.unwrap_or(true),
            always_create_outline: spec.always_create_outline.unwrap_or(false),
            generate_full_icon: spec.generate_full_icon.unwrap_or(true),
            square_shape: spec.square_shape.unwrap_or(false),
            draw_build_beam: spec.draw_build_beam.unwrap_or(true),
            draw_mine_beam: spec.draw_mine_beam.unwrap_or(true),
            draw_cell: spec.draw_cell.unwrap_or(true),
            draw_items: spec.draw_items.unwrap_or(true),
            draw_shields: spec.draw_shields.unwrap_or(true),
            draw_body: spec.draw_body.unwrap_or(true),
            draw_soft_shadow: spec.draw_soft_shadow.unwrap_or(true),
            draw_minimap: spec.draw_minimap.unwrap_or(true),
            ai_controller: spec.ai_controller.unwrap_or_default(),
            controller: spec.controller.unwrap_or_default(),
            default_command,
            commands: Vec::new(),
            stances: Vec::new(),
            heal_color: spec.heal_color.unwrap_or(pal::HEAL),
            light_color: spec.light_color.unwrap_or(pal::POWER_LIGHT),
            shield_color: spec.shield_color,
            death_sound: spec.death_sound.unwrap_or(SoundId::UNSET),
            death_sound_volume: spec.death_sound_volume.unwrap_or(1.0),
            wreck_sound: spec.wreck_sound.unwrap_or(SoundId::UNSET),
            wreck_sound_volume: spec.wreck_sound_volume.unwrap_or(1.0),
            loop_sound: spec.loop_sound.unwrap_or(SoundId::NONE),
            loop_sound_volume: spec.loop_sound_volume.unwrap_or(0.5),
            step_sound: spec.step_sound.unwrap_or(SoundId::MECH_STEP_SMALL),
            step_sound_volume: spec.step_sound_volume.unwrap_or(0.5),
            step_sound_pitch: spec.step_sound_pitch.unwrap_or(1.0),
            step_sound_pitch_range: spec.step_sound_pitch_range.unwrap_or(0.1),
            tank_move_sound: spec.tank_move_sound.unwrap_or(SoundId::TANK_MOVE),
            move_sound: spec.move_sound.unwrap_or(SoundId::NONE),
            move_sound_volume: spec.move_sound_volume.unwrap_or(1.0),
            move_sound_pitch_min: spec.move_sound_pitch_min.unwrap_or(1.0),
            move_sound_pitch_max: spec.move_sound_pitch_max.unwrap_or(1.0),
            tank_move_volume: spec.tank_move_volume.unwrap_or(0.5),
            fall_effect: spec
                .fall_effect
                .clone()
                .unwrap_or(EffectRef::Named(EffectId::FALL_SMOKE)),
            fall_engine_effect: spec
                .fall_engine_effect
                .clone()
                .unwrap_or(EffectRef::Named(EffectId::FALL_SMOKE)),
            death_explosion_effect: spec
                .death_explosion_effect
                .clone()
                .unwrap_or(EffectRef::Named(EffectId::DYNAMIC_EXPLOSION)),
            tread_effect: None,
            engine_color: spec.engine_color,
            engine_color_inner: spec.engine_color_inner.unwrap_or(Rgba::WHITE),
            trail_length: spec.trail_length.unwrap_or(0),
            trail_color: spec.trail_color,
            use_engine_elevation: spec.use_engine_elevation.unwrap_or(true),
            target_flags,
            allow_change_commands: spec.allow_change_commands.unwrap_or(true),
            outline_color: spec.outline_color.unwrap_or(pal::DARKER_METAL),
            outline_radius: spec.outline_radius.unwrap_or(3),
            outlines: spec.outlines.unwrap_or(true),
            item_capacity: spec.item_capacity.unwrap_or(-1),
            mine_tier: spec.mine_tier.unwrap_or(-1),
            mine_speed: spec.mine_speed.unwrap_or(1.0),
            mine_walls: spec.mine_walls.unwrap_or(false),
            mine_floor: spec.mine_floor.unwrap_or(true),
            mine_hardness_scaling: spec.mine_hardness_scaling.unwrap_or(true),
            mine_sound: spec.mine_sound.unwrap_or(SoundId::LOOP_MINE_BEAM),
            mine_sound_volume: spec.mine_sound_volume.unwrap_or(0.6),
            leg_count: spec.leg_count.unwrap_or(4),
            leg_group_size: spec.leg_group_size.unwrap_or(2),
            leg_length: spec.leg_length.unwrap_or(10.0),
            leg_speed: spec.leg_speed.unwrap_or(0.1),
            leg_forward_scl: spec.leg_forward_scl.unwrap_or(1.0),
            leg_base_offset: spec.leg_base_offset.unwrap_or(0.0),
            leg_move_space: spec.leg_move_space.unwrap_or(1.0),
            leg_extension: spec.leg_extension.unwrap_or(0.0),
            leg_pair_offset: spec.leg_pair_offset.unwrap_or(0.0),
            leg_length_scl: spec.leg_length_scl.unwrap_or(1.0),
            leg_straight_length: spec.leg_straight_length.unwrap_or(1.0),
            leg_max_length: spec.leg_max_length.unwrap_or(1.75),
            leg_min_length: spec.leg_min_length.unwrap_or(0.0),
            leg_splash_damage: spec.leg_splash_damage.unwrap_or(0.0),
            leg_splash_range: spec.leg_splash_range.unwrap_or(5.0),
            base_leg_straightness: spec.base_leg_straightness.unwrap_or(0.0),
            leg_straightness: spec.leg_straightness.unwrap_or(0.0),
            leg_base_under: spec.leg_base_under.unwrap_or(false),
            lock_leg_base: spec.lock_leg_base.unwrap_or(false),
            leg_continuous_move: spec.leg_continuous_move.unwrap_or(false),
            flip_back_legs: spec.flip_back_legs.unwrap_or(true),
            flip_leg_side: spec.flip_leg_side.unwrap_or(false),
            emit_walk_sound: spec.emit_walk_sound.unwrap_or(true),
            emit_walk_effect: spec.emit_walk_effect.unwrap_or(true),
            mech_land_shake: spec.mech_land_shake.unwrap_or(0.0),
            mech_side_sway: spec.mech_side_sway.unwrap_or(0.54),
            mech_front_sway: spec.mech_front_sway.unwrap_or(0.1),
            mech_stride: spec.mech_stride.unwrap_or(-1.0),
            mech_step_particles: spec.mech_step_particles.unwrap_or(false),
            mech_leg_color: spec.mech_leg_color.unwrap_or(pal::DARK_METAL),
            tread_rects: spec.tread_rects.clone(),
            tread_frames: spec.tread_frames.unwrap_or(18),
            tread_pull_offset: spec.tread_pull_offset.unwrap_or(0),
            crush_fragile: spec.crush_fragile.unwrap_or(false),
            segments: spec.segments.unwrap_or(0),
            segment_units: spec.segment_units.unwrap_or(1),
            segment_layer_order: spec.segment_layer_order.unwrap_or(true),
            segment_mag: spec.segment_mag.unwrap_or(2.0),
            segment_scl: spec.segment_scl.unwrap_or(4.0),
            segment_phase: spec.segment_phase.unwrap_or(5.0),
            segment_rot_speed: spec.segment_rot_speed.unwrap_or(1.0),
            segment_max_rot: spec.segment_max_rot.unwrap_or(30.0),
            segment_spacing: spec.segment_spacing.unwrap_or(-1.0),
            segment_rotation_range: spec.segment_rotation_range.unwrap_or(80.0),
            crawl_slowdown: spec.crawl_slowdown.unwrap_or(0.5),
            crush_damage: spec.crush_damage.unwrap_or(0.0),
            crawl_slowdown_frac: spec.crawl_slowdown_frac.unwrap_or(0.55),
            lifetime: spec.lifetime.unwrap_or(60.0 * 5.0),
            homing_delay: spec.homing_delay.unwrap_or(10.0),
            env_required: spec.env_required.clone().unwrap_or_default(),
            env_enabled: spec
                .env_enabled
                .clone()
                .unwrap_or_else(|| EnvMask::of(vec![EnvFlag::Terrestrial])),
            env_disabled: spec
                .env_disabled
                .clone()
                .unwrap_or_else(|| EnvMask::of(vec![EnvFlag::Scorching])),
            immunities,
            weapons: Vec::new(),
            abilities: spec.abilities.clone(),
            parts: spec.parts.clone(),
            engines,
            flowfield_path_type: -1,
            path_cost_id: 0,
        };
        if let Some(always_unlocked) = spec.always_unlocked {
            def.unlock.always_unlocked = always_unlocked;
        }
        if let Some(hide_details) = spec.hide_details {
            def.unlock.hide_details = hide_details;
        }
        // Explicit command/stance lists resolve at load (names).
        if !spec.commands.is_empty() {
            def.commands = spec
                .commands
                .iter()
                .map(|name| {
                    registry
                        .unit_commands
                        .iter()
                        .find(|command| command.name == *name)
                        .map(|command| command.id)
                        .ok_or_else(|| ContentError::UnknownName((*name).to_owned()))
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        if !spec.stances.is_empty() {
            def.stances = spec
                .stances
                .iter()
                .map(|name| {
                    registry
                        .unit_stances
                        .iter()
                        .find(|stance| stance.name == *name)
                        .map(|stance| stance.id)
                        .ok_or_else(|| ContentError::UnknownName((*name).to_owned()))
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(def)
    }

    /// `UnitType.isHidden()`.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// `UnitType.hasWeapons()`.
    pub fn has_weapons(&self) -> bool {
        !self.weapons.is_empty()
    }

    /// Expected sprite-region names for the region audit (`UnitType.load()`
    /// chain; plan 03 checks existence).
    pub fn region_expectations(&self) -> Vec<String> {
        let mut out = vec![
            self.name.clone(),
            format!("{}-full", self.name),
            format!("{}-cell", self.name),
            format!("{}-outline", self.name),
            format!("{}-wreck0", self.name),
            format!("{}-wreck1", self.name),
            format!("{}-wreck2", self.name),
        ];
        for index in 0..self.segments {
            out.push(format!("{}-segment{}", self.name, index));
        }
        out
    }

    /// `UnitType.researchRequirements()` — derived from the block that builds
    /// this unit (`UnitType.java:1405-1427`): reconstructor consume-items,
    /// factory plan requirements or assembler plan payload costs, scaled by
    /// `researchCostMultiplier` and rounded via `UI.roundAmount`.
    pub fn research_requirements(&self, registry: &ContentRegistry) -> Vec<ItemStack> {
        let base = first_requirements(self, registry);
        match base {
            Some(stacks) => {
                let mut out = Vec::with_capacity(stacks.len());
                for stack in stacks {
                    // `(int)(amount * multiplier)` truncation, then roundAmount.
                    let amount = ui_round_amount(
                        (stack.amount as f32 * self.research_cost_multiplier) as i32,
                    );
                    if amount > 0 {
                        out.push(ItemStack::new(stack.item, amount));
                    }
                }
                out
            }
            None => Vec::new(),
        }
    }

    /// `UnitType.getDependencies()` — reconstructors producing this unit plus
    /// the research-requirement items (`UnitType.java:766-782`).
    pub fn get_dependencies(&self, registry: &ContentRegistry) -> (Vec<BlockId>, Vec<ItemId>) {
        let mut blocks = Vec::new();
        for block in registry.blocks() {
            if block
                .reconstructor_upgrades
                .iter()
                .any(|(_, to)| *to == self.id)
            {
                blocks.push(block.id);
            }
        }
        let items = self
            .research_requirements(registry)
            .iter()
            .map(|stack| stack.item)
            .collect();
        (blocks, items)
    }
}

/// `UnitType.getRequirements()` — the producing block's item requirements
/// (`UnitType.java:1347-1395`): reconstructor first, then factory, then
/// assembler (payload requirements expanded to item costs).
pub(crate) fn first_requirements(
    unit: &UnitTypeDef,
    registry: &ContentRegistry,
) -> Option<Vec<ItemStack>> {
    for block in registry.blocks() {
        if block
            .reconstructor_upgrades
            .iter()
            .any(|(_, to)| *to == unit.id)
        {
            for consume in &block.consumes {
                if let Consume::Items(stacks) = &consume.consume {
                    return Some(stacks.clone());
                }
            }
            return None;
        }
    }
    for block in registry.blocks() {
        if let Some(plan) = block.unit_plans.iter().find(|plan| plan.unit == unit.id) {
            return Some(plan.requirements.clone());
        }
    }
    for block in registry.blocks() {
        if let Some(plan) = block
            .assembler_plans
            .iter()
            .find(|plan| plan.unit == unit.id)
        {
            let mut total = ItemSeq::with_len(registry.items().len());
            for payload in &plan.payloads {
                match payload.item {
                    ContentRef {
                        type_: ContentType::Block,
                        id,
                    } => {
                        if let Some(block_def) = registry.block(BlockId::new(id)) {
                            for stack in &block_def.requirements {
                                total.add(stack.item, stack.amount * payload.amount);
                            }
                        }
                    }
                    ContentRef {
                        type_: ContentType::Unit,
                        id,
                    } => {
                        if let Some(unit_def) = registry.unit(UnitTypeId::new(id))
                            && let Some(stacks) = total_requirements(unit_def, registry)
                        {
                            for stack in &stacks {
                                total.add(stack.item, stack.amount * payload.amount);
                            }
                        }
                    }
                    _ => {}
                }
            }
            return Some(total.to_array());
        }
    }
    None
}

/// `UnitType.getTotalRequirements()` — requirements including reconstructor
/// chains (`UnitType.java:1320-1344`).
pub fn total_requirements(
    unit: &UnitTypeDef,
    registry: &ContentRegistry,
) -> Option<Vec<ItemStack>> {
    // Reconstructor chain: consume items of the producing reconstructor plus
    // the previous unit's total requirements (`getRequirements` prevReturn).
    for block in registry.blocks() {
        if let Some(upgrade) = block
            .reconstructor_upgrades
            .iter()
            .find(|(_, to)| *to == unit.id)
        {
            for consume in &block.consumes {
                if let Consume::Items(stacks) = &consume.consume {
                    let mut total = ItemSeq::with_len(registry.items().len());
                    total.add_stacks(stacks);
                    if let Some(prev) = registry.unit(upgrade.0)
                        && let Some(prev_stacks) = total_requirements(prev, registry)
                    {
                        total.add_stacks(&prev_stacks);
                    }
                    return Some(total.to_array());
                }
            }
            return None;
        }
    }
    first_requirements(unit, registry)
}

impl Content for UnitTypeDef {
    const TYPE: ContentType = ContentType::Unit;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = UnitTypeId::new(id);
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

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    /// `UnitType.init()` metadata derivations (`type/UnitType.java:915-1127`).
    ///
    /// Cross-content pieces (bullet DPS, `wet` naval immunity, default
    /// command/stance id lists) run in the post-sweep [`link`] pass.
    fn init_self(&mut self) -> Result<(), ContentError> {
        self.allow_leg_step = self.entity_def.allow_leg_step();

        // Water preset (`UnitType.java:925-934`).
        if self.entity_def.is_naval() {
            self.naval = true;
            self.can_drown = false;
            self.emit_walk_sound = false;
            self.omni_movement = false;
            if self.shadow_elevation < 0.0 {
                self.shadow_elevation = 0.11;
            }
        }

        if self.flying && !self.env_enabled.contains(EnvFlag::Space) {
            self.env_enabled.flags.push(EnvFlag::Space);
        }

        if self.death_sound == SoundId::UNSET {
            self.death_sound = if self.hit_size < 12.0 {
                SoundId::UNIT_EXPLODE1
            } else if self.hit_size < 22.0 {
                SoundId::UNIT_EXPLODE2
            } else {
                SoundId::UNIT_EXPLODE3
            };
        }
        if self.wreck_sound == SoundId::UNSET {
            self.wreck_sound = if self.hit_size >= 22.0 {
                SoundId::WRECK_FALL_BIG
            } else {
                SoundId::WRECK_FALL
            };
        }
        if self.light_radius == -1.0 {
            self.light_radius = 60.0_f32.max(self.hit_size * 2.3);
        }
        if self.flying_layer < 0.0 {
            self.flying_layer = if self.low_altitude {
                LAYER_FLYING_UNIT_LOW
            } else {
                LAYER_FLYING_UNIT
            };
        }
        self.clip_size = self.clip_size.max(self.light_radius * 1.1);
        self.single_target =
            self.single_target || (self.weapons.len() <= 1 && !self.force_multi_target);

        if self.item_capacity < 0 {
            self.item_capacity = round_to_i32(round_to(self.hit_size * 4.0, 10.0)).max(10);
        }

        // Default range with the 4-unit margin (`UnitType.java:966-997`).
        let margin = 4.0;
        if self.range < 0.0 {
            self.range = f32::MAX;
            for weapon in &self.weapons {
                if !weapon.use_attack_range {
                    continue;
                }
                self.range = self.range.min(weapon.range() - margin);
                self.max_range = self.max_range.max(weapon.range() - margin);
            }
        }
        if self.max_range < 0.0 {
            self.max_range = 0.0_f32.max(self.range);
            for weapon in &self.weapons {
                if !weapon.use_attack_range {
                    continue;
                }
                self.max_range = self.max_range.max(weapon.range() - margin);
            }
        }
        if self.fog_radius < 0.0 {
            self.fog_radius = (58.0_f32 * 3.0).max(self.hit_size * 2.0) / 8.0;
        }
        if !self.weapons.iter().any(|weapon| weapon.use_attack_range) {
            if self.range < 0.0 || self.range == f32::MAX {
                self.range = self.mine_range;
            }
            if self.max_range < 0.0 || self.max_range == f32::MAX {
                self.max_range = self.mine_range;
            }
        }
        if self.mech_stride < 0.0 {
            self.mech_stride = 4.0 + (self.hit_size - 8.0) / 2.1;
        }
        if self.segment_spacing < 0.0 {
            self.segment_spacing = self.hit_size;
        }
        if self.aim_dst < 0.0 {
            self.aim_dst = if self.weapons.iter().any(|weapon| !weapon.rotate) {
                self.hit_size * 2.0
            } else {
                self.hit_size / 2.0
            };
        }
        if self.step_shake < 0.0 {
            self.step_shake = ((self.hit_size - 11.0) / 9.0).round();
            self.mech_step_particles = self.hit_size > 15.0;
        }
        if self.engine_size > 0.0 {
            self.engines.push(EngineSpec::new(
                0.0,
                -self.engine_offset,
                self.engine_size,
                -90.0,
            ));
        }
        self.mine_beam_offset = self.hit_size / 2.0;

        // Mirrored weapon variants (`UnitType.java:1037-1061`).
        let mut mapped: Vec<WeaponDef> = Vec::with_capacity(self.weapons.len() * 2);
        for mut weapon in self.weapons.drain(..) {
            if weapon.recoil_time < 0.0 {
                weapon.recoil_time = weapon.reload;
            }
            let mut mirror_copy = None;
            if weapon.mirror {
                let mut copy = weapon.clone();
                copy.flip();
                mirror_copy = Some(copy);
            }
            let base_index = mapped.len();
            mapped.push(weapon);
            if let Some(copy) = mirror_copy {
                let copy_index = mapped.len();
                mapped.push(copy);
                // Since there are now two weapons, reload/recoil time double.
                mapped[base_index].recoil_time *= 2.0;
                mapped[copy_index].recoil_time *= 2.0;
                mapped[base_index].reload *= 2.0;
                mapped[copy_index].reload *= 2.0;
                mapped[base_index].other_side = copy_index as i32;
                mapped[copy_index].other_side = base_index as i32;
            }
        }
        self.weapons = mapped;
        for weapon in &mut self.weapons {
            weapon.init();
        }

        self.can_heal = self.weapons.iter().any(|weapon| weapon.bullet.heals());
        self.can_attack = self.weapons.iter().any(|weapon| !weapon.no_attack);
        Ok(())
    }

    /// `UnitType.postInit()` database tag defaults (`UnitType.java:537-550`).
    fn post_init(&mut self) -> Result<(), ContentError> {
        if self
            .unlock
            .database_tag
            .as_deref()
            .is_none_or(str::is_empty)
        {
            let tag = if self.flying {
                "unit-air"
            } else if self.naval {
                "unit-naval"
            } else {
                "unit-ground"
            };
            self.unlock.database_tag = Some(tag.to_owned());
        }
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for UnitTypeDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for UnitTypeDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// Sink for generated unit waves.
pub trait UnitSink {
    /// Registers one generated bullet spec, returning its id.
    fn push_bullet(&mut self, spec: BulletSpec) -> Result<BulletId, ContentError>;
    /// Registers one generated unit spec (with its weapons and bullets).
    fn push_unit(&mut self, spec: UnitSpec) -> Result<(), ContentError>;
}

/// Sink that builds records inside a [`ContentRegistry`].
struct RegistrySink<'a> {
    registry: &'a mut ContentRegistry,
    bundle: &'a dyn BundleView,
    store: &'a dyn UnlockStore,
}

impl RegistrySink<'_> {
    /// Snapshots the values `UnitType.init()` reads from a registered bullet.
    fn resolved_bullet(&self, id: BulletId) -> Result<ResolvedBullet, ContentError> {
        let bullet = self
            .registry
            .bullet(id)
            .ok_or(ContentError::UnknownId(id.raw()))?;
        Ok(ResolvedBullet {
            id,
            range: bullet.compute_range(),
            heals: bullet.heals(),
            kill_shooter: bullet.kill_shooter,
            dps: 0.0,
        })
    }

    /// Registers a bullet (with nested inline content) and snapshots it.
    fn build_bullet(&mut self, spec: BulletSpec) -> Result<ResolvedBullet, ContentError> {
        let id = self.push_bullet(spec)?;
        self.resolved_bullet(id)
    }
}

impl UnitSink for RegistrySink<'_> {
    fn push_bullet(&mut self, spec: BulletSpec) -> Result<BulletId, ContentError> {
        // Nested inline content is registered *after* the outer bullet's own
        // constructor (upstream initializer semantics), then patched in.
        let mut spec = spec;
        let frag_bullet = spec.frag_bullet.take();
        let interval_bullet = spec.interval_bullet.take();
        let lightning_type = spec.lightning_type.take();
        let spawn_bullets = std::mem::take(&mut spec.spawn_bullets);
        let spawn_unit = spec.spawn_unit.take();
        let def = BulletDef::from_spec(&spec, self.registry)?;
        let id = self.registry.add_bullet(def)?;

        let frag = match frag_bullet {
            Some(nested) => Some(self.push_bullet(*nested)?),
            None => None,
        };
        let interval = match interval_bullet {
            Some(nested) => Some(self.push_bullet(*nested)?),
            None => None,
        };
        let lightning = match lightning_type {
            Some(nested) => Some(self.push_bullet(*nested)?),
            None => None,
        };
        let mut spawned = Vec::with_capacity(spawn_bullets.len());
        for nested in spawn_bullets {
            spawned.push(self.push_bullet(nested)?);
        }
        let (spawned_unit, spawn_range) = match spawn_unit {
            Some(unit) => {
                let lifetime = unit.lifetime;
                let speed = unit.speed;
                let before = self.registry.units().len();
                self.push_unit(*unit)?;
                let id = UnitTypeId::new((before) as u16);
                let range = match (lifetime, speed) {
                    (Some(lifetime), Some(speed)) => Some((lifetime, speed)),
                    _ => self
                        .registry
                        .unit(id)
                        .map(|unit| (unit.lifetime, unit.speed)),
                };
                (Some(id), range)
            }
            None => (None, None),
        };

        let bullet = self
            .registry
            .bullet_mut(id)
            .ok_or(ContentError::UnknownId(id.raw()))?;
        bullet.frag_bullet = frag;
        bullet.interval_bullet = interval;
        bullet.lightning_type = lightning;
        bullet.spawn_bullets = spawned;
        bullet.spawn_unit = spawned_unit;
        bullet.spawn_unit_range = spawn_range;
        Ok(id)
    }

    fn push_unit(&mut self, spec: UnitSpec) -> Result<(), ContentError> {
        // Upstream registers the unit in its constructor, then constructs
        // weapons/bullets in the initializer — the same order is kept here so
        // content ids match (`content/AGENTS.md`).
        let def = UnitTypeDef::from_spec(&spec, self.registry, self.bundle, self.store)?;
        let id = self.registry.add_unit(def)?;

        // Unit-local bullet variables (`BulletType x = new ...`), in order.
        let mut pre_slots: Vec<ResolvedBullet> = Vec::with_capacity(spec.pre_bullets.len());
        for bullet in spec.pre_bullets {
            pre_slots.push(self.build_bullet(bullet)?);
        }

        // Weapons with their bullets (inline missile units recurse via
        // `BulletSpec.spawn_unit`).
        let mut weapons = Vec::with_capacity(spec.weapons.len());
        for weapon in spec.weapons {
            let bullet = match &weapon.bullet {
                BulletRef::Placeholder => self.resolved_bullet(BulletId::new(0))?,
                BulletRef::Pre(index) => *pre_slots.get(*index).ok_or_else(|| {
                    ContentError::Parse(format!(
                        "weapon `{}` references unknown pre-bullet slot {index}",
                        weapon.name
                    ))
                })?,
                BulletRef::Inline(spec) => self.build_bullet((**spec).clone())?,
            };
            weapons.push(WeaponDef::from_spec(weapon, bullet, self.registry)?);
        }

        self.registry
            .unit_mut(id)
            .ok_or(ContentError::UnknownId(id.raw()))?
            .weapons = weapons;
        Ok(())
    }
}

/// Runs every ported unit wave into `sink` in upstream order.
pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    standard::load(sink)?;
    erekir::load(sink)?;
    special::load(sink)?;
    Ok(())
}

/// Loads units into a [`ContentRegistry`] (called by `create_base_content`).
pub(crate) fn load_into(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    let mut sink = RegistrySink {
        registry,
        bundle,
        store,
    };
    load(&mut sink)
}

/// Post-`init()` link pass (`UnitType.init()` cross-content pieces):
///
/// * computes `BulletType.estimateDPS()` bottom-up (frag/interval/spawn
///   bullets always precede their parents in vanilla construction order) and
///   patches each unit's `dpsEstimate` (`UnitType.estimateDps`,
///   `type/UnitType.java:1129-1142`) — this runs after
///   `ErekirTechTree.rebalance`, matching upstream's lazy `cachedDps`;
/// * fills naval `wet` immunities and the default command/stance lists
///   (`type/UnitType.java:1067-1121`).
pub(crate) fn link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    // Bullet DPS bottom-up in id order.
    let mut dps: Vec<f32> = Vec::with_capacity(registry.bullets().len());
    for bullet in registry.bullets() {
        let mut sum = (bullet.damage + bullet.splash_damage * 0.75)
            * if bullet.pierce {
                if bullet.pierce_cap == -1 {
                    2.0
                } else {
                    bullet.pierce_cap.clamp(1, 2) as f32
                }
            } else {
                1.0
            };
        if let Some(frag) = bullet.frag_bullet
            && frag != bullet.id
        {
            sum += dps.get(frag.index()).copied().unwrap_or(0.0) * bullet.frag_bullets as f32 / 2.0;
        }
        for spawn in &bullet.spawn_bullets {
            sum += dps.get(spawn.index()).copied().unwrap_or(0.0);
        }
        dps.push(sum);
    }

    let wet = registry.status_id("wet");
    let command_id = |name: &str| {
        registry
            .unit_commands
            .iter()
            .find(|command| command.name == name)
            .map(|command| command.id)
    };
    let stance_id = |name: &str| {
        registry
            .unit_stances
            .iter()
            .find(|stance| stance.name == name)
            .map(|stance| stance.id)
    };

    struct UnitUpdate {
        commands: Vec<UnitCommandId>,
        stances: Vec<UnitStanceId>,
        default_command: Option<UnitCommandId>,
        dps_estimate: f32,
    }

    let mut updates: Vec<UnitUpdate> = Vec::new();
    for unit in registry.units() {
        let mut dps_estimate = 0.0;
        for weapon in &unit.weapons {
            dps_estimate += (dps.get(weapon.bullet.id.index()).copied().unwrap_or(0.0)
                / weapon.reload)
                * weapon.shoot.shots as f32
                * 60.0;
        }
        if unit.weapons.iter().any(|weapon| weapon.bullet.kill_shooter) {
            dps_estimate /= 15.0;
        }

        // Default commands (`UnitType.java:1067-1103`).
        let mut commands = unit.commands.clone();
        if commands.is_empty() {
            if let Some(id) = command_id("move") {
                commands.push(id);
            }
            if unit.allowed_in_payloads
                && let Some(id) = command_id("enterPayload")
            {
                commands.push(id);
            }
            if unit.can_boost {
                if unit.build_speed > 0.0 {
                    if let Some(id) = command_id("rebuild") {
                        commands.push(id);
                    }
                    if let Some(id) = command_id("assist") {
                        commands.push(id);
                    }
                }
                if unit.mine_tier > 0
                    && let Some(id) = command_id("mine")
                {
                    commands.push(id);
                }
            }
            if unit.flying {
                if unit.can_heal
                    && let Some(id) = command_id("repair")
                {
                    commands.push(id);
                }
                if unit.build_speed > 0.0 {
                    if let Some(id) = command_id("rebuild") {
                        commands.push(id);
                    }
                    if let Some(id) = command_id("assist") {
                        commands.push(id);
                    }
                }
                if unit.mine_tier > 0
                    && let Some(id) = command_id("mine")
                {
                    commands.push(id);
                }
                if unit.entity_def.is_payload() {
                    for name in ["loadUnits", "loadBlocks", "unloadPayload", "loopPayload"] {
                        if let Some(id) = command_id(name) {
                            commands.push(id);
                        }
                    }
                }
            }
        }
        let default_command = unit.default_command.or_else(|| commands.first().copied());

        // Default stances (`UnitType.java:1109-1121`).
        let mut stances = unit.stances.clone();
        if stances.is_empty() {
            if unit.can_attack {
                for name in ["stop", "holdfire", "pursuetarget", "patrol"] {
                    if let Some(id) = stance_id(name) {
                        stances.push(id);
                    }
                }
                if !unit.flying
                    && let Some(id) = stance_id("ram")
                {
                    stances.push(id);
                }
                if unit.can_boost
                    && let Some(id) = stance_id("boost")
                {
                    stances.push(id);
                }
            } else {
                for name in ["stop", "patrol"] {
                    if let Some(id) = stance_id(name) {
                        stances.push(id);
                    }
                }
            }
        }
        updates.push(UnitUpdate {
            commands,
            stances,
            default_command,
            dps_estimate,
        });
    }

    for (index, update) in updates.into_iter().enumerate() {
        if let Some(unit) = registry.units_mut().get_mut(index) {
            unit.commands = update.commands;
            unit.stances = update.stances;
            unit.dps_estimate = update.dps_estimate;
            if unit.default_command.is_none() {
                unit.default_command = update.default_command;
            }
            if unit.naval
                && let Some(wet) = wet
                && !unit.immunities.contains(&wet)
            {
                unit.immunities.push(wet);
            }
            for weapon in &mut unit.weapons {
                weapon.bullet.dps = dps.get(weapon.bullet.id.index()).copied().unwrap_or(0.0);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    fn get<'a>(registry: &'a ContentRegistry, name: &str) -> &'a UnitTypeDef {
        registry.unit_by_name(name).expect("unit present")
    }

    /// Plan 02 §5 M5: `units::entity_def_table_covers_all_units` — every unit has
    /// an `EntityDefSpec` with a known component vocabulary, and all 19
    /// declaration groups are represented.
    #[test]
    fn entity_def_table_covers_all_units() {
        let registry = test_registry();
        assert_eq!(registry.units().len(), 65, "65 unit records");
        let mut groups = std::collections::BTreeSet::new();
        for unit in registry.units() {
            assert!(
                !unit.entity_def.components.is_empty(),
                "{} components",
                unit.name
            );
            assert!(
                !unit.entity_def.class_name.is_empty(),
                "{} class name",
                unit.name
            );
            groups.insert((unit.entity_def.components, unit.entity_def.legacy));
        }
        assert_eq!(groups.len(), 16, "19 declarations over 16 distinct groups");

        let dagger = get(&registry, "dagger");
        assert_eq!(dagger.entity_def.class_name, "MechUnit");
        assert!(!dagger.entity_def.legacy);
        let risso = get(&registry, "risso");
        assert!(risso.entity_def.is_naval());
        let mega = get(&registry, "mega");
        assert!(mega.entity_def.is_payload());
        let latum = get(&registry, "latum");
        assert!(latum.entity_def.allow_leg_step());
        let mono = get(&registry, "mono");
        assert_eq!(mono.entity_def.class_name, "UnitEntityLegacyMono");
        assert!(mono.entity_def.legacy);
    }

    /// Plan 02 §5 M5: `units::mirrored_weapon_reload_doubled` — `mirror = true`
    /// weapons yield a flipped copy with doubled reload/recoil (`UnitType.java:1037-1061`).
    #[test]
    fn mirrored_weapon_reload_doubled() {
        let registry = test_registry();
        let scepter = get(&registry, "scepter");
        // 3 authored weapons, all mirrored -> 6.
        assert_eq!(scepter.weapons.len(), 6);
        let base = &scepter.weapons[0];
        let copy = &scepter.weapons[1];
        assert_eq!(base.name, "scepter-weapon");
        assert_eq!(copy.name, "scepter-weapon");
        assert_eq!(base.reload, 90.0, "authored 45 doubled");
        assert_eq!(copy.reload, 90.0);
        assert_eq!(base.recoil_time, 90.0, "recoilTime <- reload 45, doubled");
        assert_eq!(copy.recoil_time, 90.0);
        assert_eq!(base.other_side, 1);
        assert_eq!(copy.other_side, 0);
        assert_eq!(base.x, 16.0);
        assert_eq!(copy.x, -16.0, "flip negates x");
        assert!(copy.flip_sprite);

        // The two scepter-mounts share the local `smallBullet` bullet id.
        assert_eq!(scepter.weapons[2].bullet.id, scepter.weapons[4].bullet.id);
        assert_ne!(scepter.weapons[2].bullet.id, scepter.weapons[0].bullet.id);

        // Unmirrored weapons stay single (mono has no weapons at all).
        let mono = get(&registry, "mono");
        assert_eq!(mono.weapons.len(), 0);
    }

    /// Plan 02 §5 M5: `units::research_requirements_derived` —
    /// `UnitType.researchRequirements()` from factory/reconstructor plans
    /// (`UnitType.java:1405-1427`).
    #[test]
    fn research_requirements_derived() {
        let registry = test_registry();
        let silicon = registry.item_id("silicon").unwrap();
        let lead = registry.item_id("lead").unwrap();
        let beryllium = registry.item_id("beryllium").unwrap();

        // dagger: ground-factory plan (silicon 10, lead 10) x 0.5 multiplier.
        let dagger = get(&registry, "dagger");
        assert_eq!(
            dagger.research_requirements(&registry),
            vec![ItemStack::new(silicon, 5), ItemStack::new(lead, 5)]
        );

        // nova: plan (silicon 30, lead 20, titanium 20) x 50.
        let nova = get(&registry, "nova");
        let titanium = registry.item_id("titanium").unwrap();
        assert_eq!(
            nova.research_requirements(&registry),
            vec![
                ItemStack::new(silicon, 1500),
                ItemStack::new(lead, 1000),
                ItemStack::new(titanium, 1000)
            ]
        );

        // stell: tank-fabricator plan resolves, but the authored
        // `researchCostMultiplier = 0f` zeroes every stack and
        // `researchRequirements()` filters zeros out (`UnitTypes.java:2664`).
        let stell = get(&registry, "stell");
        assert_eq!(stell.research_cost_multiplier, 0.0);
        assert!(stell.research_requirements(&registry).is_empty());
        assert_eq!(
            first_requirements(stell, &registry),
            Some(vec![
                ItemStack::new(beryllium, 40),
                ItemStack::new(silicon, 50)
            ])
        );

        // pulsar: additive-reconstructor upgrade (nova -> pulsar), consume
        // items of the reconstructor x 50.
        let pulsar = get(&registry, "pulsar");
        let reqs = pulsar.research_requirements(&registry);
        assert!(!reqs.is_empty(), "reconstructor requirements derive");
        let (blocks, _) = pulsar.get_dependencies(&registry);
        assert!(
            blocks
                .iter()
                .any(|id| registry.block(*id).unwrap().name == "additive-reconstructor"),
            "pulsar depends on the additive reconstructor"
        );

        // Units with no producer get the base (empty) requirements.
        let dummy = get(&registry, "dummy");
        assert!(dummy.research_requirements(&registry).is_empty());
    }

    /// Plan 02 §5 M5: `units::hidden_flags` — `UnitType.hidden` + the
    /// `MissileUnitType` preset hidden rule.
    #[test]
    fn hidden_flags() {
        let registry = test_registry();
        for name in [
            "renale",
            "latum",
            "block",
            "manifold",
            "assembly-drone",
            "dummy",
        ] {
            assert!(get(&registry, name).hidden, "{name} hidden");
            assert!(get(&registry, name).is_hidden(), "{name} is_hidden");
        }
        for name in ["anthicus-missile", "quell-missile", "disrupt-missile"] {
            assert!(get(&registry, name).hidden, "{name} hidden by preset");
        }
        assert!(!get(&registry, "dagger").hidden);
        assert!(!get(&registry, "flare").hidden);
    }

    /// `UnitType.init()` derived metadata (`type/UnitType.java:915-1127`):
    /// range/maxRange from weapon ranges, fog radius, item capacity, aim dst,
    /// death/wreck sound defaults, naval preset.
    #[test]
    fn derived_metadata() {
        let registry = test_registry();
        let dagger = get(&registry, "dagger");
        // bullet 2.5 speed x 60 lifetime = 150, minus the 4 margin.
        assert_eq!(dagger.range, 146.0);
        assert_eq!(dagger.max_range, 146.0);
        assert_eq!(dagger.fog_radius, 174.0 / 8.0);
        assert_eq!(dagger.item_capacity, 30);
        assert_eq!(dagger.aim_dst, 16.0, "non-rotating weapon -> hitSize*2");
        assert_eq!(dagger.step_shake, 0.0);
        assert!(!dagger.mech_step_particles);
        assert_eq!(dagger.death_sound, SoundId::UNIT_EXPLODE1);
        assert_eq!(dagger.wreck_sound, SoundId::WRECK_FALL);
        assert_eq!(dagger.commands.len(), 2, "move + enterPayload");
        assert_eq!(
            dagger.stances.len(),
            5,
            "stop/holdfire/pursuetarget/patrol/ram"
        );
        assert!(!dagger.naval);

        // Naval preset (`UnitType.java:925-934`).
        let risso = get(&registry, "risso");
        assert!(risso.naval);
        assert!(!risso.can_drown);
        assert!(!risso.omni_movement);
        assert_eq!(risso.shadow_elevation, 0.11);
        let wet = registry.status_id("wet").unwrap();
        assert!(risso.immunities.contains(&wet));

        // Erekir fog radius override + flying env rule.
        let anthicus = get(&registry, "anthicus");
        assert!(anthicus.env_enabled.contains(EnvFlag::Space) || !anthicus.flying);

        // Reign is big: death sound by hit size, mech particles.
        let reign = get(&registry, "reign");
        assert!(reign.mech_step_particles);
        assert_eq!(reign.death_sound, SoundId::UNIT_EXPLODE3);
        assert_eq!(reign.wreck_sound, SoundId::WRECK_FALL_BIG);
    }

    /// `ErekirTechTree.rebalance()` wiring (M5): weapon bullets of
    /// `ErekirUnitType`/`TankUnitType` units are scaled once by 0.75.
    #[test]
    fn rebalance_scales_erekir_unit_bullets() {
        let registry = test_registry();
        let stell = get(&registry, "stell");
        assert_eq!(stell.kind, UnitKind::TankUnitType);
        let stell_bullet = registry.bullet(stell.weapons[0].bullet.id).unwrap();
        assert_eq!(stell_bullet.damage, 30.0, "authored 40 x 0.75");

        // Serpulo units are untouched.
        let dagger = get(&registry, "dagger");
        let dagger_bullet = registry.bullet(dagger.weapons[0].bullet.id).unwrap();
        assert_eq!(dagger_bullet.damage, 9.0);

        // Every Erekir/Tank unit's weapon bullets are scaled; spot-check merui.
        let merui = get(&registry, "merui");
        assert_eq!(merui.kind, UnitKind::ErekirUnitType);
        for weapon in &merui.weapons {
            let bullet = registry.bullet(weapon.bullet.id).unwrap();
            assert!(
                bullet.damage < 1000.0,
                "merui weapon bullet {} damage {}",
                bullet.id.raw(),
                bullet.damage
            );
        }
    }

    /// Bullet metadata: internal bullets keep their ids (prefix), and the
    /// derived `range`/`despawnHit`/`lightningType` init rules hold.
    #[test]
    fn bullet_ids_and_derivations() {
        let registry = test_registry();
        // The 6 internal bullets keep upstream ids (upstream-prefix space).
        assert_eq!(registry.bullets().len(), 112);
        let placeholder = registry
            .bullet(crate::content::registries::bullets::PLACEHOLDER)
            .unwrap();
        assert_eq!(placeholder.sprite.as_deref(), Some("ohno"));

        // Lightning bullet defaults (`BulletType.init` + `bullets::link`).
        let scepter = get(&registry, "scepter");
        let main = registry.bullet(scepter.weapons[0].bullet.id).unwrap();
        assert!(main.despawn_hit, "lightning > 0 -> despawnHit");
        assert_eq!(main.lightning, 2);
        let shocked = registry.status_id("shocked").unwrap();
        // status was already set? scepter bullet has no explicit status -> shocked.
        assert_eq!(main.status, shocked);
        assert_eq!(
            main.lightning_type,
            Some(crate::content::registries::bullets::DAMAGE_LIGHTNING),
            "collides air+ground -> damageLightning"
        );

        // Zenith missile bullet: drag changes the range formula.
        let zenith = get(&registry, "zenith");
        let zb = registry.bullet(zenith.weapons[0].bullet.id).unwrap();
        let expected = 3.0 * (1.0 - (1.0_f32 - -0.003).powf(50.0)) / -0.003;
        assert!(
            (zb.range - expected).abs() < 0.01,
            "{} vs {}",
            zb.range,
            expected
        );
    }
}
