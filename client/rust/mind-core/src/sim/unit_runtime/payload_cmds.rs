// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Payload carrier commands: unit/build pickup and drop
//! (`InputHandler.requestUnitPayload`/`requestBuildPayload`/`requestDropPayload`).
//! Upstream: core/src/mindustry/input/InputHandler.java

use super::targeting::building_center;
use super::*;

impl Sim {
    /// `Payloadc` unit carrier command (`InputHandler.requestUnitPayload` /
    /// `requestBuildPayload` / `requestDropPayload`; PAR-0138).
    ///
    /// `kind`: `0` pick up a unit (`target` = its `EntitySeq`), `1` pick up the
    /// building at world `(x, y)`, `2` drop the last payload at `(x, y)`.
    /// Returns whether the carrier's payload stack changed.
    pub fn apply_payload_command(&mut self, kind: u8, x: f32, y: f32, target: Option<i32>) -> bool {
        let Some(carrier) = self.player_unit() else {
            return false;
        };
        // The drop sound needs the payload about to leave the stack
        // (`PayloadComp.playPayloadDropSound`), read before the drop.
        let dropped = if kind == 2 {
            self.ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                .and_then(|payloads| payloads.stack.last().copied())
        } else {
            None
        };
        let carrier_pos = self.ecs.0.get::<Pos>(carrier).map(|pos| (pos.x, pos.y));
        let changed = match kind {
            0 => self.payload_pickup_unit(carrier, x, y, target),
            1 => self.payload_pickup_build(carrier, x, y),
            2 => self.payload_drop(carrier, x, y),
            _ => false,
        };
        // G07 T14: `PayloadComp.pickup`/`playPayloadDropSound` sounds.
        if changed && let Some(audio) = self.ecs.0.get_resource::<AudioSinkRes>().cloned() {
            let (cx, cy) = carrier_pos.unwrap_or((x, y));
            let fx = self
                .rng_streams
                .next_float(crate::determinism::RngStream::Fx);
            let at = |sound: crate::audio::SoundId, pitch: f32| {
                audio.emit(crate::audio::AudioEvent::at(
                    sound, cx, cy, pitch, 1.0, true,
                ));
            };
            match kind {
                // `PayloadComp.pickup(Unit)` -> `payloadPickup.at(self, random(0.9, 1.1))`.
                0 => at(crate::audio::SoundId::PAYLOAD_PICKUP, 0.9 + fx * 0.2),
                // `PayloadComp.pickup(Building)` -> pitch 1 (`PayloadComp.java:117`).
                1 => at(crate::audio::SoundId::PAYLOAD_PICKUP, 1.0),
                2 => {
                    // Size: unit `hitSize`, build `block.size * tilesize`
                    // (`Payload.size()`). Deviation: the port cannot tell a
                    // ground placement from an acceptor hand-off, so the
                    // drop-size rule covers both.
                    let size = match dropped {
                        Some(crate::entities::comp::unit::comp::CarriedPayload::Unit {
                            type_id,
                            ..
                        }) => self
                            .unit_runtime()
                            .and_then(|runtime| runtime.content.unit(type_id))
                            .map(|def| def.hit_size)
                            .unwrap_or(0.0),
                        Some(crate::entities::comp::unit::comp::CarriedPayload::Build {
                            block,
                            ..
                        }) => self
                            .unit_runtime()
                            .and_then(|runtime| runtime.content.block(block))
                            .map(|def| def.size as f32 * 8.0)
                            .unwrap_or(0.0),
                        None => 0.0,
                    };
                    let sound = if size <= 12.0 {
                        crate::audio::SoundId::PAYLOAD_DROP1
                    } else if size <= 20.0 {
                        crate::audio::SoundId::PAYLOAD_DROP2
                    } else {
                        crate::audio::SoundId::PAYLOAD_DROP3
                    };
                    at(sound, 0.9 + fx * 0.2);
                }
                _ => {}
            }
        }
        changed
    }

    /// `InputHandler.requestUnitPayload` + `PayloadComp.canPickup(Unit)`.
    ///
    /// With no explicit `target`, resolves the candidate like
    /// `InputHandler.tryPickupPayload`: the closest unit to the cursor point
    /// within the carrier's reach (`carrier.hitSize * 2`).
    pub(crate) fn payload_pickup_unit(
        &mut self,
        carrier: Entity,
        x: f32,
        y: f32,
        target: Option<i32>,
    ) -> bool {
        let ecs = &mut self.ecs.0;
        let Some(mut runtime) = ecs.remove_non_send::<UnitRuntime>() else {
            return false;
        };
        let mut changed = false;
        let candidates: Vec<Entity> = if let Some(target_seq) = target {
            runtime
                .units
                .iter()
                .copied()
                .filter(|entity| {
                    ecs.get::<EntitySeq>(*entity)
                        .is_some_and(|seq| seq.0 as i32 == target_seq)
                })
                .collect()
        } else {
            let carrier_hit = ecs
                .get::<crate::entities::comp::unit::comp::HitboxComp>(carrier)
                .map(|hitbox| hitbox.hit_size)
                .unwrap_or(0.0);
            // `Units.closest(team, pay.x(), pay.y(), hitSize * 2f, ...)`: the
            // search is centered on the carrier, not the cursor
            // (`InputHandler.tryPickupPayload`, InputHandler.java:1043).
            let (cx, cy) = ecs
                .get::<Pos>(carrier)
                .map(|pos| (pos.x, pos.y))
                .unwrap_or((x, y));
            let mut near: Vec<(f32, Entity)> = runtime
                .units
                .iter()
                .copied()
                .filter(|entity| *entity != carrier)
                .filter_map(|entity| {
                    let pos = ecs.get::<Pos>(entity)?;
                    let (dx, dy) = (pos.x - cx, pos.y - cy);
                    let dist2 = dx * dx + dy * dy;
                    (dist2 <= (carrier_hit * 2.0).powi(2)).then_some((dist2, entity))
                })
                .collect();
            near.sort_by_key(|(dist2, entity)| (dist2.to_bits(), entity.index()));
            near.into_iter().map(|(_, entity)| entity).collect()
        };
        for target in candidates {
            if target == carrier {
                continue;
            }
            let team_of = |entity: Entity| ecs.get::<TeamComp>(entity).map(|t| t.team);
            let carrier_team = team_of(carrier).unwrap_or(0);
            let target_team = team_of(target).unwrap_or(0);
            let target_ai = ecs
                .get::<ControllerSlot>(target)
                .is_some_and(|slot| slot.kind != AiKind::Player);
            let target_grounded = ecs
                .get::<UnitCore>(target)
                .is_some_and(|core| core.elevation < 0.001);
            let (target_id, target_hit, target_allowed) = ecs
                .get::<UnitTypeComp>(target)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .map(|def| (def.id, def.hit_size, def.allowed_in_payloads))
                .unwrap_or((crate::content::UnitTypeId::new(0), f32::MAX, false));
            let carrier_hit = ecs
                .get::<crate::entities::comp::unit::comp::HitboxComp>(carrier)
                .map(|hitbox| hitbox.hit_size)
                .unwrap_or(0.0);
            let capacity = ecs
                .get::<crate::entities::comp::unit::comp::PayloadComp>(carrier)
                .map(|payload| payload.capacity)
                .unwrap_or(0.0);
            let used = ecs
                .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                .map(|payloads| payloads.payload_used(&runtime.content))
                .unwrap_or(0.0);
            let within = match (
                ecs.get::<Pos>(carrier).copied(),
                ecs.get::<Pos>(target).copied(),
            ) {
                (Some(carrier_pos), Some(target_pos)) => {
                    let (dx, dy) = (target_pos.x - carrier_pos.x, target_pos.y - carrier_pos.y);
                    // `u.within(unit, u.hitSize + unit.hitSize)`
                    // (InputHandler.java:1043).
                    (dx * dx + dy * dy).sqrt() <= carrier_hit + target_hit
                }
                _ => false,
            };
            // `PayloadComp.canPickup(Unit)`: `type.pickupUnits` gate.
            let pickup_units = ecs
                .get::<UnitTypeComp>(carrier)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| def.pickup_units);
            if !pickup_units
                || carrier_team != target_team
                || !target_ai
                || !target_grounded
                || !target_allowed
                || !within
                || used + target_hit * target_hit > capacity + 0.001
            {
                continue;
            }
            let health = ecs.get::<Health>(target).map(|h| h.health).unwrap_or(0.0);
            let rotation = ecs
                .get::<UnitCore>(target)
                .map(|c| c.rotation)
                .unwrap_or(0.0);
            runtime.units.retain(|entity| *entity != target);
            runtime.units_removed += 1;
            let _ = ecs.despawn(target);
            if let Some(mut payloads) =
                ecs.get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            {
                payloads
                    .stack
                    .push(crate::entities::comp::unit::comp::CarriedPayload::Unit {
                        type_id: target_id,
                        team: target_team,
                        health,
                        rotation,
                    });
                changed = true;
            }
            break;
        }
        ecs.insert_non_send(runtime);
        if changed {
            return true;
        }
        // `else { Building build = world.buildWorld(pay.x(), pay.y()); ... }`
        // (`InputHandler.java:1045-1052`): fall back to the building under the
        // carrier.
        let (cx, cy) = ecs
            .get::<Pos>(carrier)
            .map(|pos| (pos.x, pos.y))
            .unwrap_or((x, y));
        self.payload_pickup_build(carrier, cx, cy)
    }

    /// `InputHandler.requestBuildPayload` + `PayloadComp.canPickup(Building)`.
    pub(crate) fn payload_pickup_build(&mut self, carrier: Entity, x: f32, y: f32) -> bool {
        let tx = WorldGrid::to_tile(x) as i16;
        let ty = WorldGrid::to_tile(y) as i16;
        let Some(build) = self
            .grid
            .tiles
            .getn(tx as i32, ty as i32)
            .and_then(|tile| tile.build)
        else {
            return false;
        };
        let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(build) else {
            return false;
        };
        let (block, rotation) = (building.block, building.rotation);
        let build_team = self
            .ecs
            .0
            .get::<TeamComp>(build)
            .map(|team| team.team)
            .unwrap_or(0);
        let health = self
            .ecs
            .0
            .get::<Health>(build)
            .map(|h| h.health)
            .unwrap_or(0.0);

        let ecs = &mut self.ecs.0;
        let Some(runtime) = ecs.remove_non_send::<UnitRuntime>() else {
            return false;
        };
        let carrier_team = ecs.get::<TeamComp>(carrier).map(|t| t.team).unwrap_or(0);
        let carrier_pos = ecs.get::<Pos>(carrier).copied();
        let carrier_hit = ecs
            .get::<crate::entities::comp::unit::comp::HitboxComp>(carrier)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let capacity = ecs
            .get::<crate::entities::comp::unit::comp::PayloadComp>(carrier)
            .map(|payload| payload.capacity)
            .unwrap_or(0.0);
        let used = ecs
            .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            .map(|payloads| payloads.payload_used(&runtime.content))
            .unwrap_or(0.0);
        let (size, pickable) = runtime
            .content
            .block(block)
            .map(|def| {
                (
                    def.size as f32 * crate::config::TILESIZE as f32,
                    crate::world::build::block_can_pickup(def),
                )
            })
            .unwrap_or((0.0, false));
        let tile_size = crate::config::TILESIZE as f32;
        let within = match (carrier_pos, building_center(ecs, build)) {
            (Some(carrier_pos), Some((bx, by))) => {
                let (dx, dy) = (bx - carrier_pos.x, by - carrier_pos.y);
                (dx * dx + dy * dy).sqrt() <= size * 1.2 + tile_size * 5.0
            }
            _ => false,
        };
        let mut changed = false;
        if build_team == carrier_team
            && pickable
            && within
            && used + size * size <= capacity + 0.001
            && let Some(rt) = self.block_runtime.as_mut()
            && rt
                .pickup_building(&mut self.ecs.0, &mut self.grid, build)
                .is_some()
        {
            if let Some(mut payloads) = self
                .ecs
                .0
                .get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            {
                payloads
                    .stack
                    .push(crate::entities::comp::unit::comp::CarriedPayload::Build {
                        block,
                        team: carrier_team,
                        rotation,
                        health,
                    });
                changed = true;
            }
            let _ = carrier_hit;
        }
        self.ecs.0.insert_non_send(runtime);
        if changed {
            // The grid snapshot dropped the building.
            self.refresh_unit_runtime();
        }
        changed
    }

    /// `InputHandler.requestDropPayload` + `PayloadComp.dropLastPayload`.
    pub(crate) fn payload_drop(&mut self, carrier: Entity, x: f32, y: f32) -> bool {
        // `InputHandler.tryDropPayload` -> `requestDropPayload(player,
        // player.x, player.y)`: the payload lands at the carrier, never the
        // cursor (`InputHandler.java:1058-1063`).
        let _ = (x, y);
        let Some(carrier_pos) = self.ecs.0.get::<Pos>(carrier).copied() else {
            return false;
        };
        let (cx, cy) = (carrier_pos.x, carrier_pos.y);
        let Some(payload) = self
            .ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            .and_then(|payloads| payloads.stack.last().copied())
        else {
            return false;
        };
        match payload {
            crate::entities::comp::unit::comp::CarriedPayload::Unit {
                type_id,
                team,
                health,
                rotation,
            } => {
                let ecs = &mut self.ecs.0;
                let Some(mut runtime) = ecs.remove_non_send::<UnitRuntime>() else {
                    return false;
                };
                let Some(def) = runtime.content.unit(type_id) else {
                    ecs.insert_non_send(runtime);
                    return false;
                };
                let seq = runtime.seq;
                runtime.seq = seq.wrapping_add(1);
                let entity = crate::entities::comp::unit::lifecycle::spawn_unit_def(
                    ecs, seq, def, team, cx, cy, rotation,
                );
                if let Some(mut stored) = ecs.get_mut::<Health>(entity) {
                    stored.health = health;
                }
                runtime.units.push(entity);
                runtime.units_created += 1;
                if let Some(mut payloads) =
                    ecs.get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                {
                    let _ = payloads.stack.pop();
                }
                ecs.insert_non_send(runtime);
                true
            }
            crate::entities::comp::unit::comp::CarriedPayload::Build {
                block,
                team,
                rotation,
                health,
            } => {
                let size = self
                    .block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(block))
                    .map(|def| def.size)
                    .unwrap_or(1) as i16;
                let offset = (size - 1) / 2;
                let tx = WorldGrid::to_tile(cx) as i16 - offset;
                let ty = WorldGrid::to_tile(cy) as i16 - offset;
                // `tryDropPayload` -> `dropBlock`: keep carrying it when the
                // placement is invalid (Java returns false and the stack is
                // not popped).
                if !self.runtime_place(
                    tx,
                    ty,
                    block,
                    rotation,
                    team,
                    crate::world::build::PlaceFlags::PLAYER,
                ) {
                    return false;
                }
                if let Some(build) = self
                    .grid
                    .tiles
                    .getn(tx as i32 + offset as i32, ty as i32 + offset as i32)
                    .and_then(|tile| tile.build)
                    && let Some(mut stored) = self.ecs.0.get_mut::<Health>(build)
                {
                    stored.health = health;
                }
                if let Some(mut payloads) =
                    self.ecs
                        .0
                        .get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                {
                    let _ = payloads.stack.pop();
                }
                true
            }
        }
    }
}
