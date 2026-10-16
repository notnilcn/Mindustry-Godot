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
use crate::entities::comp::{Pos, TeamComp};
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::{BuildingReader, BuildingWriter};

use super::{
    TurretAmmo, TurretConfig, TurretState, accept_item, accept_liquid, accept_payload, handle_item,
    handle_liquid, handle_payload, save,
};

/// `ItemTurretBuild`/`LiquidTurretBuild`/`PowerTurretBuild` behavior.
pub struct TurretBehavior {
    /// Resolved turret knobs (shared with the harness).
    pub config: Arc<TurretConfig>,
    /// The `block` unit proxy (`TurretBuild.unit`; `Turret.java:297`),
    /// resolved from content at registration. The proxy is a detached entity
    /// (never in `UnitRuntime.units`, never saved) that carries the player's
    /// control for a possessed turret (I01 T12).
    pub block_unit: Option<Arc<crate::content::registries::units::UnitTypeDef>>,
}

impl TurretBehavior {
    /// Creates a behavior from a resolved config.
    pub fn new(config: TurretConfig) -> Self {
        Self {
            config: Arc::new(config),
            block_unit: None,
        }
    }

    /// Attaches the `block` unit def (`UnitTypes.block`; live registration).
    pub fn with_block_unit(
        mut self,
        def: Arc<crate::content::registries::units::UnitTypeDef>,
    ) -> Self {
        self.block_unit = Some(def);
        self
    }
}

impl BuildingBehavior for TurretBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let mut state = TurretState::new(self.config.clone());
        // `TurretBuild`: `public BlockUnitc unit = (BlockUnitc)UnitTypes.block.create(team);`
        // (`Turret.java:297`) — positioned on the turret every update
        // (`unit.tile(this)`, `Turret.java:440-445`) and never added to
        // `Groups.unit`, so it is never saved or updated by the unit pass.
        if let Some(def) = self.block_unit.as_ref() {
            let team = world.get::<TeamComp>(e).map(|team| team.team).unwrap_or(0);
            let (x, y) = world
                .get::<Pos>(e)
                .map(|pos| (pos.x, pos.y))
                .unwrap_or_default();
            let tile = world
                .get::<crate::entities::comp::Building>(e)
                .map(|building| building.tile.pack())
                .unwrap_or(i32::MIN);
            let seq = crate::world::blocks::units::behavior::next_unit_seq(world);
            let unit = crate::entities::comp::unit::lifecycle::spawn_unit_def(
                world, seq, def, team, x, y, 0.0,
            );
            world
                .entity_mut(unit)
                .insert(crate::entities::comp::unit::comp::BlockUnitComp { tile });
            state.block_unit = Some(unit);
        }
        world.entity_mut(e).insert(state);
    }

    /// `TurretBuild` has no explicit `onRemoved`; the proxy is dropped with the
    /// building in the port (it is never in `Groups.unit`).
    fn on_removed(&self, world: &mut World, e: Entity) {
        if let Some(unit) = world
            .get::<TurretState>(e)
            .and_then(|state| state.block_unit)
            && world.get_entity(unit).is_ok()
        {
            let _ = world.despawn(unit);
        }
    }

    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        accept_item(world, e, item)
    }

    /// `ItemTurretBuild.acceptStack` (`ItemTurret.java:133-139`):
    /// `min((int)((maxAmmo - totalAmmo) / ammoMultiplier), amount)` for a known
    /// ammo entry, else 0.
    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        let Some(state) = world.get::<TurretState>(e) else {
            return 0;
        };
        let TurretAmmo::Item(ammo) = &state.config.ammo else {
            return 0;
        };
        let Some(entry) = ammo.iter().find(|entry| entry.item == item) else {
            return 0;
        };
        let room = (state.config.max_ammo - state.total_ammo).max(0) as f32;
        ((room / entry.ammo_multiplier.max(1) as f32) as i32)
            .min(amount)
            .max(0)
    }

    /// `ItemTurretBuild.handleStack` (`ItemTurret.java:142-146`): `handleItem`
    /// once per unit of ammo.
    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        for _ in 0..amount.max(0) {
            handle_item(world, e, item);
        }
    }

    /// `ItemTurretBuild.removeStack` (`ItemTurret.java:149-152`): turrets cannot
    /// be drained.
    fn remove_stack(&self, _world: &mut World, _e: Entity, _item: ItemId, _amount: i32) -> i32 {
        0
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

    /// `Turret.sense`/`ItemTurret.sense`/`PowerTurret.sense` overrides
    /// (`J/world/blocks/defense/turrets/Turret.java:635-665`).
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        let state = world.get::<TurretState>(e)?;
        let cfg = &self.config;
        Some(match a {
            LAccess::Ammo => Sensed::Num(match cfg.ammo {
                TurretAmmo::Power(_) => world
                    .get::<crate::world::modules::PowerModule>(e)
                    .map(|p| p.status as f64)
                    .unwrap_or(0.0),
                // `Turret.sense(ammo)` = `totalAmmo`; liquid turrets never
                // increment it (`LiquidTurret` keeps its own liquid module).
                TurretAmmo::Liquid(_) => state.total_ammo as f64,
                _ => state.total_ammo as f64,
            }),
            LAccess::AmmoCapacity => Sensed::Num(match cfg.ammo {
                TurretAmmo::Power(_) => 1.0,
                _ => cfg.max_ammo as f64,
            }),
            // `BaseTurretBuild.sense`: rotation in degrees.
            LAccess::Rotation => Sensed::Num(state.rotation as f64),
            LAccess::Shooting => Sensed::Num(if state.is_shooting { 1.0 } else { 0.0 }),
            LAccess::ShootX => Sensed::Num((state.target_pos.0 / 8.0) as f64),
            LAccess::ShootY => Sensed::Num((state.target_pos.1 / 8.0) as f64),
            LAccess::Progress => Sensed::Num(if cfg.reload > 0.0 {
                (state.reload_counter / cfg.reload).clamp(0.0, 1.0) as f64
            } else {
                1.0
            }),
            // `PowerTurret.sense(heat)` only: item/liquid turrets fall through
            // to the base table (`NaN` -> null).
            LAccess::Heat => Sensed::Num(match cfg.ammo {
                TurretAmmo::Power(_) if state.heat_req > 0.0 => state.heat_req as f64,
                _ => f64::NAN,
            }),
            LAccess::CurrentAmmoType => match &cfg.ammo {
                TurretAmmo::Item(_) => state
                    .ammo
                    .last()
                    .map(|entry| {
                        Sensed::Obj(Some(crate::logic::value::LogicObject::Content(
                            crate::content::ContentRef::item(entry.item),
                        )))
                    })
                    .unwrap_or(Sensed::Obj(None)),
                // `LiquidTurretBuild.senseObject(currentAmmoType)` and the
                // continuous variant (`LiquidTurret.java:80-85`) return
                // `liquids.current()`, never the most-filled slot.
                TurretAmmo::Liquid(_) => {
                    let current = world
                        .get::<crate::world::modules::LiquidModule>(e)
                        .map(|m| m.current_liquid().index());
                    Sensed::Obj(current.map(|i| {
                        crate::logic::value::LogicObject::Content(crate::content::ContentRef::new(
                            crate::content::ContentType::Liquid,
                            i as u16,
                        ))
                    }))
                }
                _ => Sensed::Obj(None),
            },
            // `Ranged.range()/8`.
            LAccess::Range => Sensed::Num((cfg.range / 8.0) as f64),
            _ => return None,
        })
    }

    /// `TurretBuild.control` (`Turret.java:522-540`): `control shoot` sets the
    /// aim point + the 120-tick cooldown; `control shootp` follows a unit/
    /// building target through `targetPosition`.
    fn logic_control(
        &self,
        world: &mut World,
        e: Entity,
        a: crate::logic::LAccess,
        p: &[crate::logic::LVar; 4],
    ) -> bool {
        use crate::logic::LAccess;
        use crate::logic::value::conv;
        match a {
            // `control(LAccess.shoot, p1, p2, p3, p4)`:
            // `targetPos.set(World.unconv(p1), World.unconv(p2));
            //  logicControlTime = logicControlCooldown;
            //  logicShooting = !Mathf.zero(p3);`
            LAccess::Shoot => {
                let x = conv(p[0].numf());
                let y = conv(p[1].numf());
                let shooting = p[2].num() != 0.0;
                if let Some(mut state) = world.get_mut::<TurretState>(e) {
                    state.target_pos = (x, y);
                    state.logic_control_time =
                        crate::world::blocks::defense::turrets::LOGIC_CONTROL_COOLDOWN;
                    state.logic_shooting = shooting;
                    return true;
                }
                false
            }
            // `control(LAccess.shootp, Object p1, p2, ...)`:
            //  `logicControlTime = logicControlCooldown;
            //   logicShooting = !Mathf.zero(p2);
            //   if(p1 instanceof Posc pos) targetPosition(pos);`
            LAccess::Shootp => {
                let shooting = p[1].num() != 0.0;
                let target = match p[0].value_obj() {
                    Some(crate::logic::value::LogicObject::Unit(unit)) => Some(*unit),
                    Some(crate::logic::value::LogicObject::Building(build)) => Some(*build),
                    _ => None,
                };
                let aim = {
                    let Some(state) = world.get::<TurretState>(e) else {
                        return false;
                    };
                    target.and_then(|target| {
                        crate::world::blocks::defense::turrets::logic_target_position(
                            world, e, state, target,
                        )
                    })
                };
                if let Some(mut state) = world.get_mut::<TurretState>(e) {
                    if let Some((x, y)) = aim {
                        state.target_pos = (x, y);
                    }
                    state.logic_control_time =
                        crate::world::blocks::defense::turrets::LOGIC_CONTROL_COOLDOWN;
                    state.logic_shooting = shooting;
                    return true;
                }
                false
            }
            _ => false,
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

    /// `LiquidTurretBuild.peekAmmo`/`hasAmmo`/`useAmmo`/`acceptLiquid`
    /// (`LiquidTurret.java:128-155`) all key on `liquids.current()` (the last
    /// liquid added), never on the most-filled slot.
    #[test]
    fn liquid_turret_ammo_follows_current_liquid_not_most_filled() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let wave = harness.content().block_id("wave").expect("wave");
        assert!(harness.place(10, 10, wave, 0, true));
        let turret = harness.build_at(10, 10).expect("placed wave");
        let water = harness.content().liquid_id("water").expect("water");
        let slag = harness.content().liquid_id("slag").expect("slag");
        let oil = harness.content().liquid_id("oil").expect("oil");
        let entry = |liquid: LiquidId| {
            let state = harness
                .build
                .world
                .get::<TurretState>(turret)
                .expect("state");
            let TurretAmmo::Liquid(ammo) = &state.config.ammo else {
                panic!("wave is a liquid turret");
            };
            *ammo
                .iter()
                .find(|entry| entry.liquid == liquid)
                .expect("ammo entry")
        };
        let water_ammo = entry(water);
        let slag_ammo = entry(slag);
        assert_ne!(water_ammo.bullet, slag_ammo.bullet);

        // 8 water, then a small slag top-up: slag becomes `current` while water
        // stays the most-filled slot.
        handle_liquid(&mut harness.build.world, turret, water, 8.0);
        let slag_shot = 1.0 / slag_ammo.ammo_multiplier;
        handle_liquid(&mut harness.build.world, turret, slag, slag_shot);
        let world = &harness.build.world;
        assert_eq!(
            super::super::peek_ammo(world, turret),
            Some(slag_ammo.bullet),
            "peekAmmo = ammoTypes.get(liquids.current())"
        );
        assert!(
            super::super::has_ammo(world, turret),
            "one slag shot stored"
        );
        // `acceptLiquid`: another ammo liquid is refused while the current
        // one holds more than one shot (+0.001); slag itself is accepted.
        assert!(accept_liquid(world, turret, slag));
        handle_liquid(&mut harness.build.world, turret, slag, slag_shot);
        assert!(!accept_liquid(&harness.build.world, turret, oil));

        // `useAmmo` drains the current liquid (slag), leaving water untouched.
        let mut state = harness
            .build
            .world
            .get::<TurretState>(turret)
            .expect("state")
            .clone();
        let fired = super::super::use_ammo(&mut state, &mut harness.build.world, turret);
        assert_eq!(fired, Some(slag_ammo.bullet));
        let liquids = harness
            .build
            .world
            .get::<crate::world::modules::LiquidModule>(turret)
            .expect("liquid module");
        assert_eq!(liquids.get(water), 8.0, "water is not consumed");
        assert!((liquids.get(slag) - slag_shot).abs() < 1e-4);
        // Down to one shot: other ammo liquids are accepted again.
        assert!(accept_liquid(&harness.build.world, turret, oil));

        // Empty current liquid: `hasAmmo` is false even though 8 water remain
        // (`currentAmount() >= 1 / ammoMultiplier`), and `peekAmmo` still
        // names the current liquid's bullet.
        let mut liquids = harness
            .build
            .world
            .get_mut::<crate::world::modules::LiquidModule>(turret)
            .expect("liquid module");
        liquids.remove(slag, 100.0);
        let world = &harness.build.world;
        assert!(!super::super::has_ammo(world, turret));
        assert_eq!(
            super::super::peek_ammo(world, turret),
            Some(slag_ammo.bullet)
        );
    }
}
