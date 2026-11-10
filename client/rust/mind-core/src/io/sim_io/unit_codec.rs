// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit wire fields and AI/build-plan wire helpers (`TypeIO` unit fields).
//! Upstream: core/src/mindustry/io/TypeIO.java (unit fields), core/src/mindustry/io/SaveVersion.java

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::game::teams::{BlockPlan, Teams};
use crate::io::save::state::TeamPlan;
use crate::io::wire::{WireReader, WireWriter};
use crate::io::{IoError, IoResult};
use crate::sim::Sim;
use crate::sim::unit_runtime::UnitRuntime;
use crate::world::config::{config_to_type_value, type_value_to_config};

use super::DecodedUnit;

/// Writes the shared unit record body: `team, x, y, rotation, health` plus the
/// revision 2-4 extension fields.
///
/// Shared by the entity-region chunk writer
/// (`SaveVersion.writeWorldEntities`) and the nested unit-payload body
/// (`UnitPayload.write`/`Payload.write`), so both stay in lockstep as the
/// revision grows. The caller writes the class/tag and revision header.
pub(crate) fn write_unit_fields(
    world: &World,
    entity: Entity,
    chunk: &mut WireWriter,
) -> IoResult<()> {
    let (Some(core), Some(pos)) = (world.get::<UnitCore>(entity), world.get::<Pos>(entity)) else {
        return Ok(());
    };
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    let health = world.get::<Health>(entity).map(|h| h.health).unwrap_or(0.0);
    chunk.ub(team);
    chunk.f(pos.x);
    chunk.f(pos.y);
    chunk.f(core.rotation);
    chunk.f(health);
    // Revision 2: carried payload stack (`UnitPayloads.stack`).
    let payloads = world
        .get::<crate::entities::comp::unit::comp::UnitPayloads>(entity)
        .map(|payloads| payloads.stack.as_slice())
        .unwrap_or(&[]);
    chunk.us(payloads.len().min(u16::MAX as usize) as u16);
    for payload in payloads {
        match payload {
            crate::entities::comp::unit::comp::CarriedPayload::Unit {
                type_id,
                team,
                health,
                rotation,
            } => {
                chunk.ub(0);
                chunk.us(type_id.raw());
                chunk.ub(*team);
                chunk.f(*health);
                chunk.f(*rotation);
            }
            crate::entities::comp::unit::comp::CarriedPayload::Build {
                block,
                team,
                rotation,
                health,
            } => {
                chunk.ub(1);
                chunk.us(block.raw());
                chunk.ub(*team);
                chunk.ub(*rotation);
                chunk.f(*health);
            }
        }
    }
    // Revision 3 (H04 T9): unit-save completeness.
    chunk.bool(core.spawned_by_core);
    chunk.f(core.elevation);
    let shield = world
        .get::<crate::entities::comp::unit::comp::ShieldComp>(entity)
        .map(|shield| shield.shield)
        .unwrap_or(0.0);
    chunk.f(shield);
    let stack = world
        .get::<crate::entities::comp::unit::comp::ItemsComp>(entity)
        .and_then(|items| items.item);
    chunk.s(stack.map(|(item, _)| item.raw() as i16).unwrap_or(-1));
    chunk.i(stack.map(|(_, amount)| amount).unwrap_or(0));
    let statuses = world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .map(|status| status.statuses.clone())
        .unwrap_or_default();
    chunk.i(statuses.len().min(i32::MAX as usize) as i32);
    for entry in &statuses {
        chunk.s(entry.effect.raw() as i16);
        chunk.f(entry.duration);
    }
    let command = world
        .get::<crate::ai::controller::UnitCommandComp>(entity)
        .map(|command| command.0 as i16)
        .unwrap_or(-1);
    chunk.s(command);
    // Controller kind (`UnitComp.afterRead` -> `createController`;
    // T13 keeps Logic/Command state and resets the rest).
    let controller = world
        .get::<crate::ai::controller::ControllerSlot>(entity)
        .map(|slot| ai_kind_code(slot.kind))
        .unwrap_or(255);
    chunk.ub(controller);
    let plans: Vec<crate::io::typeio::codecs::BuildPlan> = world
        .get::<crate::sim::builder::BuilderPlans>(entity)
        .map(|builder| builder.plans.iter().map(unit_plan_to_wire).collect())
        .unwrap_or_default();
    chunk.i(plans.len().min(i32::MAX as usize) as i32);
    for plan in &plans {
        crate::io::typeio::codecs::write_plan(chunk, plan)?;
    }
    // Revision 4 (L06 T4): `TypeIO.writeAbilities`
    // (TypeIO.java:446-451) - `Ability.data` per ability.
    let abilities: Vec<f32> = world
        .get::<crate::combat::abilities::AbilityComp>(entity)
        .map(|comp| comp.states.iter().map(|state| state.data).collect())
        .unwrap_or_default();
    chunk.ub(abilities.len().min(u8::MAX as usize) as u8);
    for data in abilities.iter().take(u8::MAX as usize) {
        chunk.f(*data);
    }
    // `UnitTetherComp.spawnerUnitId` (UnitTetherComp.java:17,
    // kept as `spawner.id` by `update`, :34-40): the spawner's id.
    let spawner = world
        .get::<crate::entities::comp::unit::comp::UnitTetherComp>(entity)
        .and_then(|tether| tether.spawner)
        .and_then(|spawner| world.get::<crate::ecs::EntitySeq>(spawner))
        .map_or(-1, |seq| seq.0 as i32);
    chunk.i(spawner);
    Ok(())
}

/// Reads the shared unit record body written by [`write_unit_fields`]
/// (`team, x, y, rotation, health` + the revision 2-4 extensions).
///
/// `revision` is the chunk payload revision (`SIM_UNIT_REVISION`); the caller
/// has already consumed the class/tag header and, for the entity region, the
/// content name.
pub(crate) fn read_unit_fields(r: &mut WireReader, revision: u16) -> IoResult<DecodedUnit> {
    let mut decoded = DecodedUnit {
        revision,
        saved_id: -1,
        name: String::new(),
        team: r.ub()?,
        x: r.f()?,
        y: r.f()?,
        rotation: r.f()?,
        health: r.f()?,
        payloads: Vec::new(),
        spawned_by_core: false,
        elevation: 0.0,
        shield: 0.0,
        stack: None,
        statuses: Vec::new(),
        command: None,
        controller: None,
        plans: Vec::new(),
        ability_data: Vec::new(),
        spawner_unit_id: -1,
    };
    if revision >= 2 {
        let count = r.us()?;
        for _ in 0..count {
            let tag = r.ub()?;
            let payload = match tag {
                0 => crate::entities::comp::unit::comp::CarriedPayload::Unit {
                    type_id: crate::content::UnitTypeId::new(r.us()?),
                    team: r.ub()?,
                    health: r.f()?,
                    rotation: r.f()?,
                },
                1 => crate::entities::comp::unit::comp::CarriedPayload::Build {
                    block: crate::content::BlockId::new(r.us()?),
                    team: r.ub()?,
                    rotation: r.ub()?,
                    health: r.f()?,
                },
                other => {
                    return Err(IoError::corrupt(format!(
                        "unknown unit payload tag {other}"
                    )));
                }
            };
            decoded.payloads.push(payload);
        }
    }
    if revision >= 3 {
        // H04 T9: unit-save completeness (pulled from
        // `SaveVersion.java:425-448` + the unit manifests).
        decoded.spawned_by_core = r.bool()?;
        decoded.elevation = r.f()?;
        decoded.shield = r.f()?;
        let stack_item = r.s()?;
        let stack_amount = r.i()?;
        if stack_item >= 0 {
            decoded.stack = Some((crate::content::ItemId::new(stack_item as u16), stack_amount));
        }
        let status_count = r.i()?.clamp(0, 1024) as usize;
        for _ in 0..status_count {
            let effect = r.s()?;
            let duration = r.f()?;
            if effect >= 0 {
                decoded
                    .statuses
                    .push(crate::entities::comp::unit::comp::StatusEntry::new(
                        crate::content::StatusId::new(effect as u16),
                        duration,
                    ));
            }
        }
        let command = r.s()?;
        decoded.command = (command >= 0).then_some(command as u16);
        decoded.controller = ai_kind_from_code(r.ub()?);
        let plan_count = r.i()?.clamp(0, 1024) as usize;
        for _ in 0..plan_count {
            let plan = crate::io::typeio::codecs::read_plan(r)?;
            decoded.plans.push(unit_plan_from_wire(plan));
        }
    }
    if revision >= 4 {
        // `TypeIO.readAbilities` (TypeIO.java:429-444): `b len`
        // then one `f data` each.
        let len = r.ub()? as usize;
        for _ in 0..len {
            decoded.ability_data.push(r.f()?);
        }
        decoded.spawner_unit_id = r.i()?;
    }
    Ok(decoded)
}

/// Stable save code for [`crate::ai::controller::AiKind`] (revision 3;
/// append-only).
pub(super) fn ai_kind_code(kind: crate::ai::controller::AiKind) -> u8 {
    use crate::ai::controller::AiKind;
    match kind {
        AiKind::NoAi => 0,
        AiKind::Ground => 1,
        AiKind::Hug => 2,
        AiKind::Flying => 3,
        AiKind::FlyingFollow => 4,
        AiKind::Command => 5,
        AiKind::Logic => 6,
        AiKind::Missile => 7,
        AiKind::Builder => 8,
        AiKind::Miner => 9,
        AiKind::Cargo => 10,
        AiKind::Defender => 11,
        AiKind::Suicide => 12,
        AiKind::Boost => 13,
        AiKind::Assembler => 14,
        AiKind::Prebuild => 15,
        AiKind::Repair => 16,
        AiKind::Player => 17,
    }
}

pub(super) fn ai_kind_from_code(code: u8) -> Option<crate::ai::controller::AiKind> {
    use crate::ai::controller::AiKind;
    Some(match code {
        0 => AiKind::NoAi,
        1 => AiKind::Ground,
        2 => AiKind::Hug,
        3 => AiKind::Flying,
        4 => AiKind::FlyingFollow,
        5 => AiKind::Command,
        6 => AiKind::Logic,
        7 => AiKind::Missile,
        8 => AiKind::Builder,
        9 => AiKind::Miner,
        10 => AiKind::Cargo,
        11 => AiKind::Defender,
        12 => AiKind::Suicide,
        13 => AiKind::Boost,
        14 => AiKind::Assembler,
        15 => AiKind::Prebuild,
        16 => AiKind::Repair,
        17 => AiKind::Player,
        _ => return None,
    })
}

/// Wire plan -> builder plan (`TypeIO.readPlan`).
pub(super) fn unit_plan_from_wire(
    plan: crate::io::typeio::codecs::BuildPlan,
) -> crate::sim::builder::UnitBuildPlan {
    crate::sim::builder::UnitBuildPlan {
        x: plan.x as i16,
        y: plan.y as i16,
        rotation: plan.rotation as u8,
        block: plan.block,
        config: type_value_to_config(&plan.config),
        breaking: plan.breaking,
        initialized: false,
        stuck: false,
        progress: 0.0,
    }
}

/// Builder plan -> wire plan (`TypeIO.writePlan`).
pub(super) fn unit_plan_to_wire(
    plan: &crate::sim::builder::UnitBuildPlan,
) -> crate::io::typeio::codecs::BuildPlan {
    crate::io::typeio::codecs::BuildPlan {
        x: i32::from(plan.x),
        y: i32::from(plan.y),
        rotation: i32::from(plan.rotation),
        block: plan.block,
        config: config_to_type_value(&plan.config),
        breaking: plan.breaking,
    }
}

/// Units `write_entities` will actually write, in `(seq, entity, name)` order
/// (live entity + `UnitCore` + `UnitTypeComp` + `Pos` + a registered name;
/// H04 T9 `SaveVersion.writeWorldEntities` count).
pub(super) fn live_unit_rows(sim: &Sim) -> Vec<(u64, Entity, String)> {
    let world = &sim.ecs.0;
    let Some(runtime) = world.get_non_send::<UnitRuntime>() else {
        return Vec::new();
    };
    let mut rows: Vec<(u64, Entity, String)> = runtime
        .units
        .iter()
        .copied()
        .filter(|entity| world.get_entity(*entity).is_ok())
        .filter_map(|entity| {
            let unit = world.get::<UnitTypeComp>(entity)?;
            if world.get::<UnitCore>(entity).is_none() || world.get::<Pos>(entity).is_none() {
                return None;
            }
            let name = runtime
                .content
                .unit(unit.type_id)
                .map(|def| def.name.clone())?;
            let seq = world
                .get::<EntitySeq>(entity)
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            Some((seq, entity, name))
        })
        .collect();
    rows.sort_by_key(|(seq, entity, _)| (*seq, entity.index()));
    rows
}

/// Restores decoded team build plans onto the live session (`readTeamBlocks`).
pub fn apply_team_plans(teams: &mut Teams, plans: Vec<(i32, Vec<TeamPlan>)>) {
    for (team, team_plans) in plans {
        let Ok(team) = u8::try_from(team) else {
            continue;
        };
        let data = teams.get(crate::ecs::TeamId(team));
        data.plans.clear();
        for plan in team_plans {
            data.plans.push_back(BlockPlan {
                x: plan.x,
                y: plan.y,
                rotation: plan.rotation as i8,
                block: plan.block,
                config: Some(type_value_to_config(&plan.config)),
                removed: false,
            });
        }
    }
}
