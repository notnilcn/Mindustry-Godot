// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla liquid registry.
//!
//! Ported from `core/src/mindustry/content/Liquids.java` (11 liquids, exact
//! `load()` order) and `core/src/mindustry/type/{Liquid,CellLiquid}.java`
//! (metadata + `init()` gas rules; puddle behavior in plan 09).

use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::{LiquidId, StatusId};
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};
use super::fx_meta::EffectId;

/// `CellLiquid` extra fields (neoplasm).
#[derive(Debug, Clone, PartialEq)]
pub struct CellLiquidFields {
    /// Cell draw color start.
    pub color_from: Rgba,
    /// Cell draw color end.
    pub color_to: Rgba,
    /// Number of cells drawn.
    pub cells: i32,
    /// Liquid this spreads into.
    pub spread_target: Option<LiquidId>,
    /// Max spread per tick (scaled).
    pub max_spread: f32,
    /// Amount conversion factor when spreading.
    pub spread_conversion: f32,
    /// Damage dealt to the block it sits on.
    pub spread_damage: f32,
    /// Scaling applied when removing spread target.
    pub remove_scaling: f32,
}

impl Default for CellLiquidFields {
    fn default() -> Self {
        Self {
            color_from: Rgba::WHITE,
            color_to: Rgba::WHITE,
            cells: 6,
            spread_target: None,
            max_spread: 0.75,
            spread_conversion: 1.2,
            spread_damage: 0.11,
            remove_scaling: 0.25,
        }
    }
}

/// Liquid content record (`mindustry.type.Liquid`; `cell` holds `CellLiquid` extras).
#[derive(Debug, Clone, PartialEq)]
pub struct Liquid {
    /// Dense id in the liquid content space.
    pub id: LiquidId,
    /// Content name (parity ABI).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// If true, this fluid is treated as a gas (and does not create puddles).
    pub gas: bool,
    /// Color used in pipes and on the ground.
    pub color: Rgba,
    /// Color of this liquid in gas form.
    pub gas_color: Rgba,
    /// Color used in bars.
    pub bar_color: Option<Rgba>,
    /// Color used to draw lights (alpha = brightness).
    pub light_color: Rgba,
    /// 0-1 flammability; >0.5 is very flammable.
    pub flammability: f32,
    /// Temperature: 0.5 is room temperature, 1 is molten hot.
    pub temperature: f32,
    /// Heat storage capacity (water = 0.4).
    pub heat_capacity: f32,
    /// Thickness (water = 0.5, tar ~1).
    pub viscosity: f32,
    /// Explosiveness when heated.
    pub explosiveness: f32,
    /// Whether this fluid reacts in blocks at all.
    pub block_reactive: bool,
    /// If false, this liquid cannot be a coolant.
    pub coolant: bool,
    /// If true, this liquid can move through blocks as a puddle.
    pub move_through_blocks: bool,
    /// If true, this liquid can be incinerated.
    pub incinerable: bool,
    /// Associated status effect.
    pub effect: StatusId,
    /// Effect shown in puddles.
    pub particle_effect: EffectId,
    /// Particle effect rate spacing in ticks.
    pub particle_spacing: f32,
    /// Temperature at which this liquid vaporizes.
    pub boil_point: f32,
    /// If true, puddle size is capped.
    pub cap_puddles: bool,
    /// Effect when this liquid vaporizes.
    pub vapor_effect: EffectId,
    /// If true, this liquid is hidden in most UI.
    pub hidden: bool,
    /// Liquids this puddle can stay on (insertion-ordered set).
    pub can_stay_on: Vec<LiquidId>,
    /// `CellLiquid` behavior fields, when this is a cell liquid.
    pub cell: Option<CellLiquidFields>,
}

impl Liquid {
    /// Creates a liquid with upstream defaults (`Liquid(String, Color)`).
    pub fn new(name: &str, color: Rgba, bundle: &dyn BundleView, store: &dyn UnlockStore) -> Self {
        Self {
            id: LiquidId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Liquid, name, bundle, store),
            gas: false,
            color,
            gas_color: Rgba::LIGHT_GRAY,
            bar_color: None,
            light_color: Rgba::CLEAR,
            flammability: 0.0,
            temperature: 0.5,
            heat_capacity: 0.5,
            viscosity: 0.5,
            explosiveness: 0.0,
            block_reactive: true,
            coolant: true,
            move_through_blocks: false,
            incinerable: true,
            effect: StatusId::NONE,
            particle_effect: EffectId::NONE,
            particle_spacing: 60.0,
            boil_point: 2.0,
            cap_puddles: true,
            vapor_effect: EffectId::VAPOR,
            hidden: false,
            can_stay_on: Vec::new(),
            cell: None,
        }
    }

    /// Marks this liquid as a `CellLiquid`.
    pub fn as_cell_liquid(&mut self, fields: CellLiquidFields) -> &mut Self {
        self.cell = Some(fields);
        self
    }

    /// `Liquid.isHidden()`.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// `Liquid.barColor()`.
    pub fn bar_color(&self) -> Rgba {
        self.bar_color.unwrap_or(self.color)
    }

    /// `Liquid.canExtinguish()`.
    pub fn can_extinguish(&self) -> bool {
        self.flammability < 0.1 && self.temperature <= 0.5
    }

    /// Adds a `canStayOn` entry if absent (insertion order preserved).
    pub fn add_can_stay_on(&mut self, liquid: LiquidId) {
        if !self.can_stay_on.contains(&liquid) {
            self.can_stay_on.push(liquid);
        }
    }
}

impl Content for Liquid {
    const TYPE: ContentType = ContentType::Liquid;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = LiquidId::new(id);
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
        if self.cell.is_some() {
            "CellLiquid"
        } else {
            "Liquid"
        }
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    /// `Liquid.init()` gas rules (plan 02 §3.8).
    fn init_self(&mut self) -> Result<(), ContentError> {
        if self.gas {
            // Always "boils", it's a gas.
            self.boil_point = -1.0;
            // All gases are transparent.
            self.color.a = 0.6;
            // For gases, gas color is implicitly their color.
            self.gas_color = self.color;
            if self.bar_color.is_none() {
                self.bar_color = Some(self.color.with_alpha(1.0));
            }
        }
        Ok(())
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for Liquid {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for Liquid {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// Loads all 11 vanilla liquids in `Liquids.load()` order.
pub fn load(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    // Status effects load before liquids (upstream order); a missing status is a
    // load-order error, not a silent fallback.
    let missing = |name: &str| ContentError::UnknownName(name.to_owned());
    let wet = registry.status_id("wet").ok_or_else(|| missing("wet"))?;
    let melting = registry
        .status_id("melting")
        .ok_or_else(|| missing("melting"))?;
    let tarred = registry
        .status_id("tarred")
        .ok_or_else(|| missing("tarred"))?;
    let freezing = registry
        .status_id("freezing")
        .ok_or_else(|| missing("freezing"))?;

    registry.add_liquid({
        let mut liquid = Liquid::new("water", hex("596ab8"), bundle, store);
        liquid.heat_capacity = 0.4;
        liquid.effect = wet;
        liquid.boil_point = 0.5;
        liquid.gas_color = Rgba::new(0.9, 0.9, 0.9, 1.0);
        liquid.unlock.always_unlocked = true;
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("slag", hex("ffa166"), bundle, store);
        liquid.temperature = 1.0;
        liquid.viscosity = 0.7;
        liquid.effect = melting;
        liquid.light_color = hex("f0511d").with_alpha(0.4);
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("oil", hex("313131"), bundle, store);
        liquid.viscosity = 0.75;
        liquid.flammability = 1.2;
        liquid.explosiveness = 1.2;
        liquid.heat_capacity = 0.7;
        liquid.bar_color = Some(hex("6b675f"));
        liquid.effect = tarred;
        liquid.boil_point = 0.65;
        liquid.gas_color = Rgba::new(0.4, 0.4, 0.4, 1.0);
        liquid.add_can_stay_on(LiquidId::WATER);
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("cryofluid", hex("6ecdec"), bundle, store);
        liquid.heat_capacity = 0.9;
        liquid.temperature = 0.25;
        liquid.effect = freezing;
        liquid.light_color = hex("0097f5").with_alpha(0.2);
        liquid.boil_point = 0.55;
        liquid.gas_color = hex("c1e8f5");
        liquid
    })?;

    let neoplasm = registry.add_liquid({
        let mut liquid = Liquid::new("neoplasm", hex("c33e2b"), bundle, store);
        liquid.heat_capacity = 0.4;
        liquid.temperature = 0.54;
        liquid.viscosity = 0.85;
        liquid.flammability = 0.0;
        liquid.cap_puddles = false;
        liquid.move_through_blocks = true;
        liquid.incinerable = false;
        liquid.block_reactive = false;
        liquid.add_can_stay_on(LiquidId::WATER);
        liquid.add_can_stay_on(LiquidId::new(2));
        liquid.add_can_stay_on(LiquidId::new(3));
        let mut cell = CellLiquidFields {
            color_from: hex("e8803f"),
            color_to: hex("8c1225"),
            ..CellLiquidFields::default()
        };
        cell.spread_target = Some(LiquidId::WATER);
        liquid.as_cell_liquid(cell);
        liquid
    })?;
    let arkycite = registry.add_liquid({
        let mut liquid = Liquid::new("arkycite", hex("84a94b"), bundle, store);
        liquid.flammability = 0.4;
        liquid.viscosity = 0.7;
        liquid
    })?;
    // `neoplasm.canStayOn.add(arkycite)` in `Liquids.load()`.
    if let Some(neoplasm) = registry.liquid_mut(neoplasm) {
        neoplasm.add_can_stay_on(arkycite);
    }

    registry.add_liquid({
        let mut liquid = Liquid::new("gallium", hex("9a9dbf"), bundle, store);
        liquid.coolant = false;
        liquid.hidden = true;
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("ozone", hex("fc81dd"), bundle, store);
        liquid.gas = true;
        liquid.bar_color = Some(hex("d699f0"));
        liquid.explosiveness = 1.0;
        liquid.flammability = 1.0;
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("hydrogen", hex("9eabf7"), bundle, store);
        liquid.gas = true;
        liquid.flammability = 1.0;
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("nitrogen", hex("efe3ff"), bundle, store);
        liquid.gas = true;
        liquid
    })?;

    registry.add_liquid({
        let mut liquid = Liquid::new("cyanogen", hex("89e8b6"), bundle, store);
        liquid.gas = true;
        liquid.flammability = 2.0;
        liquid
    })?;

    Ok(())
}

/// Known-good color literal helper (fallback white; colors are static content data).
fn hex(value: &str) -> Rgba {
    Rgba::from_hex(value).unwrap_or(Rgba::WHITE)
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `liquids::gas_post_init` (plan 02 §5 M1): `Liquid.init()` gas rules.
    #[test]
    fn gas_post_init() {
        let registry = test_registry();
        assert_eq!(registry.liquids().len(), 11, "liquid count");
        let gas_names = ["ozone", "hydrogen", "nitrogen", "cyanogen"];
        for liquid in registry.liquids() {
            if liquid.gas {
                assert!(
                    gas_names.contains(&liquid.name.as_str()),
                    "unexpected gas {}",
                    liquid.name
                );
                assert_eq!(liquid.boil_point, -1.0, "{}: boilPoint", liquid.name);
                assert_eq!(liquid.color.a, 0.6, "{}: color alpha", liquid.name);
                assert_eq!(liquid.gas_color, liquid.color, "{}: gasColor", liquid.name);
                let expected_bar = if liquid.name == "ozone" {
                    // Ozone sets an explicit bar color; `init()` never overrides it.
                    hex("d699f0")
                } else {
                    liquid.color.with_alpha(1.0)
                };
                assert_eq!(
                    liquid.bar_color(),
                    expected_bar,
                    "{}: barColor",
                    liquid.name
                );
            }
        }
        let water = registry.liquid(LiquidId::WATER).unwrap();
        assert_eq!(water.boil_point, 0.5);
        assert!(!water.gas);
    }

    /// Liquids load in `Liquids.load()` order with those exact names.
    #[test]
    fn load_order_and_names() {
        let registry = test_registry();
        let names: Vec<&str> = registry
            .liquids()
            .iter()
            .map(|liquid| liquid.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "water",
                "slag",
                "oil",
                "cryofluid",
                "neoplasm",
                "arkycite",
                "gallium",
                "ozone",
                "hydrogen",
                "nitrogen",
                "cyanogen",
            ]
        );
        let neoplasm = registry.liquid(LiquidId::new(4)).unwrap();
        assert_eq!(neoplasm.kind_name(), "CellLiquid");
        assert_eq!(
            neoplasm.can_stay_on,
            vec![
                LiquidId::WATER,
                LiquidId::new(2),
                LiquidId::new(3),
                LiquidId::new(5),
            ]
        );
    }
}
