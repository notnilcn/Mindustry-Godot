// SPDX-License-Identifier: GPL-3.0-only

//! `BlockSink`, the sim-facing `Blocks` name/id table and the wave loader.
//! Upstream: core/src/mindustry/content/Blocks.java (`load`)

use super::*;

use indexmap::IndexMap;

use crate::content::bundle::BundleView;
use crate::content::id::{BlockId, PlanetId};
use crate::content::registries::ContentRegistry;
use crate::content::settings_store::UnlockStore;
use crate::content::ContentError;

use super::{BlockDef, BlockSpec, BlockKind};

/// Sink for generated block waves.
pub trait BlockSink {
    /// Adds one generated block spec.
    fn push(&mut self, spec: BlockSpec) -> Result<(), ContentError>;
}

/// Sink that registers block names into the sim-facing [`Blocks`] table.
pub(super) struct NamesSink<'a> {
    blocks: &'a mut Blocks,
}

impl BlockSink for NamesSink<'_> {
    fn push(&mut self, spec: BlockSpec) -> Result<(), ContentError> {
        self.blocks.register(spec.name)?;
        Ok(())
    }
}

/// Sink that builds `BlockDef` records inside a [`ContentRegistry`].
pub(super) struct RegistrySink<'a> {
    registry: &'a mut ContentRegistry,
    bundle: &'a dyn BundleView,
    store: &'a dyn UnlockStore,
}

impl BlockSink for RegistrySink<'_> {
    fn push(&mut self, spec: BlockSpec) -> Result<(), ContentError> {
        let decorates = spec.decorates.clone();
        let def = BlockDef::from_spec(spec, self.registry, self.bundle, self.store)?;
        let id = self.registry.add_block(def)?;
        // `<floor>.asFloor().decoration = this` (`Blocks.java:705-822`).
        for floor in decorates {
            let floor_id = self
                .registry
                .block_id(floor)
                .ok_or_else(|| ContentError::UnknownName(floor.to_owned()))?;
            if let Some(def) = self.registry.block_mut(floor_id) {
                def.decoration = id;
            }
        }
        Ok(())
    }
}

/// Sim-facing block name/id table (`Sim.content`).
///
/// Built from the same generated waves as [`ContentRegistry`], so scenario block
/// names stay valid. Kept as a name table (not full records) so the P0 sim and
/// `mind-gdext` bindings do not depend on the metadata record layout.
#[derive(Debug, Clone, Default)]
pub struct Blocks {
    names: Vec<String>,
    by_name: IndexMap<String, u16>,
}

impl Blocks {
    /// Creates the vanilla table (all ported waves, upstream order).
    pub fn new() -> Self {
        let mut blocks = Self::default();
        load_names(&mut blocks);
        debug_assert!(blocks.assert_invariants().is_ok());
        blocks
    }

    /// Number of registered blocks.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the registry has no blocks (never true for `Blocks::new`).
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Name for an id (parity ABI).
    pub fn name(&self, id: BlockId) -> Result<&str, ContentError> {
        self.names
            .get(id.index())
            .map(String::as_str)
            .ok_or(ContentError::UnknownId(id.raw()))
    }

    /// Id for a name (parity ABI).
    pub fn id(&self, name: &str) -> Result<BlockId, ContentError> {
        self.by_name
            .get(name)
            .map(|id| BlockId::new(*id))
            .ok_or_else(|| ContentError::UnknownName(name.to_owned()))
    }

    /// Whether a name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Appends a block, returning its id. The name is the parity ABI and is
    /// rejected on a second registration.
    pub fn register(&mut self, name: &str) -> Result<BlockId, ContentError> {
        if self.by_name.contains_key(name) {
            return Err(ContentError::DuplicateName(name.to_owned()));
        }
        let raw = u16::try_from(self.names.len()).map_err(|_| ContentError::IdSpaceExhausted)?;
        self.names.push(name.to_owned());
        self.by_name.insert(name.to_owned(), raw);
        Ok(BlockId::new(raw))
    }

    /// Asserts the append-only invariants of the block table.
    pub fn assert_invariants(&self) -> Result<(), ContentError> {
        if self.name(BlockId::AIR)? != "air" {
            return Err(ContentError::UnknownName(String::from(
                "air must stay block id 0",
            )));
        }
        if self.name(BlockId::STONE_WALL)? != "stone-wall" {
            return Err(ContentError::UnknownName(String::from(
                "stone-wall must keep its upstream id",
            )));
        }
        if self.len() != self.by_name.len() {
            return Err(ContentError::DuplicateName(String::from(
                "registry has duplicate names",
            )));
        }
        for (idx, name) in self.names.iter().enumerate() {
            if self.by_name.get(name).copied() != Some(idx as u16) {
                return Err(ContentError::UnknownName(format!(
                    "name `{name}` maps to an inconsistent id"
                )));
            }
        }
        Ok(())
    }
}

/// Runs every ported block wave into `sink` in upstream order.
///
/// M3 loads `environment`/`ore`/`crafting`/`defense`; M4 appends the remaining
/// regions (`Blocks.java` region order).
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    environment::load(sink)?;
    ore::load(sink)?;
    crafting::load(sink)?;
    defense::load(sink)?;
    distribution::load(sink)?;
    liquid::load(sink)?;
    power::load(sink)?;
    production::load(sink)?;
    storage::load(sink)?;
    turrets::load(sink)?;
    units::load(sink)?;
    units_erekir::load(sink)?;
    payloads::load(sink)?;
    sandbox::load(sink)?;
    legacy::load(sink)?;
    campaign::load(sink)?;
    logic::load(sink)?;
    Ok(())
}

/// Builds the sim-facing name table from the generated waves.
pub(super) fn load_names(blocks: &mut Blocks) {
    let mut sink = NamesSink { blocks };
    // Every wave is static vanilla data; registration cannot fail.
    let _ = load(&mut sink);
}

/// Loads blocks into a [`ContentRegistry`] (called by `create_base_content`).
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

/// `Block.postInit` auto `shownPlanets` (requires the planets registry and the
/// `planets::post_init_link` tree pass, which lands item planets first).
pub(crate) fn post_init_link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let planets: Vec<PlanetId> = registry
        .planets()
        .iter()
        .filter(|planet| !planet.sectors.is_empty())
        .map(|planet| planet.id)
        .collect();

    let mut assignments: Vec<(usize, Vec<PlanetId>)> = Vec::new();
    for (index, block) in registry.blocks().iter().enumerate() {
        if block.requirements.is_empty() || !block.unlock.shown_planets.is_empty() {
            continue;
        }
        let mut shown = Vec::new();
        for planet in &planets {
            let all = block.requirements.iter().all(|stack| {
                registry
                    .item(stack.item)
                    .map(|item| {
                        item.unlock.shown_planets.is_empty()
                            || item.unlock.shown_planets.contains(planet)
                    })
                    .unwrap_or(false)
            });
            if all {
                shown.push(*planet);
            }
        }
        if !shown.is_empty() {
            assignments.push((index, shown));
        }
    }

    for (index, shown) in assignments {
        if let Some(block) = registry.blocks_mut().get_mut(index) {
            for planet in shown {
                block.unlock.add_shown_planet(planet);
            }
        }
    }
    Ok(())
}

/// `Block.rotate`'s per-class default: the Java classes that set
/// `rotate = true` in their constructors (`world/blocks/**`, 26 sites —
/// turrets, unit factories, drills/wall-crafters, payload blocks, heat blocks,
/// conduits, thrusters, character overlays, ducts/conveyors and bridges, the
/// heater generator and the power diode).
pub fn kind_rotates(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Thruster
            | BlockKind::ItemTurret
            | BlockKind::LiquidTurret
            | BlockKind::PowerTurret
            | BlockKind::TractorBeamTurret
            | BlockKind::PointDefenseTurret
            | BlockKind::LaserTurret
            | BlockKind::ContinuousLiquidTurret
            | BlockKind::ContinuousTurret
            | BlockKind::RepairTurret
            | BlockKind::BuildTurret
            | BlockKind::Conveyor
            | BlockKind::ArmoredConveyor
            | BlockKind::DirectionalUnloader
            | BlockKind::DuctBridge
            | BlockKind::DirectionLiquidBridge
            | BlockKind::Duct
            | BlockKind::DuctRouter
            | BlockKind::StackRouter
            | BlockKind::OverflowDuct
            | BlockKind::StackConveyor
            | BlockKind::HeatConductor
            | BlockKind::HeatProducer
            | BlockKind::Conduit
            | BlockKind::ArmoredConduit
            | BlockKind::Constructor
            | BlockKind::PayloadConveyor
            | BlockKind::PayloadRouter
            | BlockKind::PayloadLoader
            | BlockKind::PayloadUnloader
            | BlockKind::PayloadMassDriver
            | BlockKind::PayloadSource
            | BlockKind::HeaterGenerator
            | BlockKind::PowerDiode
            | BlockKind::BeamDrill
            | BlockKind::WallCrafter
            | BlockKind::UnitAssembler
            | BlockKind::UnitAssemblerModule
            | BlockKind::UnitFactory
            | BlockKind::Reconstructor
    )
}
