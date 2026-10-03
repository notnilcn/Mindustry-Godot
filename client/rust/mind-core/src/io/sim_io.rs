// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-sim save/load executor for the plan-05 `IoSet` boundary (plan 04 §3.10).
//!
//! The plan-04 `MindIo` gdext autoload delegates to [`SimIoHandler`]: requests
//! queued through [`crate::sim::Sim::request_save`]/`request_load` are fulfilled
//! at the `IoSet::Capture`/`IoSet::Apply` boundary against the live grid. The
//! tile `map` region is written through plan-06's [`EcsMapSource`] and read back
//! through the plan-06 [`Context`].
//!
//! Entity/building payloads larger than the tile map remain a plan-07 seam: the
//! load side decodes buildings into `Context::pending_buildings` (consumed by an
//! ECS owner later); the handler restores the tile grid and leaves ECS rebuild to
//! the caller, exactly as the headless `io roundtrip` path documents.

use std::path::Path;

use crate::content::{
    ContentError, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use crate::ecs::BuildingComp;
use crate::entities::comp::Building;
use crate::io::IoResult;
use crate::io::fs::{FileSystem, NativeFs};
use crate::io::save::state::MapSource;
use crate::io::save::versions::v1::base_meta_tags;
use crate::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
use crate::io::wire::WireWriter;
use crate::sim::{IoHandler, IoRequest, IoResponse, IoStatus, Sim};
use crate::world::{Context, EcsMapSource, WorldGrid};

/// [`MapSource`] over the live grid that resolves multiblock centers from either
/// plan-07 [`Building`] entities or the P0 [`BuildingComp`] the base `Sim` uses.
///
/// [`EcsMapSource`] only knows the plan-07 `Building` component, so a P0 placed
/// block (which carries `BuildingComp`) would otherwise be written with its
/// entity chunk but skipped on read (`is_center` false).
struct SimMapSource<'a> {
    grid: &'a WorldGrid,
    inner: EcsMapSource<'a>,
}

impl<'a> SimMapSource<'a> {
    fn new(
        grid: &'a WorldGrid,
        content: &'a ContentRegistry,
        ecs: &'a bevy_ecs::world::World,
    ) -> Self {
        Self {
            grid,
            inner: EcsMapSource::new(grid, content, ecs),
        }
    }
}

impl MapSource for SimMapSource<'_> {
    fn width(&self) -> u16 {
        self.inner.width()
    }

    fn height(&self) -> u16 {
        self.inner.height()
    }

    fn floor_id(&self, index: usize) -> u16 {
        self.inner.floor_id(index)
    }

    fn overlay_id(&self, index: usize) -> u16 {
        self.inner.overlay_id(index)
    }

    fn block_id(&self, index: usize) -> u16 {
        self.inner.block_id(index)
    }

    fn has_building(&self, index: usize) -> bool {
        self.inner.has_building(index)
    }

    fn is_center(&self, index: usize) -> bool {
        let Some(entity) = self.grid.tiles.geti(index).build else {
            return true;
        };
        let width = self.width() as usize;
        let (x, y) = (index % width, index / width);
        if let Some(building) = self.inner.ecs.get::<Building>(entity) {
            return building.tile.x() as usize == x && building.tile.y() as usize == y;
        }
        if let Some(comp) = self.inner.ecs.get::<BuildingComp>(entity) {
            return comp.pos.x() as usize == x && comp.pos.y() as usize == y;
        }
        true
    }

    fn should_save_data(&self, index: usize) -> bool {
        self.inner.should_save_data(index)
    }

    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32) {
        self.inner.tile_data(index)
    }

    fn write_building(&self, index: usize, chunk: &mut WireWriter) -> IoResult<()> {
        self.inner.write_building(index, chunk)
    }

    fn core_team(&self, index: usize) -> Option<u8> {
        self.inner.core_team(index)
    }
}

/// Live-sim IO executor over plan 04's native `MGRS` save format.
///
/// Content is booted per call and dropped afterwards: the save/load boundary is
/// rare, and keeping `ContentRegistry` off the struct keeps the handler `Send`
/// (the registry owns a non-`Send` `Box<dyn ModErrorSink>`, while `IoHandler`
/// requires `Send`).
pub struct SimIoHandler {
    /// Save knobs (native v1, embedded assets off).
    options: SaveOptions,
}

impl Default for SimIoHandler {
    fn default() -> Self {
        Self {
            options: SaveOptions::new(),
        }
    }
}

impl SimIoHandler {
    /// Creates a new handler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a handler (content is booted on each IO call, so this never
    /// fails; the `Result` keeps a stable call-site contract with `mind-gdext`).
    pub fn boot() -> Result<Self, ContentError> {
        Ok(Self::new())
    }

    /// Captures one save request to `path`.
    fn save(&mut self, sim: &Sim, path: &Path, as_map: bool) -> IoStatus {
        let content = match boot_registry() {
            Ok(content) => content,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let width = sim.grid.tiles.width as u16;
        let height = sim.grid.tiles.height as u16;
        let name = if as_map { "map" } else { "sim" };
        let source = SimMapSource::new(&sim.grid, &content, &sim.ecs.0);
        let mut tags = base_meta_tags(width, height, 0, name);
        tags.insert("name".to_owned(), name.to_owned());
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(&content);
        ctx.map = Some(&source);
        match SaveIo::save(&NativeFs, path, &ctx, &self.options) {
            Ok(()) => IoStatus::Ok,
            Err(error) => IoStatus::Failed(error.to_string()),
        }
    }

    /// Applies one load request from `path` onto the live grid.
    fn load(&mut self, sim: &mut Sim, path: &Path) -> IoStatus {
        let content = match boot_registry() {
            Ok(content) => content,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let mut mapper = match boot_registry() {
            Ok(mapper) => mapper,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let bytes = match NativeFs.read(path) {
            Ok(bytes) => bytes,
            Err(error) => return IoStatus::Failed(error.to_string()),
        };
        let mut loaded = WorldGrid::new(0, 0);
        {
            let mut context = Context::new(&mut loaded, &content);
            let mut read = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut mapper),
                ..SaveReadState::default()
            };
            if let Err(error) = SaveIo::load_bytes(&bytes, &mut read) {
                return IoStatus::Failed(error.to_string());
            }
        }
        // Tile-only restore: buildings decoded into `Context::pending_buildings`
        // need a plan-07 ECS owner; the map/tile contract is exact.
        sim.grid = loaded;
        IoStatus::Ok
    }
}

impl IoHandler for SimIoHandler {
    fn apply(&mut self, sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse> {
        requests
            .iter()
            .map(|request| {
                let status = match request {
                    IoRequest::Load { path } => self.load(sim, path),
                    IoRequest::Save { .. } => IoStatus::Deferred,
                };
                IoResponse {
                    request: request.clone(),
                    status,
                }
            })
            .collect()
    }

    fn capture(&mut self, sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse> {
        requests
            .iter()
            .map(|request| {
                let status = match request {
                    IoRequest::Save { path, as_map } => self.save(sim, path, *as_map),
                    IoRequest::Load { .. } => IoStatus::Deferred,
                };
                IoResponse {
                    request: request.clone(),
                    status,
                }
            })
            .collect()
    }
}

/// Boots one base content registry (`createBaseContent` + `init` + `postInit`).
fn boot_registry() -> Result<ContentRegistry, ContentError> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut content = create_base_content(&bundle, &store, true)?;
    content.init()?;
    content.post_init()?;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::content::BlockId;
    use crate::world::TilePos;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mind-sim-io-{tag}-{}.msav", std::process::id()))
    }

    #[test]
    fn live_sim_save_then_load_roundtrips_the_tile_grid() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        let wall = sim.content().id("stone-wall").expect("stone-wall");
        sim.apply(Command::Place {
            x: 2,
            y: 3,
            block: wall,
        })
        .expect("place");
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));

        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        sim.request_save(&path, false);
        sim.tick().expect("capture tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one save response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);
        assert!(path.is_file(), "save file written");

        // Wipe the tile, then load it back at the next Apply boundary.
        sim.grid.fill(BlockId::AIR, BlockId::AIR);
        assert_eq!(sim.grid.block_at(TilePos::new(2, 3)), Some(BlockId::AIR));
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1, "one load response: {responses:?}");
        assert_eq!(responses[0].status, IoStatus::Ok);
        assert_eq!(sim.grid.block_at(TilePos::new(2, 3)), Some(wall));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_of_a_missing_file_is_a_failed_response() {
        let mut sim = Sim::new(1, 4, 4, BlockId::AIR, BlockId::AIR);
        sim.set_io_handler(Box::new(SimIoHandler::boot().expect("content")));
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        sim.request_load(&path);
        sim.tick().expect("apply tick");
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 1);
        assert!(matches!(responses[0].status, IoStatus::Failed(_)));
    }
}
