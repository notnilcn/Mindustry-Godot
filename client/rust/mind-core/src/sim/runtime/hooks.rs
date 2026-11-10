// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! [`SimHooks`]: the [`WorldHooks`] implementation that spawns runtime buildings for
//! [`WorldCtx`](crate::world::ops::WorldCtx) tile ops.
//!
//! Upstream: core/src/mindustry/entities/comp/BuildingComp.java (create, onRemoved).

use super::*;

/// Spawns runtime buildings for [`WorldCtx`] tile ops.
pub(super) struct SimHooks {
    pub(super) seq: AtomicU64,
    pub(super) item_count: usize,
    pub(super) liquid_count: usize,
}

impl WorldHooks for SimHooks {
    fn new_building(&self, world: &mut World, request: NewBuilding) -> Option<Entity> {
        let inst = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.instance(request.block))?;
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let tile = TilePos::new(request.x, request.y);
        let entity = inst.spawn(
            world,
            seq,
            tile,
            request.team,
            request.rot,
            self.item_count,
            self.liquid_count,
        );
        // The P0 checksum/dump stream walks `BuildingComp` entities; keeping it
        // on runtime entities makes `Sim::checksum`/`entities_by_seq` cover the
        // live path without a second entity index.
        world.entity_mut(entity).insert(BuildingComp {
            pos: tile,
            block: request.block,
            team: TeamId(request.team),
            rot: request.rot,
        });
        // K-3/T2: `PowerNodeBehavior::create_state` already inserts the
        // behavior's real `PowerNodeConfig`; only fall back to the default when
        // the kind has no registered behavior (mod nodes). The old
        // unconditional insert overwrote the knobs after `spawn` (PAR: node
        // large/surge tower dropped to 3 links / range 6).
        if matches!(
            inst.def.kind,
            BlockKind::PowerNode | BlockKind::LongPowerNode
        ) && world.get::<PowerNodeConfig>(entity).is_none()
        {
            world.entity_mut(entity).insert(PowerNodeConfig::default());
        }
        Some(entity)
    }

    /// `BuildingComp.onRemoved` (`BuildingComp.java:1145-1150`): dispatch the
    /// block behavior's removal hook (the turret's detached `block` unit proxy
    /// is dropped here, I01 T12).
    fn on_removed(&self, world: &mut World, entity: Entity) {
        if let Some(behavior) = world.get_resource::<BlockTable>().and_then(|table| {
            world
                .get::<crate::entities::comp::Building>(entity)
                .and_then(|building| table.instance(building.block))
                .map(|inst| inst.behavior.clone())
        }) {
            behavior.on_removed(world, entity);
        }
    }
}
