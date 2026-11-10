// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Installation of the live building runtime over the vanilla or a host-supplied
//! content registry, and the runtime digest entry point.
//!
//! Upstream: none (port-only runtime installation).

use super::*;

impl Sim {
    /// Installs the live building runtime with default configuration (no
    /// placement gate). Content is booted from the vanilla registry.
    pub fn install_block_runtime(&mut self) -> Result<(), RuntimeError> {
        self.install_block_runtime_with(SimRuntimeConfig::default())
    }

    /// Installs the live building runtime with explicit configuration.
    pub fn install_block_runtime_with(
        &mut self,
        config: SimRuntimeConfig,
    ) -> Result<(), RuntimeError> {
        let content = load_vanilla_content()?;
        self.install_block_runtime_from(config, content)
    }

    /// Installs the live building runtime over a prebuilt content registry
    /// (host unlock/research state; `MindSimHost`'s content snapshot).
    ///
    /// Inserts the resources the scheduled runtime systems read. Resource
    /// insertion is idempotent-safe (`init_resource` keeps a host-supplied
    /// `BuildRules`/`BuildClock`), and building it before any entity exists
    /// keeps ECS entity indices stable.
    pub fn install_block_runtime_from(
        &mut self,
        config: SimRuntimeConfig,
        content: ContentRegistry,
    ) -> Result<(), RuntimeError> {
        if self.block_runtime.is_some() {
            return Ok(());
        }
        let table = BlockTable::build_default(&content)?;
        let item_count = content.items().len();
        let liquid_count = content.liquids().len();
        let mut content_index = ContentIndex::default();
        for item in content.items() {
            content_index
                .item_ids
                .insert(item.name.clone(), item.id.index() as u16);
        }
        content_index.item_names = content
            .items()
            .iter()
            .map(|item| item.name.clone())
            .collect();
        content_index.item_buildable = content.items().iter().map(|item| item.buildable).collect();
        for block in content.blocks() {
            content_index
                .block_ids
                .insert(block.name.clone(), block.id.index() as u16);
        }
        for unit in content.units() {
            content_index
                .unit_ids
                .insert(unit.name.clone(), unit.id.index() as u16);
        }
        content_index.unit_names = content
            .units()
            .iter()
            .map(|unit| unit.name.clone())
            .collect();
        self.ecs.0.insert_resource(content_index);
        self.ecs.0.insert_resource(table);
        self.ecs.0.init_resource::<BuildRules>();
        self.ecs.0.init_resource::<BuildClock>();
        self.ecs.0.init_resource::<TileBuilds>();
        self.ecs.0.insert_resource(ModuleDims {
            items: item_count,
            liquids: liquid_count,
        });
        // T1: coolant/incinerable/heat-capacity vectors for behaviors that run
        // without a content-registry borrow (`ConsumeCoolant`, `Incinerator`,
        // mixing reactions, force-projector coolant).
        self.ecs
            .0
            .insert_resource(LiquidTraits::from_content(&content));
        // GAPS #58: item trait vectors (`Item.flammability`/`radioactivity`/
        // `explosiveness`/`charge`) for the trait-filtered consumers
        // (`ConsumeItemFlammable`/`Radioactive`/`Explosive`/`Charged`,
        // `ConsumeItemEfficiency`). A missing table rejects every item, so the
        // live runtime must install it exactly like `LiquidTraits`.
        self.ecs
            .0
            .insert_resource(crate::world::consumers::ItemTraits::from_content(&content));
        self.ecs.0.insert_resource(PowerGrids::new());
        // I04 T3: the reconstructor's `Trigger.cannotUpgrade` marker (`&World`
        // hook, interior mutability); drained by `Sim::tick`.
        self.ecs
            .0
            .init_resource::<crate::world::blocks::units::behavior::CannotUpgradeFired>();
        // The live per-team block counter (`Build.validPlace` limits + the
        // runtime checksum) is a world resource so scheduled systems (the
        // campaign team AI) can read it (PAR-0139 round 2).
        self.ecs.0.insert_resource(BlockCounter::new());
        self.block_runtime = Some(SimRuntime {
            config,
            content,
            hooks: SimHooks {
                seq: AtomicU64::new(0),
                item_count,
                liquid_count,
            },
            log: WorldEventLog::default(),
            render: NoopRenderHooks,
            facing_drills: Vec::new(),
            thermal_generators: Vec::new(),
            beam_nodes: Vec::new(),
        });
        // A host may install the runtime after loading a grid with pre-placed
        // power buildings (`World.endMapLoad` rebuild).
        self.rebuild_power_graphs();
        Ok(())
    }

    /// Deterministic digest of the live runtime state; empty when not installed.
    pub fn runtime_checksum(&self) -> Checksum {
        match &self.block_runtime {
            Some(runtime) => runtime.checksum(&self.ecs.0),
            None => Checksummer::new().finish(),
        }
    }
}
