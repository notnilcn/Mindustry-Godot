// SPDX-License-Identifier: GPL-3.0-only

//! Block/bullet/pad draw seams for the render passes.
//! Upstream: core/src/mindustry/world/blocks/* (draw rows)

use super::*;

impl MindSimHost {
    /// Plan-18 sim audio log (`MindAudio` drains it; Rust-only seam).
    pub fn audio_log(&self) -> SharedAudioLog {
        self.audio_log.clone()
    }

    // ---- G07 audio/weather seams ----

    /// T5: live ambient sources for `MindAudio`'s 20 Hz poll (Rust-only).
    pub fn ambient_snapshot(&self, out: &mut mind_core::audio::AmbientSnapshot) {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            out.entries.clear();
            return;
        };
        mind_core::audio::ambient::collect_ambient(&self.sim.ecs.0, content, out);
    }

    /// T6: per-frame `control.sound.loop` sources (Rust-only).
    pub fn audio_frame_loops(&self, out: &mut Vec<mind_core::audio::frame::LoopSource>) {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            out.clear();
            return;
        };
        let infinite = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.rules.infinite_resources);
        mind_core::audio::frame::collect_frame_loops(
            &self.sim.ecs.0,
            content,
            self.sim.tick_count() as f32,
            infinite,
            out,
        );
    }

    /// T6: turret `SoundLoop` inputs (Rust-only).
    pub fn audio_turret_loops(&self, out: &mut Vec<mind_core::audio::frame::TurretLoopSource>) {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            out.clear();
            return;
        };
        mind_core::audio::frame::collect_turret_loops(&self.sim.ecs.0, content, out);
        // H07 T11: continuous-weapon `SoundLoop`s (vela/navanax) append to the
        // same per-key fade table.
        mind_core::audio::frame::collect_weapon_loops(&self.sim.ecs.0, out);
    }

    /// T7: in-game music context state (rules/wave/boss/core hp; Rust-only).
    pub fn audio_game_state(&self) -> mind_core::audio::frame::AudioGameState {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            return Default::default();
        };
        mind_core::audio::frame::read_game_state(&self.sim.ecs.0, content)
    }

    /// H07 T6: one view per live `WeatherRuntime`, sorted by `EntitySeq`
    /// (renderer input; Rust-only).
    pub fn weather_views(&self, out: &mut Vec<mind_core::fx::WeatherStateView>) {
        out.clear();
        let mut rows: Vec<(u64, mind_core::fx::WeatherStateView)> = self
            .sim
            .ecs
            .0
            .iter_entities()
            .filter_map(|entity| {
                let state = entity.get::<mind_core::entities::comp::weather::WeatherRuntime>()?;
                let seq = entity
                    .get::<mind_core::ecs::EntitySeq>()
                    .map_or(u64::MAX, |seq| seq.0);
                Some((
                    seq,
                    mind_core::fx::WeatherStateView {
                        weather: state.weather,
                        intensity: state.intensity,
                        opacity: state.opacity,
                        wind_vector: (state.wind_x, state.wind_y),
                        life: state.life,
                    },
                ))
            })
            .collect();
        rows.sort_by_key(|(seq, _)| *seq);
        out.extend(rows.into_iter().map(|(_, view)| view));
    }

    /// Belt items at `(x, y)`: `(rotation, [(item, xs, ys); len])` for a
    /// `Conveyor`/`ArmoredConveyor` (`Conveyor.draw` item loop). `None` when
    /// the tile holds no belt. Rust-only seam for the block renderer.
    pub fn conveyor_items(&self, x: i32, y: i32) -> Option<(u8, Vec<BeltItemSlot>)> {
        let entity = self.tile_at(x, y)?.build?;
        let rot = self
            .sim
            .ecs
            .0
            .get::<mind_core::ecs::BuildingComp>(entity)?
            .rot;
        let belt = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::distribution::ConveyorBuild>(entity)?;
        let mut items = Vec::with_capacity(belt.len as usize);
        for i in 0..belt.len as usize {
            items.push((belt.ids[i], belt.xs[i], belt.ys[i]));
        }
        Some((rot, items))
    }

    /// Belt sprite state at `(x, y)`: `(blendbits, sclx, scly, frame)` for a
    /// `Conveyor`/`ArmoredConveyor` (`ConveyorBuild.draw` / `Autotiler`
    /// blending cached by the sim on proximity updates). `None` when the tile
    /// holds no belt. Rust-only seam for the block renderer.
    pub fn belt_draw(&self, x: i32, y: i32) -> Option<(u8, f32, f32, i32)> {
        let entity = self.tile_at(x, y)?.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(self.sim.grid.tile(x, y).block)?;
        let speed =
            mind_core::world::blocks::distribution::ConveyorBehavior::belt_speed(&def.name)?;
        let world = &self.sim.ecs.0;
        let (blend_bits, sclx, scly, clog_heat) = {
            let belt =
                world.get::<mind_core::world::blocks::distribution::ConveyorBuild>(entity)?;
            (
                belt.blend_bits,
                f32::from(belt.blend_sclx),
                f32::from(belt.blend_scly),
                belt.clog_heat,
            )
        };
        let building = world.get::<mind_core::entities::comp::Building>(entity)?;
        let frame = mind_core::render::autotile::conveyor_frame(
            self.sim.tick_count() as f32,
            speed,
            building.efficiency,
            building.enabled,
            clog_heat,
        );
        Some((blend_bits, sclx, scly, frame))
    }

    /// Drill draw state at `(x, y)`: `timeDrilled` (`Drill.DrillBuild.draw`
    /// rotator angle = `timeDrilled * rotateSpeed`; `Drill.rotateSpeed = 2`).
    /// `None` when the tile holds no drill. Rust-only seam for the block
    /// renderer.
    pub fn drill_draw(&self, x: i32, y: i32) -> Option<f32> {
        let entity = self.tile_at(x, y)?.build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::entities::comp::DrillState>(entity)
            .map(|state| state.time_drilled)
    }

    /// Turret draw state at `(x, y)`: `(rotation degrees, recoil offset x,
    /// recoil offset y, heat)` (`DrawTurret.draw`: `recoilOffset =
    /// trns(rotation, -pow(curRecoil, recoilPow) * recoil)`, barrel rotation
    /// `drawrot() = rotation - 90`). `None` without a turret.
    pub fn turret_draw(&self, x: i32, y: i32) -> Option<(f32, f32, f32, f32)> {
        let entity = self.tile_at(x, y)?.build?;
        let state = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)?;
        let len = -state.cur_recoil.max(0.0).powf(state.config.recoil_pow) * state.config.recoil;
        let (dx, dy) = mind_core::render::conveyor_items::trns(state.rotation, len, 0.0);
        Some((state.rotation, dx, dy, state.heat))
    }

    /// Full `DrawTurret.draw` inputs at `(x, y)` (J03 T1): the live
    /// `TurretState` counters (`DrawTurret.java:74-119`) plus the placed
    /// block's draw metadata (parts, heat color, liquid, elevation). `None`
    /// without a turret. Rust-only seam for the turret render pass.
    pub fn turret_draw_input(
        &self,
        x: i32,
        y: i32,
    ) -> Option<crate::fx::draw_turret::TurretDrawInput> {
        use mind_core::content::registries::blocks::turret_parts;
        use mind_core::world::blocks::defense::turrets::TurretState;

        let entity = self.tile_at(x, y)?.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(self.sim.grid.tile(x, y).block)?;
        let state = self.sim.ecs.0.get::<TurretState>(entity)?;
        let name = def.name.as_str();

        let len = -state.cur_recoil.max(0.0).powf(state.config.recoil_pow) * state.config.recoil;
        let (dx, dy) = mind_core::render::conveyor_items::trns(state.rotation, len, 0.0);

        // `TurretBuild.progress()` = `clamp(reloadCounter / reload)`.
        let progress = if state.config.reload > 0.0 {
            (state.reload_counter / state.config.reload).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // `drawTurret`: `Drawf.liquid(liquid, ..., liquids.get(toDraw) /
        // block.liquidCapacity, toDraw.color, drawrot)`.
        let (liquid_fraction, liquid_color) = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::modules::LiquidModule>(entity)
            .map(|liquids| {
                let liquid = liquids.current_liquid();
                let capacity = def.liquid_capacity.max(1e-6);
                let color = content
                    .liquid(liquid)
                    .map(|def| def.color)
                    .unwrap_or(mind_core::content::Rgba::WHITE);
                (liquids.get(liquid) / capacity, color)
            })
            .unwrap_or((0.0, mind_core::content::Rgba::WHITE));

        // `TurretBuild.getAmmoContent()` for `ammoParts`: the last item entry
        // (`ItemTurretBuild.getAmmoContent`, `ItemTurret.java:172-174`).
        let ammo_item = state
            .ammo
            .last()
            .and_then(|entry| content.item(entry.item))
            .map(|item| item.name.as_str())
            .unwrap_or("");
        let ammo_parts = if ammo_item.is_empty() {
            Vec::new()
        } else {
            turret_parts::ammo_parts_for(name, ammo_item)
        };

        let center = ((def.size - 1) / 2) as f32;
        let size = mind_core::config::TILESIZE as f32;

        Some(crate::fx::draw_turret::TurretDrawInput {
            handle: 0,
            name: name.to_owned(),
            // `Building.x/y` = the footprint center (`Tile.drawx + Block.offset`);
            // the caller passes the first row-major tile, exactly like the block
            // pass's anchor.
            x: (x as f32 + center) * size + def.offset,
            y: (y as f32 + center) * size + def.offset,
            // `BaseTurretBuild.drawrot()` = `rotation - 90`.
            drawrot: state.rotation - 90.0,
            // `DrawPart.params` takes the raw rotation (`DrawTurret.java:101`).
            rotation: state.rotation,
            recoil_offset: (dx, dy),
            // `Turret.init`: `elevation = size / 2f` (no vanilla override).
            elevation: def.size as f32 / 2.0,
            heat: state.heat,
            warmup: state.shoot_warmup,
            cur_recoil: state.cur_recoil,
            cur_recoils: state.cur_recoils.to_vec(),
            charge: state.charge,
            progress,
            liquid_fraction,
            liquid_color,
            heat_color: turret_parts::heat_color(name),
            parts: turret_parts::parts_for(name),
            ammo_parts,
            turret_layer: mind_core::render::layer::Layer::Turret.z(),
            shadow_layer: mind_core::render::layer::Layer::Turret.z() - 0.5,
            heat_layer: mind_core::render::layer::Layer::TurretHeat.z(),
        })
    }

    /// Per-building state for the block pass (G03 T8): door open, switch
    /// enabled, sorter/unloader/source items, damage and team mark inputs.
    pub fn block_state_draw(&self, x: i32, y: i32) -> Option<BlockStateDraw> {
        use mind_core::entities::comp::building::{DoorState, SandboxState};
        use mind_core::entities::comp::health::Health;
        use mind_core::world::blocks::distribution::sorter::SorterBuild;
        use mind_core::world::blocks::storage::unloader::UnloaderBuild;
        let entity = self.tile_at(x, y)?.build?;
        let healthf = self
            .sim
            .ecs
            .0
            .get::<Health>(entity)
            .map(Health::fraction)
            .unwrap_or(1.0);
        Some(BlockStateDraw {
            door_open: self
                .sim
                .ecs
                .0
                .get::<DoorState>(entity)
                .map(|door| door.open)
                .unwrap_or(false),
            enabled: self
                .sim
                .ecs
                .0
                .get::<mind_core::entities::comp::building::Building>(entity)
                .map(|building| building.enabled)
                .unwrap_or(true),
            sort_item: self
                .sim
                .ecs
                .0
                .get::<SorterBuild>(entity)
                .and_then(|sorter| sorter.sort_item),
            unloader_item: self
                .sim
                .ecs
                .0
                .get::<UnloaderBuild>(entity)
                .and_then(|unloader| unloader.sort_item),
            source_item: self
                .sim
                .ecs
                .0
                .get::<SandboxState>(entity)
                .and_then(|state| state.item),
            healthf,
            damaged: healthf < 0.999,
        })
    }

    /// Cheap per-building hash of the state that changes a `drawCached` bake
    /// (`BuildingCacheRenderer` chunk state sums; H03 T18). `0` without a
    /// building. Mirrors `BuildingComp.recache()` being called on config
    /// changes that never bump the world revision.
    pub fn cached_state_key(&self, x: i32, y: i32) -> u64 {
        use mind_core::render::block_sprite::{CachedBlockState, cached_state_hash, crack_index};
        let Some(state) = self.block_state_draw(x, y) else {
            return 0;
        };
        cached_state_hash(&CachedBlockState {
            door_open: state.door_open,
            enabled: state.enabled,
            item: state
                .sort_item
                .or(state.unloader_item)
                .or(state.source_item),
            damaged: state.damaged,
            crack: crack_index(state.healthf),
        })
    }

    /// `BlockRenderer` crack-overlay rows (`BlockRenderer.java:801-821`):
    /// every damaged building whose block is `!draw_dynamic` (exactly the
    /// buildings the dynamic pass skips and the static cache bakes; the port
    /// counterpart of Java's `tileExtraCachedView`). `(world x, world y,
    /// tile y, size, healthf)`.
    pub fn cached_crack_draws(&self) -> Vec<(f32, f32, i32, i32, f32)> {
        use mind_core::entities::comp::health::Health;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(building) = entity.get::<mind_core::entities::comp::Building>() else {
                continue;
            };
            let Some(def) = content.block(building.block) else {
                continue;
            };
            // `drawCracks` guards (`BuildingComp.java:1234`): `drawDynamic`
            // blocks draw their own cracks in the dynamic pass, and
            // `BlockRenderer.maxCrackSize` is 7.
            if def.draw_dynamic || def.size > 7 {
                continue;
            }
            let Some(health) = entity.get::<Health>() else {
                continue;
            };
            if !health.damaged() {
                continue;
            }
            let size = def.size.max(1);
            // `Building.x = tile.drawx() + Block.offset` (same anchor as the
            // dynamic pass and the cache bake).
            out.push((
                f32::from(building.tile.x()) * 8.0 + def.offset,
                f32::from(building.tile.y()) * 8.0 + def.offset,
                i32::from(building.tile.y()),
                size,
                health.fraction(),
            ));
        }
        out
    }

    /// Fires in `rect` (`FireComp.draw` view data; H03 T7), stable by entity
    /// index. The tile centre gets Java's `randomSeedRange` jitter; `frame`
    /// and `alpha` come from the sim's animation/warmup.
    pub fn fire_draws(&self, rect: (f32, f32, f32, f32)) -> Vec<FireDraw> {
        use mind_core::combat::fires::{FRAMES, FireState, WARMUP_DURATION};
        use mind_core::render::drawf::random_seed_range;
        let (x0, y0, w, h) = rect;
        let (x1, y1) = (x0 + w, y0 + h);
        let mut rows: Vec<(u32, FireDraw)> = Vec::new();
        for entity_ref in self.sim.ecs.0.iter_entities() {
            let Some(fire) = entity_ref.get::<FireState>() else {
                continue;
            };
            let (tx, ty) = fire.tile;
            let cx = f32::from(tx) * 8.0;
            let cy = f32::from(ty) * 8.0;
            // `Draw.rect(regions[...], x + randomSeedRange((int)y, 2), y + randomSeedRange((int)x, 2))`
            // (`FireComp.java:104-106`): the fire's world coordinates seed the
            // two axis offsets.
            let x = cx + random_seed_range(i64::from(ty) * 8, 2.0);
            let y = cy + random_seed_range(i64::from(tx) * 8, 2.0);
            if x < x0 || y < y0 || x > x1 || y > y1 {
                continue;
            }
            rows.push((
                entity_ref.id().index_u32(),
                FireDraw {
                    x,
                    y,
                    frame: (fire.animation as i32).min(FRAMES as i32 - 1),
                    alpha: (fire.warmup / WARMUP_DURATION).clamp(0.0, 1.0),
                },
            ));
        }
        rows.sort_by_key(|(index, _)| *index);
        rows.into_iter().map(|(_, draw)| draw).collect()
    }

    /// Puddles in `rect` (`PuddleComp.draw`/`Liquid.drawPuddle` view data;
    /// H03 T7), stable by entity index.
    pub fn puddle_draws(&self, rect: (f32, f32, f32, f32)) -> Vec<PuddleDraw> {
        use mind_core::combat::puddles::{MAX_LIQUID, PuddleState};
        use mind_core::render::drawf::shift_value;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let (x0, y0, w, h) = rect;
        let (x1, y1) = (x0 + w, y0 + h);
        let mut rows: Vec<(u32, PuddleDraw)> = Vec::new();
        for entity_ref in self.sim.ecs.0.iter_entities() {
            let Some(puddle) = entity_ref.get::<PuddleState>() else {
                continue;
            };
            let (tx, ty) = puddle.tile;
            let x = f32::from(tx) * 8.0;
            let y = f32::from(ty) * 8.0;
            if x < x0 || y < y0 || x > x1 || y > y1 {
                continue;
            }
            let Some(def) = content.liquid(puddle.liquid) else {
                continue;
            };
            // `float f = Mathf.clamp(amount / (maxLiquid / 1.5f))`.
            let f = (puddle.amount / (MAX_LIQUID / 1.5)).clamp(0.0, 1.0);
            // `Tmp.c1.set(color).shiftValue(-0.05f)`.
            let color = shift_value([def.color.r, def.color.g, def.color.b, def.color.a], -0.05);
            let on_liquid = content
                .block(self.sim.grid.tile(tx as i32, ty as i32).floor)
                .is_some_and(mind_core::maps::filters::block_info::is_liquid);
            let light = (def.light_color.a > 0.001 && f > 0.0).then(|| {
                (
                    [
                        def.light_color.r,
                        def.light_color.g,
                        def.light_color.b,
                        def.light_color.a,
                    ],
                    color[3] * f * 0.8,
                )
            });
            rows.push((
                entity_ref.id().index_u32(),
                PuddleDraw {
                    x,
                    y,
                    id: entity_ref.id().index_u32() as i32,
                    f,
                    color,
                    on_liquid,
                    light,
                },
            ));
        }
        rows.sort_by_key(|(index, _)| *index);
        rows.into_iter().map(|(_, draw)| draw).collect()
    }

    /// `DrawState` for a block's `DrawBlock` chain (`world/draw/*`; H03 T8).
    /// `None` without a building, a content def, or the content snapshot.
    pub fn block_draw_state(
        &self,
        x: i32,
        y: i32,
    ) -> Option<mind_core::render::draw_desc::DrawState> {
        use mind_core::content::LiquidId;
        use mind_core::entities::comp::building::CrafterState;
        use mind_core::world::blocks::heat::HeatState;
        use mind_core::world::blocks::power::generator::GeneratorState;
        use mind_core::world::modules::{LiquidModule, PowerModule};
        let tile = self.tile_at(x, y)?;
        let entity = tile.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(tile.block)?;
        let ecs = &self.sim.ecs.0;
        let size = def.size.max(1);
        let rotation = self.building_rotation(x, y);
        let cx = (x + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let cy = (y + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let crafter = ecs.get::<CrafterState>(entity);
        // `GenericCrafter` carries `warmup` on `CrafterState`; generators drive
        // their drawer from `productionEfficiency` (`ConsumeGenerator` /
        // reactors), which the port keeps on `GeneratorState`.
        let warmup = ecs
            .get::<GeneratorState>(entity)
            .map(|state| state.production_efficiency)
            .or_else(|| crafter.map(|state| state.warmup))
            .unwrap_or(0.0);
        // Dominant liquid entry (for `liquidColor`/`liquidAmount` and the
        // `Liquid.getAnimationFrame` fallback consumers).
        let (liquid_id, liquid_gas, liquid_amount, liquid_color) = ecs
            .get::<LiquidModule>(entity)
            .map(|module| {
                let (id, amount) = module
                    .liquids
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(id, amount)| (id as u16, *amount))
                    .unwrap_or((0, 0.0));
                let liquid = content.liquid(LiquidId::new(id));
                (
                    id,
                    liquid.map(|liquid| liquid.gas).unwrap_or(false),
                    amount,
                    liquid
                        .map(|liquid| liquid.color.to_rgba8888())
                        .unwrap_or(0xffff_ffff),
                )
            })
            .unwrap_or((0, false, 0.0, 0xffff_ffff));
        Some(mind_core::render::draw_desc::DrawState {
            warmup,
            progress: crafter.map(|state| state.progress).unwrap_or(0.0),
            total_progress: crafter.map(|state| state.total_progress).unwrap_or(0.0),
            // `Time.time` view clock (the same 60 Hz ticks the other passes use).
            time: godot::classes::Time::singleton().get_ticks_msec() as f32 * 0.06,
            id: entity.index_u32() as i32,
            power_status: ecs
                .get::<PowerModule>(entity)
                .map(|module| module.status)
                .unwrap_or(0.0),
            liquid_capacity: def.liquid_capacity,
            liquid_amount,
            liquid_color,
            liquid_id,
            liquid_gas,
            heat: ecs
                .get::<HeatState>(entity)
                .map(|state| state.heat)
                .unwrap_or(0.0),
            ..mind_core::render::draw_desc::DrawState::new(cx, cy, i32::from(rotation), size)
        })
    }

    /// The player team's rebuild ghosts (`TeamData.plans`, pushed by
    /// `Sim::resolve_deaths`): `(x, y, rotation, block)` in insertion order.
    pub fn team_block_plans(&self) -> Vec<(i16, i16, i8, BlockId)> {
        let team = mind_core::ecs::TeamId(self.player_team());
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .and_then(|runtime| runtime.session.teams.data(team))
            .map(|data| {
                data.plans
                    .iter()
                    .filter(|plan| !plan.removed)
                    .map(|plan| (plan.x, plan.y, plan.rotation, plan.block))
                    .collect()
            })
            .unwrap_or_default()
    }

    // ---- G03 T4 bullet draw seam (Rust-only) ----

    /// Live bullets in spawn order (`Groups.bullet`, G03 T4). Skips bullets
    /// already marked finished (removed this tick).
    pub fn bullet_draws(&self) -> Vec<BulletDraw> {
        use mind_core::combat::bullet::Bullet;
        use mind_core::ecs::EntitySeq;
        use mind_core::entities::comp::Pos;
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &entity in &runtime.bullets {
            let Some(pos) = self.sim.ecs.0.get::<Pos>(entity) else {
                continue;
            };
            let Some(bullet) = self.sim.ecs.0.get::<Bullet>(entity) else {
                continue;
            };
            if bullet.finished() {
                continue;
            }
            let id = self
                .sim
                .ecs
                .0
                .get::<EntitySeq>(entity)
                .map(|seq| seq.0)
                .unwrap_or(0);
            out.push(BulletDraw {
                id,
                x: pos.x,
                y: pos.y,
                rotation: bullet.rotation,
                time: bullet.time,
                lifetime: bullet.lifetime,
                fdata: bullet.fdata,
                def: bullet.def,
                team: 0,
            });
        }
        out
    }

    /// Pod and pad draw rows (`LaunchPayloadComp.draw` `LaunchPad.java:227-280`,
    /// `LaunchPadBuild.draw` `:105-135`, `LandingPadBuild.draw`
    /// `LandingPad.java:205-265`; H03 T13). Rust-only seam for `render/pods.rs`.
    pub fn pad_draws(&mut self) -> Vec<PadDraw> {
        let mut out = Vec::new();
        let block_size = |snapshot: &Option<mind_core::content::ContentRegistry>,
                          block: mind_core::content::BlockId| {
            snapshot
                .as_ref()
                .and_then(|content| content.block(block))
                .map(|def| def.size.max(1))
                .unwrap_or(1)
        };
        for (id, x, y, time, lifetime, block, _team) in
            mind_core::game::launch_payload::payload_rows(&mut self.sim.ecs.0)
        {
            out.push(PadDraw::Launch {
                id,
                x,
                y,
                time,
                lifetime,
                block,
            });
        }
        for (id, x, y, block, _config, arriving, arriving_timer, cooldown) in
            mind_core::game::landing_pad::landing_pad_rows(&mut self.sim.ecs.0)
        {
            out.push(PadDraw::Landing {
                id,
                x,
                y,
                size: block_size(&self.content_snapshot, block),
                arriving: arriving.map(mind_core::content::ItemId::new),
                arriving_timer,
                cooldown,
                block,
            });
        }
        for (id, x, y, block, progress) in
            mind_core::world::behavior::campaign::launch_pad_rows(&mut self.sim.ecs.0)
        {
            out.push(PadDraw::Pad {
                id,
                x,
                y,
                size: block_size(&self.content_snapshot, block),
                progress,
                block,
            });
        }
        out
    }

    /// Interplanetary Accelerator draw rows (`AcceleratorBuild.draw`,
    /// world/blocks/campaign/Accelerator.java:140-236) for
    /// `render/accelerator.rs`: the live `CampaignState`/`Building` counters plus
    /// the block's drawn footprint. Rust-only seam (like `pad_draws`).
    pub fn accelerator_draws(&mut self) -> Vec<mind_core::render::accelerator::AcceleratorDraw> {
        mind_core::render::accelerator::accelerator_rows(&mut self.sim.ecs.0)
    }
}
