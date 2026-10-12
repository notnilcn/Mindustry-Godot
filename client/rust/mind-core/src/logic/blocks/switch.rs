// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SwitchBlock`/`SwitchBuild` (plan 13 M3).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/SwitchBlock.java`. The
//! enabled flag is the inherited `Building.enabled` field upstream (the plan's
//! `SwitchBlockState` is that field); this behavior supplies the `Boolean`
//! config, the revision-1 codec and the privileged gate.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::io::entity::{EntityReader, EntityWriter};
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;

/// `SwitchBlock.SwitchBuild` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct SwitchBehavior;

impl BuildingBehavior for SwitchBehavior {
    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        if let ConfigValue::Bool(flag) = value
            && let Some(mut building) = world.get_mut::<Building>(e)
        {
            building.enabled = flag;
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<Building>(e)
            .map(|building| ConfigValue::Bool(building.enabled))
            .unwrap_or(ConfigValue::None)
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        let enabled = world
            .get::<Building>(e)
            .map(|building| building.enabled)
            .unwrap_or(false);
        w.bool(enabled);
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, revision: u8) {
        if revision != 1 {
            return;
        }
        let Ok(enabled) = r.bool() else {
            return;
        };
        if let Some(mut building) = world.get_mut::<Building>(e) {
            building.enabled = enabled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::TilePos;

    /// PAR-0108: the boolean config is the inherited `enabled` flag.
    #[test]
    fn switch_config_toggles_enabled() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Building::new(TilePos::new(2, 2), crate::content::BlockId::AIR, 0),
                crate::entities::comp::TeamComp { team: 0 },
            ))
            .id();
        assert_eq!(
            SwitchBehavior.config(&world, entity),
            ConfigValue::Bool(true)
        );
        SwitchBehavior.configured(&mut world, entity, None, ConfigValue::Bool(false));
        assert_eq!(
            SwitchBehavior.config(&world, entity),
            ConfigValue::Bool(false)
        );
        SwitchBehavior.configured(&mut world, entity, None, ConfigValue::Bool(true));
        assert_eq!(
            SwitchBehavior.config(&world, entity),
            ConfigValue::Bool(true)
        );
        // Non-boolean configs are ignored.
        SwitchBehavior.configured(&mut world, entity, None, ConfigValue::None);
        assert_eq!(
            SwitchBehavior.config(&world, entity),
            ConfigValue::Bool(true)
        );
    }
}
