// SPDX-License-Identifier: GPL-3.0-only

//! Unit factory/reconstructor build previews.
//! Upstream: core/src/mindustry/world/blocks/units/UnitFactory.java

use super::*;

/// `UnitFactory.draw` preview numbers: `(unit, progress / plan.time)` for the
/// selected plan (`UnitFactory.java:380-397`; H03 T9). Pure so it is testable
/// without a Godot host.
pub(crate) fn factory_preview(
    plans: &[mind_core::content::registries::blocks::UnitPlanDef],
    current_plan: i32,
    progress: f32,
) -> (Option<mind_core::content::UnitTypeId>, f32) {
    let plan = usize::try_from(current_plan)
        .ok()
        .and_then(|index| plans.get(index));
    let unit = plan.map(|plan| plan.unit);
    let fraction = plan
        .map(|plan| (progress / plan.time.max(f32::EPSILON)).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    (unit, fraction)
}

/// `Reconstructor.draw` preview numbers: `(upgrade target`,
/// `progress / constructTime`, arrived)` (`Reconstructor.java:287-317`;
/// H03 T9). Pure so it is testable without a Godot host.
pub(crate) fn reconstructor_preview(
    upgrades: &[(
        mind_core::content::UnitTypeId,
        mind_core::content::UnitTypeId,
    )],
    payload: Option<mind_core::content::UnitTypeId>,
    progress: f32,
    construct_time: f32,
    move_in: f32,
) -> (Option<mind_core::content::UnitTypeId>, f32, bool) {
    let target = payload.and_then(|from| {
        upgrades
            .iter()
            .find(|(source, _)| *source == from)
            .map(|(_, target)| *target)
    });
    let fraction = (progress / construct_time.max(f32::EPSILON)).clamp(0.0, 1.0);
    (target, fraction, move_in >= 1.0)
}

impl MindSimHost {
    /// Live unit draw list — `(x, y, rotation degrees, region name, flying,
    /// low altitude)` — for the plan-16 unit pass. Empty without the runtime.
    pub fn unit_draws(&self) -> Vec<(f32, f32, f32, String, bool, bool)> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        runtime
            .units
            .iter()
            .filter_map(|entity| {
                let pos = self.sim.ecs.0.get::<Pos>(*entity)?;
                let core = self.sim.ecs.0.get::<UnitCore>(*entity)?;
                let type_comp = self.sim.ecs.0.get::<UnitTypeComp>(*entity)?;
                let def = content.unit(type_comp.type_id)?;
                Some((
                    pos.x,
                    pos.y,
                    core.rotation,
                    def.name.clone(),
                    def.flying,
                    def.low_altitude,
                ))
            })
            .collect()
    }

    /// Full per-unit draw state (`UnitType.draw` composition; plan §3.4):
    /// body/team/health plus mech walk state and weapon mounts. Rust-only.
    pub fn unit_draws_full(&self) -> Vec<UnitDraw> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::health::Health;
        use mind_core::entities::comp::unit::comp::{MechComp, UnitCore, UnitTypeComp};
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        runtime
            .units
            .iter()
            .filter_map(|entity| {
                let pos = self.sim.ecs.0.get::<Pos>(*entity)?;
                let core = self.sim.ecs.0.get::<UnitCore>(*entity)?;
                let type_comp = self.sim.ecs.0.get::<UnitTypeComp>(*entity)?;
                let def = content.unit(type_comp.type_id)?;
                let team = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::TeamComp>(*entity)
                    .map(|comp| comp.team)
                    .unwrap_or(0);
                let healthf = self
                    .sim
                    .ecs
                    .0
                    .get::<Health>(*entity)
                    .map(Health::fraction)
                    .unwrap_or(1.0);
                let mech = self
                    .sim
                    .ecs
                    .0
                    .get::<MechComp>(*entity)
                    .map(|mech| MechDraw {
                        base_rotation: mech.base_rotation,
                        walk_time: mech.walk_time,
                        stride: def.mech_stride.max(0.001),
                    });
                let treads = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::TankComp>(*entity)
                    .map(|tank| TreadsDraw {
                        tread_time: tank.tread_time,
                        rects: def.tread_rects.clone(),
                        frames: def.tread_frames.max(1),
                    });
                // L04 T3: `CrawlComp` draw state (`UnitType.drawCrawl`).
                let crawl = if def.segments > 0 {
                    self.sim
                        .ecs
                        .0
                        .get::<mind_core::entities::comp::unit::comp::CrawlComp>(*entity)
                } else {
                    None
                }
                .map(|comp| CrawlDraw {
                    crawl_time: comp.crawl_time,
                    segment_rot: comp.segment_rot,
                    segments: def.segments,
                    segment_scl: def.segment_scl,
                    segment_mag: def.segment_mag,
                    segment_phase: def.segment_phase,
                });
                // L04 T3: `drawPayload` reads `payloads().first()` (oldest).
                let payload = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::UnitPayloads>(*entity)
                    .and_then(|payloads| payloads.stack.first())
                    .and_then(|payload| match payload {
                        mind_core::entities::comp::unit::comp::CarriedPayload::Unit {
                            type_id,
                            team,
                            health,
                            rotation,
                        } => {
                            let carried = content.unit(*type_id)?;
                            Some(PayloadDraw::Unit {
                                name: carried.name.clone(),
                                rotation: *rotation,
                                team: *team,
                                healthf: if carried.health > 0.0 {
                                    (*health / carried.health).clamp(0.0, 1.0)
                                } else {
                                    0.0
                                },
                                draw_body: carried.draw_body,
                                draw_cell: carried.draw_cell,
                                segments: carried.segments,
                            })
                        }
                        mind_core::entities::comp::unit::comp::CarriedPayload::Build {
                            block,
                            ..
                        } => {
                            let carried = content.block(*block)?;
                            Some(PayloadDraw::Build {
                                block: carried.name.clone(),
                                region: carried.region.clone(),
                                size: carried.size,
                            })
                        }
                    });
                let legs = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::LegsComp>(*entity)
                    .map(|comp| LegsDraw {
                        base_rotation: comp.base_rotation,
                        legs: comp
                            .legs
                            .iter()
                            .map(|leg| mind_core::render::unit_draw::LegDrawLeg {
                                foot: (leg.foot_x, leg.foot_y),
                                joint: (leg.joint_x, leg.joint_y),
                                moving: leg.moving,
                                stage: leg.step,
                            })
                            .collect(),
                        // `UnitType.drawLegs` reads `1 - unit.drownTime`; the
                        // port has no drowning yet (`UnitCore.drown_time`).
                        inv_drown: 1.0,
                        leg_base_offset: def.leg_base_offset,
                        leg_straightness: def.leg_straightness,
                        base_leg_straightness: def.base_leg_straightness,
                        leg_straight_length: def.leg_straight_length,
                        leg_extension: def.leg_extension,
                        leg_base_under: def.leg_base_under,
                        shadow_elevation: def.shadow_elevation,
                    });
                let weapons = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::weapons::UnitWeapons>(*entity)
                    .map(|weapons| {
                        weapons
                            .weapons
                            .iter()
                            .enumerate()
                            .map(|(index, weapon)| {
                                let mount = weapons.mounts.get(index);
                                WeaponDraw {
                                    name: weapon.name.clone(),
                                    x: weapon.x,
                                    y: weapon.y,
                                    mount_rotation: mount
                                        .map(|mount| mount.rotation)
                                        .unwrap_or(weapon.base_rotation),
                                    base_rotation: weapon.base_rotation,
                                    rotate: weapon.rotate,
                                    flip: weapon.flip_sprite,
                                    top: weapon.top,
                                    recoil: mount.map(|mount| mount.recoil).unwrap_or(0.0),
                                    def: weapon.clone(),
                                    mount: mount
                                        .cloned()
                                        .unwrap_or_else(|| WeaponMount::new(weapon)),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let id = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::ecs::EntitySeq>(*entity)
                    .map(|seq| seq.0)
                    .unwrap_or(0);
                let mine_tile = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::MinerComp>(*entity)
                    .filter(|miner| miner.mining)
                    .and_then(|miner| miner.mine_tile.map(|pos| (pos.x(), pos.y())));
                let item = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::ItemsComp>(*entity)
                    .and_then(|items| items.item)
                    .and_then(|(item, amount)| {
                        self.content_snapshot
                            .as_ref()
                            .and_then(|content| content.item(item))
                            .map(|item| (item.name.clone(), amount))
                    });
                let is_player = self.sim.player_unit() == Some(*entity);
                let health = self
                    .sim
                    .ecs
                    .0
                    .get::<Health>(*entity)
                    .map(|health| health.health)
                    .unwrap_or(0.0);
                // H03 T10: active status effects with their view parameters
                // (`StatusEffect.effect/effectChance/color`).
                let statuses = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::StatusComp>(*entity)
                    .map(|comp| {
                        comp.statuses
                            .iter()
                            .filter_map(|entry| content.status(entry.effect))
                            .filter(|def| {
                                !matches!(
                                    &def.effect,
                                    mind_core::content::registries::fx_meta::EffectRef::Named(id)
                                        if *id == mind_core::content::EffectId::NONE
                                ) && def.effect_chance > 0.0
                            })
                            .map(|def| (def.effect.clone(), def.effect_chance, def.color))
                            .collect()
                    })
                    .unwrap_or_default();
                // H03 T10: floor under the unit (`floorOn()`).
                let (floor_is_liquid, floor_color) = {
                    let tile = self
                        .sim
                        .grid
                        .tiles
                        .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));
                    let floor = tile.and_then(|tile| content.block(tile.floor));
                    (
                        floor.map(|def| def.is_liquid).unwrap_or(false),
                        floor
                            .and_then(|def| def.map_color)
                            .unwrap_or(mind_core::content::Rgba::WHITE),
                    )
                };
                let fin = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::TimedKillComp>(*entity)
                    .map(|timed| {
                        if timed.lifetime > 0.0 {
                            (timed.time / timed.lifetime).clamp(0.0, 1.0)
                        } else {
                            0.0
                        }
                    })
                    .unwrap_or(0.0);
                Some(UnitDraw {
                    x: pos.x,
                    y: pos.y,
                    rotation: core.rotation,
                    name: def.name.clone(),
                    flying: def.flying,
                    low_altitude: def.low_altitude,
                    team,
                    healthf,
                    elevation: core.elevation,
                    hit_size: def.hit_size,
                    mech,
                    legs,
                    treads,
                    crawl,
                    payload,
                    weapons,
                    parts: def.parts.clone(),
                    fin,
                    draw_body: def.draw_body,
                    draw_cell: def.draw_cell,
                    ground_layer: def.ground_layer,
                    id,
                    mine_tile,
                    item,
                    is_player,
                    health,
                    statuses,
                    grounded: core.elevation < 0.001,
                    hovering: def.hovering,
                    emit_walk_effect: def.emit_walk_effect,
                    floor_is_liquid,
                    floor_color,
                })
            })
            .collect()
    }

    /// Command-overlay rows for the selected unit ids
    /// (`InputHandler.drawCommanded`; T16/PAR-0157):
    /// `(x, y, hit_size, flying, target_pos)` where `target_pos` is
    /// `CommandAiState.target_pos` (`None` without a move command). Rust-only
    /// seam for the overlay pass.
    pub fn unit_command_view(&self, ids: &[i32]) -> Vec<UnitCommandRow> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let wanted: std::collections::HashSet<i32> = ids.iter().copied().collect();
        let mut out = Vec::new();
        for &entity in &runtime.units {
            let Ok(entity_ref) = self.sim.ecs.0.get_entity(entity) else {
                continue;
            };
            let Some(seq) = entity_ref.get::<mind_core::ecs::EntitySeq>() else {
                continue;
            };
            if !wanted.contains(&(seq.0 as i32)) {
                continue;
            }
            let (Some(pos), Some(type_comp)) =
                (entity_ref.get::<Pos>(), entity_ref.get::<UnitTypeComp>())
            else {
                continue;
            };
            let Some(def) = content.unit(type_comp.type_id) else {
                continue;
            };
            let target = self
                .sim
                .unit_command_state(seq.0 as i32)
                .and_then(|state| state.target_pos);
            out.push((pos.x, pos.y, def.hit_size, def.flying, target));
        }
        out
    }

    /// Force-projector domes: `(x, y, real radius, sides, rotation degrees,
    /// team, hit)` for every unbroken projector (`ForceProjector.drawShield`,
    /// `ForceProjector.java:308-330`). Rust-only seam for the shield pass
    /// (T13/PAR-0129).
    pub fn force_shields(&self) -> Vec<(f32, f32, f32, u8, f32, u8, f32)> {
        use mind_core::world::blocks::defense::shields::ForceProjectorState;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(state) = entity.get::<ForceProjectorState>() else {
                continue;
            };
            if state.broken {
                continue;
            }
            let Some(building) = entity.get::<mind_core::entities::comp::Building>() else {
                continue;
            };
            let Some(comp) = entity.get::<mind_core::ecs::BuildingComp>() else {
                continue;
            };
            let Some(def) = content.block(building.block) else {
                continue;
            };
            // `realRadius()` (the state method mirrors the Java formula).
            let radius = state.real_radius();
            if radius <= 0.001 {
                continue;
            }
            out.push((
                f32::from(building.tile.x()) * 8.0 + def.offset,
                f32::from(building.tile.y()) * 8.0 + def.offset,
                radius,
                state.sides.clamp(3, 300) as u8,
                state.shield_rotation,
                comp.team.0,
                state.hit,
            ));
        }
        out
    }

    /// Unit shield bubbles: `(x, y, radius, alpha, team, hit)` for every unit
    /// with a `ShieldComp` (`UnitType.drawShield`: radius `hitSize * 1.3`,
    /// colour `shieldColor.lerp(white, clamp(hitTime/2)).a(0.7 * shieldAlpha)`;
    /// the port has no `hitTime`, so hit is 0). Rust-only seam (T13).
    pub fn unit_shields(&self) -> Vec<(f32, f32, f32, f32, u8, f32)> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{ShieldComp, UnitTypeComp};
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(shield) = entity.get::<ShieldComp>() else {
                continue;
            };
            // `UnitType.draw`: `if(unit.shieldAlpha > 0) drawShield(unit)`
            // (`UnitType.java:1555-1556`; H03 T15). Transient per-hit flash.
            if shield.shield_alpha <= 0.0 {
                continue;
            }
            let Some(pos) = entity.get::<Pos>() else {
                continue;
            };
            let Some(type_comp) = entity.get::<UnitTypeComp>() else {
                continue;
            };
            let Some(def) = content.unit(type_comp.type_id) else {
                continue;
            };
            let team = entity
                .get::<mind_core::entities::comp::TeamComp>()
                .map(|comp| comp.team)
                .unwrap_or(0);
            out.push((
                pos.x,
                pos.y,
                def.hit_size * 1.3,
                shield.shield_alpha,
                team,
                // `UnitType.drawShield` lerps the rim toward white by
                // `clamp(hitTime / 2)`: `hitTime` is view-only state owned by
                // the unit renderer, so the port passes 0 (deviation).
                0.0,
            ));
        }
        out
    }

    /// Live unit abilities with a `draw(Unit)` override (L04 T2): ForceField,
    /// EnergyField, SuppressionField, ShieldArc, ArmorPlate, UnitSpawn and
    /// MoveLightning. The shield pass owns ForceField/ShieldArc, the field pass
    /// owns EnergyField/SuppressionField/ArmorPlate and the unit pass owns
    /// MoveLightning/UnitSpawn (their Java z is the unit's own layer).
    pub fn unit_ability_draws(&self) -> Vec<UnitAbilityDraw> {
        use mind_core::combat::abilities::AbilityComp;
        use mind_core::content::registries::units::ability::AbilityKind;
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{ShieldComp, UnitCore, UnitTypeComp};
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(comp) = entity.get::<AbilityComp>() else {
                continue;
            };
            let Some(pos) = entity.get::<Pos>() else {
                continue;
            };
            let Some(rotation) = entity.get::<UnitCore>().map(|core| core.rotation) else {
                continue;
            };
            let Some(type_comp) = entity.get::<UnitTypeComp>() else {
                continue;
            };
            let Some(def) = content.unit(type_comp.type_id) else {
                continue;
            };
            let team = entity
                .get::<mind_core::entities::comp::TeamComp>()
                .map(|team| team.team)
                .unwrap_or(0);
            let shield = entity
                .get::<ShieldComp>()
                .map(|shield| shield.shield)
                .unwrap_or(0.0);
            let id = entity
                .get::<mind_core::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(0);
            // `MoveLightningAbility.draw` reads `unit.vel().len()` (`:101`).
            let vel_len = entity
                .get::<mind_core::entities::comp::Vel>()
                .map(|vel| vel.x.hypot(vel.y))
                .unwrap_or(0.0);
            for (spec, state) in def.abilities.iter().zip(&comp.states) {
                if matches!(
                    spec.kind,
                    AbilityKind::ForceField
                        | AbilityKind::EnergyField
                        | AbilityKind::SuppressionField
                        | AbilityKind::ShieldArc
                        | AbilityKind::ArmorPlate
                        | AbilityKind::MoveLightning
                        | AbilityKind::UnitSpawn
                ) {
                    // `UnitSpawnAbility.draw` only draws while
                    // `Units.canCreate(unit.team, this.unit)`
                    // (`UnitSpawnAbility.java:65`).
                    let can_create = if spec.kind == AbilityKind::UnitSpawn {
                        spec.unit
                            .and_then(|name| content.unit_by_name(name))
                            .is_some_and(|spawn| {
                                mind_core::entities::comp::unit::queries::can_create_unit(
                                    &self.sim.ecs.0,
                                    team,
                                    spawn,
                                )
                            })
                    } else {
                        false
                    };
                    out.push(UnitAbilityDraw {
                        kind: spec.kind,
                        x: pos.x,
                        y: pos.y,
                        rotation,
                        team,
                        spec: spec.clone(),
                        state: *state,
                        shield,
                        id,
                        name: def.name.clone(),
                        vel_len,
                        can_create,
                    });
                }
            }
        }
        out
    }

    /// F6 status markers (`BuildingComp.drawStatus`/`status()`): one row per
    /// player-team building with a non-empty `consumes` —
    /// `(center tile x, center tile y, world x, world y, size, java_color)`.
    /// Rust-only seam for the status pass (T14/PAR-0097).
    pub fn building_statuses(&self) -> Vec<(i32, i32, f32, f32, i32, u32)> {
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let team = self.player_team();
        // `BuildingComp.status()` reads `state.tick` (Vars.state): the sim tick
        // drives the efficiency pulse (`BuildingComp.java:651`; I02 T3).
        let tick = self.sim.tick_count();
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(building) = entity.get::<mind_core::entities::comp::Building>() else {
                continue;
            };
            let Some(comp) = entity.get::<mind_core::ecs::BuildingComp>() else {
                continue;
            };
            if comp.team.0 != team {
                continue;
            }
            let Some(def) = content.block(building.block) else {
                continue;
            };
            if def.consumes.is_empty() {
                continue;
            }
            // `BuildingComp.status()` (`BuildingComp.java:638-652`): logic
            // disable, `noOutput` when `shouldConsume()` is false, `noInput`
            // with no efficiency/production, else the 30-tick pulse
            // `((tick / 30) % 1) < efficiency` (I02 T3; H01 T8 transcribed the
            // per-family `shouldConsume()` overrides in
            // `mind_core::world::status::java_block_status`).
            let status =
                mind_core::world::status::java_block_status(&self.sim.ecs.0, entity.id(), tick);
            let size = def.size.max(1);
            // `Building.tile` is the center tile; `Block.offset` recentres the
            // even-size sprites (same convention as `blocks.rs`).
            let cx = f32::from(building.tile.x()) * 8.0 + def.offset;
            let cy = f32::from(building.tile.y()) * 8.0 + def.offset;
            out.push((
                i32::from(building.tile.x()),
                i32::from(building.tile.y()),
                cx,
                cy,
                size,
                status.java_color(),
            ));
        }
        out
    }

    // ---- H03 T9: unit factory / reconstructor build preview seam ----

    /// `UnitFactory.draw` / `Reconstructor.draw` construct-preview state at
    /// `(x, y)` (`Drawf.construct`, `UnitFactory.java:380-397`,
    /// `Reconstructor.java:287-317`; H03 T9). `None` for any other block kind.
    pub fn unit_build_draw(&self, x: i32, y: i32) -> Option<UnitBuildDraw> {
        use mind_core::content::BlockKind;
        use mind_core::entities::comp::Building;
        use mind_core::world::blocks::units::{ReconstructorBuild, UnitFactoryBuild};
        let tile = self.tile_at(x, y)?;
        let entity = tile.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(tile.block)?;
        let size = def.size.max(1);
        // `Building.x/y` = the footprint centre (`Tile.drawx + Block.offset`).
        let cx = (x + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let cy = (y + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let rotation = self.building_rotation(x, y);
        let efficiency = self
            .sim
            .ecs
            .0
            .get::<Building>(entity)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        match def.kind {
            BlockKind::UnitFactory => {
                let state = self.sim.ecs.0.get::<UnitFactoryBuild>(entity)?;
                let (unit, fraction) =
                    factory_preview(&def.unit_plans, state.current_plan, state.progress);
                Some(UnitBuildDraw {
                    reconstructor: false,
                    cx,
                    cy,
                    size,
                    rotation,
                    unit,
                    payload: None,
                    fraction,
                    // `UnitFactory.updateTile`: `speedScl` targets 1 while
                    // `efficiency > 0 && currentPlan != -1`.
                    working: efficiency > 0.0 && unit.is_some(),
                })
            }
            BlockKind::Reconstructor => {
                let state = self.sim.ecs.0.get::<ReconstructorBuild>(entity)?;
                let (unit, fraction, arrived) = reconstructor_preview(
                    &def.reconstructor_upgrades,
                    state.payload,
                    state.progress,
                    def.construct_time,
                    state.move_in,
                );
                Some(UnitBuildDraw {
                    reconstructor: true,
                    cx,
                    cy,
                    size,
                    rotation,
                    unit,
                    payload: state.payload,
                    fraction,
                    // `Reconstructor.updateTile`: `valid` = the payload arrived
                    // (`moveInPayload()`) with a valid upgrade and
                    // `efficiency > 0`.
                    working: efficiency > 0.0 && arrived && unit.is_some(),
                })
            }
            _ => None,
        }
    }
}
