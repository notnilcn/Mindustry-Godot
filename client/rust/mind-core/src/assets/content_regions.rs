// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: annotations/src/main/java/mindustry/annotations/misc/LoadRegionProcessor.java
//         + every core/src/mindustry/world/blocks/**/*.java `@Load` field (and `world/Block.java`).

//! Content-driven `@Load` region audit (plan 03 §M6).
//!
//! One `#[derive(LoadRegions)]` struct per upstream block class that declares
//! `@Load` fields. [`load_block_regions`] walks a block's class chain exactly
//! like the generated `ContentRegions.loadRegions` (`content instanceof X` per
//! declaring class), so inherited fields (`Block.teamRegion`/`customShadowRegion`,
//! `Drill` fields on `BeamDrill`, ...) load for every applicable block.
//!
//! These structs are the **audit view** used by `mind-headless assets regions`:
//! they are transient (never stored on the content records) and their resolved
//! `Option<Region>` values are discarded after the [`RegionAudit`] is folded in.
//! Plan 07 (`RegionSlots`) reuses this layout for render-time resolution.
//!
//! `@-shadow`/`@-team` on the base `Block` class use an explicit `fallback = "@"`
//! because upstream resolves them with a not-found (non-fatal) region; the port's
//! audit reserves the fatal `fallback = "error"` default for class-specific
//! regions that every instance of that class must ship.

use crate::assets::atlas::{AtlasIndex, Region};
use crate::assets::regions::{LoadCtx, LoadRegions, RegionAudit};
use crate::content::registries::blocks::BlockDef;

/// Loads every `@Load` region for a block's class chain into `audit`.
pub fn load_block_regions(block: &BlockDef, atlas: &AtlasIndex, audit: &mut RegionAudit) {
    let size = block.size.max(0) as u32;
    let mut ctx = LoadCtx {
        atlas,
        content_name: &block.name,
        size,
        indices: [0, 0],
    };
    let mut class = block.kind.name();
    loop {
        if let Some(loader) = class_loader(class) {
            loader(&mut ctx, audit);
        }
        match parent_of(class) {
            Some(parent) => class = parent,
            None => break,
        }
    }
}

/// Audits every block in `blocks` (order preserved; deterministic error list).
pub fn audit_block_regions(blocks: &[BlockDef], atlas: &AtlasIndex) -> RegionAudit {
    let mut audit = RegionAudit::new();
    for block in blocks {
        load_block_regions(block, atlas, &mut audit);
    }
    audit
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct AcceleratorRegions {
    #[load("@-launch-arrow", fallback = "launch-arrow")]
    pub arrow_region: Option<Region>,
    #[load("select-arrow-small")]
    pub select_arrow_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct AutoDoorRegions {
    #[load("@-open")]
    pub open_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct BeamDrillRegions {
    #[load("@-glow")]
    pub glow_region: Option<Region>,
    #[load("@-beam", fallback = "drill-laser")]
    pub laser: Option<Region>,
    #[load("@-beam-boost", fallback = "drill-laser-boost")]
    pub laser_boost: Option<Region>,
    #[load("@-beam-center", fallback = "drill-laser-center")]
    pub laser_center: Option<Region>,
    #[load("@-beam-boost-center", fallback = "drill-laser-boost-center")]
    pub laser_center_boost: Option<Region>,
    #[load("@-beam-end", fallback = "drill-laser-end")]
    pub laser_end: Option<Region>,
    #[load("@-beam-boost-end", fallback = "drill-laser-boost-end")]
    pub laser_end_boost: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct BeamNodeRegions {
    #[load("@-beam", fallback = "power-beam")]
    pub laser: Option<Region>,
    #[load("@-beam-end", fallback = "power-beam-end")]
    pub laser_end: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct BlockRegions {
    #[load("@-shadow", fallback = "@")]
    pub custom_shadow_region: Option<Region>,
    #[load("@-team", fallback = "@")]
    pub team_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct BuildTurretRegions {
    #[load("@-base", fallback = "block-@size")]
    pub base_region: Option<Region>,
    #[load("@-glow")]
    pub glow_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct BurstDrillRegions {
    #[load("@-arrow-blur")]
    pub arrow_blur_region: Option<Region>,
    #[load("@-arrow")]
    pub arrow_region: Option<Region>,
    #[load("@-glow")]
    pub glow_region: Option<Region>,
    #[load("@-top-invert")]
    pub top_invert_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct CanvasBlockRegions {
    #[load("@-corner1")]
    pub corner1: Option<Region>,
    #[load("@-corner2")]
    pub corner2: Option<Region>,
    #[load("@-side1")]
    pub side1: Option<Region>,
    #[load("@-side2")]
    pub side2: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct CharacterOverlayRegions {
    #[load("character-overlay#", length = 64)]
    pub letter_regions: Vec<Option<Region>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct CliffRegions {
    #[load("cliffmask#", length = 256)]
    pub cliffs: Vec<Option<Region>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ConduitRegions {
    #[load("@-bottom-#", fallback = "conduit-bottom-#", length = 5)]
    pub bot_regions: Vec<Option<Region>>,
    #[load("@-cap")]
    pub cap_region: Option<Region>,
    #[load("@-top-#", length = 5)]
    pub top_regions: Vec<Option<Region>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ConveyorRegions {
    #[load("@-#1-#2", lengths = [7, 4])]
    pub regions: Vec<Vec<Option<Region>>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct CoreBlockRegions {
    #[load("@-thruster1", fallback = "clear-effect")]
    pub thruster1: Option<Region>,
    #[load("@-thruster2", fallback = "clear-effect")]
    pub thruster2: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DirectionBridgeRegions {
    #[load("@-arrow")]
    pub arrow_region: Option<Region>,
    #[load("@-bridge-bottom")]
    pub bridge_bot_region: Option<Region>,
    #[load("@-bridge-liquid")]
    pub bridge_liquid_region: Option<Region>,
    #[load("@-bridge")]
    pub bridge_region: Option<Region>,
    #[load("@-dir")]
    pub dir_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DirectionLiquidBridgeRegions {
    #[load("@-bottom")]
    pub bottom_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DirectionalUnloaderRegions {
    #[load("@-arrow")]
    pub arrow_region: Option<Region>,
    #[load("@-center", fallback = "unloader-center")]
    pub center_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DoorRegions {
    #[load("@-open")]
    pub open_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DrillRegions {
    #[load("@-item", fallback = "drill-item-@size")]
    pub item_region: Option<Region>,
    #[load("@-rim")]
    pub rim_region: Option<Region>,
    #[load("@-rotator")]
    pub rotator_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DuctRegions {
    #[load("@-bottom-#", fallback = "duct-bottom-#", length = 5)]
    pub bot_regions: Vec<Option<Region>>,
    #[load("@-cap")]
    pub cap_region: Option<Region>,
    #[load("@-top-#", length = 5)]
    pub top_regions: Vec<Option<Region>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DuctJunctionRegions {
    #[load("@-bottom")]
    pub bottom_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct DuctRouterRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ForceProjectorRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ItemBridgeRegions {
    #[load("@-arrow")]
    pub arrow_region: Option<Region>,
    #[load("@-bridge")]
    pub bridge_region: Option<Region>,
    #[load("@-end")]
    pub end_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ItemIncineratorRegions {
    #[load("@-liquid")]
    pub liquid_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct LandingPadRegions {
    #[load("@-pod", fallback = "advanced-launch-pad-pod")]
    pub pod_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct LaunchPadRegions {
    #[load("@-light")]
    pub light_region: Option<Region>,
    #[load("@-pod", fallback = "launchpod")]
    pub pod_region: Option<Region>,
    #[load("@-preview", fallback = "@")]
    pub preview_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct LightBlockRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct LiquidBlockRegions {
    #[load("@-bottom")]
    pub bottom_region: Option<Region>,
    #[load("@-liquid")]
    pub liquid_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct LiquidSourceRegions {
    #[load("source-bottom")]
    pub bottom_region: Option<Region>,
    #[load("cross")]
    pub cross_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct LongPowerNodeRegions {
    #[load("@-glow")]
    pub glow: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct MassDriverRegions {
    #[load("@-base")]
    pub base_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct MendProjectorRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct NuclearReactorRegions {
    #[load("@-lights")]
    pub lights_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct OverdriveProjectorRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct OverflowDuctRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PayloadConveyorRegions {
    #[load("@-edge")]
    pub edge_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PayloadLoaderRegions {
    #[load("@-over")]
    pub over_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PayloadMassDriverRegions {
    #[load("bridge-arrow")]
    pub arrow: Option<Region>,
    #[load("@-base")]
    pub base_region: Option<Region>,
    #[load("@-cap-outline")]
    pub cap_outline_region: Option<Region>,
    #[load("@-cap")]
    pub cap_region: Option<Region>,
    #[load("@-left-outline")]
    pub left_outline_region: Option<Region>,
    #[load("@-left")]
    pub left_region: Option<Region>,
    #[load("@-right-outline")]
    pub right_outline_region: Option<Region>,
    #[load("@-right")]
    pub right_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PayloadRouterRegions {
    #[load("@-over")]
    pub over_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PointDefenseTurretRegions {
    #[load("@-base", fallback = "block-@size")]
    pub base_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PowerDiodeRegions {
    #[load("@-arrow")]
    pub arrow: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct PowerNodeRegions {
    #[load("@-laser", fallback = "laser")]
    pub laser: Option<Region>,
    #[load("@-laser-end", fallback = "laser-end")]
    pub laser_end: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct RadarRegions {
    #[load("@-base")]
    pub base_region: Option<Region>,
    #[load("@-glow")]
    pub glow_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct RepairTowerRegions {
    #[load("@-glow")]
    pub glow: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct RepairTurretRegions {
    #[load("@-base", fallback = "block-@size")]
    pub base_region: Option<Region>,
    #[load("laser-white")]
    pub laser: Option<Region>,
    #[load("laser-white-end")]
    pub laser_end: Option<Region>,
    #[load("laser-top")]
    pub laser_top: Option<Region>,
    #[load("laser-top-end")]
    pub laser_top_end: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct RuneOverlayRegions {
    #[load("@#", fallback = "rune-overlay#", length = 109)]
    pub letter_regions: Vec<Option<Region>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct SeaBushRegions {
    #[load("@-bot", fallback = "@")]
    pub bot_region: Option<Region>,
    #[load("@-center")]
    pub center_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ShieldWallRegions {
    #[load("@-glow")]
    pub glow_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ShockMineRegions {
    #[load("@-team-top")]
    pub team_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ShockwaveTowerRegions {
    #[load("@-heat")]
    pub heat_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct SolidPumpRegions {
    #[load("@-rotator")]
    pub rotator_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct SorterRegions {
    #[load("@-cross", fallback = "cross-full")]
    pub cross: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct StackConveyorRegions {
    #[load("@-edge-glow", fallback = "@-glow")]
    pub edge_glow_region: Option<Region>,
    #[load("@-edge")]
    pub edge_region: Option<Region>,
    #[load("@-glow")]
    pub glow_region: Option<Region>,
    #[load("@-#", length = 3)]
    pub regions: Vec<Option<Region>>,
    #[load("@-stack")]
    pub stack_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct StackRouterRegions {
    #[load("@-glow", fallback = "arrow-glow")]
    pub glow_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct StaticWallRegions {
    #[load("@-large")]
    pub large: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct SwitchBlockRegions {
    #[load("@-on")]
    pub on_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct TargetDummyRegions {
    #[load("@-preview", fallback = "@")]
    pub preview_region: Option<Region>,
    #[load("@-tether", fallback = "@")]
    pub tether: Option<Region>,
    #[load("@-tether-end", fallback = "@")]
    pub tether_end: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct ThrusterRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct TileableLogicDisplayRegions {
    #[load("@-back")]
    pub back_region: Option<Region>,
    #[load("@-#", length = 47)]
    pub tile_region: Vec<Option<Region>>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct TractorBeamTurretRegions {
    #[load("@-base", fallback = "block-@size")]
    pub base_region: Option<Region>,
    #[load("@-laser")]
    pub laser: Option<Region>,
    #[load("@-laser-end")]
    pub laser_end: Option<Region>,
    #[load("@-laser-start", fallback = "@-laser-end")]
    pub laser_start: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct TreeBlockRegions {
    #[load("@-shadow")]
    pub shadow: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct UnitAssemblerRegions {
    #[load("@-side1")]
    pub side_region1: Option<Region>,
    #[load("@-side2")]
    pub side_region2: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct UnitAssemblerModuleRegions {
    #[load("@-side1")]
    pub side_region1: Option<Region>,
    #[load("@-side2")]
    pub side_region2: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct UnitCargoUnloadPointRegions {
    #[load("@-top")]
    pub top_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct UnloaderRegions {
    #[load("@-center", fallback = "unloader-center")]
    pub center_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct VariableReactorRegions {
    #[load("@-lights")]
    pub lights_region: Option<Region>,
}

#[derive(Default, mind_macros::LoadRegions)]
pub struct WallCrafterRegions {
    #[load("@-rotator-bottom")]
    pub rotator_bottom_region: Option<Region>,
    #[load("@-rotator")]
    pub rotator_region: Option<Region>,
    #[load("@-top")]
    pub top_region: Option<Region>,
}

/// Factory for the region struct declared by a Java class name.
fn class_loader(class: &str) -> Option<fn(&mut LoadCtx, &mut RegionAudit)> {
    Some(match class {
        "Accelerator" => |ctx, audit| {
            let mut regions = AcceleratorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "AutoDoor" => |ctx, audit| {
            let mut regions = AutoDoorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "BeamDrill" => |ctx, audit| {
            let mut regions = BeamDrillRegions::default();
            regions.load_regions(ctx, audit);
        },
        "BeamNode" => |ctx, audit| {
            let mut regions = BeamNodeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Block" => |ctx, audit| {
            let mut regions = BlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "BuildTurret" => |ctx, audit| {
            let mut regions = BuildTurretRegions::default();
            regions.load_regions(ctx, audit);
        },
        "BurstDrill" => |ctx, audit| {
            let mut regions = BurstDrillRegions::default();
            regions.load_regions(ctx, audit);
        },
        "CanvasBlock" => |ctx, audit| {
            let mut regions = CanvasBlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "CharacterOverlay" => |ctx, audit| {
            let mut regions = CharacterOverlayRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Cliff" => |ctx, audit| {
            let mut regions = CliffRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Conduit" => |ctx, audit| {
            let mut regions = ConduitRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Conveyor" => |ctx, audit| {
            let mut regions = ConveyorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "CoreBlock" => |ctx, audit| {
            let mut regions = CoreBlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "DirectionBridge" => |ctx, audit| {
            let mut regions = DirectionBridgeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "DirectionLiquidBridge" => |ctx, audit| {
            let mut regions = DirectionLiquidBridgeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "DirectionalUnloader" => |ctx, audit| {
            let mut regions = DirectionalUnloaderRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Door" => |ctx, audit| {
            let mut regions = DoorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Drill" => |ctx, audit| {
            let mut regions = DrillRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Duct" => |ctx, audit| {
            let mut regions = DuctRegions::default();
            regions.load_regions(ctx, audit);
        },
        "DuctJunction" => |ctx, audit| {
            let mut regions = DuctJunctionRegions::default();
            regions.load_regions(ctx, audit);
        },
        "DuctRouter" => |ctx, audit| {
            let mut regions = DuctRouterRegions::default();
            regions.load_regions(ctx, audit);
        },
        "ForceProjector" => |ctx, audit| {
            let mut regions = ForceProjectorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "ItemBridge" => |ctx, audit| {
            let mut regions = ItemBridgeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "ItemIncinerator" => |ctx, audit| {
            let mut regions = ItemIncineratorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "LandingPad" => |ctx, audit| {
            let mut regions = LandingPadRegions::default();
            regions.load_regions(ctx, audit);
        },
        "LaunchPad" => |ctx, audit| {
            let mut regions = LaunchPadRegions::default();
            regions.load_regions(ctx, audit);
        },
        "LightBlock" => |ctx, audit| {
            let mut regions = LightBlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "LiquidBlock" => |ctx, audit| {
            let mut regions = LiquidBlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "LiquidSource" => |ctx, audit| {
            let mut regions = LiquidSourceRegions::default();
            regions.load_regions(ctx, audit);
        },
        "LongPowerNode" => |ctx, audit| {
            let mut regions = LongPowerNodeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "MassDriver" => |ctx, audit| {
            let mut regions = MassDriverRegions::default();
            regions.load_regions(ctx, audit);
        },
        "MendProjector" => |ctx, audit| {
            let mut regions = MendProjectorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "NuclearReactor" => |ctx, audit| {
            let mut regions = NuclearReactorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "OverdriveProjector" => |ctx, audit| {
            let mut regions = OverdriveProjectorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "OverflowDuct" => |ctx, audit| {
            let mut regions = OverflowDuctRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PayloadConveyor" => |ctx, audit| {
            let mut regions = PayloadConveyorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PayloadLoader" => |ctx, audit| {
            let mut regions = PayloadLoaderRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PayloadMassDriver" => |ctx, audit| {
            let mut regions = PayloadMassDriverRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PayloadRouter" => |ctx, audit| {
            let mut regions = PayloadRouterRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PointDefenseTurret" => |ctx, audit| {
            let mut regions = PointDefenseTurretRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PowerDiode" => |ctx, audit| {
            let mut regions = PowerDiodeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "PowerNode" => |ctx, audit| {
            let mut regions = PowerNodeRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Radar" => |ctx, audit| {
            let mut regions = RadarRegions::default();
            regions.load_regions(ctx, audit);
        },
        "RepairTower" => |ctx, audit| {
            let mut regions = RepairTowerRegions::default();
            regions.load_regions(ctx, audit);
        },
        "RepairTurret" => |ctx, audit| {
            let mut regions = RepairTurretRegions::default();
            regions.load_regions(ctx, audit);
        },
        "RuneOverlay" => |ctx, audit| {
            let mut regions = RuneOverlayRegions::default();
            regions.load_regions(ctx, audit);
        },
        "SeaBush" => |ctx, audit| {
            let mut regions = SeaBushRegions::default();
            regions.load_regions(ctx, audit);
        },
        "ShieldWall" => |ctx, audit| {
            let mut regions = ShieldWallRegions::default();
            regions.load_regions(ctx, audit);
        },
        "ShockMine" => |ctx, audit| {
            let mut regions = ShockMineRegions::default();
            regions.load_regions(ctx, audit);
        },
        "ShockwaveTower" => |ctx, audit| {
            let mut regions = ShockwaveTowerRegions::default();
            regions.load_regions(ctx, audit);
        },
        "SolidPump" => |ctx, audit| {
            let mut regions = SolidPumpRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Sorter" => |ctx, audit| {
            let mut regions = SorterRegions::default();
            regions.load_regions(ctx, audit);
        },
        "StackConveyor" => |ctx, audit| {
            let mut regions = StackConveyorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "StackRouter" => |ctx, audit| {
            let mut regions = StackRouterRegions::default();
            regions.load_regions(ctx, audit);
        },
        "StaticWall" => |ctx, audit| {
            let mut regions = StaticWallRegions::default();
            regions.load_regions(ctx, audit);
        },
        "SwitchBlock" => |ctx, audit| {
            let mut regions = SwitchBlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "TargetDummy" => |ctx, audit| {
            let mut regions = TargetDummyRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Thruster" => |ctx, audit| {
            let mut regions = ThrusterRegions::default();
            regions.load_regions(ctx, audit);
        },
        "TileableLogicDisplay" => |ctx, audit| {
            let mut regions = TileableLogicDisplayRegions::default();
            regions.load_regions(ctx, audit);
        },
        "TractorBeamTurret" => |ctx, audit| {
            let mut regions = TractorBeamTurretRegions::default();
            regions.load_regions(ctx, audit);
        },
        "TreeBlock" => |ctx, audit| {
            let mut regions = TreeBlockRegions::default();
            regions.load_regions(ctx, audit);
        },
        "UnitAssembler" => |ctx, audit| {
            let mut regions = UnitAssemblerRegions::default();
            regions.load_regions(ctx, audit);
        },
        "UnitAssemblerModule" => |ctx, audit| {
            let mut regions = UnitAssemblerModuleRegions::default();
            regions.load_regions(ctx, audit);
        },
        "UnitCargoUnloadPoint" => |ctx, audit| {
            let mut regions = UnitCargoUnloadPointRegions::default();
            regions.load_regions(ctx, audit);
        },
        "Unloader" => |ctx, audit| {
            let mut regions = UnloaderRegions::default();
            regions.load_regions(ctx, audit);
        },
        "VariableReactor" => |ctx, audit| {
            let mut regions = VariableReactorRegions::default();
            regions.load_regions(ctx, audit);
        },
        "WallCrafter" => |ctx, audit| {
            let mut regions = WallCrafterRegions::default();
            regions.load_regions(ctx, audit);
        },
        _ => return None,
    })
}

/// Java `extends` parent for the classes this audit can reach.
fn parent_of(class: &str) -> Option<&'static str> {
    Some(match class {
        "Accelerator" => "Block",
        "AirBlock" => "Floor",
        "ArmoredConduit" => "Conduit",
        "ArmoredConveyor" => "Conveyor",
        "AttributeCrafter" => "GenericCrafter",
        "AutoDoor" => "Wall",
        "BaseShield" => "Block",
        "BaseTurret" => "Block",
        "Battery" => "PowerDistributor",
        "BeamDrill" => "Block",
        "BeamNode" => "PowerBlock",
        "Block" => "UnlockableContent",
        "BlockProducer" => "PayloadBlock",
        "BufferedItemBridge" => "ItemBridge",
        "BuildTurret" => "BaseTurret",
        "BurstDrill" => "Drill",
        "CanvasBlock" => "Block",
        "CharacterOverlay" => "OverlayFloor",
        "Cliff" => "Block",
        "ColoredFloor" => "Floor",
        "ColoredWall" => "StaticWall",
        "Conduit" => "LiquidBlock",
        "ConstructBlock" => "Block",
        "Constructor" => "BlockProducer",
        "ConsumeGenerator" => "PowerGenerator",
        "ContinuousLiquidTurret" => "ContinuousTurret",
        "ContinuousTurret" => "Turret",
        "Conveyor" => "Block",
        "CoreBlock" => "StorageBlock",
        "DirectionBridge" => "Block",
        "DirectionLiquidBridge" => "DirectionBridge",
        "DirectionalUnloader" => "Block",
        "Door" => "Wall",
        "Drill" => "Block",
        "Duct" => "Block",
        "DuctBridge" => "DirectionBridge",
        "DuctJunction" => "Block",
        "DuctRouter" => "Block",
        "EmptyFloor" => "Floor",
        "Floor" => "Block",
        "ForceProjector" => "Block",
        "Fracker" => "SolidPump",
        "GenericCrafter" => "Block",
        "HeatConductor" => "Block",
        "HeatCrafter" => "GenericCrafter",
        "HeatProducer" => "GenericCrafter",
        "HeaterGenerator" => "ConsumeGenerator",
        "ImpactReactor" => "PowerGenerator",
        "Incinerator" => "Block",
        "ItemBridge" => "Block",
        "ItemIncinerator" => "Block",
        "ItemSource" => "Block",
        "ItemTurret" => "Turret",
        "ItemVoid" => "Block",
        "Junction" => "Block",
        "LandingPad" => "Block",
        "LaserTurret" => "PowerTurret",
        "LaunchPad" => "Block",
        "LegacyBlock" => "Block",
        "LegacyCommandCenter" => "LegacyBlock",
        "LegacyMechPad" => "LegacyBlock",
        "LegacyUnitFactory" => "LegacyBlock",
        "LightBlock" => "Block",
        "LiquidBlock" => "Block",
        "LiquidBridge" => "ItemBridge",
        "LiquidJunction" => "LiquidBlock",
        "LiquidRouter" => "LiquidBlock",
        "LiquidSource" => "Block",
        "LiquidTurret" => "Turret",
        "LiquidVoid" => "Block",
        "LogicBlock" => "Block",
        "LogicDisplay" => "Block",
        "LongPowerNode" => "PowerNode",
        "MappableContent" => "Content",
        "MassDriver" => "Block",
        "MemoryBlock" => "Block",
        "MendProjector" => "Block",
        "MessageBlock" => "Block",
        "NuclearReactor" => "PowerGenerator",
        "OreBlock" => "OverlayFloor",
        "OverdriveProjector" => "Block",
        "OverflowDuct" => "Block",
        "OverflowGate" => "Block",
        "OverlayFloor" => "Floor",
        "PayloadBlock" => "Block",
        "PayloadConveyor" => "Block",
        "PayloadDeconstructor" => "PayloadBlock",
        "PayloadLoader" => "PayloadBlock",
        "PayloadMassDriver" => "PayloadBlock",
        "PayloadRouter" => "PayloadConveyor",
        "PayloadSource" => "PayloadBlock",
        "PayloadUnloader" => "PayloadLoader",
        "PayloadVoid" => "PayloadBlock",
        "PointDefenseTurret" => "ReloadTurret",
        "PowerBlock" => "Block",
        "PowerDiode" => "Block",
        "PowerDistributor" => "PowerBlock",
        "PowerGenerator" => "PowerDistributor",
        "PowerNode" => "PowerBlock",
        "PowerSource" => "PowerNode",
        "PowerTurret" => "Turret",
        "PowerVoid" => "PowerBlock",
        "Prop" => "Block",
        "Pump" => "LiquidBlock",
        "Radar" => "Block",
        "Reconstructor" => "UnitBlock",
        "RegenProjector" => "Block",
        "ReloadTurret" => "BaseTurret",
        "RemoveOre" => "OverlayFloor",
        "RemoveWall" => "Block",
        "RepairTower" => "Block",
        "RepairTurret" => "Block",
        "Router" => "Block",
        "RuneOverlay" => "OverlayFloor",
        "SeaBush" => "Prop",
        "Seaweed" => "Prop",
        "Separator" => "Block",
        "ShallowLiquid" => "Floor",
        "ShieldWall" => "Wall",
        "ShockMine" => "Block",
        "ShockwaveTower" => "Block",
        "SolarGenerator" => "PowerGenerator",
        "SolidPump" => "Pump",
        "Sorter" => "Block",
        "SpawnBlock" => "OverlayFloor",
        "StackConveyor" => "Block",
        "StackRouter" => "DuctRouter",
        "StaticProp" => "Prop",
        "StaticTree" => "StaticWall",
        "StaticWall" => "Prop",
        "SteamVent" => "Floor",
        "StorageBlock" => "Block",
        "SwitchBlock" => "Block",
        "TallBlock" => "Block",
        "TargetDummy" => "Block",
        "ThermalGenerator" => "PowerGenerator",
        "Thruster" => "Wall",
        "TileableLogicDisplay" => "LogicDisplay",
        "TractorBeamTurret" => "BaseTurret",
        "TreeBlock" => "Block",
        "Turret" => "ReloadTurret",
        "UnitAssembler" => "PayloadBlock",
        "UnitAssemblerModule" => "PayloadBlock",
        "UnitBlock" => "PayloadBlock",
        "UnitCargoLoader" => "Block",
        "UnitCargoUnloadPoint" => "Block",
        "UnitFactory" => "UnitBlock",
        "Unloader" => "Block",
        "UnlockableContent" => "MappableContent",
        "VariableReactor" => "PowerGenerator",
        "Wall" => "Block",
        "WallCrafter" => "Block",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
  "format": 1,
  "pages": [{"index":0,"type":"main","file":"sprites.png","width":64,"height":64,"sha256":""}],
  "regions": [
    {"name":"foo","page":0,"x":0,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-team-0","page":0,"x":8,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-team-1","page":0,"x":16,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-grid-0-0","page":0,"x":0,"y":8,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-grid-0-1","page":0,"x":8,"y":8,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-grid-1-0","page":0,"x":16,"y":8,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-grid-1-1","page":0,"x":24,"y":8,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"fb","page":0,"x":24,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"error","page":0,"x":32,"y":0,"w":3,"h":3,"offsets":[0,0],"pageType":"main"}
  ]
}"#;

    /// Mirrors a `LoadRegionProcessor` field set: scalar, array with `length`,
    /// positional/`value` patterns and an explicit `fallback`.
    #[derive(Default, mind_macros::LoadRegions)]
    struct FixtureRegions {
        #[load("@")]
        region: Option<Region>,
        #[load("@-missing")]
        missing: Option<Region>,
        #[load("@-team-#", length = 2)]
        teams: Vec<Option<Region>>,
        #[load("@-grid-#1-#2", lengths = [2, 2])]
        grid: Vec<Vec<Option<Region>>>,
        #[load(value = "@-gone", fallback = "fb")]
        fallback: Option<Region>,
    }

    #[test]
    fn derive_loads_scalars_arrays_and_records_audit() {
        let index = AtlasIndex::from_manifest_json(FIXTURE).unwrap();
        let mut ctx = LoadCtx {
            atlas: &index,
            content_name: "foo",
            size: 1,
            indices: [0, 0],
        };
        let mut audit = RegionAudit::new();
        let mut regions = FixtureRegions::default();
        regions.load_regions(&mut ctx, &mut audit);

        // Scalar hit.
        assert_eq!(regions.region.as_ref().unwrap().name, "foo");
        // Default (`fallback=error`) miss: Arc `find(name)` bridges to `error`
        // and the port records it in the fatal `errors` bucket.
        assert_eq!(regions.missing.as_ref().unwrap().name, "error");
        assert_eq!(audit.errors, vec!["foo-missing".to_owned()]);
        // Array `#` index templating in declaration order.
        assert_eq!(regions.teams[0].as_ref().unwrap().name, "foo-team-0");
        assert_eq!(regions.teams[1].as_ref().unwrap().name, "foo-team-1");
        // Two-dimensional `#1`/`#2` loops (outer = dimension 0).
        assert_eq!(regions.grid[0][0].as_ref().unwrap().name, "foo-grid-0-0");
        assert_eq!(regions.grid[0][1].as_ref().unwrap().name, "foo-grid-0-1");
        assert_eq!(regions.grid[1][0].as_ref().unwrap().name, "foo-grid-1-0");
        assert_eq!(regions.grid[1][1].as_ref().unwrap().name, "foo-grid-1-1");
        // Explicit fallback resolves, recorded as recoverable.
        assert_eq!(regions.fallback.as_ref().unwrap().name, "fb");
        assert_eq!(audit.explicit_fallbacks, vec!["foo-gone".to_owned()]);
        assert!(audit.has_errors());
    }
}
