// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Content stats (`world/meta/{Stat,StatCat,StatUnit,Stats,StatValues}.java`),
//! H05 T10a: the model behind the content-info (`?`) dialog, `Stats` collection
//! and the base `setStats` bodies for block/item/liquid.
//!
//! Family-specific `setStats` bodies (crafters, drills, turrets, factories, …)
//! land in the sibling modules of H05 T10b-T10e; each calls [`content_stats`]'s
//! block base first, exactly like the Java `super.setStats()` chain.
//!
//! Display values are built as [`Cell`] rows. Text cells carry Java-style markup
//! (color tags and Iconc glyphs); the `MindCampaign.content_info_json` endpoint
//! runs them through `render_markup` so GDScript can draw them as labels.
//!
//! Deviations from the plan's sketch (recorded in the task summary):
//! * [`StatCtx`] holds the registry and the live bundle instead of a
//!   `BlockTable`; every base stat reads `BlockDef` fields directly.
//! * `Block.instantTransfer`/`maxConsecutive` have no port field yet, so the
//!   `itemsMoved = Instant` branch is skipped.

pub mod logistics;
pub mod production;
pub mod turret_ammo;
pub mod unit_blocks;
pub mod values;

use serde::Serialize;

use crate::assets::bundle::Bundle;
use crate::content::load::ContentRegistry;
use crate::content::{BlockId, ContentRef, ContentType, ItemId, LiquidId};
use crate::world::block::BlockTable;

/// `StatCat` (`StatCat.java`): declaration order is the display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StatCat {
    /// General.
    General,
    /// Power.
    Power,
    /// Liquids.
    Liquids,
    /// Items.
    Items,
    /// Crafting.
    Crafting,
    /// Function.
    Function,
    /// Optional.
    Optional,
}

impl StatCat {
    /// Declaration index (`StatCat.id`).
    pub const fn id(self) -> usize {
        self as usize
    }

    /// `StatCat.name`.
    pub const fn name(self) -> &'static str {
        match self {
            StatCat::General => "general",
            StatCat::Power => "power",
            StatCat::Liquids => "liquids",
            StatCat::Items => "items",
            StatCat::Crafting => "crafting",
            StatCat::Function => "function",
            StatCat::Optional => "optional",
        }
    }
}

/// `Stat` (`Stat.java`, 88 entries in declaration order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stat {
    /// Health.
    Health,
    /// Armor.
    Armor,
    /// Size.
    Size,
    /// Display size.
    DisplaySize,
    /// Build time.
    BuildTime,
    /// Build cost.
    BuildCost,
    /// Memory capacity.
    MemoryCapacity,
    /// Explosiveness.
    Explosiveness,
    /// Flammability.
    Flammability,
    /// Radioactivity.
    Radioactivity,
    /// Charge.
    Charge,
    /// Heat capacity.
    HeatCapacity,
    /// Viscosity.
    Viscosity,
    /// Temperature.
    Temperature,
    /// Flying.
    Flying,
    /// Speed.
    Speed,
    /// Build speed.
    BuildSpeed,
    /// Mine speed.
    MineSpeed,
    /// Mine tier.
    MineTier,
    /// Payload capacity.
    PayloadCapacity,
    /// Base deflect chance.
    BaseDeflectChance,
    /// Lightning chance.
    LightningChance,
    /// Lightning damage.
    LightningDamage,
    /// Abilities.
    Abilities,
    /// Can boost.
    CanBoost,
    /// Boosting speed.
    BoostingSpeed,
    /// Max units.
    MaxUnits,
    /// Damage multiplier.
    DamageMultiplier,
    /// Health multiplier.
    HealthMultiplier,
    /// Speed multiplier.
    SpeedMultiplier,
    /// Reload multiplier.
    ReloadMultiplier,
    /// Build speed multiplier.
    BuildSpeedMultiplier,
    /// Reactive.
    Reactive,
    /// Healing.
    Healing,
    /// Immunities.
    Immunities,
    /// Item capacity.
    ItemCapacity,
    /// Items moved.
    ItemsMoved,
    /// Launch time.
    LaunchTime,
    /// Max consecutive.
    MaxConsecutive,
    /// Liquid capacity.
    LiquidCapacity,
    /// Power capacity.
    PowerCapacity,
    /// Power use.
    PowerUse,
    /// Power damage.
    PowerDamage,
    /// Power range.
    PowerRange,
    /// Power connections.
    PowerConnections,
    /// Base power generation.
    BasePowerGeneration,
    /// Meltdown time.
    MeltdownTime,
    /// Warmup time.
    WarmupTime,
    /// Tiles.
    Tiles,
    /// Input.
    Input,
    /// Output.
    Output,
    /// Production time.
    ProductionTime,
    /// Max efficiency.
    MaxEfficiency,
    /// Drill tier.
    DrillTier,
    /// Drill speed.
    DrillSpeed,
    /// Link range.
    LinkRange,
    /// Instructions.
    Instructions,
    /// Weapons.
    Weapons,
    /// Bullet.
    Bullet,
    /// Speed increase.
    SpeedIncrease,
    /// Repair time.
    RepairTime,
    /// Repair speed.
    RepairSpeed,
    /// Range.
    Range,
    /// Shoot range.
    ShootRange,
    /// Inaccuracy.
    Inaccuracy,
    /// Shots.
    Shots,
    /// Reload.
    Reload,
    /// Crush damage.
    CrushDamage,
    /// Leg splash damage.
    LegSplashDamage,
    /// Targets air.
    TargetsAir,
    /// Targets ground.
    TargetsGround,
    /// Damage.
    Damage,
    /// Status.
    Status,
    /// Frequency.
    Frequency,
    /// Ammo.
    Ammo,
    /// Ammo capacity.
    AmmoCapacity,
    /// Ammo use.
    AmmoUse,
    /// Shield health.
    ShieldHealth,
    /// Cooldown time.
    CooldownTime,
    /// Regeneration rate.
    RegenerationRate,
    /// Activation time.
    ActivationTime,
    /// Module tier.
    ModuleTier,
    /// Unit type.
    UnitType,
    /// Receive rate.
    ReceiveRate,
    /// Booster.
    Booster,
    /// Boost effect.
    BoostEffect,
    /// Affinities.
    Affinities,
    /// Opposites.
    Opposites,
}

impl Stat {
    /// Declaration index (`Stat.id`).
    pub const fn id(self) -> usize {
        self as usize
    }

    /// `Stat.name` (three Java entries lowercase differently:
    /// `boostingspeed`, `moduletier`, `unittype`).
    pub const fn name(self) -> &'static str {
        match self {
            Stat::Health => "health",
            Stat::Armor => "armor",
            Stat::Size => "size",
            Stat::DisplaySize => "displaySize",
            Stat::BuildTime => "buildTime",
            Stat::BuildCost => "buildCost",
            Stat::MemoryCapacity => "memoryCapacity",
            Stat::Explosiveness => "explosiveness",
            Stat::Flammability => "flammability",
            Stat::Radioactivity => "radioactivity",
            Stat::Charge => "charge",
            Stat::HeatCapacity => "heatCapacity",
            Stat::Viscosity => "viscosity",
            Stat::Temperature => "temperature",
            Stat::Flying => "flying",
            Stat::Speed => "speed",
            Stat::BuildSpeed => "buildSpeed",
            Stat::MineSpeed => "mineSpeed",
            Stat::MineTier => "mineTier",
            Stat::PayloadCapacity => "payloadCapacity",
            Stat::BaseDeflectChance => "baseDeflectChance",
            Stat::LightningChance => "lightningChance",
            Stat::LightningDamage => "lightningDamage",
            Stat::Abilities => "abilities",
            Stat::CanBoost => "canBoost",
            Stat::BoostingSpeed => "boostingspeed",
            Stat::MaxUnits => "maxUnits",
            Stat::DamageMultiplier => "damageMultiplier",
            Stat::HealthMultiplier => "healthMultiplier",
            Stat::SpeedMultiplier => "speedMultiplier",
            Stat::ReloadMultiplier => "reloadMultiplier",
            Stat::BuildSpeedMultiplier => "buildSpeedMultiplier",
            Stat::Reactive => "reactive",
            Stat::Healing => "healing",
            Stat::Immunities => "immunities",
            Stat::ItemCapacity => "itemCapacity",
            Stat::ItemsMoved => "itemsMoved",
            Stat::LaunchTime => "launchTime",
            Stat::MaxConsecutive => "maxConsecutive",
            Stat::LiquidCapacity => "liquidCapacity",
            Stat::PowerCapacity => "powerCapacity",
            Stat::PowerUse => "powerUse",
            Stat::PowerDamage => "powerDamage",
            Stat::PowerRange => "powerRange",
            Stat::PowerConnections => "powerConnections",
            Stat::BasePowerGeneration => "basePowerGeneration",
            Stat::MeltdownTime => "meltdownTime",
            Stat::WarmupTime => "warmupTime",
            Stat::Tiles => "tiles",
            Stat::Input => "input",
            Stat::Output => "output",
            Stat::ProductionTime => "productionTime",
            Stat::MaxEfficiency => "maxEfficiency",
            Stat::DrillTier => "drillTier",
            Stat::DrillSpeed => "drillSpeed",
            Stat::LinkRange => "linkRange",
            Stat::Instructions => "instructions",
            Stat::Weapons => "weapons",
            Stat::Bullet => "bullet",
            Stat::SpeedIncrease => "speedIncrease",
            Stat::RepairTime => "repairTime",
            Stat::RepairSpeed => "repairSpeed",
            Stat::Range => "range",
            Stat::ShootRange => "shootRange",
            Stat::Inaccuracy => "inaccuracy",
            Stat::Shots => "shots",
            Stat::Reload => "reload",
            Stat::CrushDamage => "crushDamage",
            Stat::LegSplashDamage => "legSplashDamage",
            Stat::TargetsAir => "targetsAir",
            Stat::TargetsGround => "targetsGround",
            Stat::Damage => "damage",
            Stat::Status => "status",
            Stat::Frequency => "frequency",
            Stat::Ammo => "ammo",
            Stat::AmmoCapacity => "ammoCapacity",
            Stat::AmmoUse => "ammoUse",
            Stat::ShieldHealth => "shieldHealth",
            Stat::CooldownTime => "cooldownTime",
            Stat::RegenerationRate => "regenerationRate",
            Stat::ActivationTime => "activationTime",
            Stat::ModuleTier => "moduletier",
            Stat::UnitType => "unittype",
            Stat::ReceiveRate => "receiveRate",
            Stat::Booster => "booster",
            Stat::BoostEffect => "boostEffect",
            Stat::Affinities => "affinities",
            Stat::Opposites => "opposites",
        }
    }

    /// `Stat.category` (defaults to `StatCat.general`).
    pub const fn category(self) -> StatCat {
        match self {
            Stat::ItemCapacity | Stat::ItemsMoved | Stat::LaunchTime | Stat::MaxConsecutive => {
                StatCat::Items
            }
            Stat::LiquidCapacity => StatCat::Liquids,
            Stat::PowerCapacity
            | Stat::PowerUse
            | Stat::PowerDamage
            | Stat::PowerRange
            | Stat::PowerConnections
            | Stat::BasePowerGeneration
            | Stat::MeltdownTime
            | Stat::WarmupTime => StatCat::Power,
            Stat::Tiles
            | Stat::Input
            | Stat::Output
            | Stat::ProductionTime
            | Stat::MaxEfficiency
            | Stat::DrillTier
            | Stat::DrillSpeed
            | Stat::LinkRange
            | Stat::Instructions => StatCat::Crafting,
            Stat::Weapons
            | Stat::Bullet
            | Stat::SpeedIncrease
            | Stat::RepairTime
            | Stat::RepairSpeed
            | Stat::Range
            | Stat::ShootRange
            | Stat::Inaccuracy
            | Stat::Shots
            | Stat::Reload
            | Stat::CrushDamage
            | Stat::LegSplashDamage
            | Stat::TargetsAir
            | Stat::TargetsGround
            | Stat::Damage
            | Stat::Status
            | Stat::Frequency
            | Stat::Ammo
            | Stat::AmmoCapacity
            | Stat::AmmoUse
            | Stat::ShieldHealth
            | Stat::CooldownTime
            | Stat::RegenerationRate
            | Stat::ActivationTime
            | Stat::ModuleTier
            | Stat::UnitType
            | Stat::ReceiveRate => StatCat::Function,
            Stat::Booster | Stat::BoostEffect | Stat::Affinities | Stat::Opposites => {
                StatCat::Optional
            }
            _ => StatCat::General,
        }
    }

    /// All 88 stats in declaration order (`Stat.all`).
    pub const ALL: [Stat; 88] = [
        Stat::Health,
        Stat::Armor,
        Stat::Size,
        Stat::DisplaySize,
        Stat::BuildTime,
        Stat::BuildCost,
        Stat::MemoryCapacity,
        Stat::Explosiveness,
        Stat::Flammability,
        Stat::Radioactivity,
        Stat::Charge,
        Stat::HeatCapacity,
        Stat::Viscosity,
        Stat::Temperature,
        Stat::Flying,
        Stat::Speed,
        Stat::BuildSpeed,
        Stat::MineSpeed,
        Stat::MineTier,
        Stat::PayloadCapacity,
        Stat::BaseDeflectChance,
        Stat::LightningChance,
        Stat::LightningDamage,
        Stat::Abilities,
        Stat::CanBoost,
        Stat::BoostingSpeed,
        Stat::MaxUnits,
        Stat::DamageMultiplier,
        Stat::HealthMultiplier,
        Stat::SpeedMultiplier,
        Stat::ReloadMultiplier,
        Stat::BuildSpeedMultiplier,
        Stat::Reactive,
        Stat::Healing,
        Stat::Immunities,
        Stat::ItemCapacity,
        Stat::ItemsMoved,
        Stat::LaunchTime,
        Stat::MaxConsecutive,
        Stat::LiquidCapacity,
        Stat::PowerCapacity,
        Stat::PowerUse,
        Stat::PowerDamage,
        Stat::PowerRange,
        Stat::PowerConnections,
        Stat::BasePowerGeneration,
        Stat::MeltdownTime,
        Stat::WarmupTime,
        Stat::Tiles,
        Stat::Input,
        Stat::Output,
        Stat::ProductionTime,
        Stat::MaxEfficiency,
        Stat::DrillTier,
        Stat::DrillSpeed,
        Stat::LinkRange,
        Stat::Instructions,
        Stat::Weapons,
        Stat::Bullet,
        Stat::SpeedIncrease,
        Stat::RepairTime,
        Stat::RepairSpeed,
        Stat::Range,
        Stat::ShootRange,
        Stat::Inaccuracy,
        Stat::Shots,
        Stat::Reload,
        Stat::CrushDamage,
        Stat::LegSplashDamage,
        Stat::TargetsAir,
        Stat::TargetsGround,
        Stat::Damage,
        Stat::Status,
        Stat::Frequency,
        Stat::Ammo,
        Stat::AmmoCapacity,
        Stat::AmmoUse,
        Stat::ShieldHealth,
        Stat::CooldownTime,
        Stat::RegenerationRate,
        Stat::ActivationTime,
        Stat::ModuleTier,
        Stat::UnitType,
        Stat::ReceiveRate,
        Stat::Booster,
        Stat::BoostEffect,
        Stat::Affinities,
        Stat::Opposites,
    ];
}

/// `StatUnit` (`StatUnit.java`): measurement units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatUnit {
    /// `blocks`.
    Blocks,
    /// `blocksSquared`.
    BlocksSquared,
    /// `worldUnits`.
    WorldUnits,
    /// `tilesSecond`.
    TilesSecond,
    /// `powerSecond`.
    PowerSecond,
    /// `liquidSecond`.
    LiquidSecond,
    /// `itemsSecond`.
    ItemsSecond,
    /// `liquidUnits`.
    LiquidUnits,
    /// `powerUnits`.
    PowerUnits,
    /// `powerEquilibrium`.
    PowerEquilibrium,
    /// `heatUnits`.
    HeatUnits,
    /// `degrees`.
    Degrees,
    /// `seconds`.
    Seconds,
    /// `minutes`.
    Minutes,
    /// `shots`.
    Shots,
    /// `perSecond`.
    PerSecond,
    /// `perMinute`.
    PerMinute,
    /// `perShot`.
    PerShot,
    /// `perLeg`.
    PerLeg,
    /// `perSide`.
    PerSide,
    /// `timesSpeed`.
    TimesSpeed,
    /// `multiplier`.
    Multiplier,
    /// `percent`.
    Percent,
    /// `shieldHealth`.
    ShieldHealth,
    /// `none`.
    None,
    /// `items`.
    Items,
    /// `instant`.
    Instant,
}

impl StatUnit {
    /// `StatUnit.name`.
    pub const fn name(self) -> &'static str {
        match self {
            StatUnit::Blocks => "blocks",
            StatUnit::BlocksSquared => "blocksSquared",
            StatUnit::WorldUnits => "worldUnits",
            StatUnit::TilesSecond => "tilesSecond",
            StatUnit::PowerSecond => "powerSecond",
            StatUnit::LiquidSecond => "liquidSecond",
            StatUnit::ItemsSecond => "itemsSecond",
            StatUnit::LiquidUnits => "liquidUnits",
            StatUnit::PowerUnits => "powerUnits",
            StatUnit::PowerEquilibrium => "powerEquilibrium",
            StatUnit::HeatUnits => "heatUnits",
            StatUnit::Degrees => "degrees",
            StatUnit::Seconds => "seconds",
            StatUnit::Minutes => "minutes",
            StatUnit::Shots => "shots",
            StatUnit::PerSecond => "perSecond",
            StatUnit::PerMinute => "perMinute",
            StatUnit::PerShot => "perShot",
            StatUnit::PerLeg => "perLeg",
            StatUnit::PerSide => "perSide",
            StatUnit::TimesSpeed => "timesSpeed",
            StatUnit::Multiplier => "multiplier",
            StatUnit::Percent => "percent",
            StatUnit::ShieldHealth => "shieldHealth",
            StatUnit::None => "none",
            StatUnit::Items => "items",
            StatUnit::Instant => "instant",
        }
    }

    /// `StatUnit.space` (false for the `new StatUnit(name, false)` entries).
    pub const fn space(self) -> bool {
        !matches!(
            self,
            StatUnit::PerSecond
                | StatUnit::PerMinute
                | StatUnit::PerShot
                | StatUnit::TimesSpeed
                | StatUnit::Multiplier
                | StatUnit::Percent
        )
    }

    /// `StatUnit.icon` icon-font name (`"[accent]" + Iconc.power + "[]"` etc.;
    /// emitted as a leading [`Cell::Icon`] by `values::number`).
    pub const fn icon_name(self) -> Option<&'static str> {
        match self {
            StatUnit::PowerSecond | StatUnit::PowerUnits => Some("power"),
            StatUnit::LiquidSecond | StatUnit::LiquidUnits => Some("liquid"),
            StatUnit::HeatUnits => Some("waves"),
            _ => None,
        }
    }

    /// `StatUnit.localized()`: `unit.<lowercase name>`, `none` is empty.
    pub fn localized(self, ctx: &StatCtx) -> String {
        if self == StatUnit::None {
            return String::new();
        }
        ctx.unit_label(self)
    }

    /// The inline color + icon markup of `StatUnit.icon` (empty when absent).
    ///
    /// Java embeds `Iconc.power`/`liquid`/`waves` glyphs; the port emits the
    /// equivalent `[icon name="…"]` tag `render_markup` produces for Iconc
    /// characters (`MindIconEffect` resolves it against the icon font).
    pub fn markup(self) -> String {
        let Some(icon) = self.icon_name() else {
            return String::new();
        };
        let color = match self {
            StatUnit::PowerSecond | StatUnit::PowerUnits => "accent",
            StatUnit::LiquidSecond | StatUnit::LiquidUnits => "sky",
            StatUnit::HeatUnits => "red",
            _ => return String::new(),
        };
        format!("[{color}][icon name=\"{icon}\"][]")
    }
}

/// One display cell (`ContentInfoDialog`'s table cells).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Cell {
    /// A markup text cell.
    Text {
        /// Raw Java-style markup (rendered through `render_markup`).
        t: String,
    },
    /// A content icon with an optional outline amount label.
    Icon {
        /// Atlas region (`item-<name>-ui`, `liquid-<name>-ui`, …).
        region: String,
        /// Outline amount label text (bottom-left).
        amount: Option<String>,
        /// Icon pixel size (`iconMed` = 32).
        size: u8,
        /// Click-through content name.
        content: Option<String>,
    },
    /// A fixed-width spacer.
    Space {
        /// Width in pixels.
        w: f32,
    },
    /// Starts a new line inside the value.
    Row,
    /// A gray panel box around nested cells.
    Panel {
        /// Nested cells.
        cells: Vec<Cell>,
    },
}

/// One stat's value rows (each `Stats.add` call appends one row of cells).
pub type StatValues = Vec<Vec<Cell>>;

/// One displayed stat: the stat plus its value rows.
pub type StatEntry = (Stat, StatValues);

/// One category section: the category plus its sorted stat entries.
pub type CategoryStats = (StatCat, Vec<StatEntry>);

/// `Stats` ordered container (`Stats.java`).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// `Stats.useCategories`.
    pub use_categories: bool,
    /// `Stats.timePeriod` in ticks (`-1` = unset).
    pub time_period: f32,
    map: Vec<CategoryStats>,
}

impl Stats {
    /// Empty container with the upstream defaults.
    pub fn new() -> Self {
        Self {
            use_categories: false,
            time_period: -1.0,
            map: Vec::new(),
        }
    }

    /// `Stats.add(stat, value)`: appends one value (a cell row) for `stat`.
    pub fn add(&mut self, stat: Stat, cells: Vec<Cell>) {
        let cat = stat.category();
        if !self.map.iter().any(|(existing, _)| *existing == cat) {
            self.map.push((cat, Vec::new()));
        }
        if let Some((_, list)) = self.map.iter_mut().find(|(existing, _)| *existing == cat) {
            match list.iter_mut().find(|(existing, _)| *existing == stat) {
                Some((_, values)) => values.push(cells),
                None => list.push((stat, vec![cells])),
            }
        }
    }

    /// `Stats.add(stat, value, unit)`.
    pub fn add_number(&mut self, ctx: &StatCtx, stat: Stat, value: f32, unit: StatUnit) {
        self.add(stat, values::number(ctx, value, unit));
    }

    /// `Stats.addPercent(stat, value)` (`(int)(value * 100)` + percent unit).
    pub fn add_percent(&mut self, ctx: &StatCtx, stat: Stat, value: f32) {
        self.add(
            stat,
            values::number(ctx, (value * 100.0) as i32 as f32, StatUnit::Percent),
        );
    }

    /// `Stats.replace`: removes the old values, then adds.
    pub fn replace(&mut self, stat: Stat, cells: Vec<Cell>) {
        self.remove(stat);
        self.add(stat, cells);
    }

    /// `Stats.remove`.
    pub fn remove(&mut self, stat: Stat) {
        if let Some((_, list)) = self.map.iter_mut().find(|(cat, _)| *cat == stat.category()) {
            list.retain(|(existing, _)| *existing != stat);
        }
    }

    /// Whether any stat was added.
    pub fn is_empty(&self) -> bool {
        self.map.iter().all(|(_, list)| list.is_empty())
    }

    /// `Stats.toMap()`: categories and stats sorted by declaration id.
    pub fn to_map(&self) -> Vec<CategoryStats> {
        let mut out: Vec<CategoryStats> = self
            .map
            .iter()
            .map(|(cat, list)| {
                let mut entries = list.clone();
                entries.sort_by_key(|(stat, _)| stat.id());
                (*cat, entries)
            })
            .collect();
        out.sort_by_key(|(cat, _)| cat.id());
        out
    }
}

/// Live content/display context for [`content_stats`].
pub struct StatCtx<'a> {
    /// Content registry.
    pub registry: &'a ContentRegistry,
    /// Live bundle for labels (`Core.bundle`).
    pub bundle: &'a Bundle,
    /// Runtime block table (family knobs + behaviors). `None` in pure-content
    /// tests; family bodies that need family data then emit no rows.
    pub blocks: Option<&'a BlockTable>,
    /// Whether a world is loaded (`state.isGame()`); the live-map stat branches
    /// (`StatValues.blocks`) show `@stat.showinmap` in the menu.
    pub in_game: bool,
    /// Active planet name (`state.getPlanet()`), for the Erekir-only rows
    /// (`NuclearReactor` heat output).
    pub planet: Option<&'a str>,
    /// `state.isCampaign()`: the `unlockedNow()` branches show lock icons for
    /// content that is not unlocked (outside a campaign everything is shown).
    pub campaign: bool,
}

impl StatCtx<'_> {
    /// `<type>.<name>.name` through the bundle, falling back to the name.
    pub fn localized_name(&self, type_name: &str, name: &str) -> String {
        let key = format!("{type_name}.{name}.name");
        let value = self.bundle.get(&key);
        if value == key {
            name.to_owned()
        } else {
            value.to_owned()
        }
    }

    /// Localized item name.
    pub fn item_name(&self, name: &str) -> String {
        self.localized_name("item", name)
    }

    /// Localized liquid name.
    pub fn liquid_name(&self, name: &str) -> String {
        self.localized_name("liquid", name)
    }

    /// `StatUnit.localized()`.
    pub fn unit_label(&self, unit: StatUnit) -> String {
        let key = format!("unit.{}", unit.name().to_lowercase());
        self.bundle.get(&key).to_owned()
    }

    /// `Stat.localized()`: `stat.<lowercase name>`, falling back to the name.
    pub fn stat_label(&self, stat: Stat) -> String {
        let key = format!("stat.{}", stat.name().to_lowercase());
        let value = self.bundle.get(&key);
        if value == key {
            stat.name().to_owned()
        } else {
            value.to_owned()
        }
    }

    /// `Stats.statInfo` tooltip key (`stat.<name>.info`), when the bundle has it.
    pub fn stat_tooltip(&self, stat: Stat) -> Option<String> {
        let key = format!("stat.{}.info", stat.name().to_lowercase());
        self.bundle.has(&key).then_some(key)
    }

    /// `StatCat.localized()`: `category.<name>`.
    pub fn category_label(&self, cat: StatCat) -> String {
        let key = format!("category.{}", cat.name());
        self.bundle.get(&key).to_owned()
    }
}

/// `UnlockableContent.computeStats()` dispatch (`ContentInfoDialog.show`).
///
/// Blocks run the base body plus every family override (T10a base, T10b-T10e
/// families); item/liquid have their base bodies; unit/status/sector/planet
/// return empty stats (family bodies land in T10d/T10e).
pub fn content_stats(ctx: &StatCtx, content: ContentRef) -> Stats {
    match content.type_ {
        ContentType::Block => production::family_block_stats(ctx, BlockId::new(content.id)),
        ContentType::Item => item_stats(ctx, ItemId::new(content.id)),
        ContentType::Liquid => liquid_stats(ctx, LiquidId::new(content.id)),
        _ => Stats::new(),
    }
}

/// `Block.setStats` (`Block.java:644-678`) — the base every block family calls.
pub fn block_stats(ctx: &StatCtx, id: BlockId) -> Stats {
    block_stats_timed(ctx, id, -1.0)
}

/// [`block_stats`] with an explicit `Stats.timePeriod`.
///
/// Family bodies that set `stats.timePeriod` BEFORE `super.setStats()` in Java
/// (`GenericCrafter`, `Separator`, `Fracker`, `ConsumeGenerator`, …
/// `Stats.timePeriod` default is `-1`) must set it before the consumer rows are
/// built, so the family passes it through here instead of assigning afterwards.
pub fn block_stats_timed(ctx: &StatCtx, id: BlockId, time_period: f32) -> Stats {
    let Some(def) = ctx.registry.block(id) else {
        return Stats::new();
    };
    let mut stats = Stats::new();
    stats.use_categories = true;
    stats.time_period = time_period;

    let size = def.size.to_string();
    stats.add(
        Stat::Size,
        vec![values::text(values::format_args(
            "@x@",
            &[size.clone(), size],
        ))],
    );

    // `synthetic()` = `update || destructible`.
    if def.update || def.destructible {
        stats.add_number(ctx, Stat::Health, def.health as f32, StatUnit::None);
        if def.armor > 0.0 {
            stats.add_number(ctx, Stat::Armor, def.armor, StatUnit::None);
        }
    }

    // `canBeBuilt()` = visibility not `hidden`/`debugOnly`.
    let can_be_built = !matches!(
        def.build_visibility,
        crate::content::registries::blocks::BuildVisibility::Hidden
            | crate::content::registries::blocks::BuildVisibility::DebugOnly
    );
    if can_be_built && !def.requirements.is_empty() {
        stats.add_number(
            ctx,
            Stat::BuildTime,
            def.build_time / 60.0,
            StatUnit::Seconds,
        );
        stats.add(
            Stat::BuildCost,
            values::items(ctx, &def.requirements, false),
        );
    }

    // `Block.instantTransfer` has no port field yet (T10c owns the conveyor
    // `itemsMoved` display); the `Instant` branch is skipped.

    for consumer in &def.consumes {
        consume_display(ctx, &mut stats, consumer);
    }

    if def.has_liquids {
        stats.add_number(
            ctx,
            Stat::LiquidCapacity,
            def.liquid_capacity,
            StatUnit::LiquidUnits,
        );
    }
    if def.has_items {
        if def.item_capacity > 0 {
            stats.add_number(
                ctx,
                Stat::ItemCapacity,
                def.item_capacity as f32,
                StatUnit::Items,
            );
        } else {
            stats.add(
                Stat::ItemCapacity,
                vec![values::text(format!(
                    "{}. {}",
                    ctx.bundle.get("none"),
                    ctx.bundle.get("unabletoclog")
                ))],
            );
        }
    }

    stats
}

/// One consumer's `display(stats)` (`Consume*.display`).
fn consume_display(
    ctx: &StatCtx,
    stats: &mut Stats,
    spec: &crate::content::registries::blocks::ConsumeSpec,
) {
    use crate::content::registries::blocks::Consume;
    // Java filter consumers use `Stat.booster` when `.booster` is set; the port
    // does not track the flag, so every consumer reports under `Stat.input`.
    match &spec.consume {
        Consume::Items(stacks) => {
            let cells = if stats.time_period < 0.0 {
                values::items(ctx, stacks, true)
            } else {
                values::items_per_second(ctx, stats.time_period, stacks)
            };
            stats.add(Stat::Input, cells);
        }
        Consume::Liquid { liquid, amount } => {
            stats.add(
                Stat::Input,
                values::liquid(ctx, *liquid, amount * 60.0, true),
            );
        }
        Consume::Liquids(stacks) => {
            stats.add(Stat::Input, values::liquids(ctx, stacks, 1.0, true));
        }
        Consume::Power { usage, buffered } => {
            if *buffered > 0.0 {
                stats.add_number(ctx, Stat::PowerCapacity, *buffered, StatUnit::None);
            } else if *usage > 0.0 {
                stats.add_number(ctx, Stat::PowerUse, usage * 60.0, StatUnit::PowerSecond);
            }
        }
        // `ConsumeCoolant extends ConsumeLiquidFilter`, but the port's consumer
        // record has no temperature/flammability filter; T10c owns its display.
        Consume::Coolant { .. } => {}
    }
}

/// `Item.setStats` (`Item.java:105-110`).
pub fn item_stats(ctx: &StatCtx, id: ItemId) -> Stats {
    let Some(item) = ctx.registry.item(id) else {
        return Stats::new();
    };
    let mut stats = Stats::new();
    stats.add_percent(ctx, Stat::Explosiveness, item.explosiveness);
    stats.add_percent(ctx, Stat::Flammability, item.flammability);
    stats.add_percent(ctx, Stat::Radioactivity, item.radioactivity);
    stats.add_percent(ctx, Stat::Charge, item.charge);
    stats
}

/// `Liquid.setStats` (`Liquid.java:162-168`).
pub fn liquid_stats(ctx: &StatCtx, id: LiquidId) -> Stats {
    let Some(liquid) = ctx.registry.liquid(id) else {
        return Stats::new();
    };
    let mut stats = Stats::new();
    stats.add_percent(ctx, Stat::Explosiveness, liquid.explosiveness);
    stats.add_percent(ctx, Stat::Flammability, liquid.flammability);
    stats.add_percent(ctx, Stat::Temperature, liquid.temperature);
    stats.add_percent(ctx, Stat::HeatCapacity, liquid.heat_capacity);
    stats.add_percent(ctx, Stat::Viscosity, liquid.viscosity);
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::bundle::parse_properties;
    use crate::content::bundle::MemoryBundle;
    use crate::content::registries::create_base_content;
    use crate::content::settings_store::MemoryUnlockStore;

    fn bundle() -> Bundle {
        Bundle::from_layers(vec![parse_properties(
            "unit.seconds=seconds\nunit.percent=%\nunit.items=items\nunit.liquidunits=liquid units\n\
             category.general=General\ncategory.items=Items\ncategory.power=Power\n\
             stat.size=Size\nstat.health=Health\nstat.buildtime=Build Time\n\
             stat.buildcost=Build Cost\nstat.itemcapacity=Item Capacity\nstat.liquidcapacity=Liquid Capacity\n\
             stat.explosiveness=Explosiveness\nstat.flammability=Flammability\n\
             stat.radioactivity=Radioactivity\nstat.charge=Charge\nstat.temperature=Temperature\n\
             stat.heatcapacity=Heat Capacity\nstat.viscosity=Viscosity\n\
             item.copper.name=Copper\nliquid.cryofluid.name=Cryofluid\nnone=<none>\nunabletoclog=Unable to clog\n",
        )])
    }

    fn registry() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        registry.init().expect("init");
        registry.post_init().expect("post_init");
        registry
    }

    fn ctx<'a>(registry: &'a ContentRegistry, bundle: &'a Bundle) -> StatCtx<'a> {
        StatCtx {
            registry,
            bundle,
            blocks: None,
            in_game: false,
            planet: None,
            campaign: false,
        }
    }

    #[test]
    fn stat_ids() {
        assert_eq!(Stat::Health as usize, 0);
        assert_eq!(Stat::ItemCapacity.category(), StatCat::Items);
        assert_eq!(Stat::Opposites as usize, 87);
        assert_eq!(Stat::ALL.len(), 88);
        assert_eq!(Stat::ALL[87], Stat::Opposites);
        assert_eq!(Stat::BoostEffect.category(), StatCat::Optional);
        assert_eq!(Stat::PowerCapacity.category(), StatCat::Power);
        assert_eq!(StatCat::Liquids.id(), 2);
        // Declaration order is dense and matches `ALL`.
        for (index, stat) in Stat::ALL.iter().enumerate() {
            assert_eq!(stat.id(), index, "{stat:?} out of order");
        }
    }

    #[test]
    fn copper_item_stats() {
        let registry = registry();
        let bundle = bundle();
        let ctx = ctx(&registry, &bundle);
        let id = registry.item_id("copper").expect("copper");
        let stats = content_stats(&ctx, ContentRef::item(id));
        let map = stats.to_map();
        assert_eq!(map.len(), 1);
        let (cat, rows) = &map[0];
        assert_eq!(*cat, StatCat::General);
        assert_eq!(rows.len(), 4);
        let names: Vec<&str> = rows.iter().map(|(stat, _)| stat.name()).collect();
        assert_eq!(
            names,
            vec!["explosiveness", "flammability", "radioactivity", "charge"]
        );
        for (_, values) in rows {
            assert_eq!(
                values[0],
                vec![Cell::Text { t: "0".into() }, Cell::Text { t: "%".into() }]
            );
        }
    }

    #[test]
    fn duo_block_stats() {
        let registry = registry();
        let bundle = bundle();
        let ctx = ctx(&registry, &bundle);
        let id = registry.block_id("duo").expect("duo");
        let stats = content_stats(&ctx, ContentRef::block(id));
        assert!(stats.use_categories);
        let map = stats.to_map();
        let general = map
            .iter()
            .find(|(cat, _)| *cat == StatCat::General)
            .expect("general category");
        let row = |stat: Stat| -> Vec<Vec<Cell>> {
            general
                .1
                .iter()
                .find(|(existing, _)| *existing == stat)
                .map(|(_, values)| values.clone())
                .unwrap_or_default()
        };
        // `Blocks.java` duo: size 1x1, health 250.
        assert_eq!(row(Stat::Size), vec![vec![values::text("1x1")]]);
        assert_eq!(row(Stat::Health), vec![vec![values::text("250")]]);
        // 35 copper x cost 1 x buildCostMultiplier 0.5 = 17.5 ticks -> 0.291 s.
        assert_eq!(
            row(Stat::BuildTime),
            vec![vec![
                Cell::Text { t: "0.291".into() },
                Cell::Text {
                    t: " seconds".into()
                }
            ]]
        );
        assert_eq!(
            row(Stat::BuildCost),
            vec![vec![
                Cell::Icon {
                    region: "item-copper-ui".into(),
                    amount: Some("35".into()),
                    size: 32,
                    content: Some("copper".into()),
                },
                Cell::Space { w: 5.0 },
            ]]
        );
        // `ItemTurret` consumes copper ammo (Turret.setStats) -> T10e; the base
        // body only adds the ammo-store capacity, so nothing is asserted here.
        let _ = &map;
    }

    #[test]
    fn cryofluid_liquid_stats() {
        let registry = registry();
        let bundle = bundle();
        let ctx = ctx(&registry, &bundle);
        let id = registry.liquid_id("cryofluid").expect("cryofluid");
        let stats = content_stats(&ctx, ContentRef::liquid(id));
        let map = stats.to_map();
        assert_eq!(map.len(), 1);
        let rows = &map[0].1;
        let names: Vec<&str> = rows.iter().map(|(stat, _)| stat.name()).collect();
        // `Stats.toMap()` sorts by `Stat.id`, so the display order is
        // heatCapacity(11) / viscosity(12) / temperature(13) despite the
        // declaration order in `Liquid.setStats`.
        assert_eq!(
            names,
            vec![
                "explosiveness",
                "flammability",
                "heatCapacity",
                "viscosity",
                "temperature"
            ]
        );
        let value = |name: &str| -> Cell {
            rows.iter()
                .find(|(stat, _)| stat.name() == name)
                .map(|(_, values)| values[0][0].clone())
                .unwrap_or(Cell::Row)
        };
        // `Liquids.java:40-47` cryofluid: temperature 0.25, heatCapacity 0.9,
        // viscosity stays at the 0.5 default.
        assert_eq!(value("temperature"), Cell::Text { t: "25".into() });
        assert_eq!(value("heatCapacity"), Cell::Text { t: "90".into() });
        assert_eq!(value("viscosity"), Cell::Text { t: "50".into() });
    }

    #[test]
    fn stats_sort_and_dedup() {
        let mut stats = Stats::new();
        stats.add(Stat::PowerUse, vec![values::text("a")]);
        stats.add(Stat::Health, vec![values::text("b")]);
        stats.add(Stat::Health, vec![values::text("c")]);
        let map = stats.to_map();
        assert_eq!(map[0].0, StatCat::General);
        assert_eq!(map[1].0, StatCat::Power);
        assert_eq!(map[0].1.len(), 1);
        assert_eq!(map[0].1[0].1.len(), 2);
    }
}
