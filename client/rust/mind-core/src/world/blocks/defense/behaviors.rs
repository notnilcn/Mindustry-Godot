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
use crate::entities::comp::Building;
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};
use crate::world::modules::PowerModule;

use super::projectors::{
    BaseShieldState, OverdriveProjectorState, RegenProjectorState, ShockwaveTowerState,
    update_base_shield, update_overdrive_projector, update_regen_projector, update_shockwave_tower,
};
use super::shields::{
    ForceProjectorState, MendProjectorState, ShieldWallState, ShockMineState, TargetDummyState,
    update_force_projector_state, update_mend_projector_world, update_shield_wall,
    update_target_dummy,
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
}

impl BuildingBehavior for ForceProjectorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<ForceProjectorState>() else {
            return;
        };
        let eff = efficiency(world, e);
        update_force_projector_state(world, e, &mut state, eff, false, 0.0);
        world.entity_mut(e).insert(state);
    }
}

/// `MendProjector` behavior (`MendBuild` construction + state tick).
#[derive(Debug, Clone)]
pub struct MendProjectorBehavior {
    /// Initial `MendBuild` state.
    pub state: MendProjectorState,
}

impl BuildingBehavior for MendProjectorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.state.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<MendProjectorState>() else {
            return;
        };
        let eff = efficiency(world, e);
        update_mend_projector_world(world, e, &mut state, eff, true, false);
        world.entity_mut(e).insert(state);
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

    fn update_tile(&self, world: &mut World, e: Entity) {
        if let Some(mut state) = world.get_mut::<ShockMineState>(e)
            && state.timer_damage > 0.0
        {
            state.timer_damage -= 1.0;
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
/// registered when the plan-10 turret ammo fixtures are present in `content`
/// (`CombatHarness` installs them; the fixture name map is a bridge until the
/// ammo bullets join the base content registry — `content/registries/bullets.rs`
/// is outside this family's ownership).
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

    if let Some(names) = super::turrets::fixture_names() {
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
