// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EmpBulletType` behavior (`entities/bullet/EmpBulletType.java`).
//!
//! On hit, damages/drains every enemy in `radius` (buildings: `damage *
//! powerDamageScl`; units: `damage * unitDamageScl`) and applies the status.
//! Ally overdrive/heal behaviour depends on plan 09's power graph and plan 07's
//! overdrive hooks and is applied through those seams when present.

use bevy_ecs::entity::Entity;

use crate::content::StatusId;
use crate::ecs::EntitySeq;
use crate::entities::comp::{Building, Health, Pos, TeamComp};

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, hit_bullet};

/// `EmpBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmpBehavior;

impl BulletBehavior for EmpBehavior {
    fn hit(&self, ctx: &mut CombatCtx<'_>, b: Entity, x: f32, y: f32, create_frags: bool) {
        hit_bullet(ctx, b, x, y, create_frags);
        let Some(state) = ctx.bullet(b).cloned() else {
            return;
        };
        if state.has(super::super::ABSORBED) {
            return;
        }
        let Some(def) = ctx.content.bullet(state.def) else {
            return;
        };
        let radius = def.emp_radius;
        let power_scl = def.power_damage_scl;
        let unit_scl = def.unit_damage_scl;
        let hit_units = def.hit_units;
        let status = def.status;
        let status_duration = def.status_duration;
        let status_chance = def.status_chance;
        let base_damage = state.damage;
        let team = ctx.team(b);

        let radius2 = radius * radius;
        let mut targets: Vec<(u64, Entity, bool)> = Vec::new();
        for entity_ref in ctx.world.iter_entities() {
            let Some(pos) = entity_ref.get::<Pos>() else {
                continue;
            };
            if entity_ref.get::<Health>().is_none() {
                continue;
            }
            if entity_ref.get::<TeamComp>().is_some_and(|t| t.team == team) {
                continue;
            }
            let is_building = entity_ref.get::<Building>().is_some();
            if !is_building && !hit_units {
                continue;
            }
            let dx = pos.x - x;
            let dy = pos.y - y;
            if dx * dx + dy * dy > radius2 {
                continue;
            }
            let seq = entity_ref
                .get::<EntitySeq>()
                .map(|s| s.0)
                .unwrap_or(u64::MAX);
            targets.push((seq, entity_ref.id(), is_building));
        }
        targets.sort_by_key(|(seq, entity, _)| (*seq, entity.index()));

        for (_, entity, is_building) in targets {
            let damage = if is_building {
                base_damage * power_scl
            } else {
                base_damage * unit_scl
            };
            crate::combat::damage::area::apply_health(ctx.world, entity, damage);
            if status != StatusId::NONE {
                let applies = status_chance >= 1.0
                    || ctx
                        .rng
                        .chance(crate::determinism::RngStream::Sim, status_chance as f64);
                if applies {
                    crate::combat::damage::status::apply_status(
                        ctx.world,
                        entity,
                        status,
                        status_duration,
                    );
                }
            }
        }
    }
}

/// Static EMP-behavior instance.
pub static EMP: EmpBehavior = EmpBehavior;
