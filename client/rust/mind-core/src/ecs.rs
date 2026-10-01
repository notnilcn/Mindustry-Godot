// SPDX-License-Identifier: GPL-3.0-only

//! ECS components and the stable entity-ordering helper.
//!
//! Ported from `core/src/mindustry/entities/comp/*` (P0 subset: a placed block is
//! a `BuildingComp` entity). The C# `ComponentRegistration`/`EntityRegistry`
//! framework is replaced by `bevy_ecs` archetypes (HIGH_LEVEL_PLAN §1).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::world::TilePos;

/// Team identifier (P0: `sharded = 0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct TeamId(pub u8);

impl TeamId {
    /// Default team, matching Mindustry's `Team.sharded` id at P0.
    pub const SHARDED: TeamId = TeamId(0);
}

/// Monotonic per-entity sequence, used as the deterministic iteration key and the
/// `build_id` in dumps/checksums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Component)]
pub struct EntitySeq(pub u64);

/// A placed block.
///
/// Save/network IO is derived (`plan 04 M3`); revision manifests live in
/// `mind-core/revisions/BuildingComp/`. Field changes are serialization
/// changes (HLP §6.2): append `#[entity(since = N)]` fields, never reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, mind_derive::EntityIo)]
#[entity(name = "BuildingComp")]
pub struct BuildingComp {
    /// Tile the block occupies.
    pub pos: TilePos,
    /// Block content id.
    pub block: BlockId,
    /// Owning team.
    pub team: TeamId,
    /// Rotation (0 at P0).
    pub rot: u8,
}

/// Monotonic `EntitySeq` allocator.
#[derive(Debug, Clone, Default)]
pub struct EntitySequencer {
    next: u64,
}

impl EntitySequencer {
    /// Allocates the next sequence value.
    pub fn alloc(&mut self) -> u64 {
        let value = self.next;
        self.next = self.next.wrapping_add(1);
        value
    }

    /// Next value that will be allocated.
    pub fn peek(&self) -> u64 {
        self.next
    }
}

/// Thin wrapper over a `bevy_ecs::World` with the P0 helpers.
#[derive(Debug, Default)]
pub struct MindWorld(pub World);

impl MindWorld {
    /// Creates an empty world.
    pub fn new() -> Self {
        Self(World::new())
    }

    /// Spawns a placed block with its sequence.
    pub fn spawn_building(&mut self, seq: u64, comp: BuildingComp) -> Entity {
        self.0.spawn((EntitySeq(seq), comp)).id()
    }

    /// Despawns an entity, returning whether it existed.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        self.0.despawn(entity)
    }

    /// Sequence of an entity, if it has one.
    pub fn seq_of(&self, entity: Entity) -> Option<u64> {
        self.0.get::<EntitySeq>(entity).map(|seq| seq.0)
    }

    /// All building entities sorted by [`EntitySeq`] — the canonical iteration order.
    pub fn entities_by_seq(&self) -> Vec<(u64, Entity, BuildingComp)> {
        let mut buildings: Vec<(u64, Entity, BuildingComp)> = self
            .0
            .iter_entities()
            .filter_map(|entity_ref| {
                let seq = entity_ref.get::<EntitySeq>()?;
                let comp = entity_ref.get::<BuildingComp>()?;
                Some((seq.0, entity_ref.id(), *comp))
            })
            .collect();
        buildings.sort_by_key(|(seq, _, _)| *seq);
        buildings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_seq_is_monotonic_and_ordering_is_stable() {
        let mut world = MindWorld::new();
        let mut sequencer = EntitySequencer::default();
        let first = sequencer.alloc();
        let second = sequencer.alloc();
        let third = sequencer.alloc();
        assert_eq!((first, second, third), (0, 1, 2));

        world.spawn_building(
            third,
            BuildingComp {
                pos: TilePos::new(2, 2),
                block: BlockId::STONE_WALL,
                team: TeamId::SHARDED,
                rot: 0,
            },
        );
        world.spawn_building(
            first,
            BuildingComp {
                pos: TilePos::new(1, 1),
                block: BlockId::STONE_WALL,
                team: TeamId::SHARDED,
                rot: 0,
            },
        );
        world.spawn_building(
            second,
            BuildingComp {
                pos: TilePos::new(3, 3),
                block: BlockId::STONE_WALL,
                team: TeamId::SHARDED,
                rot: 0,
            },
        );

        let ordered = world.entities_by_seq();
        let seqs: Vec<u64> = ordered.iter().map(|(seq, _, _)| *seq).collect();
        assert_eq!(seqs, vec![0, 1, 2]);
        assert_eq!(ordered[0].2.pos, TilePos::new(1, 1));

        let entity = ordered[1].1;
        assert_eq!(world.seq_of(entity), Some(1));
        assert!(world.despawn(entity));
        assert_eq!(world.entities_by_seq().len(), 2);
    }
}
