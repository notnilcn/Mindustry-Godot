// SPDX-License-Identifier: GPL-3.0-only

//! `Sim::apply`: the canonical `Command` application path.
//!
//! Upstream: core/src/mindustry/core/Logic.java.

use crate::sim::*;

impl Sim {
    /// Applies one command to the simulation. Commands are applied *before* the
    /// tick they are stamped with executes (`ScenarioPlayer` enforces the stamping).
    ///
    /// Invalid placements/breaks (occupied tile / empty tile) are deterministic
    /// no-ops, logged at debug level; out-of-bounds and unknown blocks are errors.
    pub fn apply(&mut self, command: Command) -> Result<(), SimError> {
        match command {
            Command::SelectBlock { block } => {
                self.content.name(block).map_err(SimError::from)?;
                self.selected_block = block;
            }
            Command::Place { x, y, block } => {
                self.content.name(block).map_err(SimError::from)?;
                if block == BlockId::AIR {
                    return Err(SimError::CannotPlaceAir);
                }
                let pos = TilePos::new(x, y);
                self.grid.index(pos)?;
                if self.block_runtime.is_some() {
                    // Java: a placement carries the player's team
                    // (`state.rules.defaultTeam`); prefer the live campaign
                    // session, then the build guard resource.
                    let team = self
                        .ecs
                        .0
                        .get_resource::<crate::game::runtime::CampaignRuntime>()
                        .map(|runtime| runtime.session.rules.default_team)
                        .or_else(|| {
                            self.ecs
                                .0
                                .get_resource::<crate::world::limits::BuildRules>()
                                .map(|rules| rules.default_team)
                        })
                        .unwrap_or(TeamId::SHARDED.0);
                    // `Control.input` dev placement uses the API form
                    // (`Control.java:505,521`).
                    if self.runtime_place(
                        x,
                        y,
                        block,
                        0,
                        team,
                        crate::world::build::PlaceFlags::API,
                    ) {
                        self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                            x,
                            y,
                            block,
                            team,
                        }));
                    } else {
                        log::debug!(
                            "place command rejected at ({x}, {y}) by the live block runtime"
                        );
                    }
                } else {
                    let current = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                    if current == BlockId::AIR {
                        let seq = self.entity_seq.alloc();
                        let comp = BuildingComp {
                            pos,
                            block,
                            team: TeamId::SHARDED,
                            rot: 0,
                        };
                        let entity = self.ecs.spawn_building(seq, comp);
                        self.grid.set_block(pos, block, TeamId::SHARDED.0, 0)?;
                        self.grid.set_entity(pos, Some(entity))?;
                        self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                            x,
                            y,
                            block,
                            team: TeamId::SHARDED.0,
                        }));
                    } else {
                        log::debug!(
                            "place command rejected at ({x}, {y}): tile already holds block {}",
                            current.raw()
                        );
                    }
                }
            }
            Command::Break { x, y } => {
                let pos = TilePos::new(x, y);
                self.grid.index(pos)?;
                let current = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                if current == BlockId::AIR {
                    log::debug!("break command rejected at ({x}, {y}): tile is empty");
                } else if self.block_runtime.is_some() {
                    let team = self
                        .grid
                        .entity_at(pos)
                        .and_then(|entity| {
                            self.ecs.0.get::<crate::entities::comp::TeamComp>(entity)
                        })
                        .map(|team| team.team)
                        .unwrap_or(0);
                    if self.runtime_break(x, y) {
                        self.push_event(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                            x,
                            y,
                            block: current,
                            // `Command::Break` is a deconstruct/API break.
                            destroyed: false,
                            team,
                        }));
                    }
                } else {
                    let team = self
                        .grid
                        .entity_at(pos)
                        .and_then(|entity| {
                            self.ecs.0.get::<crate::entities::comp::TeamComp>(entity)
                        })
                        .map(|team| team.team)
                        .unwrap_or(0);
                    if let Some(entity) = self.grid.entity_at(pos) {
                        self.ecs.despawn(entity);
                    }
                    self.grid.clear(pos)?;
                    self.push_event(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                        x,
                        y,
                        block: current,
                        // `Command::Break` is a deconstruct/API break.
                        destroyed: false,
                        team,
                    }));
                }
            }
        }
        self.commands_applied = self.commands_applied.wrapping_add(1);
        Ok(())
    }
}
