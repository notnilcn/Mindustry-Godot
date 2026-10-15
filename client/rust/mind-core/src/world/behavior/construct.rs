// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ConstructBlock`/`ConstructBuild` IO (`world/blocks/ConstructBlock.java`).
//!
//! PAR-0098 §2d: the construct block has no per-tick behavior of its own (the
//! builder unit drives it through [`crate::world::construct`]), but Java
//! serializes the in-progress state: `progress` (f32), `previous.id`/`current.id`
//! (shorts), and the `accumulator`/`totalAccumulator`/`itemsLeft` triplets behind
//! a count byte (`-1` when unset). `version() = 1`; `itemsLeft` only exists from
//! revision 1.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::{BlockId, ItemStack};
use crate::world::behavior::{BuildingBehavior, BuildingReader, BuildingWriter};
use crate::world::block::BlockTable;
use crate::world::construct::ConstructState;
use crate::world::limits::BuildRules;

/// `ConstructBlock.ConstructBuild` behavior (IO only).
#[derive(Debug, Default, Clone, Copy)]
pub struct ConstructBehavior;

impl ConstructBehavior {
    /// The target block's `(build_time, requirements)` for the read path.
    fn target_def(world: &World, block: BlockId) -> Option<(f32, Vec<ItemStack>)> {
        let table = world.get_resource::<BlockTable>()?;
        let def = &table.get(block)?.def;
        Some((def.build_time, def.requirements.clone()))
    }
}

impl BuildingBehavior for ConstructBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `ConstructBlock.sense` (`ConstructBlock.java:214-221`).
            LAccess::Progress => Sensed::Num(
                world
                    .get::<ConstructState>(e)
                    .map(|state| f64::from(state.progress.clamp(0.0, 1.0)))
                    .unwrap_or(f64::NAN),
            ),
            LAccess::Breaking => Sensed::Num(
                if world
                    .get::<ConstructState>(e)
                    .is_some_and(|state| state.active_deconstruct)
                {
                    1.0
                } else {
                    0.0
                },
            ),
            _ => return None,
        })
    }
    fn version(&self, _world: &World, _e: Entity) -> u8 {
        // `ConstructBuild.version() = 1`.
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        let Some(state) = world.get::<ConstructState>(e) else {
            return;
        };
        w.f(state.progress);
        w.s(state.previous.raw() as i16);
        w.s(state.current.raw() as i16);
        if state.accumulator.is_empty() {
            w.b(-1);
        } else {
            w.b(state.accumulator.len() as i8);
            for index in 0..state.accumulator.len() {
                w.f(state.accumulator.get(index).copied().unwrap_or(0.0));
                w.f(state.total_accumulator.get(index).copied().unwrap_or(0.0));
                w.i(state.items_left.get(index).copied().unwrap_or(0));
            }
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        let progress = r.f().unwrap_or(0.0);
        let previous_id = r.s().unwrap_or(-1);
        let current_id = r.s().unwrap_or(-1);
        let acsize = r.b().unwrap_or(-1);

        let mut accumulator: SmallVec<[f32; 4]> = SmallVec::new();
        let mut total_accumulator: SmallVec<[f32; 4]> = SmallVec::new();
        let mut items_left: SmallVec<[i32; 4]> = SmallVec::new();
        if acsize != -1 {
            for _ in 0..acsize.max(0) {
                let accumulated = r.f().unwrap_or(0.0);
                let total = r.f().unwrap_or(0.0);
                // `itemsLeft` exists from revision 1.
                let left = if revision >= 1 { r.i().unwrap_or(0) } else { 0 };
                accumulator.push(accumulated);
                total_accumulator.push(total);
                items_left.push(left);
            }
        }

        let previous = if previous_id >= 0 {
            BlockId::new(previous_id as u16)
        } else {
            BlockId::AIR
        };
        let current = if current_id >= 0 {
            BlockId::new(current_id as u16)
        } else {
            BlockId::AIR
        };
        // Java leaves `wasConstructing`/`activeDeconstruct` at their defaults on
        // read; a deconstruction stores `current == previous` (`setDeconstruct`).
        let deconstruct = previous == current;
        let target = if deconstruct { previous } else { current };
        let cost_multiplier = world
            .get_resource::<BuildRules>()
            .map(|rules| rules.build_cost_multiplier)
            .unwrap_or(1.0);

        let mut state = ConstructState::construct(previous, current, 0.0);
        if let Some((build_time, requirements)) = Self::target_def(world, target) {
            if deconstruct {
                state.set_deconstruct(previous, build_time, &requirements, cost_multiplier);
            } else {
                state.set_construct(
                    previous,
                    current,
                    build_time,
                    &requirements,
                    cost_multiplier,
                );
            }
        }
        // Restore the serialized progress/accumulators (`set_construct` leaves
        // progress untouched; `set_deconstruct` resets it to 1).
        state.progress = progress;
        if !accumulator.is_empty() {
            state.accumulator = accumulator;
            state.total_accumulator = total_accumulator;
            state.items_left = items_left;
        }
        world.entity_mut(e).insert(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::wire::{WireReader, WireWriter};

    #[test]
    fn construct_state_round_trips() {
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        let e = world
            .spawn(crate::entities::comp::Building::new(
                crate::world::TilePos::new(2, 2),
                BlockId::new(1),
                0,
            ))
            .id();
        let copper_wall = BlockId::new(1);
        let mut state = ConstructState::construct(BlockId::AIR, copper_wall, 3.0);
        state.progress = 0.75;
        state.accumulator = smallvec::smallvec![0.5];
        state.total_accumulator = smallvec::smallvec![0.25];
        state.items_left = smallvec::smallvec![3];
        world.entity_mut(e).insert(state);

        let mut bytes = Vec::new();
        {
            let mut writer = WireWriter::new(&mut bytes);
            ConstructBehavior.write(&world, e, &mut writer);
        }
        world.entity_mut(e).remove::<ConstructState>();

        let mut reader = WireReader::new(&bytes);
        ConstructBehavior.read(&mut world, e, &mut reader, 1);
        let state = world.get::<ConstructState>(e).expect("state");
        assert_eq!(state.progress, 0.75);
        assert_eq!(state.previous, BlockId::AIR);
        assert_eq!(state.current, copper_wall);
        assert_eq!(state.accumulator.as_slice(), &[0.5]);
        assert_eq!(state.total_accumulator.as_slice(), &[0.25]);
        assert_eq!(state.items_left.as_slice(), &[3]);
    }
}
