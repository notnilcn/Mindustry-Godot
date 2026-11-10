// SPDX-License-Identifier: GPL-3.0-only

//! Shared view-data structs handed from the sim host to the render passes.
//! Upstream: core/src/mindustry/entities/units/UnitType.java (draw composition), core/src/mindustry/world/blocks/* (draw state rows)

use super::*;

/// One belt slot as the renderer consumes it: `(item, xs, ys)` (`Conveyor.draw`
/// item loop; `ConveyorBuild.ids/xs/ys`).
pub type BeltItemSlot = (ItemId, f32, f32);

/// One unit's full draw state (`UnitType.draw` composition; plan §3.4).
pub struct UnitDraw {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Body rotation in degrees (`UnitCore.rotation`).
    pub rotation: f32,
    /// Unit type name (`UnitType.name`).
    pub name: String,
    /// `UnitType.flying`.
    pub flying: bool,
    /// `UnitType.lowAltitude`.
    pub low_altitude: bool,
    /// Owning team id.
    pub team: u8,
    /// Health fraction (`Unit.healthf()`).
    pub healthf: f32,
    /// Flight elevation `0..=1` (`UnitCore.elevation`).
    pub elevation: f32,
    /// Unit hit radius (`UnitType.hitSize`); `Units.closestOverlap` adds half.
    pub hit_size: f32,
    /// Mech walk state (`MechComp`); `None` for non-mechs.
    pub mech: Option<MechDraw>,
    /// Legged-unit state (`LegsComp` + `UnitType` leg knobs); `None` for
    /// non-legged units (J03 T4).
    pub legs: Option<LegsDraw>,
    /// Tank tread state (`TankComp` + `UnitType` tread knobs); `None` for
    /// non-tanks (J03 T5).
    pub treads: Option<TreadsDraw>,
    /// Crawl segment state (`CrawlComp` + `UnitType` segment knobs); `None`
    /// for non-crawl units (`UnitType.drawCrawl`, L04 T3).
    pub crawl: Option<CrawlDraw>,
    /// First carried payload (`PayloadComp.payloads().first()`;
    /// `UnitType.drawPayload`, L04 T3).
    pub payload: Option<PayloadDraw>,
    /// Live weapon mounts (`UnitWeapons`).
    pub weapons: Vec<WeaponDraw>,
    /// `UnitType.parts` (drawn after body/weapons/items, `UnitType.java:1561-1578`).
    pub parts: Vec<DrawPartSpec>,
    /// `Scaled.fin()` (`TimedKillComp.fin`, `time / lifetime`); `0` when the
    /// unit is not timed.
    pub fin: f32,
    /// `UnitType.drawBody` (gates the body quad).
    pub draw_body: bool,
    /// `UnitType.drawCell` (gates the team cell quad).
    pub draw_cell: bool,
    /// `UnitType.groundLayer` (draw z base; `Layer.groundUnit` default).
    pub ground_layer: f32,
    /// `EntitySeq.0` (view-state key; entities are reused).
    pub id: u64,
    /// `MinerComp.mine_tile` when `MinerComp.mining` (world tile).
    pub mine_tile: Option<(i16, i16)>,
    /// `ItemsComp.item` as `(item name, amount)`.
    pub item: Option<(String, i32)>,
    /// Whether this is the local player's unit.
    pub is_player: bool,
    /// Absolute health (`Health.health`).
    pub health: f32,
    /// Active status effects with view parameters `(effect, chance, color)`
    /// (`StatusEffect.update` particles; H03 T10).
    pub statuses: Vec<(
        mind_core::content::registries::fx_meta::EffectRef,
        f32,
        mind_core::content::Rgba,
    )>,
    /// `isGrounded()` = `elevation < 0.001` (`UnitComp.java:80-82`; H03 T10).
    pub grounded: bool,
    /// `UnitType.hovering` (walk-effect gate; H03 T10).
    pub hovering: bool,
    /// `UnitType.emitWalkEffect` (H03 T10).
    pub emit_walk_effect: bool,
    /// `floorOn().isLiquid` (walk effect is `Fx.ripple` on liquid floors;
    /// H03 T10).
    pub floor_is_liquid: bool,
    /// `tile.getFloorColor()` (`Floor.mapColor`; H03 T10).
    pub floor_color: mind_core::content::Rgba,
}

/// `MechComp` walk state resolved for drawing (`drawMech`).
pub struct MechDraw {
    /// Decoupled base rotation in degrees (`baseRotation`).
    pub base_rotation: f32,
    /// Walk-cycle timer (`walkTime`).
    pub walk_time: f32,
    /// `UnitType.mechStride`.
    pub stride: f32,
}

/// `TankComp` tread animation state (`drawTank`) + `UnitType` tread knobs.
pub struct TreadsDraw {
    /// `TankComp.treadTime` (`unit.treadTime()`).
    pub tread_time: f32,
    /// `UnitType.treadRects`.
    pub rects: Vec<mind_core::content::registries::units::TreadRect>,
    /// `UnitType.treadFrames`.
    pub frames: i32,
}

/// `CrawlComp` state + `UnitType` segment knobs resolved for `drawCrawl`
/// (`UnitType.java:1902-1937`; L04 T3).
pub struct CrawlDraw {
    /// `CrawlComp.crawlTime` (distance travelled).
    pub crawl_time: f32,
    /// `CrawlComp.segmentRot` (trailing segment rotation).
    pub segment_rot: f32,
    /// `UnitType.segments`.
    pub segments: i32,
    /// `UnitType.segmentScl`.
    pub segment_scl: f32,
    /// `UnitType.segmentMag`.
    pub segment_mag: f32,
    /// `UnitType.segmentPhase`.
    pub segment_phase: f32,
}

/// The first payload on a carrier's stack (`PayloadComp.payloads().first()`),
/// resolved to the draw data the hand-written unit pass consumes
/// (`UnitType.drawPayload` + `UnitPayload.draw`/`BuildPayload.draw`; L04 T3).
pub enum PayloadDraw {
    /// A carried unit (`UnitPayload`): its type and pickup snapshot.
    Unit {
        /// `UnitType.name` (body/cell region prefix).
        name: String,
        /// Pickup rotation in degrees (`Payload.set` writes `unit.rotation`).
        rotation: f32,
        /// Owning team (segment/unit cell tint).
        team: u8,
        /// `health / type.health` (the carried unit's body tint).
        healthf: f32,
        /// `UnitType.drawBody`.
        draw_body: bool,
        /// `UnitType.drawCell`.
        draw_cell: bool,
        /// `UnitType.segments` (`0` = normal body); a carried crawl unit has
        /// no live segment state, so the composed body region stands in.
        segments: i32,
    },
    /// A carried building (`BuildPayload`): the composed `-full` icon and
    /// `Block.size` (`Building.payloadDraw`; `BuildPayload.set` ignores the
    /// payload rotation).
    Build {
        /// `Block.name` (resolves `block-<name>-full`).
        block: String,
        /// `Block.region` (second icon candidate).
        region: String,
        /// `Block.size` in tiles.
        size: i32,
    },
}

/// `LegsComp` state + `UnitType` leg knobs resolved for `drawLegs` (J03 T4).
pub struct LegsDraw {
    /// `LegsComp.baseRotation` (`unit.baseRotation()`).
    pub base_rotation: f32,
    /// Live legs in index order (foot/joint from `LegsComp.Leg`).
    pub legs: Vec<mind_core::render::unit_draw::LegDrawLeg>,
    /// `1 - drownTime`; the port has no drowning yet, so 1.
    pub inv_drown: f32,
    /// `UnitType.legBaseOffset`.
    pub leg_base_offset: f32,
    /// `UnitType.legStraightness`.
    pub leg_straightness: f32,
    /// `UnitType.baseLegStraightness`.
    pub base_leg_straightness: f32,
    /// `UnitType.legStraightLength`.
    pub leg_straight_length: f32,
    /// `UnitType.legExtension`.
    pub leg_extension: f32,
    /// `UnitType.legBaseUnder`.
    pub leg_base_under: bool,
    /// `UnitType.shadowElevation` (`0` = off).
    pub shadow_elevation: f32,
}

/// One command-overlay row: `(x, y, hit_size, flying, target_pos)`
/// (`InputHandler.drawCommanded` inputs).
pub type UnitCommandRow = (f32, f32, f32, bool, Option<(f32, f32)>);

/// One ability's live draw state (`Ability.draw(Unit)`; L04 T2).
pub struct UnitAbilityDraw {
    /// Ability class tag.
    pub kind: mind_core::content::registries::units::ability::AbilityKind,
    /// Unit center x.
    pub x: f32,
    /// Unit center y.
    pub y: f32,
    /// Unit rotation (degrees).
    pub rotation: f32,
    /// Unit team id.
    pub team: u8,
    /// Ability metadata.
    pub spec: mind_core::content::registries::units::ability::AbilitySpec,
    /// Index-aligned runtime state (shield pools, view lerps).
    pub state: mind_core::combat::abilities::AbilityState,
    /// `Unit.shield` (`ShieldComp.shield`).
    pub shield: f32,
    /// `EntitySeq.0` (deterministic suppression particle seed).
    pub id: u64,
    /// `UnitType.name` (`ArmorPlateAbility`'s `<name>-armor` region and the
    /// `UnitSpawnAbility` full-icon lookup; L04 T2 residual).
    pub name: String,
    /// `unit.vel().len()` (`MoveLightningAbility.draw` speed scale, `:90`).
    pub vel_len: f32,
    /// `Units.canCreate(unit.team, this.unit)` at draw time — the
    /// `UnitSpawnAbility.draw` gate (`UnitSpawnAbility.java:65`). `false` for
    /// every other kind.
    pub can_create: bool,
}

/// One live weapon mount (`Weapon.draw` inputs).
pub struct WeaponDraw {
    /// Weapon region name (`Weapon.name`).
    pub name: String,
    /// Unit-relative mount x (`Weapon.x`).
    pub x: f32,
    /// Unit-relative mount y (`Weapon.y`).
    pub y: f32,
    /// Aiming rotation relative to the unit, degrees (`WeaponMount.rotation`).
    pub mount_rotation: f32,
    /// Rest rotation when `rotate` is false (`Weapon.baseRotation`).
    pub base_rotation: f32,
    /// Whether the mount rotates toward the target (`Weapon.rotate`).
    pub rotate: bool,
    /// Mirrored mount (`Weapon.flipSprite`).
    pub flip: bool,
    /// Outline drawn over the body (`Weapon.top`); non-top weapons draw their
    /// `<name>-outline` sprite under the body (`drawWeaponOutlines`).
    pub top: bool,
    /// Visual recoil `0..1`.
    pub recoil: f32,
    /// The resolved weapon definition (`Weapon.draw` layer offset, parts,
    /// heat colour, flip sprite).
    pub def: WeaponDef,
    /// The live mount state (`DrawPart.params` warmup/reload/heat/recoil).
    pub mount: WeaponMount,
}

/// `Block.drawPlace` range-circle kinds (G03 T2): which Arc helper the
/// placement preview uses and which colour applies. The range value comes
/// from `MindSimHost::block_place_range`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceRangeKind {
    /// `BaseTurret.drawPlace` (`BaseTurret.java:84-90`): `Drawf.dashCircle`
    /// in `Pal.placing`.
    Turret,
    /// `PowerNode.drawPlace` (`PowerNode.java:159-173`): `Drawf.circles` in
    /// `Pal.placing` at `laserRange * 8`.
    PowerNode,
    /// `MendProjector`/`OverdriveProjector.drawPlace`: `Drawf.dashCircle` in
    /// the projector's `baseColor`.
    Projector,
}

/// Player-builder draw state (`BuilderComp.drawBuilding`, G03 T3).
pub struct BuilderView {
    /// Player unit position (world px).
    pub x: f32,
    /// Player unit position (world px).
    pub y: f32,
    /// Player unit rotation (degrees).
    pub rotation: f32,
    /// `UnitType.buildBeamOffset`.
    pub beam_offset: f32,
    /// `UnitType.buildRange`.
    pub build_range: f32,
    /// `UnitType.drawBuildBeam`.
    pub draw_beam: bool,
    /// `BuilderComp.updateBuilding`.
    pub update_building: bool,
    /// Live plan queue copy (head first).
    pub plans: Vec<mind_core::sim::builder::UnitBuildPlan>,
}

/// One live bullet for the draw pass (`Groups.bullet`; G03 T4).
#[derive(Clone)]
pub struct BulletDraw {
    /// `EntitySeq` (`Bullet.id`; bevy entities are reused).
    pub id: u64,
    /// World position.
    pub x: f32,
    /// World position.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Ticks lived.
    pub time: f32,
    /// Lifetime in ticks.
    pub lifetime: f32,
    /// Effect parameter (`Bullet.fdata`).
    pub fdata: f32,
    /// Bullet content id.
    pub def: mind_core::content::BulletId,
    /// Owning team (bullets are teamless in the port; kept for parity).
    pub team: u8,
}

/// One power-node laser link (`PowerNode.drawLaser`, G03 T7).
pub struct LaserLink {
    /// Source building centre (world px).
    pub x1: f32,
    /// Source building centre (world px).
    pub y1: f32,
    /// Source `Block.size`.
    pub size1: i32,
    /// Target building centre (world px).
    pub x2: f32,
    /// Target building centre (world px).
    pub y2: f32,
    /// Target `Block.size`.
    pub size2: i32,
    /// Owning graph satisfaction (`setupColor`).
    pub satisfaction: f32,
    /// Source block id (region fallbacks).
    pub block: BlockId,
}

/// One beam-node laser (`BeamNode.BeamNodeBuild.draw`, I02 T5). The endpoints
/// are already inset (`BeamNode.java:225-231`).
pub struct BeamLaserLink {
    /// Beam start x (source centre + `size * 4 * d4`).
    pub x1: f32,
    /// Beam start y.
    pub y1: f32,
    /// Beam end x (destination centre - `4 * d4`).
    pub x2: f32,
    /// Beam end y.
    pub y2: f32,
    /// Source graph satisfaction (`setupColor`).
    pub satisfaction: f32,
    /// Source block id (`<name>-beam` region fallback).
    pub block: BlockId,
}

/// One item-bridge link (`ItemBridge.draw`, G03 T7).
pub struct BridgeLink {
    /// Source tile.
    pub x: i32,
    /// Source tile.
    pub y: i32,
    /// Linked tile.
    pub ox: i32,
    /// Linked tile.
    pub oy: i32,
    /// `ItemBridgeBuild.warmup`.
    pub warmup: f32,
    /// `ItemBridgeBuild.time`.
    pub time: f32,
    /// `Block.hasPower` (fade-in only for phase conveyors).
    pub has_power: bool,
    /// Source block id (region names).
    pub block: BlockId,
}

/// Per-building draw state (`Door`/`SwitchBlock`/`Sorter`/`ItemSource`/
/// `Unloader` + cracks/team marks; G03 T8).
pub struct BlockStateDraw {
    /// `DoorState.open`.
    pub door_open: bool,
    /// `Building.enabled`.
    pub enabled: bool,
    /// `SorterBuild.sort_item`.
    pub sort_item: Option<mind_core::content::ItemId>,
    /// `UnloaderBuild.sort_item`.
    pub unloader_item: Option<mind_core::content::ItemId>,
    /// `SandboxState.item` (item source).
    pub source_item: Option<mind_core::content::ItemId>,
    /// `Health.fraction`.
    pub healthf: f32,
    /// `damaged()` = `health < maxHealth - 0.001`.
    pub damaged: bool,
}

/// Fire draw row (`FireComp.draw`; H03 T7).
pub struct FireDraw {
    /// World centre x (tile centre + `randomSeedRange(y, 2)` jitter).
    pub x: f32,
    /// World centre y (tile centre + `randomSeedRange(x, 2)` jitter).
    pub y: f32,
    /// `min((int)animation, frames - 1)`.
    pub frame: i32,
    /// `clamp(warmup / warmupDuration)`.
    pub alpha: f32,
}

/// Puddle draw row (`PuddleComp.draw` / `Liquid.drawPuddle`; H03 T7).
pub struct PuddleDraw {
    /// World centre (`Puddle.x/y`).
    pub x: f32,
    /// World centre (`Puddle.x/y`).
    pub y: f32,
    /// Entity id (`Puddle.id`; seeds the sine phases and satellites).
    pub id: i32,
    /// `clamp(amount / (maxLiquid / 1.5))`.
    pub f: f32,
    /// `liquid.color.shiftValue(-0.05)`.
    pub color: [f32; 4],
    /// `tile.floor().isLiquid`.
    pub on_liquid: bool,
    /// `(lightColor, opacity)` when `lightColor.a > 0.001 && f > 0`.
    pub light: Option<([f32; 4], f32)>,
}

/// One pod or pad draw row (`LaunchPad.java`/`LandingPad.java`; H03 T13).
pub enum PadDraw {
    /// Flying launch payload (`LaunchPayloadComp.draw`): `time`/`lifetime`
    /// drive the `fin`/`fout` path.
    Launch {
        /// `LaunchPayload.id()` (view draw seed).
        id: u32,
        /// Pod centre x (`x`).
        x: f32,
        /// Pod centre y (`y`).
        y: f32,
        /// Ticks in flight.
        time: f32,
        /// Total flight time (120).
        lifetime: f32,
        /// Source pad block (`<block>-pod` region).
        block: mind_core::content::BlockId,
    },
    /// Landing pad: an arriving pod or the cooldown pod (`LandingPadBuild.draw`).
    Landing {
        /// Entity index (view state key).
        id: u32,
        /// Pad centre x.
        x: f32,
        /// Pad centre y.
        y: f32,
        /// Pad size in tiles.
        size: i32,
        /// `arriving` item (`None` = nothing inbound).
        arriving: Option<mind_core::content::ItemId>,
        /// `arrivingTimer` (`0..=1`).
        arriving_timer: f32,
        /// `cooldown` (`0..=1`).
        cooldown: f32,
        /// Pad block (`<block>-pod` region).
        block: mind_core::content::BlockId,
    },
    /// Idle launch pad: pod + corner progress lights (`LaunchPadBuild.draw`).
    Pad {
        /// Entity index (view state key).
        id: u32,
        /// Pad centre x.
        x: f32,
        /// Pad centre y.
        y: f32,
        /// Pad size in tiles.
        size: i32,
        /// `min(items.total/itemCapacity, launchCounter/launchTime)`.
        progress: f32,
        /// Pad block (`<block>-pod` / `<block>-light` regions).
        block: mind_core::content::BlockId,
    },
}

/// Unit factory / reconstructor build preview (`Drawf.construct`,
/// `Drawf.java:586-603`; H03 T9). Rust-only seam for `render/unit_build.rs`.
pub struct UnitBuildDraw {
    /// `true`: reconstructor (payload upgrade); `false`: unit factory.
    pub reconstructor: bool,
    /// Block centre world x (`Building.x`).
    pub cx: f32,
    /// Block centre world y (`Building.y`).
    pub cy: f32,
    /// `Block.size` in tiles (`Drawf.construct` line size = `size*8 - 4`).
    pub size: i32,
    /// `Building.rotation` (`0..=3`; drawn as `rotation * 90`).
    pub rotation: u8,
    /// Unit being constructed: the factory plan's unit or the reconstructor
    /// upgrade target (`None` = nothing to preview).
    pub unit: Option<mind_core::content::UnitTypeId>,
    /// Reconstructor payload unit (`Reconstructor.draw` icon at
    /// `1 - progress/constructTime`; the port tracks no payload rotation).
    pub payload: Option<mind_core::content::UnitTypeId>,
    /// `progress / plan.time` (factory) or `progress / constructTime`.
    pub fraction: f32,
    /// `efficiency > 0` with a valid plan/payload (Java `speedScl` target).
    pub working: bool,
}

/// `OverlayRenderer.drawTop` inputs (H03 T5).
pub struct OverlayView {
    /// Player (alpha) centre in world px.
    pub player: Option<(f32, f32)>,
    /// Player team id.
    pub player_team: u8,
    /// Non-player-team units `(x, y, team)`.
    pub enemy_units: Vec<(f32, f32, u8)>,
    /// Enemy core centres `(x, y, team)`.
    pub enemy_cores: Vec<(f32, f32, u8)>,
    /// Ground spawn tiles in world px.
    pub spawns: Vec<(f32, f32)>,
}

/// `Building.drawConfigure` dispatch (`BuildingComp.java:1732-1737`).
/// H06 T12 constructs `Logic`; `BridgeLink::range` stays for its use.
#[allow(dead_code)]
pub(crate) enum ConfigOverlay {
    /// Generic accent square (half-size `size * 4 + 1`).
    Generic { x: f32, y: f32, size: i32 },
    /// `PowerNode.drawConfigure` (`PowerNode.java:458-480`): pulsing accent
    /// rings at the node and at `laserRange`, one `Pal.place` square per link.
    PowerNodeLinks {
        x: f32,
        y: f32,
        size: i32,
        laser_range: f32,
        links: Vec<(f32, f32, i32)>,
    },
    /// `ItemBridge.drawConfigure` (`ItemBridge.java:282-296`): accent/place/
    /// breakInvalid squares on the candidate targets.
    BridgeLink {
        x: f32,
        y: f32,
        size: i32,
        range: i32,
        targets: Vec<(f32, f32, bool)>,
    },
    /// `LogicBuild.drawConfigure` (`LogicBlock.java:661-683`): the non-privileged
    /// link-range circles, one `Pal.place` square per valid link and the link
    /// names above them.
    Logic {
        /// Building centre x/y (world px).
        x: f32,
        y: f32,
        /// `Block.size` (the `super.drawConfigure()` outline).
        size: i32,
        /// `LogicBlock.range` circles (non-privileged only; `None` = none).
        range: Option<f32>,
        /// Valid links: `(centre x, centre y, block size, link name)`.
        links: Vec<(f32, f32, i32, String)>,
    },
}

/// `Building.drawSelect` dispatch (`OverlayRenderer.java:213-229`).
/// H06 T12 constructs `MessageText`/`LogicTag`.
#[allow(dead_code)]
pub(crate) enum SelectOverlay {
    /// Range circle(s) (`Drawf.dashCircle` / `Drawf.circles`).
    Range {
        x: f32,
        y: f32,
        range: f32,
        color: u32,
        circles: bool,
    },
    /// `MessageBlock.drawSelect` (`MessageBlock.java`): the stored message
    /// above the block.
    MessageText {
        /// Building centre x/y (world px).
        x: f32,
        y: f32,
        /// The message text.
        text: String,
    },
    /// `LogicBuild.drawSelect` (`LogicBlock.java:685-725`): the world-processor
    /// tag/icon and one square per unit this processor drives.
    LogicTag {
        /// Building centre x/y (world px).
        x: f32,
        y: f32,
        /// World-processor tag text (drawn above the block).
        tag: Option<String>,
        /// World-processor icon tag: `(Fonts.unicodeToName, UI region name)`.
        icon: Option<(String, String)>,
        /// Driven units: `(x, y, hit size, rotation)` (`LogicAI.controller == e`).
        units: Vec<(f32, f32, f32, f32)>,
    },
    /// No overlay.
    None,
}
