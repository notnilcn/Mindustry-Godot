// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Synthetic world fixture for the save round-trip (plan 04 M4 §5/§7b).
//!
//! A self-contained world implementing every M4 IO trait ([`MapSource`],
//! [`WorldContext`], [`EntitySource`], [`EntitySink`]) over plain vectors, so
//! `mind-headless io roundtrip` can prove the native v1 map/entities regions
//! end-to-end before plan 06 lands the real `World`/`Tiles` (the fixture is the
//! plan's "synthetic 64×64 map fixture" until then; traits stay — 06 swaps the
//! implementation, not the seam).
//!
//! Not a port of any single upstream file; behavior mirrors
//! `SaveVersion.readMap`/`writeMap` consumer semantics.

use std::collections::BTreeMap;

use super::super::IoResult;
use super::super::wire::{WireReader, WireWriter};
use super::chunk::SaveScratch;
use super::state::{EntitySink, EntitySource, MapSource, TeamPlan, WorldContext};
use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::ecs::{BuildingComp, TeamId};
use crate::io::entity::{DuplicateIdTracker, EntityCodec, EntityIdMap};
use crate::world::TilePos;

/// A flat in-memory world for save round-trip verification.
pub struct FixtureWorld<'a> {
    registry: &'a ContentRegistry,
    /// World width.
    pub width: u16,
    /// World height.
    pub height: u16,
    /// Floor per tile (row-major).
    pub floors: Vec<BlockId>,
    /// Overlay per tile.
    pub overlays: Vec<BlockId>,
    /// Block per tile.
    pub blocks: Vec<BlockId>,
    /// Team per tile.
    pub teams: Vec<u8>,
    /// Rotation per tile.
    pub rots: Vec<u8>,
    /// Tile data byte per tile.
    pub data: Vec<u8>,
    /// Floor data byte per tile.
    pub floor_data: Vec<u8>,
    /// Overlay data byte per tile.
    pub overlay_data: Vec<u8>,
    /// Extra data int per tile.
    pub extra_data: Vec<i32>,
    /// Building components by tile index (multiblock centers; 1×1 here).
    pub buildings: BTreeMap<usize, BuildingComp>,
    /// Entity id per building tile index.
    pub entity_ids: BTreeMap<usize, i32>,
    /// Next free entity id (`EntityGroup.nextId`).
    pub next_entity_id: i32,
    /// AI-team build plans (written into the `entities` region).
    pub team_plans: Vec<(i32, Vec<TeamPlan>)>,
    /// Simulated tick counter (kept in meta tags).
    pub tick: u64,
    /// Simulated wave counter (kept in meta tags).
    pub wave: i32,
    generating: bool,
    duplicates: DuplicateIdTracker,
}

impl<'a> FixtureWorld<'a> {
    /// An empty `width`×`height` world.
    pub fn new(registry: &'a ContentRegistry, width: u16, height: u16) -> Self {
        let len = width as usize * height as usize;
        Self {
            registry,
            width,
            height,
            floors: vec![BlockId::AIR; len],
            overlays: vec![BlockId::AIR; len],
            blocks: vec![BlockId::AIR; len],
            teams: vec![0; len],
            rots: vec![0; len],
            data: vec![0; len],
            floor_data: vec![0; len],
            overlay_data: vec![0; len],
            extra_data: vec![0; len],
            buildings: BTreeMap::new(),
            entity_ids: BTreeMap::new(),
            next_entity_id: 1,
            team_plans: Vec::new(),
            tick: 0,
            wave: 0,
            generating: false,
            duplicates: DuplicateIdTracker::new(),
        }
    }

    /// The deterministic synthetic 64×64 fixture (plan 04 §5 M4).
    ///
    /// Content pattern (all lookups by name, skipped when a block is absent):
    /// checkerboard floors, sparse ore overlays, a wall ring, placed buildings
    /// (conveyors with configs via team plans too), one `save_data` tile and
    /// one `save_data` floor to exercise the 7-byte tile-data path.
    pub fn synthetic(registry: &'a ContentRegistry, width: u16, height: u16) -> Self {
        let mut world = Self::new(registry, width, height);
        let block_id = |name: &str| registry.block_id(name);
        let floor_cycle: Vec<BlockId> = ["stone", "sand-floor", "shallow-water", "ice"]
            .iter()
            .filter_map(|name| block_id(name))
            .collect();
        let ore = block_id("ore-copper");
        let wall = block_id("stone-wall");
        let conveyor = block_id("conveyor");
        let router = block_id("router");

        // Floors + overlays.
        for y in 0..height {
            for x in 0..width {
                let index = world.index(x, y);
                if !floor_cycle.is_empty() {
                    world.floors[index] =
                        floor_cycle[(x as usize + y as usize) % floor_cycle.len()];
                }
                if let Some(ore) = ore
                    && (x * 7 + y * 13) % 29 == 0
                {
                    world.overlays[index] = ore;
                }
            }
        }

        // Wall ring.
        if let Some(wall) = wall {
            for x in 0..width {
                world.set_block_at(world.index(x, 0), wall);
                world.set_block_at(world.index(x, height - 1), wall);
            }
            for y in 0..height {
                world.set_block_at(world.index(0, y), wall);
                world.set_block_at(world.index(width - 1, y), wall);
            }
        }

        // Placed buildings (have entities): a conveyor line + a router.
        if let Some(conveyor) = conveyor {
            for x in 4..12u16 {
                world.place(x, 8, conveyor, 1, (x % 4) as u8);
            }
        }
        if let Some(router) = router {
            world.place(12, 8, router, 1, 0);
        }

        // One block with `save_data` (any kind) to exercise the 7-byte path.
        if let Some(save_data_block) = registry
            .blocks()
            .iter()
            .find(|def| def.save_data && !creates_building_kind(def.kind))
        {
            let index = world.index(2, 2);
            world.set_block_at(index, save_data_block.id);
            world.data[index] = 0x5A;
            world.floor_data[index] = 0x11;
            world.overlay_data[index] = 0x22;
            world.extra_data[index] = 0x0BAD_F00D_u32 as i32;
        }
        // One save-data overlay (CharacterOverlay kind carries save_data upstream).
        if let Some(save_data_overlay) = registry.blocks().iter().find(|def| {
            def.save_data
                && matches!(
                    def.kind,
                    BlockKind::CharacterOverlay | BlockKind::RuneOverlay | BlockKind::ColoredFloor
                )
        }) {
            let index = world.index(3, 3);
            world.overlays[index] = save_data_overlay.id;
            world.overlay_data[index] = 0x77;
        }

        // Team build plans (written into the entities region).
        if let Some(conveyor) = conveyor {
            world.team_plans.push((
                1,
                vec![
                    TeamPlan {
                        x: 20,
                        y: 20,
                        rotation: 1,
                        block: conveyor,
                        config: crate::io::typeio::TypeValue::Null,
                    },
                    TeamPlan {
                        x: 21,
                        y: 20,
                        rotation: 0,
                        block: conveyor,
                        config: crate::io::typeio::TypeValue::Int(3),
                    },
                ],
            ));
        }

        world.wave = 3;
        world
    }

    /// Row-major index of `(x, y)`.
    pub fn index(&self, x: u16, y: u16) -> usize {
        x as usize + y as usize * self.width as usize
    }

    /// Sets a block without entity handling (fixture construction helper).
    pub fn set_block_at(&mut self, index: usize, block: BlockId) {
        self.blocks[index] = block;
    }

    /// Places a block with entity handling identical to the load path.
    pub fn place(&mut self, x: u16, y: u16, block: BlockId, team: u8, rot: u8) {
        let index = self.index(x, y);
        self.blocks[index] = block;
        self.teams[index] = team;
        self.rots[index] = rot;
        if self.creates_building(block) {
            self.spawn_building(index, team, rot);
        }
    }

    /// One deterministic fixture tick: increments the counter and mutates the
    /// save-data tiles every 30 ticks. Data on non-`save_data` tiles is NOT
    /// persisted (upstream `shouldSaveData` semantics), so only those tiles
    /// may change.
    pub fn tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        if self.tick.is_multiple_of(30) {
            for index in [2 + 2 * self.width as usize, 3 + 3 * self.width as usize] {
                if index < self.data.len() && self.should_save_data(index) {
                    self.data[index] = self.data[index].wrapping_add(1);
                    self.extra_data[index] = self.extra_data[index].wrapping_add(7);
                }
            }
        }
        if self.tick.is_multiple_of(60) {
            self.wave = self.wave.saturating_add(1);
        }
    }

    /// Applies the team plans read from a save (`TeamData.plans` assignment).
    pub fn apply_team_plans(&mut self, plans: Vec<(i32, Vec<TeamPlan>)>) {
        self.team_plans = plans;
    }

    /// Whether a block creates a building entity (`block.hasBuilding()`
    /// approximated: every non-environment kind; plan 07 owns the exact flag).
    pub fn creates_building(&self, block: BlockId) -> bool {
        self.registry
            .block(block)
            .map(|def| creates_building_kind(def.kind))
            .unwrap_or(false)
    }

    fn spawn_building(&mut self, index: usize, team: u8, rot: u8) {
        let pos = TilePos::new(
            (index % self.width as usize) as i16,
            (index / self.width as usize) as i16,
        );
        let comp = BuildingComp {
            pos,
            block: self.blocks[index],
            team: TeamId(team),
            rot,
        };
        let id = self.next_entity_id;
        self.next_entity_id = self.next_entity_id.saturating_add(1);
        self.buildings.insert(index, comp);
        self.entity_ids.insert(index, id);
    }

    /// Canonical fixture checksum (xxh3-64 over the full IO-visible state,
    /// row-major; the M4 roundtrip oracle).
    pub fn checksum(&self) -> u64 {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes.extend_from_slice(&self.tick.to_le_bytes());
        bytes.extend_from_slice(&self.wave.to_le_bytes());
        for i in 0..self.floors.len() {
            bytes.extend_from_slice(&self.floors[i].raw().to_le_bytes());
            bytes.extend_from_slice(&self.overlays[i].raw().to_le_bytes());
            bytes.extend_from_slice(&self.blocks[i].raw().to_le_bytes());
            bytes.push(self.teams[i]);
            bytes.push(self.rots[i]);
            bytes.push(self.data[i]);
            bytes.push(self.floor_data[i]);
            bytes.push(self.overlay_data[i]);
            bytes.extend_from_slice(&self.extra_data[i].to_le_bytes());
        }
        for (index, comp) in &self.buildings {
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.extend_from_slice(&self.entity_ids[index].to_le_bytes());
            bytes.extend_from_slice(&comp.pos.0.to_le_bytes());
            bytes.extend_from_slice(&comp.pos.1.to_le_bytes());
            bytes.extend_from_slice(&comp.block.raw().to_le_bytes());
            bytes.push(comp.team.0);
            bytes.push(comp.rot);
        }
        for (team, plans) in &self.team_plans {
            bytes.extend_from_slice(&team.to_le_bytes());
            for plan in plans {
                bytes.extend_from_slice(&plan.x.to_le_bytes());
                bytes.extend_from_slice(&plan.y.to_le_bytes());
                bytes.extend_from_slice(&plan.rotation.to_le_bytes());
                bytes.extend_from_slice(&plan.block.raw().to_le_bytes());
                bytes.extend_from_slice(format!("{:?}", plan.config).as_bytes());
            }
        }
        xxhash_rust::xxh3::xxh3_64(&bytes)
    }

    /// Checksum as 16 lowercase hex digits.
    pub fn checksum_hex(&self) -> String {
        format!("{:016x}", self.checksum())
    }

    /// Restores meta-tag state after a load (`tick`/`wave`), mirroring
    /// `SaveVersion.readMeta` assigning `state.wave`/`state.tick`.
    pub fn apply_meta(&mut self, tags: &crate::io::StringMap) {
        if let Some(tick) = tags.get("tick").and_then(|v| v.parse::<u64>().ok()) {
            self.tick = tick;
        }
        if let Some(wave) = tags.get("wave").and_then(|v| v.parse::<i32>().ok()) {
            self.wave = wave;
        }
    }
}

/// Whether a block kind creates a building entity upstream
/// (`!environment`); plan 07 replaces this with the exact `hasBuilding` flag.
pub fn creates_building_kind(kind: BlockKind) -> bool {
    !matches!(
        kind,
        BlockKind::AirBlock
            | BlockKind::SpawnBlock
            | BlockKind::RemoveWall
            | BlockKind::RemoveOre
            | BlockKind::Cliff
            | BlockKind::Floor
            | BlockKind::EmptyFloor
            | BlockKind::OverlayFloor
            | BlockKind::OreBlock
            | BlockKind::StaticWall
            | BlockKind::StaticProp
            | BlockKind::StaticTree
            | BlockKind::Prop
            | BlockKind::TreeBlock
            | BlockKind::TallBlock
            | BlockKind::SeaBush
            | BlockKind::Seaweed
            | BlockKind::ShallowLiquid
            | BlockKind::CharacterOverlay
            | BlockKind::RuneOverlay
            | BlockKind::ColoredFloor
            | BlockKind::SteamVent
    )
}

impl MapSource for FixtureWorld<'_> {
    fn width(&self) -> u16 {
        self.width
    }

    fn height(&self) -> u16 {
        self.height
    }

    fn floor_id(&self, index: usize) -> u16 {
        self.floors[index].raw()
    }

    fn overlay_id(&self, index: usize) -> u16 {
        self.overlays[index].raw()
    }

    fn block_id(&self, index: usize) -> u16 {
        self.blocks[index].raw()
    }

    fn has_building(&self, index: usize) -> bool {
        self.buildings.contains_key(&index)
    }

    fn is_center(&self, _index: usize) -> bool {
        // 1×1 blocks only at this stage; multiblock centers are plan 06/07.
        true
    }

    fn should_save_data(&self, index: usize) -> bool {
        // `Tile.shouldSaveData`: floor.saveData || overlay.saveData || block.saveData.
        let save = |id: BlockId| {
            self.registry
                .block(id)
                .map(|def| def.save_data)
                .unwrap_or(false)
        };
        save(self.floors[index]) || save(self.overlays[index]) || save(self.blocks[index])
    }

    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32) {
        (
            self.data[index],
            self.floor_data[index],
            self.overlay_data[index],
            self.extra_data[index],
        )
    }

    fn write_building(&self, index: usize, chunk: &mut WireWriter) -> IoResult<()> {
        let comp = self
            .buildings
            .get(&index)
            .ok_or_else(|| super::super::IoError::corrupt("missing building at center tile"))?;
        chunk.ub(BuildingComp::TILE_VERSION);
        comp.write(chunk)
    }
}

impl EntitySource for FixtureWorld<'_> {
    fn entity_id_map(&self) -> EntityIdMap {
        EntityIdMap::new()
    }

    fn team_plans(&self) -> Vec<(i32, Vec<TeamPlan>)> {
        self.team_plans.clone()
    }

    fn entity_count(&self) -> usize {
        // Upstream excludes buildings from `Groups.all` (`excludeGroups =
        // {"all"}, serialize = false` on BuildingComp): buildings travel in the
        // map region only. The fixture has no non-tile entities yet (units,
        // decals, ... are plan 05+), so the entity chunk section is empty.
        0
    }

    fn write_entities(&self, _w: &mut WireWriter, _scratch: &mut SaveScratch) -> IoResult<()> {
        // See `entity_count`: no non-tile entities at this stage. Plan 05's
        // entity runtime writes real chunks here.
        Ok(())
    }
}

/// Read-side [`WorldContext`] adapter over a shared fixture cell.
///
/// The load path needs the world as `WorldContext` (map region) and
/// `EntitySink` (entities region) at once; both adapters borrow the same
/// `RefCell`, and the region readers never interleave calls.
pub struct FixtureContext<'b, 'a: 'b>(pub &'b std::cell::RefCell<FixtureWorld<'a>>);

/// Read-side [`EntitySink`] adapter over a shared fixture cell.
pub struct FixtureSink<'b, 'a: 'b>(pub &'b std::cell::RefCell<FixtureWorld<'a>>);

impl WorldContext for FixtureContext<'_, '_> {
    fn tile_count(&self) -> usize {
        self.0.borrow().floors.len()
    }

    fn resize(&mut self, width: u16, height: u16) {
        let registry = self.0.borrow().registry;
        *self.0.borrow_mut() = FixtureWorld::new(registry, width, height);
    }

    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, wall: u16) {
        let mut world = self.0.borrow_mut();
        let index = world.index(x, y);
        world.floors[index] = BlockId::new(floor);
        world.overlays[index] = BlockId::new(overlay);
        world.blocks[index] = BlockId::new(wall);
    }

    fn is_generating(&self) -> bool {
        self.0.borrow().generating
    }

    fn begin(&mut self) {
        self.0.borrow_mut().generating = true;
    }

    fn end(&mut self) {
        self.0.borrow_mut().generating = false;
    }

    fn set_block(&mut self, index: usize, block: u16) {
        let mut world = self.0.borrow_mut();
        let block = BlockId::new(block);
        world.blocks[index] = block;
        if world.creates_building(block) {
            let team = world.teams[index];
            let rot = world.rots[index];
            world.spawn_building(index, team, rot);
        }
    }

    fn has_building(&self, index: usize) -> bool {
        self.0.borrow().buildings.contains_key(&index)
    }

    fn block_has_building_io(&self, index: usize) -> bool {
        let world = self.0.borrow();
        world.creates_building(world.blocks[index])
    }

    fn set_tile_data(
        &mut self,
        index: usize,
        data: u8,
        floor_data: u8,
        overlay_data: u8,
        extra_data: i32,
    ) {
        let mut world = self.0.borrow_mut();
        world.data[index] = data;
        world.floor_data[index] = floor_data;
        world.overlay_data[index] = overlay_data;
        world.extra_data[index] = extra_data;
    }

    fn read_building(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> IoResult<()> {
        debug_assert_eq!(version, BuildingComp::TILE_VERSION);
        let revision = reader.us()?;
        let mut world = self.0.borrow_mut();
        let (team, rot) = {
            let comp = world
                .buildings
                .get_mut(&index)
                .ok_or_else(|| super::super::IoError::corrupt("tile entity without a building"))?;
            comp.read(reader, revision)?;
            (comp.team.0, comp.rot)
        };
        // Upstream `Tile.team()`/`Tile.rotation()` delegate to the building:
        // mirror the deserialized values back onto the tile arrays.
        world.teams[index] = team;
        world.rots[index] = rot;
        Ok(())
    }
}

impl EntitySink for FixtureSink<'_, '_> {
    fn supports_class(&self, class_id: u8, custom_name: Option<&str>) -> bool {
        class_id == BuildingComp::CLASS_ID || custom_name == Some(BuildingComp::NAME)
    }

    fn read_entity(
        &mut self,
        class_id: u8,
        _custom_name: Option<&str>,
        id: i32,
        r: &mut WireReader,
    ) -> IoResult<()> {
        debug_assert_eq!(class_id, BuildingComp::CLASS_ID);
        let revision = r.us()?;
        let mut comp = BuildingComp {
            pos: TilePos::new(0, 0),
            block: BlockId::AIR,
            team: TeamId(0),
            rot: 0,
        };
        comp.read(r, revision)?;
        let mut world = self.0.borrow_mut();
        let index = world.index(comp.pos.0 as u16, comp.pos.1 as u16);
        // Duplicate ids are reassigned with a warning (upstream).
        let id = if world.duplicates.claim(id) {
            id
        } else {
            log::warn!("Duplicate entity ID in save: {id} (reassigning)");
            let fresh = world.next_entity_id;
            world.next_entity_id = world.next_entity_id.saturating_add(1);
            fresh
        };
        world.next_entity_id = world.next_entity_id.max(id.saturating_add(1));
        world.buildings.insert(index, comp);
        world.entity_ids.insert(index, id);
        Ok(())
    }

    fn after_read_all(&mut self) {
        // Nothing to recompute at this stage (upstream runs afterReadAll on
        // every entity + building; the hook is exercised by the roundtrip).
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::save::version::WriteContext;
    use crate::io::save::versions::v1::base_meta_tags;
    use crate::io::save::{SaveIo, SaveOptions, SaveReadState};

    /// `io::save::tests::save_then_load_preserves_unit_and_map`-shaped fixture
    /// round-trip (plan 04 §7a adaptation for the synthetic world).
    #[test]
    fn fixture_save_load_roundtrip() {
        let registry = test_registry();
        let mut world = FixtureWorld::synthetic(&registry, 64, 64);
        for _ in 0..600 {
            world.tick();
        }
        let before = world.checksum_hex();

        let mut tags = base_meta_tags(64, 64, world.wave, "synthetic");
        tags.insert("tick".to_owned(), world.tick.to_string());
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(&registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new()).unwrap();

        // Load into a fresh world (context + sink adapters share one cell).
        let cell = std::cell::RefCell::new(FixtureWorld::new(&registry, 0, 0));
        let mut context = FixtureContext(&cell);
        let mut sink = FixtureSink(&cell);
        let mut loaded_registry = test_registry();
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut loaded_registry),
            entities: Some(&mut sink),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).unwrap();

        // Meta survived; team plans were collected; tile data survived.
        assert_eq!(state.tags.get("tick").unwrap(), "600");
        assert_eq!(state.team_plans.len(), 1);
        assert!(!state.all_buildings.is_empty());
        let state_tags = state.tags.clone();
        let state_team_plans = state.team_plans.clone();
        drop(state);
        let mut loaded_world = cell.into_inner();
        loaded_world.apply_meta(&state_tags);
        loaded_world.apply_team_plans(state_team_plans);
        assert_eq!(loaded_world.checksum_hex(), before);

        // The temporary mapper never escapes the load (upstream finally).
        assert!(
            loaded_registry
                .get_by_id(crate::content::ContentType::Block, i32::MAX)
                .is_none()
        );
    }
}
