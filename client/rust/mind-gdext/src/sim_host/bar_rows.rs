// SPDX-License-Identifier: GPL-3.0-only

//! Building status-bar rows (`setBars` ports).
//! Upstream: core/src/mindustry/world/blocks/* (setBars)

use mind_core::content::BlockKind;
use mind_core::content::registries::blocks::{BlockDef, Consume};

/// One hovered-building bar row (`Block.setBars`, G03 T6).
pub struct BarRow {
    /// Bar key (`"health"`, `"power"`, `"liquid"`, `"progress"`, ...).
    pub key: &'static str,
    /// Bundle key (no `@`).
    pub label_key: String,
    /// Arguments for the bundle format (`{0}`, `{1}`, `{2}`).
    pub label_args: Vec<String>,
    /// `Bar` colour RGBA8888.
    pub color: u32,
    /// Fraction `0..=1`.
    pub value: f32,
}

/// `Turret` family bar state (`BaseTurret.java:104-110`,
/// `Turret.java:201-207`, `ItemTurret.java:61-67`).
#[derive(Debug, Clone, Default)]
pub struct TurretBars {
    pub total_ammo: i32,
    pub max_ammo: i32,
    pub heat_req: f32,
    pub heat_requirement: f32,
    pub max_heat_efficiency: f32,
    pub activation_timer: f32,
    pub activation_time: f32,
}

/// Power-graph bar state (`PowerNode.java:112-141`, `PowerDiode.java:33-52`).
#[derive(Debug, Clone, Default)]
pub struct PowerBars {
    pub balance: f32,
    pub produced: f32,
    pub needed: f32,
    pub stored: f32,
    pub capacity: f32,
    pub links: usize,
    pub max_nodes: usize,
    /// Diode back/front graph stored fractions (`PowerDiode.bar`).
    pub back: Option<f32>,
    pub front: Option<f32>,
}

/// `PowerGenerator`/`ImpactReactor` bar state
/// (`PowerGenerator.java:88-96`, `ImpactReactor.java:47-54`).
#[derive(Debug, Clone, Default)]
pub struct GeneratorBars {
    /// `getPowerProduction()` (per tick, efficiency included).
    pub production: f32,
    /// `ConsPower.usage` (ImpactReactor net output).
    pub usage: f32,
    /// `productionEfficiency`.
    pub efficiency: f32,
}

/// A factory/reconstructor/cargo-loader `units` bar (three arguments).
#[derive(Debug, Clone, Default)]
pub struct UnitCapBars {
    pub name: String,
    pub count: i32,
    /// `None` = uncapped (`∞`).
    pub cap: Option<i32>,
    pub use_cap: bool,
    /// `false` when the factory has no current plan (`[lightgray]✖`).
    pub has_unit: bool,
}

/// `PayloadLoader` bar state (`PayloadLoader.java:60-66`).
#[derive(Debug, Clone, Default)]
pub struct PayloadLoaderBars {
    pub progress: f32,
    pub payload_has_items: bool,
    pub payload_items: i32,
}

/// Live values the `Block.setBars` family overrides read (J05 T4, L05 T1).
#[derive(Debug, Clone, Default)]
pub struct BarExtras {
    /// `HeatBlock.heat()` and `heatOutput`/`heatRequirement`
    /// (`HeatProducer.java:40`, `HeatConductor.java:34`, `HeatCrafter.java:28`).
    pub heat: Option<(f32, f32)>,
    /// `DrillBuild.lastDrillSpeed`/`warmup` (`Drill.java:117-119`).
    pub drill: Option<(f32, f32)>,
    /// `WallCrafterBuild`: `(lastEfficiency, warmup, drillTime)`
    /// (`WallCrafter.java:68-72`; the bar reads
    /// `lastEfficiency * 60 / drillTime`).
    pub wall_crafter: Option<(f32, f32, f32)>,
    /// `Building.timeScale()`.
    pub time_scale: f32,
    /// `CoreBuild.storageCapacity` plus the unlocked item count
    /// (`CoreBlock.java:148-152`).
    pub core_capacity: Option<(f32, f32)>,
    /// `UnitFactoryBuild`: `(progress, currentPlan)`
    /// (`UnitFactory.java:121-125`).
    pub factory: Option<(f32, i32)>,
    pub turret: Option<TurretBars>,
    pub power: Option<PowerBars>,
    pub generator: Option<GeneratorBars>,
    /// `NuclearReactorBuild.heat` (`NuclearReactor.java:103`).
    pub reactor_heat: Option<f32>,
    /// `VariableReactorBuild`: `(heat, maxHeat, instability)`
    /// (`VariableReactor.java:57-63`).
    pub variable_reactor: Option<(f32, f32, f32)>,
    /// `OverdriveBuild`: `(realBoost, maxBoost)` (`OverdriveProjector.java:80`).
    pub overdrive: Option<(f32, f32)>,
    /// `ForceBuild` shield fill (`ForceProjector.java:99`).
    pub shield: Option<f32>,
    /// `SolidPumpBuild`: `(lastPump, warmup, efficiency)`
    /// (`SolidPump.java:51-55`).
    pub pump: Option<(f32, f32, f32)>,
    pub payload_loader: Option<PayloadLoaderBars>,
    /// `UnitAssemblerBuild`: `(progress, time, unit bars)`
    /// (`UnitAssembler.java:128-143`).
    pub assembler: Option<(f32, f32, Option<UnitCapBars>)>,
    /// `Reconstructor`/`UnitFactory` `units` bar (`UnitFactory.java:127-136`).
    pub factory_units: Option<UnitCapBars>,
    /// `UnitCargoLoader` `units` bar (`UnitCargoLoader.java:51-61`).
    pub cargo_units: Option<UnitCapBars>,
    /// `LaunchPadBuild`: `(launchCounter, launchTime)`
    /// (`LaunchPad.java:69-70`).
    pub launch: Option<(f32, f32)>,
    /// `LandingPadBuild.cooldown` (`LandingPad.java:113-115`).
    pub landing_cooldown: Option<f32>,
    /// `Accelerator` graph battery stored and `powerBufferRequirement`.
    pub accelerator: Option<(f32, f32)>,
    /// `TargetDummyBuild`: `(reset, resetTime)` (`TargetDummy.java:64-73`).
    pub target_dummy: Option<(f32, f32)>,
    /// `GenericCrafter.outputLiquids` output bars: `(name, rgba8888, amount,
    /// capacity)` (`GenericCrafter.java:81-98`).
    pub output_liquids: Vec<(String, u32, f32, f32)>,
}

impl BarExtras {
    fn turret_heat_efficiency(&self) -> f32 {
        self.turret
            .as_ref()
            .map_or(1.0, |turret| turret.max_heat_efficiency.max(1.0))
    }
}

pub(super) fn amount(value: f32) -> String {
    mind_core::ui::text::format_amount_default(value as i64)
        .replace("[gray]", "")
        .replace("[]", "")
}

pub(super) fn cap_text(cap: Option<i32>) -> String {
    cap.map_or_else(|| String::from("∞"), |cap| cap.to_string())
}

pub(super) fn push_unit_cap_rows(rows: &mut Vec<BarRow>, bars: &UnitCapBars) {
    // `bar.unitcap` = `{0} {1}/{2}` (`UnitFactory.java:127-136`).
    let name = format!(":{}:", bars.name);
    if !bars.has_unit {
        rows.push(BarRow {
            key: "units",
            label_key: String::from("bar.unitcap"),
            label_args: vec![name, String::from("0"), cap_text(bars.cap)],
            color: 0xec7b_4cff,
            value: 0.0,
        });
        return;
    }
    let cap = if bars.use_cap { bars.cap } else { None };
    let value = match (bars.use_cap, cap) {
        (true, Some(cap)) if cap > 0 => (bars.count as f32 / cap as f32).clamp(0.0, 1.0),
        _ => 1.0,
    };
    rows.push(BarRow {
        key: "units",
        label_key: String::from("bar.unitcap"),
        label_args: vec![name, bars.count.to_string(), cap_text(cap)],
        color: 0xec7b_4cff,
        value,
    });
}

/// Default rows every family starts from (`Block.setBars` prelude): health
/// (or the target-dummy `dps` bar), the base `power` bar, the item bar and
/// the liquid bars.
fn push_common_rows(
    def: &BlockDef,
    health: f32,
    power_status: Option<f32>,
    items_total: Option<i32>,
    liquids: Option<&[f32]>,
    liquid_info: &[(String, u32)],
    extras: &BarExtras,
) -> Vec<BarRow> {
    let mut rows = Vec::new();
    // `TargetDummy.java:64-73`: `removeBar("health")` and a `dps` bar.
    if def.kind == BlockKind::TargetDummy {
        if let Some((reset, reset_time)) = extras.target_dummy {
            let fraction = if reset_time > 0.0 {
                (1.0 - reset / reset_time).clamp(0.0, 1.0)
            } else {
                0.0
            };
            rows.push(BarRow {
                key: "dps",
                label_key: String::from("bar.dps"),
                label_args: Vec::new(),
                color: 0xff89_47ff,
                value: fraction,
            });
        }
    } else {
        rows.push(BarRow {
            key: "health",
            label_key: String::from("stat.health"),
            label_args: Vec::new(),
            color: 0xff34_1cff,
            value: health.clamp(0.0, 1.0),
        });
    }
    if let Some(status) = power_status {
        let buffered = def.consumes.iter().find_map(|spec| match &spec.consume {
            Consume::Power { buffered, .. } if *buffered > 0.0 => Some(*buffered),
            _ => None,
        });
        let (label_key, label_args) = match buffered {
            Some(capacity) => (
                String::from("bar.poweramount"),
                vec![((status * capacity) as i32).to_string()],
            ),
            None => (String::from("bar.power"), Vec::new()),
        };
        rows.push(BarRow {
            key: "power",
            label_key,
            label_args,
            color: 0xec7b_4cff,
            value: status.clamp(0.0, 1.0),
        });
    }
    // `ItemSource.java:41`, `Unloader.java:74`, `DirectionalUnloader.java:79`:
    // the item bar is removed for these families.
    let removes_items = matches!(
        def.kind,
        BlockKind::ItemSource | BlockKind::Unloader | BlockKind::DirectionalUnloader
    );
    // `LiquidSource.java:45`, `LiquidVoid.java:23`, `LiquidJunction.java:29`:
    // the liquid bar is removed for these families.
    let removes_liquid = matches!(
        def.kind,
        BlockKind::LiquidSource | BlockKind::LiquidVoid | BlockKind::LiquidJunction
    );
    // `LaunchPad.java:69-70` appends its own `items` bar after the base rows.
    let launch_items = def.kind == BlockKind::LaunchPad;
    if def.has_items
        && def.configurable
        && def.item_capacity > 0
        && !removes_items
        && !launch_items
        && let Some(total) = items_total
    {
        rows.push(BarRow {
            key: "items",
            label_key: String::from("bar.items"),
            label_args: vec![total.to_string()],
            color: 0x2ea7_56ff,
            value: (total as f32 / def.item_capacity as f32).clamp(0.0, 1.0),
        });
    }
    // `GenericCrafter.java:81-98`: output liquids replace the dynamic bar.
    if !extras.output_liquids.is_empty() {
        for (name, color, amount, capacity) in &extras.output_liquids {
            rows.push(BarRow {
                key: "liquid",
                label_key: format!("liquid.{name}.name"),
                label_args: Vec::new(),
                color: *color,
                value: if *capacity > 0.0 {
                    (amount / capacity).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            });
        }
    } else if def.has_liquids && def.liquid_capacity > 0.0 && !removes_liquid {
        let amounts = liquids.unwrap_or(&[]);
        let mut added = false;
        for spec in &def.consumes {
            let consumed: Vec<mind_core::content::LiquidId> = match &spec.consume {
                Consume::Liquid { liquid, .. } => vec![*liquid],
                Consume::Liquids(stacks) => stacks.iter().map(|stack| stack.liquid).collect(),
                _ => Vec::new(),
            };
            for liquid in consumed {
                let Some((name, color)) = liquid_info.get(liquid.index()) else {
                    continue;
                };
                let amount = amounts.get(liquid.index()).copied().unwrap_or(0.0);
                rows.push(BarRow {
                    key: "liquid",
                    label_key: format!("liquid.{name}.name"),
                    label_args: Vec::new(),
                    color: *color,
                    value: (amount / def.liquid_capacity).clamp(0.0, 1.0),
                });
                added = true;
            }
        }
        if !added {
            // Dynamic current-liquid bar (`bar.liquid` when empty).
            let (index, amount) = amounts.iter().enumerate().fold(
                (usize::MAX, 0.0f32),
                |(best, max), (i, amount)| {
                    if *amount > max {
                        (i, *amount)
                    } else {
                        (best, max)
                    }
                },
            );
            let (label_key, color) = if amount > 0.0 {
                liquid_info
                    .get(index)
                    .map(|(name, color)| (format!("liquid.{name}.name"), *color))
                    .unwrap_or((String::from("bar.liquid"), 0x596a_b8ff))
            } else {
                (String::from("bar.liquid"), 0x596a_b8ff)
            };
            rows.push(BarRow {
                key: "liquid",
                label_key,
                label_args: Vec::new(),
                color,
                value: (amount / def.liquid_capacity).clamp(0.0, 1.0),
            });
        }
    }
    rows
}

/// `CoreBlock.setBars` (`CoreBlock.java:148-152`): storage capacity against
/// the unlocked item count.
fn push_core_rows(rows: &mut Vec<BarRow>, extras: &BarExtras, items_total: Option<i32>) {
    if let Some((capacity, unlocked)) = extras.core_capacity
        && capacity > 0.0
        && unlocked > 0.0
    {
        rows.push(BarRow {
            key: "capacity",
            label_key: String::from("bar.capacity"),
            label_args: vec![amount(capacity)],
            color: 0x2ea7_56ff,
            value: (items_total.unwrap_or(0) as f32 / (capacity * unlocked)).clamp(0.0, 1.0),
        });
    }
}

/// `Drill.setBars` (`Drill.java:117-119`): `lastDrillSpeed * 60 * timeScale`.
fn push_drill_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((speed, warmup)) = extras.drill {
        rows.push(BarRow {
            key: "drillspeed",
            label_key: String::from("bar.drillspeed"),
            label_args: vec![format!("{:.2}", speed * 60.0 * extras.time_scale)],
            color: 0xff89_47ff,
            value: warmup.clamp(0.0, 1.0),
        });
    }
}

// `WallCrafter.setBars` (`WallCrafter.java:68-72`):
// `lastEfficiency * 60 / drillTime` (no `timeScale` factor — it is
// already folded into `lastEfficiency`).
fn push_wall_crafter_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((last_efficiency, warmup, drill_time)) = extras.wall_crafter {
        rows.push(BarRow {
            key: "drillspeed",
            label_key: String::from("bar.drillspeed"),
            label_args: vec![format!(
                "{:.2}",
                last_efficiency * 60.0 / drill_time.max(0.0001)
            )],
            color: 0xff89_47ff,
            value: warmup.clamp(0.0, 1.0),
        });
    }
}

/// `HeatProducer`/`HeatConductor` heat bar (`HeatProducer.java:40`,
/// `HeatConductor.java:34`).
fn push_heat_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((heat, output)) = extras.heat
        && output > 0.0
    {
        rows.push(BarRow {
            key: "heat",
            label_key: String::from("bar.heat"),
            label_args: Vec::new(),
            color: 0xf680_21ff,
            value: (heat / output).clamp(0.0, 1.0),
        });
    }
}

/// `HeatCrafter.setBars` (`HeatCrafter.java:28-34`).
fn push_heat_crafter_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((heat, requirement)) = extras.heat
        && requirement > 0.0
    {
        // `HeatCrafter.java:28-34`: `bar.heatpercent` = `{0} ({1}%)`.
        rows.push(BarRow {
            key: "heat",
            label_key: String::from("bar.heatpercent"),
            label_args: vec![
                format!("{}", heat as i32),
                format!(
                    "{}",
                    (heat / requirement).min(extras.turret_heat_efficiency()) * 100.0
                ),
            ],
            color: 0xf680_21ff,
            value: (heat / requirement).clamp(0.0, 1.0),
        });
    }
}

/// `UnitFactory`/`Reconstructor` progress + unit-cap rows
/// (`UnitFactory.java:121-136`).
fn push_unit_factory_rows(rows: &mut Vec<BarRow>, def: &BlockDef, extras: &BarExtras) {
    if let Some((progress, plan)) = extras.factory {
        let time = usize::try_from(plan)
            .ok()
            .and_then(|index| def.unit_plans.get(index))
            .map(|unit| unit.time)
            .unwrap_or(0.0);
        let fraction = if time > 0.0 {
            (progress / time).clamp(0.0, 1.0)
        } else {
            0.0
        };
        rows.push(BarRow {
            key: "progress",
            label_key: String::from("bar.progress"),
            label_args: vec![format!("{}", (fraction * 100.0) as i32)],
            color: 0xff89_47ff,
            value: fraction,
        });
    }
    if let Some(units) = &extras.factory_units {
        push_unit_cap_rows(rows, units);
    }
}

/// `UnitAssembler.setBars` (`UnitAssembler.java:128-143`).
fn push_assembler_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((progress, _time, units)) = &extras.assembler {
        rows.push(BarRow {
            key: "progress",
            label_key: String::from("bar.progress"),
            label_args: vec![format!("{}", (progress * 100.0) as i32)],
            color: 0xff89_47ff,
            value: progress.clamp(0.0, 1.0),
        });
        if let Some(units) = units {
            push_unit_cap_rows(rows, units);
        }
    }
}

/// `UnitCargoLoader` units bar (`UnitCargoLoader.java:51-61`).
fn push_cargo_loader_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some(units) = &extras.cargo_units {
        push_unit_cap_rows(rows, units);
    }
}

/// Turret family bars (`BaseTurret.java:104-110`, `Turret.java:201-207`,
/// `ItemTurret.java:61-67`).
fn push_turret_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some(turret) = &extras.turret {
        if turret.activation_time > 0.0 {
            // `BaseTurret.java:104-110`.
            let active = turret.activation_timer > 0.0;
            rows.push(BarRow {
                key: "activationtimer",
                label_key: String::from(if active {
                    "bar.activationtimer"
                } else {
                    "bar.activated"
                }),
                label_args: if active {
                    vec![format!(
                        "{}",
                        (turret.activation_timer / 60.0).ceil() as i32
                    )]
                } else {
                    Vec::new()
                },
                color: if active { 0xf680_21ff } else { 0x8ca9_e8ff },
                value: (1.0 - turret.activation_timer / turret.activation_time).clamp(0.0, 1.0),
            });
        }
        if turret.heat_requirement > 0.0 {
            // `Turret.java:201-207`.
            let percent =
                (turret.heat_req / turret.heat_requirement).min(turret.max_heat_efficiency) * 100.0;
            rows.push(BarRow {
                key: "heat",
                label_key: String::from("bar.heatpercent"),
                label_args: vec![
                    format!("{}", turret.heat_req as i32),
                    format!("{}", percent as i32),
                ],
                color: 0xf680_21ff,
                value: (turret.heat_req / turret.heat_requirement).clamp(0.0, 1.0),
            });
        }
        if turret.max_ammo > 0 {
            // `ItemTurret.java:61-67`.
            rows.push(BarRow {
                key: "ammo",
                label_key: String::from("stat.ammo"),
                label_args: Vec::new(),
                color: 0xff89_47ff,
                value: (turret.total_ammo as f32 / turret.max_ammo as f32).clamp(0.0, 1.0),
            });
        }
    }
}

/// `PowerNode`/`BeamNode`/`PowerDiode`/`Accelerator` bars
/// (`PowerNode.java:112-141`, `PowerDiode.java:33-52`).
fn push_power_rows(rows: &mut Vec<BarRow>, def: &BlockDef, extras: &BarExtras) {
    if let Some(power) = &extras.power
        && def.kind != BlockKind::PowerDiode
    {
        // `PowerNode.java:115-122`, `BeamNode.java:53-54`.
        rows.push(BarRow {
            key: "power",
            label_key: String::from("bar.powerbalance"),
            label_args: vec![format!(
                "{}{}",
                if power.balance >= 0.0 { "+" } else { "" },
                amount(power.balance * 60.0)
            )],
            color: 0xec7b_4cff,
            value: if power.needed > 0.0 {
                (power.produced / power.needed).clamp(0.0, 1.0)
            } else {
                0.0
            },
        });
        rows.push(BarRow {
            key: "batteries",
            label_key: String::from("bar.powerstored"),
            label_args: vec![amount(power.stored), amount(power.capacity)],
            color: 0xec7b_4cff,
            value: if power.capacity > 0.0 {
                (power.stored / power.capacity).clamp(0.0, 1.0)
            } else {
                0.0
            },
        });
        if def.kind != BlockKind::BeamNode && power.max_nodes > 0 {
            rows.push(BarRow {
                key: "connections",
                label_key: String::from("bar.powerlines"),
                label_args: vec![power.links.to_string(), power.max_nodes.to_string()],
                color: 0x2ea7_56ff,
                value: (power.links as f32 / power.max_nodes as f32).clamp(0.0, 1.0),
            });
        }
    }
    // `PowerDiode.java:33-48`: `bar.input`/`bar.output` are always
    // present; `bar(Building)` returns 0 when the side has no graph.
    if def.kind == BlockKind::PowerDiode {
        let (back, front) = extras.power.as_ref().map_or((0.0, 0.0), |power| {
            (power.back.unwrap_or(0.0), power.front.unwrap_or(0.0))
        });
        for (key, label, value) in [("back", "bar.input", back), ("front", "bar.output", front)] {
            rows.push(BarRow {
                key,
                label_key: String::from(label),
                label_args: Vec::new(),
                color: 0xec7b_4cff,
                value: value.clamp(0.0, 1.0),
            });
        }
    }
    if def.kind == BlockKind::Accelerator
        && let Some((stored, requirement)) = extras.accelerator
        && requirement > 0.0
    {
        rows.push(BarRow {
            key: "powerBufferRequirement",
            label_key: String::from("bar.powerbuffer"),
            label_args: vec![amount(stored), amount(requirement)],
            color: 0xec7b_4cff,
            value: (stored / requirement).clamp(0.0, 1.0),
        });
    }
}

/// `NuclearReactor.setBars` (`NuclearReactor.java:103`).
fn push_nuclear_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some(heat) = extras.reactor_heat {
        rows.push(BarRow {
            key: "heat",
            label_key: String::from("bar.heat"),
            label_args: Vec::new(),
            color: 0xf680_21ff,
            value: heat.clamp(0.0, 1.0),
        });
    }
}

/// `VariableReactor.setBars` (`VariableReactor.java:57-63`).
fn push_variable_reactor_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((heat, max_heat, instability)) = extras.variable_reactor {
        rows.push(BarRow {
            key: "instability",
            label_key: String::from("bar.instability"),
            label_args: Vec::new(),
            color: 0x665c_9fff,
            value: instability.clamp(0.0, 1.0),
        });
        if max_heat > 0.0 {
            rows.push(BarRow {
                key: "heat",
                label_key: String::from("bar.heatpercent"),
                label_args: vec![
                    format!("{}", heat as i32),
                    format!("{}", (heat / max_heat).clamp(0.0, 1.0) * 100.0),
                ],
                color: 0xf680_21ff,
                value: (heat / max_heat).clamp(0.0, 1.0),
            });
        }
    }
}

/// Generator bars (`PowerGenerator.java:88-96`, `ImpactReactor.java:47-54`,
/// `HeaterGenerator.java:38-42`).
fn push_generator_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some(generator) = &extras.generator {
        // `PowerGenerator.java:88-96`; `ImpactReactor.java:44-54` nets
        // the consumed power.
        let production = (generator.production - generator.usage).max(0.0);
        rows.push(BarRow {
            key: "power",
            label_key: String::from("bar.poweroutput"),
            label_args: vec![format!("{:.1}", production * 60.0 * extras.time_scale)],
            color: 0xec7b_4cff,
            value: generator.efficiency.clamp(0.0, 1.0),
        });
    }
    // `HeaterGenerator.java:38-42`: `super.setBars()` (the power row
    // above) then `addBar("heat", ... Pal.lightOrange ...)`. Only
    // `HeaterGenerator` carries `HeatState`, so the other generators
    // stay power-only without a block-name check.
    if let Some((heat, output)) = extras.heat
        && output > 0.0
    {
        rows.push(BarRow {
            key: "heat",
            label_key: String::from("bar.heat"),
            label_args: Vec::new(),
            color: 0xf680_21ff,
            value: (heat / output).clamp(0.0, 1.0),
        });
    }
}

/// `OverdriveProjector.setBars` (`OverdriveProjector.java:80`).
fn push_overdrive_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((boost, max)) = extras.overdrive {
        rows.push(BarRow {
            key: "boost",
            label_key: String::from("bar.boost"),
            label_args: vec![format!(
                "{}",
                ((boost * 100.0 - 100.0).max(0.0)).round() as i32
            )],
            color: 0xffd3_7fff,
            value: if max > 0.0 {
                (boost / max).clamp(0.0, 1.0)
            } else {
                0.0
            },
        });
    }
}

/// `ForceProjector.setBars` (`ForceProjector.java:99`).
fn push_force_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some(fill) = extras.shield {
        rows.push(BarRow {
            key: "shield",
            label_key: String::from("stat.shieldhealth"),
            label_args: Vec::new(),
            color: 0xffd3_7fff,
            value: fill.clamp(0.0, 1.0),
        });
    }
}

/// `SolidPump`/`Fracker` efficiency bar (`SolidPump.java:51-55`).
fn push_pump_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some((last_pump, warmup, efficiency)) = extras.pump {
        rows.push(BarRow {
            key: "efficiency",
            label_key: String::from("bar.pumpspeed"),
            label_args: vec![format!("{:.1}", last_pump * 60.0)],
            color: 0xff89_47ff,
            value: (warmup * efficiency).clamp(0.0, 1.0),
        });
    }
}

/// `PayloadLoader`/`PayloadUnloader` progress bar (`PayloadLoader.java:60-66`).
fn push_payload_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    // `PayloadLoader.java:60-66`: the bar is added unconditionally;
    // the live extras carry the carried build's fill.
    let loader = extras.payload_loader.clone().unwrap_or_default();
    let has_items = loader.payload_has_items;
    rows.push(BarRow {
        key: "progress",
        label_key: String::from(if has_items {
            "bar.items"
        } else {
            "bar.loadprogress"
        }),
        label_args: if has_items {
            vec![loader.payload_items.to_string()]
        } else {
            Vec::new()
        },
        color: 0x2ea7_56ff,
        value: loader.progress.clamp(0.0, 1.0),
    });
}

/// `LaunchPad.setBars` (`LaunchPad.java:69-70`).
fn push_launch_rows(
    rows: &mut Vec<BarRow>,
    def: &BlockDef,
    extras: &BarExtras,
    items_total: Option<i32>,
) {
    if let Some(total) = items_total {
        rows.push(BarRow {
            key: "items",
            label_key: String::from("bar.items"),
            label_args: vec![total.to_string()],
            color: 0x2ea7_56ff,
            value: (total as f32 / def.item_capacity.max(1) as f32).clamp(0.0, 1.0),
        });
    }
    if let Some((counter, time)) = extras.launch {
        rows.push(BarRow {
            key: "progress",
            label_key: String::from("bar.launchcooldown"),
            label_args: Vec::new(),
            color: 0xff89_47ff,
            value: if time > 0.0 {
                (counter / time).clamp(0.0, 1.0)
            } else {
                0.0
            },
        });
    }
}

/// `LandingPad.setBars` (`LandingPad.java:113-115`).
fn push_landing_rows(rows: &mut Vec<BarRow>, extras: &BarExtras) {
    if let Some(cooldown) = extras.landing_cooldown {
        rows.push(BarRow {
            key: "cooldown",
            label_key: String::from("bar.cooldown"),
            label_args: Vec::new(),
            color: 0xf680_21ff,
            value: cooldown.clamp(0.0, 1.0),
        });
    }
}

/// `Block.setBars` rows for one building, in Java order.
///
/// `def` is the block, `health`/`power_status`/`items_total`/`liquids` the
/// live component state, `liquid_info` the registry names/colours and `extras`
/// the family-specific values from [`BarExtras`].
pub fn building_bar_rows(
    def: &BlockDef,
    health: f32,
    power_status: Option<f32>,
    items_total: Option<i32>,
    liquids: Option<&[f32]>,
    liquid_info: &[(String, u32)],
    extras: &BarExtras,
) -> Vec<BarRow> {
    let mut rows = push_common_rows(
        def,
        health,
        power_status,
        items_total,
        liquids,
        liquid_info,
        extras,
    );
    // Family `addBar` calls append after the defaults, in `setBars` order.
    match def.kind {
        BlockKind::CoreBlock => push_core_rows(&mut rows, extras, items_total),
        BlockKind::Drill => push_drill_rows(&mut rows, extras),
        BlockKind::WallCrafter => push_wall_crafter_rows(&mut rows, extras),
        BlockKind::HeatProducer | BlockKind::HeatConductor => push_heat_rows(&mut rows, extras),
        BlockKind::HeatCrafter => push_heat_crafter_rows(&mut rows, extras),
        BlockKind::UnitFactory | BlockKind::Reconstructor => {
            push_unit_factory_rows(&mut rows, def, extras)
        }
        BlockKind::UnitAssembler => push_assembler_rows(&mut rows, extras),
        BlockKind::UnitCargoLoader => push_cargo_loader_rows(&mut rows, extras),
        BlockKind::ItemTurret
        | BlockKind::LiquidTurret
        | BlockKind::PowerTurret
        | BlockKind::LaserTurret
        | BlockKind::ContinuousLiquidTurret
        | BlockKind::ContinuousTurret
        | BlockKind::PointDefenseTurret
        | BlockKind::TractorBeamTurret
        | BlockKind::RepairTurret
        | BlockKind::BuildTurret => push_turret_rows(&mut rows, extras),
        BlockKind::PowerNode
        | BlockKind::LongPowerNode
        | BlockKind::BeamNode
        | BlockKind::PowerDiode
        | BlockKind::Accelerator => push_power_rows(&mut rows, def, extras),
        BlockKind::NuclearReactor => push_nuclear_rows(&mut rows, extras),
        BlockKind::VariableReactor => push_variable_reactor_rows(&mut rows, extras),
        BlockKind::ConsumeGenerator
        | BlockKind::ThermalGenerator
        | BlockKind::SolarGenerator
        | BlockKind::HeaterGenerator
        | BlockKind::ImpactReactor => push_generator_rows(&mut rows, extras),
        BlockKind::OverdriveProjector => push_overdrive_rows(&mut rows, extras),
        BlockKind::ForceProjector => push_force_rows(&mut rows, extras),
        BlockKind::SolidPump | BlockKind::Fracker => push_pump_rows(&mut rows, extras),
        BlockKind::PayloadLoader | BlockKind::PayloadUnloader => {
            push_payload_rows(&mut rows, extras)
        }
        BlockKind::LaunchPad => push_launch_rows(&mut rows, def, extras, items_total),
        BlockKind::LandingPad => push_landing_rows(&mut rows, extras),
        _ => {}
    }
    rows
}
