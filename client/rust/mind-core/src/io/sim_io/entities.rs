// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live entity sink/source for the `IoSet::Capture`/`Apply` boundary.
//! Upstream: core/src/mindustry/io/SaveVersion.java (world entities), core/src/mindustry/io/TypeIO.java

use crate::content::ContentRegistry;
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::UnitCore;
use crate::game::runtime::CampaignRuntime;
use crate::io::save::chunk::SaveScratch;
use crate::io::save::state::{EntitySink, EntitySource, TeamPlan};
use crate::io::wire::{WireReader, WireWriter};
use crate::io::{IoError, IoResult};
use crate::sim::Sim;
use crate::sim::unit_runtime::UnitRuntime;
use crate::world::config::{ConfigValue, config_to_type_value};

use super::unit_codec::{live_unit_rows, read_unit_fields, write_unit_fields};
use super::{
    DecodedFire, DecodedPuddle, DecodedUnit, SIM_FIRE_CLASS_ID, SIM_FIRE_REVISION,
    SIM_PUDDLE_CLASS_ID, SIM_PUDDLE_REVISION, SIM_UNIT_CLASS_ID, SIM_UNIT_REVISION,
};

/// Read-side entity sink collecting the native live-unit/fire/puddle chunks.
#[derive(Default)]
pub(super) struct LiveEntitySink {
    pub(super) units: Vec<DecodedUnit>,
    pub(super) fires: Vec<DecodedFire>,
    pub(super) puddles: Vec<DecodedPuddle>,
}

impl EntitySink for LiveEntitySink {
    fn supports_class(&self, class_id: u8, custom_name: Option<&str>) -> bool {
        if matches!(
            class_id,
            SIM_UNIT_CLASS_ID | SIM_FIRE_CLASS_ID | SIM_PUDDLE_CLASS_ID
        ) {
            return true;
        }
        // H04 T10: upstream (Java `MSAV`) classes. A custom id-map name means
        // a mod entity reusing the byte; only the vanilla classes decode.
        custom_name.is_none() && crate::io::java_entities::supports_java_class(class_id)
    }

    fn read_entity(
        &mut self,
        class_id: u8,
        _custom_name: Option<&str>,
        id: i32,
        content: Option<&ContentRegistry>,
        r: &mut WireReader,
    ) -> IoResult<()> {
        match class_id {
            SIM_UNIT_CLASS_ID => {
                let revision = r.us()?;
                if revision == 0 || revision > SIM_UNIT_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live unit revision {revision}"
                    )));
                }
                let name = r.str()?;
                let mut decoded = read_unit_fields(r, revision)?;
                decoded.saved_id = id;
                decoded.name = name;
                self.units.push(decoded);
            }
            SIM_FIRE_CLASS_ID => {
                let revision = r.us()?;
                if revision != SIM_FIRE_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live fire revision {revision}"
                    )));
                }
                let mut state = crate::combat::fires::FireState {
                    tile: (0, 0),
                    time: 0.0,
                    lifetime: 0.0,
                    puddle_flammability: 0.0,
                    spread_timer: 0.0,
                    fireball_timer: 0.0,
                    damage_timer: 0.0,
                    warmup: 0.0,
                    animation: 0.0,
                };
                crate::combat::fires::read_fire(&mut state, r, revision as u8)?;
                self.fires.push(DecodedFire {
                    tile: state.tile,
                    time: state.time,
                    lifetime: state.lifetime,
                });
            }
            SIM_PUDDLE_CLASS_ID => {
                let revision = r.us()?;
                if revision != SIM_PUDDLE_REVISION {
                    return Err(IoError::corrupt(format!(
                        "unsupported live puddle revision {revision}"
                    )));
                }
                let mut state = crate::combat::puddles::PuddleState {
                    tile: (0, 0),
                    liquid: crate::content::LiquidId::WATER,
                    amount: 0.0,
                    accepting: 0.0,
                    update_time: 0.0,
                    last_ripple: 0.0,
                    effect_time: 0.0,
                };
                crate::combat::puddles::read_puddle(&mut state, r, revision as u8)?;
                self.puddles.push(DecodedPuddle {
                    tile: state.tile,
                    liquid: state.liquid,
                    amount: state.amount,
                });
            }
            // H04 T10: upstream `FireComp`/`PuddleComp`/unit chunks
            // (`supports_class` only routes the decodable Java classes here).
            crate::io::java_entities::JAVA_FIRE_CLASS_ID => {
                if let Some(fire) = crate::io::java_entities::read_java_fire(r)? {
                    self.fires.push(fire);
                }
            }
            crate::io::java_entities::JAVA_PUDDLE_CLASS_ID => {
                let Some(content) = content else {
                    return Ok(());
                };
                if let Some(puddle) = crate::io::java_entities::read_java_puddle(r, content)? {
                    self.puddles.push(puddle);
                }
            }
            other => {
                let Some(content) = content else {
                    return Ok(());
                };
                if let Some(unit) = crate::io::java_entities::read_java_unit(other, id, r, content)?
                {
                    self.units.push(unit);
                }
            }
        }
        Ok(())
    }

    fn after_read_all(&mut self) {}
}

/// Write-side entity source over the live `UnitRuntime` and campaign teams.
pub(super) struct LiveEntitySource<'a> {
    pub(super) sim: &'a Sim,
}

impl EntitySource for LiveEntitySource<'_> {
    fn team_plans(&self) -> Vec<(i32, Vec<TeamPlan>)> {
        let Some(runtime) = self.sim.ecs.0.get_resource::<CampaignRuntime>() else {
            return Vec::new();
        };
        let teams = &runtime.session.teams;
        teams
            .active
            .iter()
            .filter_map(|team| {
                let data = teams.get_or_null(*team)?;
                if data.plans.is_empty() {
                    return None;
                }
                let plans = data
                    .plans
                    .iter()
                    .map(|plan| TeamPlan {
                        x: plan.x,
                        y: plan.y,
                        rotation: i16::from(plan.rotation),
                        block: plan.block,
                        config: config_to_type_value(
                            plan.config.as_ref().unwrap_or(&ConfigValue::None),
                        ),
                    })
                    .collect();
                Some((i32::from(team.0), plans))
            })
            .collect()
    }

    fn entity_count(&self) -> usize {
        // H04 T9: the count must equal the chunks `write_entities` emits
        // (`SaveVersion.java:425-439`); the old `unit_count` counted units the
        // writer skips, so the reader ate fire/puddle bytes as unit chunks.
        let units = live_unit_rows(self.sim).len();
        let mut fires = 0usize;
        let mut puddles = 0usize;
        for entity in self.sim.ecs.0.iter_entities() {
            if entity.get::<crate::combat::fires::FireState>().is_some() {
                fires += 1;
            }
            if entity
                .get::<crate::combat::puddles::PuddleState>()
                .is_some()
            {
                puddles += 1;
            }
        }
        units + fires + puddles
    }

    /// Units `write_entities` will actually write, in `(seq, entity, name)`
    /// order (live entity + `UnitCore` + `UnitTypeComp` + `Pos` + a registered
    /// name; H04 T9 `SaveVersion.writeWorldEntities` count).
    fn write_entities(&self, w: &mut WireWriter, scratch: &mut SaveScratch) -> IoResult<()> {
        let world = &self.sim.ecs.0;
        if world.get_non_send::<UnitRuntime>().is_none() {
            return Ok(());
        }
        for (seq, entity, name) in live_unit_rows(self.sim) {
            // The row helper already filters these; keep the guard explicit for
            // the chunk header fields the body writer reads.
            if world.get::<UnitCore>(entity).is_none() || world.get::<Pos>(entity).is_none() {
                continue;
            }
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_UNIT_CLASS_ID);
                // `SaveVersion.writeEntity`: `out.i(entity.id())` — the entity
                // id is part of the chunk header and restored on load (I01 T10).
                chunk.i(seq as i32);
                chunk.us(SIM_UNIT_REVISION);
                chunk.str(&name)?;
                write_unit_fields(world, entity, chunk)
            })?;
        }
        // Fires (`FireComp.write`: tile/time/lifetime) then puddles
        // (`PuddleComp.write`: amount/tile/liquid), both in tile order (they
        // carry no `EntitySeq`).
        let mut fires: Vec<crate::combat::fires::FireState> = world
            .iter_entities()
            .filter_map(|entity| entity.get::<crate::combat::fires::FireState>().copied())
            .collect();
        fires.sort_by_key(|state| (state.tile.0, state.tile.1));
        for state in fires {
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_FIRE_CLASS_ID);
                chunk.i(0);
                chunk.us(SIM_FIRE_REVISION);
                crate::combat::fires::write_fire(&state, chunk)?;
                Ok(())
            })?;
        }
        let mut puddles: Vec<crate::combat::puddles::PuddleState> = world
            .iter_entities()
            .filter_map(|entity| entity.get::<crate::combat::puddles::PuddleState>().copied())
            .collect();
        puddles.sort_by_key(|state| (state.tile.0, state.tile.1));
        for state in puddles {
            scratch.write_chunk(w, |chunk| {
                chunk.ub(SIM_PUDDLE_CLASS_ID);
                chunk.i(0);
                chunk.us(SIM_PUDDLE_REVISION);
                crate::combat::puddles::write_puddle(&state, chunk)?;
                Ok(())
            })?;
        }
        Ok(())
    }
}
