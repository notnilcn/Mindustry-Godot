// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightBlock`/`LightBuild` behavior (`world/blocks/power/LightBlock.java`).
//!
//! PAR-0093: the illuminator stores an integer color (`Pal.accent` by default),
//! configured through the color picker (`config(Integer.class, (tile, value) ->
//! tile.color = value)`) and saved as an `i` in revision 0. Light rendering is
//! the render agent's half; the packed color is exposed through
//! `MindSimHost::light_color`.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::pal::ACCENT;
use crate::entities::comp::Building;
use crate::io::entity::{EntityReader, EntityWriter};
use crate::world::behavior::BuildingBehavior;
use crate::world::config::{ConfigKind, ConfigValue};

/// `LightBuild` runtime state.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct LightState {
    /// Packed RGBA color (`LightBuild.color`; `Pal.accent.rgba()` default).
    pub color: u32,
    /// `LightBuild.smoothTime` (lerps toward `timeScale` in `updateTile`).
    pub smooth_time: f32,
}

impl Default for LightState {
    fn default() -> Self {
        Self {
            color: ACCENT.to_rgba8888(),
            smooth_time: 1.0,
        }
    }
}

/// `LightBlock.LightBuild` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct LightBehavior;

impl BuildingBehavior for LightBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(LightState::default());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // `LightBuild.updateTile`: `smoothTime = lerpDelta(smoothTime, timeScale, 0.1f)`.
        let time_scale = world
            .get::<Building>(e)
            .map(|building| building.time_scale)
            .unwrap_or(1.0);
        if let Some(mut state) = world.get_mut::<LightState>(e) {
            state.smooth_time += (time_scale - state.smooth_time) * 0.1;
        }
    }

    fn config_kinds(&self) -> &'static [ConfigKind] {
        &[ConfigKind::Number]
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<LightState>(e)
            .map(|state| ConfigValue::Number(f64::from(state.color)))
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        // `LightBuild.config(Integer.class)`: the picker's `res.rgba()`.
        if let ConfigValue::Number(number) = value
            && let Some(mut state) = world.get_mut::<LightState>(e)
        {
            state.color = number as i64 as u32;
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        // `LightBuild.write`: `write.i(color)`.
        let color = world
            .get::<LightState>(e)
            .map(|state| state.color)
            .unwrap_or(0);
        w.i(color as i32);
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, _revision: u8) {
        // `LightBuild.read`: `color = read.i()`.
        if let Ok(color) = r.i()
            && let Some(mut state) = world.get_mut::<LightState>(e)
        {
            state.color = color as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn spawn_light(world: &mut EcsWorld) -> Entity {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named("illuminator").expect("illuminator").clone();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        inst.spawn(
            world,
            0,
            TilePos::new(2, 2),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        )
    }

    #[test]
    fn light_config_round_trip() {
        let mut world = EcsWorld::new();
        let e = spawn_light(&mut world);
        // Default is `Pal.accent.rgba()`.
        assert_eq!(
            LightBehavior.config(&world, e),
            ConfigValue::Number(f64::from(0xffd37fffu32))
        );
        LightBehavior.configured(
            &mut world,
            e,
            None,
            ConfigValue::Number(f64::from(0x1234abcd)),
        );
        assert_eq!(
            LightBehavior.config(&world, e),
            ConfigValue::Number(f64::from(0x1234abcd))
        );
        assert_eq!(LightBehavior.config_kinds(), &[ConfigKind::Number]);
    }

    #[test]
    fn light_color_round_trips_through_io() {
        let mut world = EcsWorld::new();
        let e = spawn_light(&mut world);
        LightBehavior.configured(
            &mut world,
            e,
            None,
            ConfigValue::Number(f64::from(0x0a0b0c0d)),
        );
        let mut bytes = Vec::new();
        {
            let mut writer = crate::io::wire::WireWriter::new(&mut bytes);
            LightBehavior.write(&world, e, &mut writer);
        }
        if let Some(mut state) = world.get_mut::<LightState>(e) {
            state.color = 0;
        }
        let mut reader = crate::io::wire::WireReader::new(&bytes);
        LightBehavior.read(&mut world, e, &mut reader, 0);
        assert_eq!(
            LightBehavior.config(&world, e),
            ConfigValue::Number(f64::from(0x0a0b0c0d))
        );
    }
}
