// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Generator-facing content metadata (plan 03 §5 M3 "content metadata
// contract"): `game/Team.java` (static team table), `world/Block.java`
// (`icons`/`makeIconRegions`/`getRegionsToOutline`/outline fields),
// `content/Blocks.java` (per-block variants/blend groups/autotile flags and
// drawer compositions), and the per-class icon overrides in
// `world/blocks/**` + `world/draw/**`. Extracted from the upstream sources
// cited per table; plan 12 owns the runtime `Team` port, plans 07/16/17 the
// `DrawBlock` drawer framework (see the plan Changelog for the handshake).
//
// Unit metadata (`UnitType.getRegionsToOutline`, weapon/part/tread/segment
// tables) was completed once plan 02 M5 merged (see [`part_outline_regions`],
// [`unit_region_expectations`], and `generate::units`).

use mind_core::content::registries::blocks::BlockKind;
use mind_core::content::registries::units::UnitTypeDef;
use mind_core::content::registries::units::parts::{DrawPartKind, DrawPartSpec};

/// `Vars.tilesize`.
pub const TILE_SIZE: i32 = 8;

/// Static team data for generators (`game/Team.java`).
#[derive(Debug, Clone, Copy)]
pub struct TeamSpec {
    /// Team id (`Team.id`).
    pub id: u8,
    /// Team name (`Team.name`).
    pub name: &'static str,
    /// Team color (`Team.color`).
    pub color: u32,
    /// `palettei[0..3]` (magic-color recolor targets).
    pub palette: [u32; 3],
    /// `hasPalette`.
    pub has_palette: bool,
}

/// One channel of `Color.mul(factor)` in u8 space (`(int)(channel/255*factor*255)`).
const fn mul_channel(channel: u32, factor: f32) -> u32 {
    (channel as f32 / 255.0 * factor * 255.0) as u32
}

/// `setPalette(color)` derivation: `[color, color*0.75, color*0.5]`
/// (`hasPalette = false`).
const fn derive_palette(color: u32) -> [u32; 3] {
    let r = (color >> 24) & 0xff;
    let g = (color >> 16) & 0xff;
    let b = (color >> 8) & 0xff;
    [
        color,
        channel_triple(r, g, b, 0.75),
        channel_triple(r, g, b, 0.5),
    ]
}

/// Builds `(r,g,b)` scaled by `factor` with opaque alpha.
const fn channel_triple(r: u32, g: u32, b: u32, factor: f32) -> u32 {
    (mul_channel(r, factor) << 24)
        | (mul_channel(g, factor) << 16)
        | (mul_channel(b, factor) << 8)
        | 0xff
}

/// `Team.all` base teams in id order (palettes from `Team.<statics>`;
/// `setPalette(color)` = `[color, color*0.75, color*0.5]`, `hasPalette = false`).
pub const TEAMS: &[TeamSpec] = &[
    TeamSpec {
        id: 0,
        name: "derelict",
        color: 0x4d4e_58ff,
        palette: derive_palette(0x4d4e_58ff),
        has_palette: false,
    },
    TeamSpec {
        id: 1,
        name: "sharded",
        color: 0xffd3_7fff,
        palette: [0xffd3_7fff, 0xeab6_78ff, 0xd481_6bff],
        has_palette: true,
    },
    TeamSpec {
        id: 2,
        name: "crux",
        color: 0xf255_55ff,
        palette: [0xfc8e_6cff, 0xf255_55ff, 0xa045_53ff],
        has_palette: true,
    },
    TeamSpec {
        id: 3,
        name: "malis",
        color: 0xa27c_e5ff,
        palette: [0xc7a4_f5ff, 0x896f_d6ff, 0x504c_baff],
        has_palette: true,
    },
    TeamSpec {
        id: 4,
        name: "green",
        color: 0x54d6_7dff,
        palette: derive_palette(0x54d6_7dff),
        has_palette: false,
    },
    TeamSpec {
        id: 5,
        name: "blue",
        color: 0x6c87_fdff,
        palette: derive_palette(0x6c87_fdff),
        has_palette: false,
    },
    TeamSpec {
        id: 6,
        name: "neoplastic",
        color: 0xe054_38ff,
        palette: derive_palette(0xe054_38ff),
        has_palette: false,
    },
];

/// Team name of the default (sharded) team top overlay (`Team.sharded.id`).
pub const SHARDED_ID: u8 = 1;

/// `team-icons` tint override for `derelict` (`Color.valueOf("b7b8c9")`).
pub const DERELICT_ICON_COLOR: u32 = 0xb7b8_c9ff;

/// Looks up a team by name.
pub fn team_by_name(name: &str) -> Option<&'static TeamSpec> {
    TEAMS.iter().find(|team| team.name == name)
}

/// `Block.outlineColor` default (`Color.valueOf("404049")`).
pub const DEFAULT_OUTLINE_COLOR: u32 = 0x4040_49ff;
/// `Pal.darkOutline` (`Color.valueOf("2d2f39")`).
pub const DARK_OUTLINE: u32 = 0x2d2f_39ff;
/// `Pal.gray` (`Color.valueOf("454545")`).
pub const PAL_GRAY: u32 = 0x4545_45ff;
/// `Pal.darkerGray` (`new Color(0.2, 0.2, 0.2, 1)`).
pub const PAL_DARKER_GRAY: u32 = 0x3333_33ff;
/// `Block.outlineRadius` default (4; no vanilla block overrides it).
pub const DEFAULT_OUTLINE_RADIUS: i32 = 4;

/// `Block.outlineColor` per-block overrides (`content/Blocks.java`; every
/// other block uses [`DEFAULT_OUTLINE_COLOR`]).
pub fn outline_color(block: &str) -> u32 {
    match block {
        "radar" => 0x4a4b_53ff,
        "breach" | "diffuse" | "sublimate" | "titan" | "disperse" | "afflict" | "lustre"
        | "scathe" | "smite" | "malign" => DARK_OUTLINE,
        _ => DEFAULT_OUTLINE_COLOR,
    }
}

/// `Block.outlineIcon` per class (upstream: `BaseTurret`, `Radar`,
/// `MassDriver`, `PayloadMassDriver`, `RepairTurret`).
pub fn outline_icon(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Radar
            | BlockKind::MassDriver
            | BlockKind::PayloadMassDriver
            | BlockKind::BuildTurret
            | BlockKind::RepairTurret
            | BlockKind::ItemTurret
            | BlockKind::LiquidTurret
            | BlockKind::PowerTurret
            | BlockKind::ContinuousTurret
            | BlockKind::ContinuousLiquidTurret
            | BlockKind::LaserTurret
            | BlockKind::PointDefenseTurret
            | BlockKind::TractorBeamTurret
    )
}

/// `Block.outlinedIcon` per class (`Turret` constructor sets 1; everything
/// else keeps the -1 default). Only meaningful when [`outline_icon`] is true.
pub fn outlined_icon(kind: BlockKind) -> i32 {
    match kind {
        BlockKind::ItemTurret
        | BlockKind::LiquidTurret
        | BlockKind::PowerTurret
        | BlockKind::ContinuousTurret
        | BlockKind::ContinuousLiquidTurret
        | BlockKind::LaserTurret => 1,
        _ => -1,
    }
}

/// Whether the kind is a `Turret` subclass (`DrawTurret` drawer rules apply).
pub fn is_turret_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::ItemTurret
            | BlockKind::LiquidTurret
            | BlockKind::PowerTurret
            | BlockKind::ContinuousTurret
            | BlockKind::ContinuousLiquidTurret
            | BlockKind::LaserTurret
    )
}

/// Whether the kind is a `Floor` subclass (`Floor.icons` + `block_colors`
/// floor multiplier).
pub fn is_floor_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Floor
            | BlockKind::EmptyFloor
            | BlockKind::OverlayFloor
            | BlockKind::OreBlock
            | BlockKind::ShallowLiquid
            | BlockKind::ColoredFloor
            | BlockKind::CharacterOverlay
            | BlockKind::RuneOverlay
            | BlockKind::SpawnBlock
            | BlockKind::RemoveOre
    )
}

/// Whether the kind's `icons()` delegate to a `DrawBlock` drawer.
fn uses_drawer(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::GenericCrafter
            | BlockKind::AttributeCrafter
            | BlockKind::HeatCrafter
            | BlockKind::HeatProducer
            | BlockKind::Separator
            | BlockKind::HeatConductor
            | BlockKind::Battery
            | BlockKind::ConsumeGenerator
            | BlockKind::ThermalGenerator
            | BlockKind::SolarGenerator
            | BlockKind::NuclearReactor
            | BlockKind::ImpactReactor
            | BlockKind::BeamNode
            | BlockKind::LongPowerNode
            | BlockKind::VariableReactor
            | BlockKind::HeaterGenerator
            | BlockKind::Pump
            | BlockKind::RegenProjector
    )
}

/// Per-block generator metadata extracted from `content/Blocks.java`.
#[derive(Debug, Clone, Copy)]
pub struct BlockMeta {
    /// `variants` (0 disables the `<name>1..N` series).
    pub variants: u32,
    /// `Floor.autotile` / `StaticWall.autotile`.
    pub autotile: bool,
    /// `Floor.autotileVariants`.
    pub autotile_variants: u32,
    /// `Floor.blendGroup` content name (`""` = self).
    pub blend_group: &'static str,
    /// `Floor.drawEdgeOut`.
    pub draw_edge_out: bool,
    /// `Floor.drawEdgeIn`.
    pub draw_edge_in: bool,
    /// `ShallowLiquid.liquidBase` content name.
    pub shallow_liquid_base: Option<&'static str>,
    /// `ShallowLiquid.floorBase` content name.
    pub shallow_floor_base: Option<&'static str>,
    /// `ShallowLiquid.liquidOpacity`.
    pub shallow_liquid_opacity: f32,
    /// `Floor.wallOre`.
    pub wall_ore: bool,
}

impl Default for BlockMeta {
    fn default() -> Self {
        Self {
            variants: 0,
            autotile: false,
            autotile_variants: 0,
            blend_group: "",
            draw_edge_out: true,
            draw_edge_in: true,
            shallow_liquid_base: None,
            shallow_floor_base: None,
            shallow_liquid_opacity: 0.35,
            wall_ore: false,
        }
    }
}

/// Class-default `variants` from the upstream constructors
/// (`Floor(name)` = 3, `StaticWall(name)` = 2, `StaticTree`/`SeaBush` = 0, …).
fn default_variants(kind: BlockKind) -> u32 {
    match kind {
        BlockKind::StaticWall | BlockKind::ColoredWall | BlockKind::SteamVent => 2,
        kind if is_floor_kind(kind) => 3,
        _ => 0,
    }
}

/// Extracted table (`content/Blocks.java`, this checkout). Names not listed
/// keep the class default from [`default_variants`]; autotile floors take 0
/// (`Floor.load`).
pub fn block_meta(name: &str, kind: BlockKind) -> BlockMeta {
    let mut meta = BlockMeta {
        variants: default_variants(kind),
        ..BlockMeta::default()
    };

    // Explicit variants (`variants = N` / constructor args in Blocks.java).
    match name {
        "metal-floor" | "metal-floor-2" | "metal-floor-3" | "metal-floor-4" | "metal-floor-5"
        | "dark-panel-1" | "dark-panel-2" | "dark-panel-3" | "dark-panel-4" | "dark-panel-5"
        | "dark-panel-6" | "space" | "deep-water" | "shallow-water" | "tainted-water"
        | "deep-tainted-water" | "tar" | "pooled-cryofluid" | "molten-slag" | "arkycite-floor"
        | "salt" | "core-zone" => meta.variants = 0,
        "metal-floor-damaged"
        | "crater-stone"
        | "mud"
        | "rough-rhyolite"
        | "ferric-craters"
        | "yellow-stone-plates"
        | "arkyic-stone"
        | "ice-snow"
        | "shale"
        | "moss"
        | "spore-moss"
        | "arkyic-wall"
        | "red-diamond-wall"
        | "spore-cluster"
        | "redweed"
        | "arkyic-boulder"
        | "crystal-cluster"
        | "vibrant-crystal-cluster"
        | "crystal-blocks"
        | "crystal-orbs"
        | "red-ice-boulder"
        | "graphitic-wall"
        | "rhyolite-boulder"
        | "scrap-wall-huge" => meta.variants = 3,
        "carbon-stone"
        | "beryllic-stone"
        | "crystal-floor"
        | "red-stone"
        | "dense-red-stone"
        | "crystalline-stone-wall"
        | "red-stone-boulder"
        | "scrap-wall-large" => {
            meta.variants = 4;
        }
        "crystalline-stone" => meta.variants = 5,
        "boulder"
        | "snow-boulder"
        | "shale-boulder"
        | "sand-boulder"
        | "dacite-boulder"
        | "basalt-boulder"
        | "carbon-boulder"
        | "ferric-boulder"
        | "beryllic-boulder"
        | "yellow-stone-boulder"
        | "crystalline-boulder"
        | "steam-vent" => meta.variants = 2,
        "scrap-wall" => meta.variants = 5,
        _ => {}
    }

    // Blend groups (`parent = blendGroup = X` / `blendGroup = X`).
    match name {
        "crater-stone" | "char" | "stone-vent" => meta.blend_group = "stone",
        "hotrock" | "magmarock" | "basalt-vent" => meta.blend_group = "basalt",
        "rhyolite-crater" | "rhyolite-vent" => meta.blend_group = "rhyolite",
        "ferric-craters" => meta.blend_group = "ferricStone",
        "carbon-vent" => meta.blend_group = "carbonStone",
        "arkyic-vent" => meta.blend_group = "arkyicStone",
        "yellow-stone-vent" => meta.blend_group = "yellowStone",
        "red-stone-vent" => meta.blend_group = "denseRedStone",
        "crystalline-vent" => meta.blend_group = "crystallineStone",
        _ => {}
    }

    // Edge flags.
    if name == "space" || name.starts_with("metal-tiles-") || name == "colored-floor" {
        meta.draw_edge_out = false;
        meta.draw_edge_in = false;
    }

    // Autotile (`Floor.autotile` / `StaticWall.autotile` + variant counts).
    if name.starts_with("metal-tiles-") {
        meta.autotile = true;
        meta.autotile_variants = match name {
            "metal-tiles-11" => 3,
            "metal-tiles-12" => 4,
            _ => 0,
        };
    } else if matches!(
        name,
        "metal-wall-1" | "metal-wall-2" | "metal-wall-3" | "colored-wall" | "colored-floor"
    ) {
        meta.autotile = true;
    }

    // Shallow liquids (`ShallowLiquid.set(...)`, Blocks.java:411-413).
    match name {
        "darksand-tainted-water" => {
            meta.shallow_liquid_base = Some("tainted-water");
            meta.shallow_floor_base = Some("darksand");
        }
        // `liquidBase.region` uses the floor's resolved region name: the
        // `water` block's region is `shallow-water`.
        "sand-water" => {
            meta.shallow_liquid_base = Some("shallow-water");
            meta.shallow_floor_base = Some("sand-floor");
        }
        "darksand-water" => {
            meta.shallow_liquid_base = Some("shallow-water");
            meta.shallow_floor_base = Some("darksand");
        }
        _ => {}
    }

    // Ore wall variants (`Floor.wallOre`).
    if matches!(
        name,
        "ore-wall-thorium" | "ore-wall-beryllium" | "ore-wall-graphite" | "ore-wall-tungsten"
    ) {
        meta.wall_ore = true;
    }

    if meta.autotile {
        meta.variants = 0;
    }
    meta
}

/// `DrawTurret` base prefixes (`drawer = new DrawTurret("reinforced-")` in
/// `content/Blocks.java`).
pub fn draw_turret_prefix(block: &str) -> &'static str {
    match block {
        "breach" | "diffuse" | "sublimate" | "titan" | "disperse" | "afflict" | "lustre"
        | "scathe" | "smite" | "malign" => "reinforced-",
        _ => "",
    }
}

/// The 47-slice autotile content table (`content/Blocks.java`: `autotile = true`
/// blocks + variants), used by the M2 cross-check.
pub const AUTOTILE_BLOCKS: &[(&str, u32)] = &[
    ("metal-tiles-1", 1),
    ("metal-tiles-2", 1),
    ("metal-tiles-3", 1),
    ("metal-tiles-4", 1),
    ("metal-tiles-5", 1),
    ("metal-tiles-6", 1),
    ("metal-tiles-7", 1),
    ("metal-tiles-8", 1),
    ("metal-tiles-9", 1),
    ("metal-tiles-10", 1),
    ("metal-tiles-11", 3),
    ("metal-tiles-12", 4),
    ("metal-tiles-13", 1),
    ("metal-wall-1", 1),
    ("metal-wall-2", 1),
    ("metal-wall-3", 1),
    ("colored-floor", 1),
    ("colored-wall", 1),
];

/// Context for icon/metadata lookups; `has` mirrors pack-time
/// `Core.atlas.has(name)`.
pub struct IconCtx<'a> {
    /// Content name (parity ABI).
    pub name: &'a str,
    /// Resolved `Block.region` (content name unless overridden).
    pub region: &'a str,
    /// Java class tag.
    pub kind: BlockKind,
    /// Multiblock size in tiles (`Block.size`).
    pub size: i32,
    /// `Block.variants`.
    pub variants: u32,
    /// Atlas presence predicate (pack-time `Core.atlas.has`).
    pub has: &'a dyn Fn(&str) -> bool,
}

impl IconCtx<'_> {
    fn found(&self, name: &str) -> bool {
        (self.has)(name)
    }

    fn suffixed(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.name)
    }

    /// `PayloadBlock.findFactoryRegion(suffix)`: `<name><suffix>` else
    /// `factory<suffix>-<size>` (mod-prefixed fallbacks are plan 20's).
    fn factory_region(&self, suffix: &str) -> String {
        let explicit = self.suffixed(suffix);
        if self.found(&explicit) {
            return explicit;
        }
        let fallback = format!("factory{suffix}-{}", self.size);
        if self.found(&fallback) {
            return fallback;
        }
        explicit
    }
}

/// Icon-contributing `DrawBlock` suffixes per block (plan 03 §5 M3 drawer
/// table). `""` means the block's own region; entries are name suffixes.
/// Drawers that do not override `icons()` (`DrawFlame`, `DrawLiquidTile`,
/// `DrawGlowRegion`, …) contribute nothing and are omitted.
fn drawer_regions(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "phase-weaver" | "phase-synthesizer" => &["-bottom", "-weave", ""],
        "cryofluid-mixer"
        | "melter"
        | "silicon-arc-furnace"
        | "atmospheric-concentrator"
        | "oxidation-chamber"
        | "slag-heater"
        | "carbide-crucible"
        | "surge-crucible"
        | "cyanogen-synthesizer"
        | "regen-projector" => &["-bottom", ""],
        "electrolyzer" => &["-bottom", ""],
        "separator" | "disassembler" => &["-bottom", "-spinner", ""],
        "spore-press" => &["-bottom", "-piston-icon", "", "-top"],
        "pulverizer" => &["", "-rotator", "-top"],
        "cultivator" => &["-bottom", "", "-top"],
        "vent-condenser" => &["-bottom", "-rotator", "-mid", ""],
        "flux-reactor" => &["-bottom", "-mid", ""],
        "neoplasia-reactor" => &["-bottom", "-center", ""],
        "chemical-combustion-chamber" | "pyrolysis-generator" => {
            &["-bottom", "-piston-icon", "-mid", ""]
        }
        "steam-generator" => &["", "-turbine", "-turbine", "-cap"],
        "turbine-condenser" => &["", "-rotator"],
        _ => return None,
    })
}

/// Turret `RegionPart` outline suffixes (`content/Blocks.java` drawers with
/// `outline && drawRegion`; `outline = false` / `drawRegion = false` parts and
/// `ShapePart`/`HaloPart` are omitted).
fn turret_part_outlines(name: &str) -> &'static [&'static str] {
    match name {
        "duo" => &["-barrel-l", "-barrel-r"],
        "scatter" => &["-mid"],
        "salvo" => &["-side-r", "-side-l", "-barrel"],
        "cyclone" => &["-barrel-3", "-barrel-2", "-barrel-1"],
        "diffuse" => &["-front-r", "-front-l"],
        "sublimate" => &[
            "-back-r",
            "-back-l",
            "-front-r",
            "-front-l",
            "-nozzle-r",
            "-nozzle-l",
        ],
        "titan" => &["-barrel", "-side-r", "-side-l"],
        "disperse" => &["-side-r", "-side-l", "-mid", "-blade-r", "-blade-l"],
        "afflict" => &["-blade-r", "-blade-l"],
        "lustre" => &["-blade-r", "-blade-l", "-inner-r", "-inner-l", "-mid"],
        "scathe" => &["-blade-r", "-blade-l", "-side-r", "-side-l", "-mid"],
        "smite" => &[
            "-mid", "-blade-r", "-blade-l", "-front-r", "-front-l", "-back-r", "-back-l",
            "-spine-r", "-spine-l",
        ],
        "malign" => &[
            "-mouth", "-end", "-front-r", "-front-l", "-back-r", "-back-l", "-mid",
        ],
        _ => &[],
    }
}

/// `DrawTurret.load` base resolution: `<name>-base` else `<prefix>block-<size>`.
fn turret_base(ctx: &IconCtx) -> String {
    let explicit = ctx.suffixed("-base");
    if ctx.found(&explicit) {
        return explicit;
    }
    format!("{}block-{}", draw_turret_prefix(ctx.name), ctx.size)
}

/// `Block.icons()`/class overrides — the `getGeneratedIcons()` region list.
pub fn generated_icons(ctx: &IconCtx) -> Vec<String> {
    let name = ctx.name;
    let region = ctx.region;
    match ctx.kind {
        // `Floor.icons()` / `Wall.icons()`: has(name) ? name : name1.
        kind if is_floor_kind(kind)
            || kind == BlockKind::Wall
            || kind == BlockKind::ColoredWall =>
        {
            vec![if ctx.found(name) {
                name.to_owned()
            } else {
                format!("{name}1")
            }]
        }
        // `Prop.icons()`: variants == 0 ? super : name1.
        BlockKind::Prop
        | BlockKind::StaticProp
        | BlockKind::SeaBush
        | BlockKind::Seaweed
        | BlockKind::StaticTree
        | BlockKind::TreeBlock
        | BlockKind::TallBlock => {
            if ctx.variants == 0 {
                default_block_icons(ctx)
            } else {
                vec![format!("{name}1")]
            }
        }
        // `Conveyor.icons()`: regions[0][0] (`@-#1-#2`, lengths {7,4}).
        BlockKind::Conveyor => vec![format!("{name}-0-0")],
        // `Duct.icons()`: `duct-bottom` + topRegions[0] (`@-top-#`, length 5).
        BlockKind::Duct => vec![String::from("duct-bottom"), format!("{name}-top-0")],
        // `Conduit.icons()`: `conduit-bottom` + topRegions[0].
        BlockKind::Conduit => vec![String::from("conduit-bottom"), format!("{name}-top-0")],
        // `Sorter.icons()` / `ItemSource.icons()`.
        BlockKind::Sorter | BlockKind::ItemSource => {
            vec![String::from("source-bottom"), region.to_owned()]
        }
        // `DirectionalUnloader.icons()`.
        BlockKind::DirectionalUnloader => vec![
            region.to_owned(),
            ctx.suffixed("-top"),
            ctx.suffixed("-arrow"),
        ],
        // `DuctRouter.icons()` / `OverflowDuct.icons()`.
        BlockKind::DuctRouter | BlockKind::OverflowDuct => {
            vec![region.to_owned(), ctx.suffixed("-top")]
        }
        // `MassDriver.icons()`.
        BlockKind::MassDriver => vec![ctx.suffixed("-base"), region.to_owned()],
        // `DirectionBridge.icons()` (unused by vanilla content; kept for parity).
        BlockKind::DirectionLiquidBridge => vec![
            ctx.suffixed("-bottom"),
            region.to_owned(),
            ctx.suffixed("-dir"),
        ],
        // `Radar.icons()` (`@-base`, no fallback).
        BlockKind::Radar => vec![ctx.suffixed("-base"), region.to_owned()],
        // `BuildTurret`/`PointDefenseTurret`/`TractorBeamTurret`/`RepairTurret`.
        BlockKind::BuildTurret
        | BlockKind::PointDefenseTurret
        | BlockKind::TractorBeamTurret
        | BlockKind::RepairTurret => vec![turret_base(ctx), region.to_owned()],
        // `Turret` subclasses via `DrawTurret.icons()`.
        kind if is_turret_kind(kind) => {
            let preview = {
                let explicit = ctx.suffixed("-preview");
                if ctx.found(&explicit) {
                    explicit
                } else {
                    region.to_owned()
                }
            };
            let top = ctx.suffixed("-top");
            let mut icons = vec![turret_base(ctx), preview];
            if ctx.found(&top) {
                icons.push(top);
            }
            icons
        }
        // `Thruster.icons()` / `Drill`-family / `BlockProducer`-family.
        BlockKind::Thruster
        | BlockKind::BeamDrill
        | BlockKind::BurstDrill
        | BlockKind::ItemIncinerator
        | BlockKind::WallCrafter => vec![region.to_owned(), ctx.suffixed("-top")],
        BlockKind::PayloadDeconstructor
        | BlockKind::PayloadVoid
        | BlockKind::UnitAssemblerModule => {
            vec![region.to_owned(), ctx.factory_region("-top")]
        }
        BlockKind::Drill | BlockKind::SolidPump => vec![
            region.to_owned(),
            ctx.suffixed("-rotator"),
            ctx.suffixed("-top"),
        ],
        BlockKind::PayloadSource => {
            vec![
                region.to_owned(),
                ctx.factory_region("-out"),
                ctx.factory_region("-top"),
            ]
        }
        BlockKind::PayloadLoader => vec![
            region.to_owned(),
            ctx.factory_region("-in"),
            ctx.factory_region("-out"),
            ctx.factory_region("-top"),
        ],
        BlockKind::PayloadConveyor => vec![ctx.suffixed("-icon")],
        BlockKind::PayloadMassDriver => vec![
            ctx.suffixed("-base"),
            ctx.factory_region("-out"),
            region.to_owned(),
        ],
        BlockKind::LiquidJunction => vec![region.to_owned()],
        BlockKind::LiquidRouter | BlockKind::LiquidSource => {
            vec![ctx.suffixed("-bottom"), region.to_owned()]
        }
        BlockKind::LaunchPad => {
            let explicit = ctx.suffixed("-preview");
            vec![if ctx.found(&explicit) {
                explicit
            } else {
                region.to_owned()
            }]
        }
        BlockKind::TargetDummy => {
            let explicit = ctx.suffixed("-preview");
            vec![if ctx.found(&explicit) {
                explicit
            } else {
                region.to_owned()
            }]
        }
        BlockKind::UnitAssembler => {
            vec![
                region.to_owned(),
                ctx.suffixed("-side1"),
                ctx.factory_region("-top"),
            ]
        }
        BlockKind::Reconstructor => vec![
            region.to_owned(),
            ctx.factory_region("-in"),
            ctx.factory_region("-out"),
            ctx.factory_region("-top"),
        ],
        BlockKind::UnitFactory => vec![
            region.to_owned(),
            ctx.factory_region("-out"),
            ctx.factory_region("-top"),
        ],
        // `GenericCrafter`/`Battery`/`PowerGenerator`/`Pump`/`RegenProjector`/
        // `Separator`/`HeatConductor` drawer icons.
        kind if uses_drawer(kind) => drawer_regions(name)
            .map(|parts| {
                parts
                    .iter()
                    .map(|part| {
                        if part.is_empty() {
                            region.to_owned()
                        } else {
                            ctx.suffixed(part)
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![region.to_owned()]),
        // `Block.icons()` default.
        _ => default_block_icons(ctx),
    }
}

/// `Block.icons()` (base default): variant/team region handling.
fn default_block_icons(ctx: &IconCtx) -> Vec<String> {
    let base = if ctx.variants > 0 {
        format!("{}1", ctx.name)
    } else {
        ctx.region.to_owned()
    };
    let team = ctx.suffixed("-team");
    if ctx.found(&team) {
        vec![base, format!("{}-team-sharded", ctx.name)]
    } else {
        vec![base]
    }
}

/// `Block.makeIconRegions()` — empty for every vanilla block (only mod
/// classes override it).
pub fn make_icon_regions(_ctx: &IconCtx) -> Vec<String> {
    Vec::new()
}

/// `Block.getRegionsToOutline()` — class-level region names to save as
/// `<region>-outline`. The turret body is handled by
/// [`turret_body_outlined`] because it depends on `<name>-preview`.
pub fn regions_to_outline(ctx: &IconCtx) -> Vec<String> {
    if ctx.kind == BlockKind::PayloadMassDriver {
        return vec![
            ctx.suffixed("-left"),
            ctx.suffixed("-right"),
            ctx.suffixed("-cap"),
        ];
    }
    if is_turret_kind(ctx.kind) {
        return turret_part_outlines(ctx.name)
            .iter()
            .map(|suffix| ctx.suffixed(suffix))
            .collect();
    }
    Vec::new()
}

/// `DrawTurret.getRegionsToOutline` body-region condition: the turret body is
/// outlined unless `outlinedIcon` selects the preview fallback (which equals
/// the body region).
pub fn turret_body_outlined(ctx: &IconCtx) -> bool {
    is_turret_kind(ctx.kind) && ctx.found(ctx.region) && ctx.found(&ctx.suffixed("-preview"))
}

/// Region index chosen by the `outlineIcon` block
/// (`outlinedIcon >= 0 ? min(outlinedIcon, len-1) : len-1`).
pub fn outline_icon_index(kind: BlockKind, icon_count: usize) -> Option<usize> {
    if !outline_icon(kind) || icon_count == 0 {
        return None;
    }
    let outlined = outlined_icon(kind);
    Some(if outlined >= 0 {
        (outlined as usize).min(icon_count - 1)
    } else {
        icon_count - 1
    })
}

/// Unit team-cell recolor magic colors (`Generators.java` unit-icons:
/// `0xffffffff → 0xffa664ff`, `0xdcc6c6ff`/`0xdcc5c5ff → 0xd06b53ff`).
pub const UNIT_CELL_WHITE: u32 = 0xffff_ffff;
/// Unit cell recolor target for [`UNIT_CELL_WHITE`].
pub const UNIT_CELL_ORANGE: u32 = 0xffa6_64ff;
/// Unit cell recolor source grays.
pub const UNIT_CELL_GRAY: [u32; 2] = [0xdcc6_c6ff, 0xdcc5_c5ff];
/// Unit cell recolor target for the grays.
pub const UNIT_CELL_DARK_ORANGE: u32 = 0xd06b_53ff;

/// `RegionPart.getOutlines` region names for a draw-part subtree.
///
/// `RegionPart.load(name)` resolves `realName = name + suffix` (no vanilla part
/// sets an explicit `name`); `getOutlines` emits `regions` when
/// `outline && drawRegion`. No vanilla unit part sets `turretShading`, so the
/// mirrored `-r`/`-l` branch never triggers for vanilla; it is still modelled.
pub fn part_outline_regions(parts: &[DrawPartSpec], name: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_part_outlines(parts, name, &mut out);
    out
}

fn walk_part_outlines(parts: &[DrawPartSpec], name: &str, out: &mut Vec<String>) {
    for part in parts {
        if part.kind == DrawPartKind::RegionPart && part.outline && part.draw_region {
            let real = format!("{name}{}", part.suffix);
            if part.mirror && part.turret_shading {
                out.push(format!("{real}-r"));
                out.push(format!("{real}-l"));
            } else {
                out.push(real);
            }
        }
        walk_part_outlines(&part.children, name, out);
    }
}

/// `UnitType.load()` region-name resolution (name-based; `has` is the pack-time
/// `Core.atlas.has` predicate). Mirrors the `Core.atlas.find(name, fallback)`
/// chain exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRegions {
    /// `region = find(name)`.
    pub region: String,
    /// `previewRegion = find(name + "-preview", name)`.
    pub preview: String,
    /// `legRegion = find(name + "-leg")`.
    pub leg: Option<String>,
    /// `jointRegion = find(name + "-joint")`.
    pub joint: Option<String>,
    /// `baseJointRegion = find(name + "-joint-base")`.
    pub base_joint: Option<String>,
    /// `footRegion = find(name + "-foot")`.
    pub foot: Option<String>,
    /// `treadRegion = find(name + "-treads")`.
    pub tread: Option<String>,
    /// `legBaseRegion = find(name + "-leg-base", name + "-leg")`.
    pub leg_base: Option<String>,
    /// `baseRegion = find(name + "-base")`.
    pub base: Option<String>,
    /// `cellRegion = find(name + "-cell", find("power-cell"))`.
    pub cell: Option<String>,
    /// `outlineRegion = find(name + "-outline")`.
    pub outline: Option<String>,
    /// `segmentRegions[i] = find(name + "-segment" + i)`.
    pub segments: Vec<String>,
    /// `wreckRegions[i] = find(name + "-wreck" + i)`.
    pub wrecks: [String; 3],
    /// `treadRegions[r][i] = find(name + "-treads" + r + "-" + i)`.
    pub tread_slices: Vec<Vec<String>>,
}

/// Resolves [`UnitRegions`] for `unit` against the pack-time `has` predicate.
pub fn unit_regions(unit: &UnitTypeDef, has: &dyn Fn(&str) -> bool) -> UnitRegions {
    let find = |candidate: &str| has(candidate).then(|| candidate.to_owned());
    let name = &unit.name;
    let preview = if has(&format!("{name}-preview")) {
        format!("{name}-preview")
    } else {
        name.clone()
    };
    let cell = find(&format!("{name}-cell")).or_else(|| find("power-cell"));
    let leg = find(&format!("{name}-leg"));
    let segments = (0..unit.segments)
        .map(|i| format!("{name}-segment{i}"))
        .collect();
    let wrecks = [
        format!("{name}-wreck0"),
        format!("{name}-wreck1"),
        format!("{name}-wreck2"),
    ];
    let tread_slices = if find(&format!("{name}-treads")).is_some() {
        (0..unit.tread_rects.len())
            .map(|r| {
                (0..unit.tread_frames)
                    .map(|i| format!("{name}-treads{r}-{i}"))
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    UnitRegions {
        region: name.clone(),
        preview,
        leg_base: find(&format!("{name}-leg-base")).or(leg.clone()),
        leg,
        joint: find(&format!("{name}-joint")),
        base_joint: find(&format!("{name}-joint-base")),
        foot: find(&format!("{name}-foot")),
        tread: find(&format!("{name}-treads")),
        base: find(&format!("{name}-base")),
        cell,
        outline: find(&format!("{name}-outline")),
        segments,
        wrecks,
        tread_slices,
    }
}

/// Every region name the `unit-icons` pass can create for `unit` *if* its
/// prerequisite source regions exist (used by the region inventory audit).
pub fn unit_region_expectations(unit: &UnitTypeDef, has: &dyn Fn(&str) -> bool) -> Vec<String> {
    let regions = unit_regions(unit, has);
    let mut out = Vec::new();

    // `type.getRegionsToOutline(toOutline)` → `<region>-outline`.
    for region in part_outline_regions(&unit.parts, &unit.name) {
        if has(&region) {
            out.push(format!("{region}-outline"));
        }
    }
    for weapon in &unit.weapons {
        for region in part_outline_regions(&weapon.parts, &weapon.name) {
            if has(&region) {
                out.push(format!("{region}-outline"));
            }
        }
        // Weapon body outline: saved as `<name>-outline` when under/top, else
        // the weapon region is replaced in place (same region name).
        if !weapon.name.is_empty()
            && has(&weapon.name)
            && (!weapon.top || weapon.parts.iter().any(|part| part.under))
        {
            out.push(format!("{}-outline", weapon.name));
        }
    }

    // Tank tread slices.
    for slices in &regions.tread_slices {
        out.extend(slices.iter().cloned());
    }

    // Crawl segment outlines + composite body.
    if unit.segments > 0 {
        for i in 0..unit.segments {
            if has(&format!("{}-segment{i}", unit.name)) {
                out.push(format!("{}-segment-outline{i}", unit.name));
            }
        }
        out.push(unit.name.clone());
    }

    // Composed body (`unit-<name>-full`) + fit-scaled UI icon.
    if unit.generate_full_icon && has(&regions.preview) {
        out.push(format!("unit-{}-full", unit.name));
    }
    if has(&regions.preview) {
        out.push(format!("unit-{}-ui", unit.name));
    }

    // Wrecks (only emitted when a body image was composed).
    if has(&regions.preview) {
        for wreck in &regions.wrecks {
            out.push(wreck.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_palettes_match_upstream() {
        assert_eq!(TEAMS.len(), 7);
        assert!(TEAMS[1].has_palette);
        assert!(TEAMS[2].has_palette);
        assert!(TEAMS[3].has_palette);
        assert!(!TEAMS[0].has_palette);
        assert_eq!(TEAMS[1].palette[0], 0xffd3_7fff);
        assert_eq!(TEAMS[0].name, "derelict");
        assert!(!team_by_name("nope").is_some());
    }

    #[test]
    fn autotile_table_matches_sources() {
        assert_eq!(AUTOTILE_BLOCKS.len(), 18);
        assert!(AUTOTILE_BLOCKS.contains(&("metal-tiles-11", 3)));
        assert!(AUTOTILE_BLOCKS.contains(&("metal-tiles-12", 4)));
        let meta = block_meta("metal-tiles-12", BlockKind::Floor);
        assert!(meta.autotile);
        assert_eq!(meta.autotile_variants, 4);
        assert_eq!(meta.variants, 0);
    }

    #[test]
    fn floor_defaults_and_shallows() {
        let darksand = block_meta("darksand", BlockKind::Floor);
        assert_eq!(darksand.variants, 3);
        let wall = block_meta("stone-wall", BlockKind::StaticWall);
        assert_eq!(wall.variants, 2);
        let tree = block_meta("redweed", BlockKind::Prop);
        assert_eq!(tree.variants, 3);
        let shallow = block_meta("sand-water", BlockKind::ShallowLiquid);
        assert_eq!(shallow.shallow_liquid_base, Some("shallow-water"));
        assert_eq!(shallow.shallow_floor_base, Some("sand-floor"));
        assert!((shallow.shallow_liquid_opacity - 0.35).abs() < f32::EPSILON);
    }

    #[test]
    fn generated_icons_rules() {
        let found: &dyn Fn(&str) -> bool = &|name: &str| {
            matches!(
                name,
                "copper-wall" | "copper-wall-team" | "meltdown-preview" | "block-3"
            )
        };
        let ctx = IconCtx {
            name: "copper-wall",
            region: "copper-wall",
            kind: BlockKind::Wall,
            size: 1,
            variants: 0,
            has: found,
        };
        assert_eq!(generated_icons(&ctx), vec!["copper-wall".to_owned()]);

        let turret = IconCtx {
            name: "meltdown",
            region: "meltdown",
            kind: BlockKind::LaserTurret,
            size: 3,
            variants: 0,
            has: &|name: &str| matches!(name, "meltdown" | "meltdown-preview"),
        };
        assert_eq!(
            generated_icons(&turret),
            vec!["block-3".to_owned(), "meltdown-preview".to_owned()]
        );
        assert_eq!(outline_icon_index(turret.kind, 2), Some(1));
        assert!(turret_body_outlined(&turret));
    }
}
