// Legacy `MSAV` map import regression (PAR-0155): the authored campaign maps
// carry Arc-style bare `Infinity` float literals in their rules; the import
// normalizes them to the port's `NEVER_F32` sentinel.
#![cfg(feature = "msav-import")]
#![allow(clippy::expect_used)]

use mind_core::content::{
    ContentType, MappedId, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use mind_core::editor::context::EditorContext;
use mind_core::io::json::rules::NEVER_F32;
use mind_core::io::save::SaveIo;
use mind_core::io::save::state::SaveReadState;
use mind_core::world::WorldGrid;

/// Loads one authored map with a real world context, mirroring
/// `MindSimHost::load_grid_file`.
fn load_map(name: &str, registry: &mut mind_core::content::ContentRegistry) -> bool {
    let path = format!(
        "{}/../../../assets/maps/serpulo/{name}.msav",
        env!("CARGO_MANIFEST_DIR")
    );
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("unreadable {path}");
    };
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let mut context = EditorContext::new(&mut grid, registry);
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut *registry),
        ..SaveReadState::default()
    };
    SaveIo::load_bytes(&bytes, &mut state).is_ok()
}

#[test]
fn authored_maps_with_bare_infinity_import() {
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&MemoryBundle::new(), &store, true).expect("content");

    // The maps that failed before PAR-0155's fix (bare `scaling:Infinity` rules)
    // plus the previously-working control.
    assert!(
        load_map("cruxscape", &mut registry),
        "cruxscape.msav imports"
    );
    assert!(
        load_map("hidden/76", &mut registry),
        "hidden/76.msav imports"
    );
    assert!(
        load_map("geothermalStronghold", &mut registry),
        "geothermalStronghold.msav imports"
    );
}

#[test]
fn bare_infinity_maps_to_the_never_sentinel() {
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&MemoryBundle::new(), &store, true).expect("content");
    let path = format!(
        "{}/../../../assets/maps/serpulo/cruxscape.msav",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).expect("cruxscape.msav");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let mut context = EditorContext::new(&mut grid, &registry);
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut registry),
        ..SaveReadState::default()
    };
    SaveIo::load_bytes(&bytes, &mut state).expect("cruxscape loads");
    let rules = state.rules.expect("rules parsed");
    assert!(
        rules
            .spawns
            .iter()
            .any(|group| group.unit_scaling == NEVER_F32),
        "the bare `Infinity` scaling imports as NEVER_F32 ({:?})",
        rules
            .spawns
            .iter()
            .map(|group| group.unit_scaling)
            .collect::<Vec<_>>()
    );
}

/// PAR-0161: the Java Ground Zero save (v7 `MSAV`, written by the reference
/// client) restores the per-kind building bodies on load: drill progress,
/// belt contents and the core inventory/`commandPos` survive; a derelict
/// building with no body does not panic.
#[test]
fn java_save_restores_per_kind_bodies() {
    use mind_core::content::ItemId;
    use mind_core::entities::comp::DrillState;
    use mind_core::sim::Sim;
    use mind_core::world::blocks::distribution::conveyor::ConveyorBuild;
    use mind_core::world::blocks::storage::core_block::CoreBuild;
    use mind_core::world::modules::ItemModule;

    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&MemoryBundle::new(), &store, true).expect("content");
    let path = format!(
        "{}/tests/fixtures/saves/java-gz-r05.msav",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).expect("java gz r05 fixture");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending = {
        let mut context = EditorContext::new(&mut grid, &registry);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut registry),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("fixture loads");
        context.take_pending_buildings()
    };
    grid.end_map_load(&registry);

    let mut sim = Sim::new(
        1,
        1,
        1,
        mind_core::content::BlockId::AIR,
        mind_core::content::BlockId::AIR,
    );
    sim.grid = grid;
    sim.install_block_runtime().expect("block runtime");
    let spawned = sim.materialize_map_buildings(&pending);
    assert!(spawned > 0, "the fixture spawns buildings");

    let entity_at = |sim: &Sim, x: i32, y: i32| sim.grid.tile(x, y).build.expect("building");
    let drill = entity_at(&sim, 135, 51);
    let state = sim.ecs.0.get::<DrillState>(drill).expect("drill state");
    assert!(
        (state.progress - 420.7119).abs() < 1e-3,
        "drill progress {}",
        state.progress
    );
    assert!(
        (state.warmup - 1.0).abs() < 1e-6,
        "drill warmup {}",
        state.warmup
    );

    let conveyor = entity_at(&sim, 133, 55);
    let belt = sim
        .ecs
        .0
        .get::<ConveyorBuild>(conveyor)
        .expect("conveyor state");
    assert_eq!(belt.len, 1, "one item on the belt");
    assert_eq!(belt.ids[0], ItemId::COPPER, "copper on the belt");

    let core = entity_at(&sim, 129, 55);
    let items = sim.ecs.0.get::<ItemModule>(core).expect("core items");
    assert_eq!(items.items[ItemId::COPPER.index()], 4000, "core copper");
    let core_build = sim.ecs.0.get::<CoreBuild>(core).expect("core build");
    assert_eq!(core_build.command_pos, None, "no command pos");

    // Derelict (team 0) router without a per-kind body: materialized, no panic.
    let router = entity_at(&sim, 130, 126);
    let team = sim
        .ecs
        .0
        .get::<mind_core::entities::comp::TeamComp>(router)
        .expect("team");
    assert_eq!(team.team, 0, "router is derelict");
}

/// PAR-0161 guard: the fixture's content header maps every vanilla id to
/// itself (identity). If this fails, per-kind bodies would need content
/// remapping. Verified 2026-10-13 by a header parse of the same fixture.
#[test]
fn java_save_content_order_is_identity() {
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&MemoryBundle::new(), &store, true).expect("content");
    let path = format!(
        "{}/tests/fixtures/saves/java-gz-r05.msav",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).expect("java gz r05 fixture");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let mut context = EditorContext::new(&mut grid, &registry);
    let mut state = SaveReadState {
        context: Some(&mut context),
        content: Some(&mut registry),
        ..SaveReadState::default()
    };
    SaveIo::load_bytes(&bytes, &mut state).expect("fixture loads");
    let mapper = state.last_mapper.as_ref().expect("content mapper");
    for i in 0..22u16 {
        assert_eq!(
            mapper.map(ContentType::Item, i32::from(i)),
            MappedId::Id(i),
            "item {i} maps to itself"
        );
    }
    for i in 0..447u16 {
        assert_eq!(
            mapper.map(ContentType::Block, i32::from(i)),
            MappedId::Id(i),
            "block {i} maps to itself"
        );
    }
}
