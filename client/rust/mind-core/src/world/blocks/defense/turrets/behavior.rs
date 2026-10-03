// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret `BuildingBehavior` (`ItemTurret`/`LiquidTurret`/`PowerTurret`
//! `acceptItem`/`handleItem`/`acceptLiquid`/`handleLiquid` + turret save).
//!
//! Plan 10 §3.2/R-10-1: plan 07's `BuildingBehavior::update_tile` only receives
//! `&mut World` (no content registry), so the per-tick turret engine stays
//! harness-driven ([`super::update_turrets`]). This behavior wires the pieces
//! that *do* fit the frozen 07 surface — logistics item/liquid acceptance, the
//! `TurretState` component lifecycle and the §6.3 save fields — so placed
//! turrets can be fed through plan 08's transfer path and round-trip through
//! plan 07's `BuildingCodec`. `update_tile` is intentionally left as the trait
//! default (no-op) to avoid double-updating alongside the harness pass.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ItemId, LiquidId};
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::{BuildingReader, BuildingWriter};

use super::{
    TurretConfig, TurretState, accept_item, accept_liquid, accept_payload, handle_item,
    handle_liquid, handle_payload, save,
};

/// `ItemTurretBuild`/`LiquidTurretBuild`/`PowerTurretBuild` behavior.
pub struct TurretBehavior {
    /// Resolved turret knobs (shared with the harness).
    pub config: Arc<TurretConfig>,
}

impl TurretBehavior {
    /// Creates a behavior from a resolved config.
    pub fn new(config: TurretConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

impl BuildingBehavior for TurretBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world
            .entity_mut(e)
            .insert(TurretState::new(self.config.clone()));
    }

    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        accept_item(world, e, item)
    }

    fn handle_item(&self, world: &mut World, e: Entity, _src: Entity, item: ItemId) {
        handle_item(world, e, item);
    }

    fn accept_liquid(&self, world: &World, e: Entity, _src: Entity, liquid: LiquidId) -> bool {
        accept_liquid(world, e, liquid)
    }

    fn handle_liquid(
        &self,
        world: &mut World,
        e: Entity,
        _src: Entity,
        liquid: LiquidId,
        amount: f32,
    ) {
        handle_liquid(world, e, liquid, amount);
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        accept_payload(world, e, payload)
    }

    fn handle_payload(&self, world: &mut World, e: Entity, _source: Entity, payload: PayloadRef) {
        handle_payload(world, e, payload);
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        save::version(self.config.kind)
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<TurretState>(e) {
            let _ = save::write(w, state);
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        if let Some(mut state) = world.get_mut::<TurretState>(e) {
            let _ = save::read(r, &mut state, revision);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    /// `ItemTurretBuild.acceptItem`/`handleItem` through the behavior seam.
    #[test]
    fn placed_turret_accepts_and_stores_item() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let duo = harness.content().block_id("duo").expect("duo");
        assert!(harness.place(10, 10, duo, 0, true));
        let turret = harness.build_at(10, 10).expect("placed duo");
        // `create_state` ran during placement, so the turret owns `TurretState`.
        assert!(
            harness.build.world.get::<TurretState>(turret).is_some(),
            "TurretState inserted by create_state"
        );
        let copper = harness.content().item_id("copper").expect("copper");
        assert!(accept_item(&harness.build.world, turret, copper));
        for _ in 0..3 {
            handle_item(&mut harness.build.world, turret, copper);
        }
        let state = harness.build.world.get::<TurretState>(turret).unwrap();
        assert_eq!(state.total_ammo, 6, "3 items x duo copper ammoMultiplier 2");
        assert_eq!(state.ammo[0].amount, 6);
    }

    /// Full save/load round-trip through plan-07's `BuildingCodec` (plan 10
    /// §6.3/§7e): a placed `duo` with copper ammo, reload and rotation survives
    /// `write` -> `read` on a fresh entity.
    #[test]
    fn placed_turret_ammo_and_reload_roundtrip() {
        use crate::io::wire::{WireReader, WireWriter};
        use crate::world::TilePos;
        use crate::world::building_io::BuildingCodec;

        let mut harness = CombatHarness::new(32, 32, 7);
        let duo = harness.content().block_id("duo").expect("duo");
        assert!(harness.place(10, 10, duo, 0, true));
        let src = harness.build_at(10, 10).expect("placed duo");
        let copper = harness.content().item_id("copper").expect("copper");
        for _ in 0..3 {
            handle_item(&mut harness.build.world, src, copper);
        }
        {
            let mut state = harness.build.world.get_mut::<TurretState>(src).unwrap();
            state.reload_counter = 0.5;
            state.rotation = 42.0;
        }

        let mut bytes = Vec::new();
        {
            let mut w = WireWriter::new(&mut bytes);
            BuildingCodec::write(&harness.build.world, src, &mut w, false).expect("write");
        }

        let inst = harness
            .build
            .table()
            .instance(duo)
            .expect("duo instance")
            .clone();
        let (items, liquids) = harness.build.module_slot_counts();
        let dst = inst.spawn(
            &mut harness.build.world,
            99,
            TilePos::new(12, 10),
            0,
            0,
            items,
            liquids,
        );
        inst.behavior.create_state(&mut harness.build.world, dst);
        let mut r = WireReader::new(&bytes);
        BuildingCodec::read(&mut harness.build.world, dst, &mut r, 3).expect("read");

        let state = harness
            .build
            .world
            .get::<TurretState>(dst)
            .expect("dst state");
        assert_eq!(state.reload_counter, 0.5);
        assert_eq!(state.rotation, 42.0);
        assert_eq!(state.total_ammo, 6, "3 copper x duo ammoMultiplier 2");
        assert_eq!(state.ammo.first().map(|entry| entry.amount), Some(6));
    }

    /// `LiquidTurretBuild.acceptLiquid` through the behavior seam.
    #[test]
    fn placed_liquid_turret_accepts_matching_liquid() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let wave = harness.content().block_id("wave").expect("wave");
        assert!(harness.place(10, 10, wave, 0, true));
        let turret = harness.build_at(10, 10).expect("placed wave");
        let water = harness.content().liquid_id("water").expect("water");
        // `wave` accepts water, slag, cryofluid and oil only.
        assert!(accept_liquid(&harness.build.world, turret, water));
        handle_liquid(&mut harness.build.world, turret, water, 5.0);
        let liquids = harness
            .build
            .world
            .get::<crate::world::modules::LiquidModule>(turret)
            .expect("liquid module");
        assert!(liquids.get(water) > 0.0, "water stored in the turret");
    }
}
