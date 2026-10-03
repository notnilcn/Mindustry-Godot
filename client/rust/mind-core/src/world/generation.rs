// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent resources the world-generation path installs at boot (plan 06
//! M9 / plan 13 M7).
//!
//! The privileged logic VM (`getblock`/`setblock`, `ulocate`, `sensor`) reads
//! [`WorldGrid`], [`GlobalVars`](crate::logic::globals::GlobalVars),
//! [`LogicContentIndex`](crate::logic::world::LogicContentIndex) and
//! [`LogicWorldState`](crate::logic::world::LogicWorldState) from the ECS world.
//! Historically the plan-06 `LogicFilter` built these ad hoc per call; booting
//! them once and reusing the same resources lets map-generation scripts, the
//! editor and the headless harness observe one live world.

use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::logic::globals::GlobalVars;
use crate::logic::world::{LogicContentIndex, LogicWorldState};

use super::WorldGrid;

/// Installs the generation [`WorldGrid`] plus the logic resources the filter VM
/// reads, once per world boot. Idempotent for the logic resources: an existing
/// `GlobalVars`/`LogicContentIndex`/`LogicWorldState` is kept (only the map
/// dimensions on the state are refreshed).
pub fn install_generation_resources(ecs: &mut World, content: &ContentRegistry, grid: WorldGrid) {
    let width = grid.tiles.width;
    let height = grid.tiles.height;
    ecs.insert_resource(grid);
    ensure_generation_resources(ecs, content, width, height);
}

/// Installs the non-grid logic resources if absent and refreshes the map
/// dimensions on [`LogicWorldState`].
pub fn ensure_generation_resources(
    ecs: &mut World,
    content: &ContentRegistry,
    width: i32,
    height: i32,
) {
    if ecs.get_resource::<GlobalVars>().is_none() {
        GlobalVars::install_world(ecs, content);
    }
    if ecs.get_resource::<LogicContentIndex>().is_none() {
        ecs.insert_resource(LogicContentIndex::from_content(content));
    }
    match ecs.get_resource_mut::<LogicWorldState>() {
        Some(mut state) => {
            state.is_host = true;
            if width > 0 {
                state.map_width = width;
                state.map_height = height;
            }
        }
        None => {
            let mut state = LogicWorldState::new();
            state.is_host = true;
            state.map_width = width;
            state.map_height = height;
            ecs.insert_resource(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent_and_sets_dimensions() {
        let content = crate::content::test_support::test_registry();
        let mut ecs = World::new();
        install_generation_resources(&mut ecs, &content, WorldGrid::new(8, 4));
        assert!(ecs.get_resource::<WorldGrid>().is_some());
        assert!(ecs.get_resource::<GlobalVars>().is_some());
        assert!(ecs.get_resource::<LogicContentIndex>().is_some());
        let state = ecs.get_resource::<LogicWorldState>().expect("state");
        assert_eq!((state.map_width, state.map_height), (8, 4));

        // Second call keeps the resources and refreshes only the dimensions.
        ensure_generation_resources(&mut ecs, &content, 16, 2);
        assert!(ecs.get_resource::<GlobalVars>().is_some());
        assert!(ecs.get_resource::<LogicContentIndex>().is_some());
        let state = ecs.get_resource::<LogicWorldState>().expect("state");
        assert_eq!((state.map_width, state.map_height), (16, 2));
    }
}
