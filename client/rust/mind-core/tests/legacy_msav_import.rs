// Legacy `MSAV` map import regression (PAR-0155): the authored campaign maps
// carry Arc-style bare `Infinity` float literals in their rules; the import
// normalizes them to the port's `NEVER_F32` sentinel.
#![cfg(feature = "msav-import")]
#![allow(clippy::expect_used)]

use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
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
