// SPDX-License-Identifier: GPL-3.0-only

//! `Sim::flush_events`: tick-end event dispatch (fog, stats, audio, event bus).
//!
//! Upstream: none (port-only event pump; listener bodies mirror core/Logic.java).

use crate::sim::*;

impl Sim {
    /// P0 flush: dispatch queued events synchronously at tick end.
    pub(in crate::sim) fn flush_events(&mut self) {
        if self.pending_events.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_events);
        for event in pending {
            // H09 T1 fog listeners (`FogControl.java:64-95`): placed/broken
            // fog-radius buildings mark the dynamic flush pending.
            self.handle_fog_block_event(&event);
            // `Logic`'s `BlockBuildEndEvent`/`BlockDestroyEvent` listeners
            // (`Logic.java:186-216`; H09 T10): placed/deconstructed/destroyed
            // counts plus the campaign planet maps, keyed by block name.
            match &event {
                SimEvent::BlockPlacedEvent(placed) => {
                    if let Ok(name) = self.content.name(placed.block)
                        && let Some(mut runtime) = self
                            .ecs
                            .0
                            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                        && runtime.session.rules.default_team == placed.team
                    {
                        runtime.session.stats.add_placed(name, 1);
                        runtime.session.stats.buildings_built += 1;
                        if runtime.session.is_campaign()
                            && let Some((planet, _)) = runtime.session.sector
                            && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                        {
                            stats.add_building_built(name, 1);
                        }
                    }
                }
                SimEvent::BlockBrokenEvent(broken) => {
                    if let Ok(name) = self.content.name(broken.block)
                        && let Some(mut runtime) = self
                            .ecs
                            .0
                            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                    {
                        let default_team = runtime.session.rules.default_team;
                        if broken.destroyed {
                            if broken.team == default_team {
                                runtime.session.stats.buildings_destroyed += 1;
                                if runtime.session.is_campaign()
                                    && let Some((planet, _)) = runtime.session.sector
                                    && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                                {
                                    stats.add_building_destroyed(name, 1);
                                }
                            } else {
                                runtime.session.stats.add_destroyed(name, 1);
                                if runtime.session.is_campaign()
                                    && let Some((planet, _)) = runtime.session.sector
                                    && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                                {
                                    stats.add_enemy_building_destroyed(name, 1);
                                }
                            }
                        } else if broken.team == default_team {
                            runtime.session.stats.buildings_deconstructed += 1;
                            if runtime.session.is_campaign()
                                && let Some((planet, _)) = runtime.session.sector
                                && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                            {
                                stats.add_building_deconstructed(name, 1);
                            }
                        }
                    }
                }
                _ => {}
            }
            let record = dump::block_event_record(self, &event);
            if let Some(record) = record {
                self.event_log.push(record);
            }
            // G07 T3: live place/break sounds
            // (`ConstructBlock.constructFinish`/`deconstructFinish`,
            // `ConstructBlock.java:58-120`; `PAR-0073`). Fog gating
            // (`fogControl.isVisibleTile`) is skipped (fog is G03).
            if let Some(audio) = self
                .ecs
                .0
                .get_resource::<crate::audio::AudioSinkRes>()
                .cloned()
                && let Some(content) = self.block_runtime.as_ref().map(|runtime| runtime.content())
            {
                match &event {
                    SimEvent::BlockPlacedEvent(placed) => {
                        if let Some(def) = content.block(placed.block) {
                            let (x, y) =
                                crate::audio::blocks::world_center(def, placed.x, placed.y);
                            audio.emit(crate::audio::AudioEvent::Construct {
                                sound: crate::audio::blocks::place_sound(def),
                                x,
                                y,
                                place: true,
                                pitch_change: true,
                            });
                        }
                    }
                    SimEvent::BlockBrokenEvent(broken) if !broken.destroyed => {
                        if let Some(def) = content.block(broken.block) {
                            let (x, y) =
                                crate::audio::blocks::world_center(def, broken.x, broken.y);
                            audio.emit(crate::audio::AudioEvent::Construct {
                                sound: crate::audio::blocks::break_sound(def),
                                x,
                                y,
                                place: false,
                                pitch_change: true,
                            });
                        }
                    }
                    _ => {}
                }
            }
            self.events.dispatch(event);
        }
    }
}
