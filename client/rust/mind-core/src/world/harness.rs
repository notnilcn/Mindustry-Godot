// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic build harness used by plan-07 tests and `mind-headless` block
//! scenarios.
//!
//! Bundles the content registry, `WorldGrid`, plan-07 ECS runtime and the
//! placement/construct API so scenarios can drive `place`/`configure`/`destroy`
//! without the P0 `Sim` (whose checksum stream stays frozen). This mirrors the
//! `Build`/`ConstructBlock` flow builders use (plan 11 calls the same helpers).

use std::sync::atomic::{AtomicU64, Ordering};

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry, MemoryBundle, MemoryUnlockStore};
use crate::determinism::{Checksummer, SimRng};
use crate::ecs::EntitySeq;
use crate::entities::comp::{Building, TeamComp};
use crate::world::block::{BlockTable, TILE_SIZE};
use crate::world::config::ConfigValue;
use crate::world::construct::ConstructState;
use crate::world::limits::{BlockCounter, BuildRules};
use crate::world::ops::{WorldCtx, WorldEventLog};
use crate::world::update::update_buildings;
use crate::world::{Context, NewBuilding, NoopRenderHooks, TilePos, WorldGrid, WorldHooks};

/// Recorded build event (scenario assertion surface).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildEventRecord {
    /// `BlockBuildBeginEvent`.
    Begin {
        /// x.
        x: i16,
        /// y.
        y: i16,
        /// team.
        team: u8,
        /// deconstruction.
        breaking: bool,
    },
    /// `BlockBuildEndEvent`.
    End {
        /// x.
        x: i16,
        /// y.
        y: i16,
        /// team.
        team: u8,
        /// deconstruction.
        breaking: bool,
        /// carried config present.
        has_config: bool,
    },
    /// `BuildRotateEvent`.
    Rotate {
        /// x.
        x: i16,
        /// y.
        y: i16,
        /// previous.
        previous: u8,
        /// next.
        rotation: u8,
    },
}

/// Hooks bridging plan 06's `WorldCtx` to the plan-07 building runtime.
struct HarnessHooks {
    seq: AtomicU64,
    item_count: usize,
    liquid_count: usize,
}

impl WorldHooks for HarnessHooks {
    fn new_building(&self, world: &mut World, request: NewBuilding) -> Option<Entity> {
        let inst = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.instance(request.block))?;
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        Some(inst.spawn(
            world,
            seq,
            TilePos::new(request.x, request.y),
            request.team,
            request.rot,
            self.item_count,
            self.liquid_count,
        ))
    }
}

/// The deterministic build world (plan 07 harness).
pub struct BuildHarness {
    /// ECS world holding buildings + the `BlockTable`/`BuildRules`/`BlockCounter`.
    pub world: World,
    /// Tile grid.
    pub grid: WorldGrid,
    /// Content registry.
    pub content: ContentRegistry,
    /// Rule knobs.
    pub rules: BuildRules,
    /// Live block counts.
    pub counter: BlockCounter,
    /// Recorded build events in fire order.
    pub events: Vec<BuildEventRecord>,
    /// Deterministic RNG (plan-03 style seeded stream).
    pub rng: SimRng,
    log: WorldEventLog,
    hooks: HarnessHooks,
    item_count: usize,
    liquid_count: usize,
}

impl BuildHarness {
    /// Creates a flat `width x height` build world with full vanilla content.
    pub fn new(width: i32, height: i32, seed: u64) -> Self {
        let content = Self::load_content();
        let table = BlockTable::build_default(&content).expect("block table");
        let item_count = content.items().len();
        let liquid_count = content.liquids().len();
        let mut world = World::new();
        world.insert_resource(table);
        world.insert_resource(BuildRules::default());
        world.insert_resource(BlockCounter::new());
        let mut grid = WorldGrid::new(width, height);
        grid.fill(BlockId::AIR, BlockId::AIR);
        Self {
            world,
            grid,
            content,
            rules: BuildRules::default(),
            counter: BlockCounter::new(),
            events: Vec::new(),
            rng: SimRng::new(seed),
            log: WorldEventLog::default(),
            hooks: HarnessHooks {
                seq: AtomicU64::new(0),
                item_count,
                liquid_count,
            },
            item_count,
            liquid_count,
        }
    }

    /// Builds the full vanilla registry (init + post-init).
    pub fn load_content() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry =
            crate::content::create_base_content(&bundle, &store, true).expect("base content");
        registry.init().expect("content init");
        registry.post_init().expect("content post-init");
        registry
    }

    /// Content registry.
    pub fn content(&self) -> &ContentRegistry {
        &self.content
    }

    /// Resolved block table (from the ECS resource).
    pub fn table(&self) -> &BlockTable {
        self.world
            .get_resource::<BlockTable>()
            .expect("harness block table")
    }

    /// Runs `f` with a [`WorldCtx`] over the harness grid/ECS (disjoint field
    /// borrows keep the hooks/content available).
    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut WorldCtx<'_>) -> R) -> R {
        let render = NoopRenderHooks;
        let mut ctx = WorldCtx {
            grid: &mut self.grid,
            content: &self.content,
            ecs: &mut self.world,
            hooks: &self.hooks,
            render: &render,
            log: &mut self.log,
        };
        f(&mut ctx)
    }

    /// Places a block, going through construction unless `instant` is set.
    pub fn place(&mut self, x: i32, y: i32, block: BlockId, rot: u8, instant: bool) -> bool {
        let team = self.rules.default_team;
        let valid = crate::world::build::valid_place(
            &self.content,
            self.table(),
            &self.rules,
            &self.counter,
            &self.grid,
            block,
            team,
            rot,
            x,
            y,
        );
        if !valid {
            return false;
        }
        // Remove proximity links of any building being overwritten.
        self.remove_overwritten_proximity(x, y, block);

        let def = self.content.block(block).cloned();
        let Some(def) = def else {
            return false;
        };
        let construct = !instant && !self.rules.instant_build && !self.rules.cheat;
        let place_block = if construct {
            self.construct_block_id(def.size).unwrap_or(block)
        } else {
            block
        };

        self.with_ctx(|ctx| ctx.set_block(x as i16, y as i16, place_block, team, rot));

        if construct && let Some(entity) = self.grid.tile(x, y).build {
            let cost = (def.build_time / 60.0).max(0.01);
            self.world
                .entity_mut(entity)
                .insert(ConstructState::construct(BlockId::AIR, block, cost));
        }
        // Recompute proximity for the new building.
        self.refresh_proximity_at(x, y);
        self.refresh_drill_ore_at(x, y);
        self.events.push(BuildEventRecord::Begin {
            x: x as i16,
            y: y as i16,
            team,
            breaking: false,
        });
        if !construct {
            self.counter.add(team, block, 1);
            self.events.push(BuildEventRecord::End {
                x: x as i16,
                y: y as i16,
                team,
                breaking: false,
                has_config: false,
            });
        }
        true
    }

    /// Breaks the block at `(x, y)`, going through deconstruction unless `instant`.
    pub fn break_block(&mut self, x: i32, y: i32, instant: bool) -> bool {
        if !crate::world::build::valid_break(&self.content, &self.grid, &self.rules, 0, x, y) {
            return false;
        }
        let Some(entity) = self.grid.tile(x, y).build else {
            return false;
        };
        let block = self.grid.tile(x, y).block;
        let cost = self
            .content
            .block(block)
            .map(|def| (def.build_time / 60.0).max(0.01))
            .unwrap_or(0.01);
        let team = self.rules.default_team;

        let _ =
            crate::world::proximity::remove_from_proximity(&mut self.world, entity, &self.content);

        let instant = instant || self.rules.instant_build || self.rules.cheat;
        if instant {
            self.with_ctx(|ctx| ctx.remove_block(x as i16, y as i16));
        } else {
            let size = self.content.block(block).map(|def| def.size).unwrap_or(1);
            let construct = self.construct_block_id(size).unwrap_or(BlockId::AIR);
            self.with_ctx(|ctx| ctx.set_block(x as i16, y as i16, construct, team, 0));
            if let Some(entity) = self.grid.tile(x, y).build {
                self.world
                    .entity_mut(entity)
                    .insert(ConstructState::deconstruct(block, cost));
            }
        }
        self.events.push(BuildEventRecord::Begin {
            x: x as i16,
            y: y as i16,
            team,
            breaking: true,
        });
        if instant {
            self.events.push(BuildEventRecord::End {
                x: x as i16,
                y: y as i16,
                team,
                breaking: true,
                has_config: false,
            });
        }
        true
    }

    /// Advances every active construction by `amount`; finishes completed ones.
    pub fn construct_tick(&mut self, amount: f32) {
        let mut finished: Vec<(Entity, BlockId, bool)> = Vec::new();
        let mut query = self.world.query::<(Entity, &mut ConstructState)>();
        for (entity, mut state) in query.iter_mut(&mut self.world) {
            if state.active_deconstruct {
                let (done, _refund) = state.advance_deconstruct(amount, 0.5);
                if done {
                    finished.push((entity, state.previous, true));
                }
            } else if state.advance_construct(amount) {
                finished.push((entity, state.current, false));
            }
        }
        for (entity, block, deconstruct) in finished {
            self.finish(entity, block, deconstruct);
        }
    }

    fn finish(&mut self, entity: Entity, block: BlockId, deconstruct: bool) {
        let Some(building) = self.world.get::<Building>(entity) else {
            return;
        };
        let tile = building.tile;
        let team = self
            .world
            .get::<TeamComp>(entity)
            .map(|t| t.team)
            .unwrap_or(0);
        let target = if deconstruct { BlockId::AIR } else { block };
        let _ =
            crate::world::proximity::remove_from_proximity(&mut self.world, entity, &self.content);
        self.with_ctx(|ctx| ctx.set_block(tile.x(), tile.y(), target, team, 0));
        self.refresh_proximity_at(tile.x() as i32, tile.y() as i32);
        self.refresh_drill_ore_at(tile.x() as i32, tile.y() as i32);
        if !deconstruct {
            self.counter.add(team, block, 1);
        }
        self.events.push(BuildEventRecord::End {
            x: tile.x(),
            y: tile.y(),
            team,
            breaking: deconstruct,
            has_config: false,
        });
    }

    /// Applies a config value to the building at `(x, y)`.
    pub fn configure(&mut self, x: i32, y: i32, value: ConfigValue) -> bool {
        let Some(entity) = self.grid.tile(x, y).build else {
            return false;
        };
        let Some(block) = self.world.get::<Building>(entity).map(|b| b.block) else {
            return false;
        };
        let Some(inst) = self.table().instance(block) else {
            return false;
        };
        inst.behavior
            .configured(&mut self.world, entity, None, value);
        true
    }

    /// Advances the runtime by one tick (`UpdateBuildings`).
    pub fn tick(&mut self) {
        update_buildings(&mut self.world);
    }

    /// Deterministic checksum over the grid + buildings (FNV-1a-64).
    pub fn checksum_value(&self) -> crate::determinism::Checksum {
        let mut c = Checksummer::new();
        for (pos, index) in self.grid.iter_row_major() {
            let tile = self.grid.tile_ref(index);
            c.part(&pos.x());
            c.part(&pos.y());
            c.part(&tile.block.raw());
            match tile.build {
                Some(entity) => {
                    let seq = self
                        .world
                        .get::<EntitySeq>(entity)
                        .map(|seq| seq.0)
                        .unwrap_or(u64::MAX);
                    c.part(&1u8);
                    c.part(&seq);
                }
                None => {
                    c.part(&0u8);
                    c.part(&0u64);
                }
            }
        }
        let mut entities: Vec<(u64, Entity)> = self
            .world
            .iter_entities()
            .filter_map(|entity_ref| {
                entity_ref.get::<Building>()?;
                let seq = entity_ref
                    .get::<EntitySeq>()
                    .map(|seq| seq.0)
                    .unwrap_or(u64::MAX);
                Some((seq, entity_ref.id()))
            })
            .collect();
        entities.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        for (seq, entity) in entities {
            c.part(&seq);
            if let Some(building) = self.world.get::<Building>(entity) {
                c.part(&building.block.raw());
                c.part(&building.rotation);
                c.part(&building.tile.pack());
            }
        }
        c.finish()
    }

    /// Deterministic checksum value.
    pub fn checksum(&self) -> u64 {
        self.checksum_value().value()
    }

    /// Checksum as 16 hex digits.
    pub fn checksum_hex(&self) -> String {
        self.checksum_value().to_hex()
    }

    /// Number of live buildings.
    pub fn building_count(&self) -> usize {
        self.world
            .iter_entities()
            .filter(|entity| entity.get::<Building>().is_some())
            .count()
    }

    /// Block id at a tile (`air` when empty).
    pub fn block_at(&self, x: i32, y: i32) -> BlockId {
        if self.grid.tiles.in_bounds(x, y) {
            self.grid.tile(x, y).block
        } else {
            BlockId::AIR
        }
    }

    /// Building entity at a tile.
    pub fn build_at(&self, x: i32, y: i32) -> Option<Entity> {
        self.grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.grid.tile(x, y).build)
            .flatten()
    }

    /// Resolves a construct singleton by size.
    pub fn construct_block_id(&self, size: i32) -> Option<BlockId> {
        self.content.block_id(&format!("build{size}"))
    }

    /// Serializes the harness grid through plan 04's `Context` (used by
    /// `read_building` round-trip tests).
    pub fn context(&mut self) -> Context<'_> {
        Context::new(&mut self.grid, &self.content)
    }

    fn remove_overwritten_proximity(&mut self, x: i32, y: i32, _block: BlockId) {
        if let Some(entity) = self.build_at(x, y) {
            let _ = crate::world::proximity::remove_from_proximity(
                &mut self.world,
                entity,
                &self.content,
            );
        }
    }

    fn refresh_proximity_at(&mut self, x: i32, y: i32) {
        if let Some(entity) = self.build_at(x, y) {
            let _ = crate::world::proximity::update_proximity(
                &mut self.world,
                &self.grid,
                &self.content,
                entity,
            );
        }
        // Neighbors may need recompute too.
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if let Some(entity) = self.build_at(nx, ny) {
                let _ = crate::world::proximity::update_proximity(
                    &mut self.world,
                    &self.grid,
                    &self.content,
                    entity,
                );
            }
        }
    }

    /// Recomputes a drill's dominant ore from the grid (`Drill.countOre`).
    pub fn refresh_drill_ore_at(&mut self, x: i32, y: i32) {
        if let Some(entity) = self.build_at(x, y) {
            crate::world::behavior::production::refresh_drill_ore(
                &mut self.world,
                &self.grid,
                &self.content,
                entity,
            );
        }
    }

    /// Renders the per-tile block layout for dumps (`(x, y, block-name)`).
    pub fn block_layout(&self) -> Vec<(i16, i16, String)> {
        self.grid
            .iter_row_major()
            .filter(|(_, index)| self.grid.tile_ref(*index).block != BlockId::AIR)
            .map(|(pos, index)| {
                let block = self.grid.tile_ref(index).block;
                let name = self
                    .content
                    .block(block)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| format!("?{}", block.raw()));
                (pos.x(), pos.y(), name)
            })
            .collect()
    }

    /// The item/liquid slot counts used when spawning modules.
    pub fn module_slot_counts(&self) -> (usize, usize) {
        (self.item_count, self.liquid_count)
    }

    /// Center pixel of a tile (spawn helper).
    pub fn tile_center(x: i32, y: i32) -> (f32, f32) {
        ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_construct_destroy_cycle() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let wall = harness
            .content()
            .block_id("copper-wall")
            .expect("copper-wall");
        assert!(harness.place(4, 4, wall, 0, false));
        // Place goes through the size-1 construct singleton.
        assert_eq!(harness.construct_block_id(1), Some(harness.block_at(4, 4)));
        harness.construct_tick(10_000.0);
        assert_eq!(harness.block_at(4, 4), wall);
        assert!(harness.break_block(4, 4, true));
        assert_eq!(harness.block_at(4, 4), BlockId::AIR);
        // Event order: Begin(place), End(place), Begin(break), End(break).
        assert_eq!(harness.events.len(), 4);
        assert!(matches!(
            harness.events[0],
            BuildEventRecord::Begin {
                breaking: false,
                ..
            }
        ));
        assert!(matches!(
            harness.events[1],
            BuildEventRecord::End {
                breaking: false,
                ..
            }
        ));
        assert!(matches!(
            harness.events[2],
            BuildEventRecord::Begin { breaking: true, .. }
        ));
        assert!(matches!(
            harness.events[3],
            BuildEventRecord::End { breaking: true, .. }
        ));
    }

    #[test]
    fn multiblock_cover_and_clear() {
        let mut harness = BuildHarness::new(16, 16, 7);
        // `copper-wall-large` is a 2x2 multiblock: (5,5) covers (5,5)..(6,6).
        let large = harness
            .content()
            .block_id("copper-wall-large")
            .expect("copper-wall-large");
        assert!(harness.place(5, 5, large, 0, true));
        let center = harness.build_at(5, 5).expect("center");
        for x in 5..=6 {
            for y in 5..=6 {
                assert_eq!(harness.build_at(x, y), Some(center));
            }
        }
        // Break from a covered edge tile; the center's footprint clears.
        assert!(harness.break_block(6, 6, true));
        for x in 5..=6 {
            for y in 5..=6 {
                assert_eq!(harness.block_at(x, y), BlockId::AIR);
            }
        }
    }

    #[test]
    fn checksum_is_reproducible() {
        let mut first = BuildHarness::new(16, 16, 7);
        let wall = first.content().block_id("copper-wall").expect("wall");
        first.place(4, 4, wall, 0, true);
        let mut second = BuildHarness::new(16, 16, 7);
        second.place(4, 4, wall, 0, true);
        assert_eq!(first.checksum(), second.checksum());
    }
}
