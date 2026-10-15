// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuildingBehavior` wrappers for the defense family (plan 10 M7 wiring).
//!
//! Ported from `world/blocks/defense/*.java`. The per-tick state math lives in
//! [`super::shields`] and [`super::projectors`]; these wrappers insert the
//! family state at construction and drive it from the normal plan-07
//! `update_buildings` tick, so placed projectors/mines work outside the combat
//! harness. Bullet absorption that needs bullet metadata (absorbable flags,
//! shield damage multipliers) stays on the combat pass.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior, PendingShockMineTriggers};
use crate::world::modules::PowerModule;

use super::projectors::{
    BaseShieldState, OverdriveProjectorState, RegenProjectorState, ShockwaveTowerState,
    update_base_shield, update_overdrive_projector, update_regen_projector, update_shockwave_tower,
};
use super::shields::{
    ForceProjectorState, MendProjectorState, ShieldWallState, ShockMineState, TargetDummyState,
    deflect_bullets_world, update_force_projector_state, update_mend_projector_world,
    update_shield_wall, update_target_dummy,
};
use super::turrets::{behavior::TurretBehavior, config_for};

fn efficiency(world: &World, e: Entity) -> f32 {
    world
        .get::<Building>(e)
        .map(|building| building.efficiency)
        .unwrap_or(0.0)
}

fn optional_efficiency(world: &World, e: Entity) -> f32 {
    world
        .get::<Building>(e)
        .map(|building| building.optional_efficiency)
        .unwrap_or(0.0)
}

fn potential_efficiency(world: &World, e: Entity) -> f32 {
    world
        .get::<Building>(e)
        .map(|building| building.potential_efficiency)
        .unwrap_or(0.0)
}

/// `ForceProjector` behavior (`ForceBuild` construction + state tick).
#[derive(Debug, Clone)]
pub struct ForceProjectorBehavior {
    /// Initial `ForceBuild` state.
    pub state: ForceProjectorState,
    /// `ForceProjector.phaseUseTime` (`ForceProjector.java:32`).
    pub phase_use_time: f32,
    /// `Items.phaseFabric` id for the `itemConsumer` gate
    /// (`Blocks.java:1976`; resolved at registration).
    pub phase_item: Option<crate::content::ItemId>,
    /// `ForceProjector.coolantConsumption` (`ForceProjector.java:41`).
    pub coolant_amount: f32,
}

impl BuildingBehavior for ForceProjectorBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `ForceProjector.sense` (`ForceProjector.java:285-289`).
            LAccess::Heat => Sensed::Num(
                world
                    .get::<crate::world::blocks::defense::shields::ForceProjectorState>(e)
                    .map(|state| f64::from(state.buildup))
                    .unwrap_or(f64::NAN),
            ),
            LAccess::Shield => {
                let state =
                    world.get::<crate::world::blocks::defense::shields::ForceProjectorState>(e)?;
                Sensed::Num(if state.broken {
                    0.0
                } else {
                    f64::from((state.shield_health - state.buildup).max(0.0))
                })
            }
            _ => return None,
        })
    }
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<ForceProjectorState>() else {
            return;
        };
        let eff = efficiency(world, e);
        let delta = crate::world::update::delta(world, e);

        // `ForceProjector.java:207-215`: phase fabric gates the phase heat;
        // `timer(timerUse, phaseUseTime / timeScale)` consumes one per cycle
        // while charged (`consume()` triggers the item consumer).
        let phase_valid = self.phase_item.is_some_and(|item| {
            world
                .get::<crate::world::modules::ItemModule>(e)
                .is_some_and(|items| items.has(item))
        });
        if phase_valid && !state.broken {
            let interval = self.phase_use_time / delta.max(0.000_1);
            let ripe = crate::world::update::run_timer(world, e, 0, interval);
            if ripe
                && eff > 0.0
                && let Some(item) = self.phase_item
                && let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e)
            {
                items.remove(item, 1);
            }
        }

        // `ForceProjector.java:230-240`: an accepted coolant is consumed while
        // there is buildup and scales the decay rate by its heat capacity.
        let mut coolant_heat_capacity = 0.0f32;
        if state.buildup > 0.0 {
            let edelta = eff * delta;
            if let Some((liquid, heat_capacity)) = self.coolant_pick(world, e, edelta)
                && let Some(mut liquids) = world.get_mut::<crate::world::modules::LiquidModule>(e)
            {
                liquids.remove(liquid, self.coolant_amount * edelta);
                coolant_heat_capacity = heat_capacity;
            }
        }

        update_force_projector_state(
            world,
            e,
            &mut state,
            eff,
            phase_valid,
            coolant_heat_capacity,
            delta,
        );

        // `ForceProjector.java:256`: `deflectBullets()` last.
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        if let Some((x, y)) = world.get::<Pos>(e).map(|p| (p.x, p.y))
            && let Some(rt) = world.remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        {
            deflect_bullets_world(world, &rt.content, &mut state, team, x, y);
            world.insert_non_send(rt);
        }
        world.entity_mut(e).insert(state);
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let (broken, buildup, radscl, warmup, phase_heat) = world
            .get::<ForceProjectorState>(e)
            .map(|state| {
                (
                    state.broken,
                    state.buildup,
                    state.radscl,
                    state.warmup,
                    state.phase_heat,
                )
            })
            .unwrap_or((false, 0.0, 0.0, 0.0, 0.0));
        w.bool(broken);
        w.f(buildup);
        w.f(radscl);
        w.f(warmup);
        w.f(phase_heat);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let broken = r.bool().unwrap_or(false);
        let buildup = r.f().unwrap_or(0.0);
        let radscl = r.f().unwrap_or(0.0);
        let warmup = r.f().unwrap_or(0.0);
        let phase_heat = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<ForceProjectorState>(e) {
            state.broken = broken;
            state.buildup = buildup;
            state.radscl = radscl;
            state.warmup = warmup;
            state.phase_heat = phase_heat;
        }
    }
}

impl ForceProjectorBehavior {
    /// `ConsumeCoolant.getConsumed`/`efficiency` (`ConsumeLiquidFilter.java:63-83`):
    /// the current liquid when the coolant filter accepts it and it is present,
    /// otherwise the first content-order liquid that does; `None` when nothing
    /// is consumable.
    fn coolant_pick(
        &self,
        world: &World,
        e: Entity,
        edelta: f32,
    ) -> Option<(crate::content::LiquidId, f32)> {
        if edelta <= 0.000_000_1 {
            return None;
        }
        let traits = world.get_resource::<crate::world::modules::LiquidTraits>()?;
        let module = world.get::<crate::world::modules::LiquidModule>(e)?;
        let current = module.current_liquid();
        let consumed = if traits.coolant_accepts(current, true, false)
            && module.current_liquid_amount() > 0.0
        {
            current
        } else {
            (0..module.liquids.len())
                .map(|index| crate::content::LiquidId::new(index as u16))
                .find(|&liquid| {
                    traits.coolant_accepts(liquid, true, false) && module.get(liquid) > 0.0
                })?
        };
        // `ConsumeLiquidFilter.efficiency(this) > 0`.
        if module.get(consumed) <= 0.0 {
            return None;
        }
        Some((consumed, traits.heat_capacity[consumed.index()]))
    }
}

/// `MendProjector` behavior (`MendBuild` construction + state tick).
#[derive(Debug, Clone)]
pub struct MendProjectorBehavior {
    /// Initial `MendBuild` state.
    pub state: MendProjectorState,
}

impl BuildingBehavior for MendProjectorBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `MendProjector.sense(progress)` (`MendProjector.java:126-129`).
            LAccess::Progress => {
                let state =
                    world.get::<crate::world::blocks::defense::shields::MendProjectorState>(e)?;
                Sensed::Num(if state.reload > 0.0 {
                    f64::from((state.charge / state.reload).clamp(0.0, 1.0))
                } else {
                    0.0
                })
            }
            _ => return None,
        })
    }
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<MendProjectorState>() else {
            return;
        };
        let eff = efficiency(world, e);
        let optional = optional_efficiency(world, e);
        update_mend_projector_world(world, e, &mut state, eff, true, optional);
        world.entity_mut(e).insert(state);
    }
    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let (heat, phase_heat) = world
            .get::<MendProjectorState>(e)
            .map(|state| (state.heat, state.phase_heat))
            .unwrap_or((0.0, 0.0));
        w.f(heat);
        w.f(phase_heat);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let heat = r.f().unwrap_or(0.0);
        let phase_heat = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<MendProjectorState>(e) {
            state.heat = heat;
            state.phase_heat = phase_heat;
        }
    }
}

/// `ShieldWall` behavior (`ShieldWallBuild` construction + state tick).
#[derive(Debug, Clone)]
pub struct ShieldWallBehavior {
    /// Initial `ShieldWallBuild` state.
    pub state: ShieldWallState,
}

impl BuildingBehavior for ShieldWallBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<ShieldWallState>() else {
            return;
        };
        let eff = efficiency(world, e);
        let powered = world
            .get::<PowerModule>(e)
            .is_some_and(|module| module.status > 0.0);
        update_shield_wall(&mut state, eff, powered);
        world.entity_mut(e).insert(state);
    }
    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let shield = world
            .get::<ShieldWallState>(e)
            .map(|state| state.shield)
            .unwrap_or_default();
        w.f(shield);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let shield = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<ShieldWallState>(e) {
            state.shield = shield;
            if shield > 0.0 {
                state.shield_radius = 1.0;
            }
        }
    }
}

/// `ShockMine` behavior (`ShockMineBuild` construction + cooldown tick).
#[derive(Debug, Clone)]
pub struct ShockMineBehavior {
    /// Initial `ShockMineBuild` state.
    pub state: ShockMineState,
}

impl BuildingBehavior for ShockMineBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    /// `ShockMineBuild.unitOn` (`ShockMine.java:69-85`): an enemy unit on the
    /// mine's tile triggers the tendrils at most every `cooldown` ticks and
    /// costs the mine `tileDamage` health. The tendril creation needs a
    /// `CombatCtx`, so the trigger is queued for the unit pass to drain.
    fn unit_on(&self, world: &mut World, e: Entity, unit: Entity) {
        let enabled = world
            .get::<Building>(e)
            .is_some_and(|building| building.enabled);
        let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
        let unit_team = world.get::<TeamComp>(unit).map(|t| t.team).unwrap_or(0);
        if !enabled || unit_team == team {
            return;
        }
        let now = crate::world::update::build_time(world);
        let tile_damage = {
            let Some(mut state) = world.get_mut::<ShockMineState>(e) else {
                return;
            };
            // `timer(timerDamage, cooldown)`: the first call fires.
            if state
                .last_trigger
                .is_some_and(|last| now - last < state.cooldown)
            {
                return;
            }
            state.last_trigger = Some(now);
            state.tile_damage
        };
        crate::combat::damage::area::apply_health(world, e, tile_damage);
        {
            let mut pending = world.get_resource_or_insert_with(PendingShockMineTriggers::default);
            pending.0.push((e, team));
        }
    }
}

/// `TargetDummy` behavior (`TargetDummyBuild` construction + dps tick).
#[derive(Debug, Clone, Copy, Default)]
pub struct TargetDummyDefenseBehavior;

impl BuildingBehavior for TargetDummyDefenseBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(TargetDummyState::default());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        if let Some(mut state) = world.get_mut::<TargetDummyState>(e) {
            update_target_dummy(&mut state);
        }
    }
}

/// `OverdriveProjector` behavior (`OverdriveBuild`).
#[derive(Debug, Clone)]
pub struct OverdriveProjectorBehavior {
    /// Initial `OverdriveBuild` state.
    pub state: OverdriveProjectorState,
}

impl BuildingBehavior for OverdriveProjectorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<OverdriveProjectorState>() else {
            return;
        };
        let eff = efficiency(world, e);
        let optional = optional_efficiency(world, e);
        update_overdrive_projector(world, e, &mut state, eff, optional);
        world.entity_mut(e).insert(state);
    }
    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let (heat, phase_heat) = world
            .get::<OverdriveProjectorState>(e)
            .map(|state| (state.heat, state.phase_heat))
            .unwrap_or((0.0, 0.0));
        w.f(heat);
        w.f(phase_heat);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let heat = r.f().unwrap_or(0.0);
        let phase_heat = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<OverdriveProjectorState>(e) {
            state.heat = heat;
            state.phase_heat = phase_heat;
        }
    }
}

/// `RegenProjector` behavior (`RegenProjectorBuild`).
#[derive(Debug, Clone)]
pub struct RegenProjectorBehavior {
    /// Initial `RegenProjectorBuild` state.
    pub state: RegenProjectorState,
}

impl BuildingBehavior for RegenProjectorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<RegenProjectorState>() else {
            return;
        };
        let eff = efficiency(world, e);
        let optional = optional_efficiency(world, e);
        update_regen_projector(world, e, &mut state, eff, optional);
        world.entity_mut(e).insert(state);
    }
}

/// `ShockwaveTower` behavior (`ShockwaveTowerBuild`).
#[derive(Debug, Clone)]
pub struct ShockwaveTowerBehavior {
    /// Initial `ShockwaveTowerBuild` state.
    pub state: ShockwaveTowerState,
}

impl BuildingBehavior for ShockwaveTowerBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `ShockwaveTower.sense` (`ShockwaveTower.java:109-115`).
            LAccess::Progress => {
                let state = world
                    .get::<crate::world::blocks::defense::projectors::ShockwaveTowerState>(e)?;
                Sensed::Num(if state.reload > 0.0 {
                    f64::from((state.reload_counter / state.reload).clamp(0.0, 1.0))
                } else {
                    0.0
                })
            }
            LAccess::Heat => Sensed::Num(
                world
                    .get::<crate::world::blocks::heat::HeatState>(e)
                    .map(|state| state.heat as f64)
                    .unwrap_or(f64::NAN),
            ),
            _ => return None,
        })
    }
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<ShockwaveTowerState>() else {
            return;
        };
        let potential = potential_efficiency(world, e);
        update_shockwave_tower(world, e, &mut state, potential);
        world.entity_mut(e).insert(state);
    }
}

/// `BaseShield` behavior (`BaseShieldBuild`).
#[derive(Debug, Clone)]
pub struct BaseShieldBehavior {
    /// Initial `BaseShieldBuild` state.
    pub state: BaseShieldState,
}

impl BuildingBehavior for BaseShieldBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<BaseShieldState>() else {
            return;
        };
        let eff = efficiency(world, e);
        update_base_shield(world, e, &mut state, eff);
        world.entity_mut(e).insert(state);
    }
    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let (smooth_radius, broken) = world
            .get::<BaseShieldState>(e)
            .map(|state| (state.smooth_radius, state.broken))
            .unwrap_or((0.0, false));
        w.f(smooth_radius);
        w.bool(broken);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        if revision >= 1 {
            let smooth_radius = r.f().unwrap_or(0.0);
            let broken = r.bool().unwrap_or(false);
            if let Some(mut state) = world.get_mut::<BaseShieldState>(e) {
                state.smooth_radius = smooth_radius;
                state.broken = broken;
            }
        }
    }
}

/// The 28 vanilla turrets + `build-tower` (`Blocks.java` turret region).
pub const TURRET_BLOCKS: &[&str] = &[
    "duo",
    "scatter",
    "scorch",
    "hail",
    "salvo",
    "swarmer",
    "fuse",
    "ripple",
    "cyclone",
    "foreshadow",
    "spectre",
    "breach",
    "diffuse",
    "wave",
    "tsunami",
    "lancer",
    "arc",
    "meltdown",
    "parallax",
    "segment",
    "titan",
    "disperse",
    "afflict",
    "lustre",
    "smite",
    "malign",
    "sublimate",
    "scathe",
    "build-tower",
];

/// Whether every bullet a config references exists in `content` (guards the
/// fixture-name map against unrelated content registries).
fn ammo_in_content(config: &super::turrets::TurretConfig, content: &ContentRegistry) -> bool {
    use super::turrets::TurretAmmo;
    let valid = |id: crate::content::BulletId| id.index() < content.bullets().len();
    match &config.ammo {
        TurretAmmo::Item(entries) => entries.iter().all(|entry| valid(entry.bullet)),
        TurretAmmo::Liquid(entries) => entries.iter().all(|entry| valid(entry.bullet)),
        TurretAmmo::Power(bullet) => valid(*bullet),
        TurretAmmo::Payload(entries) => entries.iter().all(|entry| valid(entry.bullet)),
    }
}

/// Registers the vanilla defense family into plan 07's registry.
///
/// Projectors/mines/walls are always registered. Turrets and `build-tower` are
/// registered when the turret ammo bullets are named in `content`
/// (`turrets::install_turret_ammo` at live content boot, or the
/// `CombatHarness` fixtures).
pub fn register(registry: &mut BehaviorRegistry, content: &ContentRegistry) {
    // `Blocks.java:1921 mender`.
    registry.register_named(
        "mender",
        Arc::new(MendProjectorBehavior {
            state: MendProjectorState {
                reload: 200.0,
                range: 40.0,
                heal_percent: 4.0,
                phase_boost: 4.0,
                phase_range_boost: 20.0,
                ..MendProjectorState::default()
            },
        }),
    );
    // `Blocks.java:1934 mend-projector`.
    registry.register_named(
        "mend-projector",
        Arc::new(MendProjectorBehavior {
            state: MendProjectorState {
                reload: 250.0,
                range: 85.0,
                heal_percent: 11.0,
                phase_boost: 15.0,
                phase_range_boost: 50.0,
                ..MendProjectorState::default()
            },
        }),
    );
    // `Blocks.java:1946 overdrive-projector`.
    registry.register_named(
        "overdrive-projector",
        Arc::new(OverdriveProjectorBehavior {
            state: OverdriveProjectorState::default(),
        }),
    );
    // `Blocks.java:1954 overdrive-dome`.
    registry.register_named(
        "overdrive-dome",
        Arc::new(OverdriveProjectorBehavior {
            state: OverdriveProjectorState {
                range: 200.0,
                speed_boost: 2.5,
                use_time: 300.0,
                ..OverdriveProjectorState::default()
            },
        }),
    );
    // `Blocks.java:1966 force-projector`.
    registry.register_named(
        "force-projector",
        Arc::new(ForceProjectorBehavior {
            state: ForceProjectorState {
                shield_health: 750.0,
                cooldown_normal: 1.5,
                cooldown_liquid: 1.2,
                ..ForceProjectorState::default()
            },
            // `ForceProjector.java:32` `phaseUseTime`.
            phase_use_time: 350.0,
            // `ForceProjector.java:41` `coolantConsumption`.
            coolant_amount: 0.1,
            phase_item: content.item_id("phase-fabric"),
        }),
    );
    // `Blocks.java:1980 shock-mine`.
    registry.register_named(
        "shock-mine",
        Arc::new(ShockMineBehavior {
            state: ShockMineState {
                damage: 25.0,
                tile_damage: 7.0,
                length: 10,
                tendrils: 4,
                ..ShockMineState::default()
            },
        }),
    );
    registry.register_named("target-dummy", Arc::new(TargetDummyDefenseBehavior));
    // `Blocks.java:2011 regen-projector`.
    registry.register_named(
        "regen-projector",
        Arc::new(RegenProjectorBehavior {
            state: RegenProjectorState {
                range: 28,
                heal_percent: 4.0 / 60.0,
                ..RegenProjectorState::default()
            },
        }),
    );
    // `Blocks.java:2040 shockwave-tower`.
    registry.register_named(
        "shockwave-tower",
        Arc::new(ShockwaveTowerBehavior {
            state: ShockwaveTowerState {
                range: 170.0,
                reload: 80.0,
                ..ShockwaveTowerState::default()
            },
        }),
    );
    // `Blocks.java:2050 shield-projector` / `:2058 large-shield-projector`.
    registry.register_named(
        "shield-projector",
        Arc::new(BaseShieldBehavior {
            state: BaseShieldState::default(),
        }),
    );
    registry.register_named(
        "large-shield-projector",
        Arc::new(BaseShieldBehavior {
            state: BaseShieldState {
                radius: 400.0,
                ..BaseShieldState::default()
            },
        }),
    );
    // `Blocks.java:1905 shielded-wall` reuses the shield-wall state machine.
    registry.register_named(
        "shielded-wall",
        Arc::new(ShieldWallBehavior {
            state: ShieldWallState::default(),
        }),
    );

    let names = content.bullet_names();
    if !names.is_empty() {
        for name in TURRET_BLOCKS {
            if let Some(config) = config_for(content, name, names)
                && ammo_in_content(&config, content)
            {
                registry.register_named(name, Arc::new(TurretBehavior::new(config)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::world::BuildHarness;
    use crate::world::blocks::default_registry;

    #[test]
    fn default_registry_installs_defense_behaviors() {
        let harness = BuildHarness::new(8, 8, 7);
        let registry = default_registry(harness.content());
        for name in [
            "mender",
            "mend-projector",
            "overdrive-projector",
            "overdrive-dome",
            "force-projector",
            "shock-mine",
            "target-dummy",
            "regen-projector",
            "shockwave-tower",
            "shield-projector",
            "large-shield-projector",
            "shielded-wall",
        ] {
            assert!(
                registry.get_named(name).is_some(),
                "defense override missing for {name}"
            );
        }
    }

    /// gap5 GAP-5 / K-2: `overdrive-projector` boosts a nearby building's
    /// `timeScale` through the normal behavior tick (`BuildingComp.applyBoost`).
    #[test]
    fn overdrive_projector_boosts_nearby_building() {
        use crate::entities::comp::Building as BuildingComp;
        use crate::world::modules::PowerModule;

        let mut harness = BuildHarness::new(8, 8, 7);
        let overdrive = harness
            .content()
            .block_id("overdrive-projector")
            .expect("overdrive-projector");
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(0, 0, overdrive, 0, true));
        assert!(harness.place(2, 0, wall, 0, true));
        let projector = harness.build_at(0, 0).expect("projector");
        let target = harness.build_at(2, 0).expect("wall");
        let mut boosted = false;
        for _ in 0..80 {
            if let Some(mut power) = harness.world.get_mut::<PowerModule>(projector) {
                power.status = 1.0;
            }
            harness.tick();
            if harness
                .world
                .get::<BuildingComp>(target)
                .is_some_and(|building| building.time_scale > 1.0)
            {
                boosted = true;
                break;
            }
        }
        assert!(boosted, "overdrive boosted the wall's timeScale");
    }

    /// H02 T9 (`Blocks.java:2394`): `canOverdrive = false` blocks (phase
    /// conduit) are skipped by the overdrive projector.
    #[test]
    fn overdrive_skips_phase_conduit() {
        use crate::entities::comp::Building as BuildingComp;
        use crate::world::modules::PowerModule;

        let mut harness = BuildHarness::new(8, 8, 7);
        let overdrive = harness
            .content()
            .block_id("overdrive-projector")
            .expect("overdrive-projector");
        let conduit = harness
            .content()
            .block_id("phase-conduit")
            .expect("phase-conduit");
        assert!(harness.place(0, 0, overdrive, 0, true));
        assert!(harness.place(2, 0, conduit, 0, true));
        let projector = harness.build_at(0, 0).expect("projector");
        let target = harness.build_at(2, 0).expect("phase-conduit");
        for _ in 0..80 {
            if let Some(mut power) = harness.world.get_mut::<PowerModule>(projector) {
                power.status = 1.0;
            }
            harness.tick();
        }
        assert_eq!(
            harness
                .world
                .get::<BuildingComp>(target)
                .map(|building| building.time_scale),
            Some(1.0),
            "phase conduit is never overdriven"
        );
    }

    /// H02 T9 (`BuildingComp.java:467-474`): a weaker boost does not refresh
    /// the duration set by a stronger one.
    #[test]
    fn apply_boost_keeps_duration_for_weaker_boost() {
        use super::super::projectors::apply_boost;
        use crate::entities::comp::Building as BuildingComp;

        let mut harness = BuildHarness::new(8, 8, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(2, 0, wall, 0, true));
        let target = harness.build_at(2, 0).expect("wall");
        apply_boost(&mut harness.world, target, 2.5, 100.0);
        apply_boost(&mut harness.world, target, 1.5, 500.0);
        let building = harness.world.get::<BuildingComp>(target).expect("building");
        assert_eq!(building.time_scale, 2.5);
        assert_eq!(building.time_scale_duration, 100.0);
    }

    /// gap5 GAP-5 / K-2: `regen-projector` heals a damaged building.
    #[test]
    fn regen_projector_heals_damaged_building() {
        use crate::entities::comp::Health;
        use crate::world::modules::{LiquidModule, PowerModule};

        let mut harness = BuildHarness::new(8, 8, 7);
        let regen = harness
            .content()
            .block_id("regen-projector")
            .expect("regen-projector");
        let wall = harness.content().block_id("copper-wall").expect("wall");
        // Footprints are anchored at the block center; a 3x3 projector needs
        // its center at least one tile from the edge.
        assert!(harness.place(3, 3, regen, 0, true));
        assert!(harness.place(6, 3, wall, 0, true));
        let projector = harness.build_at(3, 3).expect("projector");
        let target = harness.build_at(6, 3).expect("wall");
        if let Some(mut health) = harness.world.get_mut::<Health>(target) {
            health.health = 50.0;
        }
        for _ in 0..200 {
            if let Some(mut power) = harness.world.get_mut::<PowerModule>(projector) {
                power.status = 1.0;
            }
            if let Some(hydrogen) = harness.content().liquid_id("hydrogen")
                && let Some(mut liquids) = harness.world.get_mut::<LiquidModule>(projector)
            {
                liquids.add(hydrogen, 1000.0, 1000.0);
            }
            harness.tick();
        }
        let healed = harness
            .world
            .get::<Health>(target)
            .map(|health| health.health)
            .unwrap_or(0.0);
        assert!(healed > 50.0, "regen projector healed the wall: {healed}");
    }

    /// `shockwave-tower` deals one wave of damage to nearby enemy bullets.
    #[test]
    fn shockwave_tower_damages_enemy_bullets() {
        use crate::combat::harness::CombatHarness;
        use crate::world::blocks::defense::projectors::{
            ShockwaveTowerState, update_shockwave_tower,
        };

        let mut harness = CombatHarness::new(16, 16, 7);
        let tower = harness
            .content()
            .block_id("shockwave-tower")
            .expect("shockwave-tower");
        assert!(harness.place(5, 5, tower, 0, true));
        let entity = harness.build_at(5, 5).expect("tower");
        let (x, y) = CombatHarness::tile_center(6, 5);
        let bullet = harness.spawn_bullet("fuse", x, y, 0.0, 2).expect("bullet");
        let mut state = harness
            .build
            .world
            .get_mut::<ShockwaveTowerState>(entity)
            .map(|state| state.clone())
            .expect("state");
        state.reload_counter = state.reload;
        update_shockwave_tower(&mut harness.build.world, entity, &mut state, 1.0);
        assert!(
            harness.build.world.get_entity(bullet).is_err(),
            "enemy bullet destroyed by shockwave"
        );
    }
}

// --- G04 T3a: per-kind save IO (`Building.write`/`read`) ---------------------
// Java bodies: ForceProjector.java:342-359, MendProjector.java:167-178,
// OverdriveProjector.java:162-173, BaseShield.java:149-169 (version 1),
// ShieldWall.java:120-130.
