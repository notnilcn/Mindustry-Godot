// SPDX-License-Identifier: GPL-3.0-only

//! Build-plan, placement-range and unit-plan views for the HUD.
//! Upstream: core/src/mindustry/input/InputHandler.java (plans/placement)

use super::*;

impl MindSimHost {
    /// Linearly searches the grid for `(x, y, block-id)` pairs of a rectangle;
    /// Rust-only seam for `MindCampaign.write_schematic_selection`.
    pub fn tile_region(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i16, i16, u16)> {
        let (min_x, max_x) = (x0.min(x1), x0.max(x1));
        let (min_y, max_y) = (y0.min(y1), y0.max(y1));
        let mut out = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let Ok(x16) = i16::try_from(x) else { continue };
                let Ok(y16) = i16::try_from(y) else { continue };
                if let Some(block) = self
                    .sim
                    .grid
                    .block_at(mind_core::world::TilePos::new(x16, y16))
                {
                    out.push((x16, y16, block.raw()));
                }
            }
        }
        out
    }

    /// Immediate block placement at a tile (schematic placement seam). The P0
    /// `Sim::apply` path carries rotation `0` only; rotated schematics degrade
    /// to unrotated until the rotate relay command lands (I-2).
    pub fn place_block_at(&mut self, x: i16, y: i16, block: BlockId, rot: i8) -> bool {
        // The live runtime carries rotation/team and routes through the real
        // tile ops (`SimCommand::Place`); the P0 command has no rotation and is
        // kept for runtime-less worlds (T18).
        if self.sim.has_block_runtime() {
            let rotation = rot.rem_euclid(4);
            self.pending_commands.push_back(SimCommand::Place {
                x,
                y,
                block: block.raw(),
                rotation,
                team: self.player_team(),
                player: None,
            });
            self.world_dirty = true;
            return true;
        }
        match self.sim.apply(Command::Place { x, y, block }) {
            Ok(()) => {
                self.world_dirty = true;
                true
            }
            Err(error) => {
                log::warn!("schematic place at ({x}, {y}) rejected: {error}");
                false
            }
        }
    }

    /// Whether player construction plans are accepted (`Sim::player_can_build`;
    /// T18 schematic placement).
    pub fn player_can_build(&self) -> bool {
        self.sim.player_can_build()
    }

    /// Queues a player construction plan (`Sim::queue_player_place`; T18).
    pub fn queue_player_place_at(&mut self, x: i16, y: i16, block: BlockId, rot: i8) -> bool {
        let rotation = u8::try_from(rot).unwrap_or(0).rem_euclid(4);
        self.sim.queue_player_place(x, y, block, rotation)
    }

    /// Queues a player construction plan carrying a config (G08 T5: schematic
    /// copies, rebuild ghosts, line plan configs, `nextConfig`).
    pub fn queue_player_place_config(
        &mut self,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        config: mind_core::world::config::ConfigValue,
    ) -> bool {
        self.sim.queue_player_place_config(x, y, block, rot, config)
    }

    /// `Block.rotate` for a block id (G08 T6b plan rotation math).
    pub fn block_rotates(&self, id: BlockId) -> bool {
        mind_core::world::blocks::payloads::payload_block::block_rotates(self.world(), id)
    }

    /// `Schematics.create` over the live world (F-drag / break-cut copy): the
    /// selection rect plus the paste cursor resolve to select plans
    /// (`Schematics.toPlans`, priority-sorted).
    pub fn copy_schematic_plans(
        &self,
        start: (i32, i32),
        end: (i32, i32),
        cursor: (i32, i32),
    ) -> Vec<mind_core::input::ClientPlan> {
        let (x1, y1) = start;
        let (x2, y2) = end;
        let (cx, cy) = cursor;
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let world = mind_core::game::schematics::EcsSchematicWorld {
            grid: &self.sim.grid,
            world: self.world(),
            content,
            team: Some(self.player_team()),
        };
        let schem = mind_core::game::schematics::Schematics::new().create(
            &world,
            content,
            x1,
            y1,
            x2,
            y2,
            mind_core::game::rules::MAX_SCHEMATIC_SIZE,
        );
        mind_core::game::schematics::plans_for(&schem, cx, cy, true, content)
            .iter()
            .map(mind_core::input::ClientPlan::from_build_plan)
            .collect()
    }

    /// `InputHandler.rebuildArea` (B drag): re-queues team ghost plans in the
    /// rect. Returns the queued count.
    pub fn queue_team_rebuild(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> usize {
        self.sim.queue_team_rebuild(x1, y1, x2, y2)
    }

    /// `InputHandler.removeSelection` plan half (right drag): drops queued
    /// player plans and team ghosts overlapping the clamped rect.
    pub fn remove_plans_in(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, max_length: i32) {
        self.sim.remove_plans_in(x1, y1, x2, y2, max_length);
    }

    /// `updatePick`'s `tryBlock`/`tryConfig` (`PlacementFragment.java:141-143`):
    /// the hovered building's block (a `ConstructBuild` resolves to its
    /// `current`) and its config, copied only for `copyConfig` blocks
    /// (`ItemBridge.copyConfig = false`, `ItemBridge.java:60`). Rust-only seam
    /// for the middle-click pick.
    pub fn pick_target_at(
        &self,
        x: i32,
        y: i32,
    ) -> Option<(BlockId, Option<mind_core::world::config::ConfigValue>)> {
        let entity = self.tile_at(x, y)?.build?;
        let block = self
            .world()
            .get::<mind_core::entities::comp::Building>(entity)
            .map(|building| building.block)?;
        let effective = self
            .world()
            .get::<mind_core::world::ConstructState>(entity)
            .map_or(block, |state| state.current);
        let def = self.content_registry()?.block(block)?;
        let copy = !matches!(
            def.kind,
            BlockKind::ItemBridge | BlockKind::BufferedItemBridge
        );
        let config = if copy {
            let value = mind_core::world::config::read_config(self.world(), entity);
            (!matches!(value, mind_core::world::config::ConfigValue::None)).then_some(value)
        } else {
            None
        };
        Some((effective, config))
    }

    /// `updatePick`'s plan override (`PlacementFragment.java:145-154`): the
    /// player unit's queued non-breaking plan whose `Block.bounds(plan.x,
    /// plan.y)` contains the world point, with its carried config.
    pub fn player_plan_at_world(
        &self,
        wx: f32,
        wy: f32,
    ) -> Option<(BlockId, mind_core::world::config::ConfigValue)> {
        let registry = self.content_registry()?;
        for plan in self.sim.player_build_plans() {
            if plan.breaking || plan.block == BlockId::AIR {
                continue;
            }
            let Some(def) = registry.block(plan.block) else {
                continue;
            };
            // `Block.bounds(x, y)`: centre `x*tilesize + offset`, edge
            // `size*tilesize` (`Block.java` `bounds`; tilesize 8).
            let offset = ((def.size + 1) % 2) as f32 * 4.0;
            let half = def.size as f32 * 4.0;
            let cx = f32::from(plan.x) * 8.0 + offset;
            let cy = f32::from(plan.y) * 8.0 + offset;
            if wx >= cx - half && wx < cx + half && wy >= cy - half && wy < cy + half {
                return Some((plan.block, plan.config.clone()));
            }
        }
        None
    }

    // ---- G03 T2 placement preview seams (Rust-only) ----

    /// `InputHandler.validPlace` for the placement-preview ghost. Rust-only;
    /// preview only, no side effects (G03 T2).
    pub fn preview_can_place(&self, x: i32, y: i32, block: BlockId, rot: u8) -> bool {
        let Some((x, y)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        self.sim.preview_can_place(x, y, block, rot)
    }

    /// `Block.drawPlace` range circle for `block` (G03 T2): `(radius world px,
    /// kind, rgba8888 colour)`. `None` = the block draws no range circle.
    ///
    /// Turret ranges come from the ported `TurretConfig`
    /// (`BaseTurret.java:84-90`); power-node `laserRange` (6/15/40) and the
    /// projector table are transcribed from `Blocks.java:2483-2504,1921-1958`.
    pub fn block_place_range(&self, block: BlockId) -> Option<(f32, PlaceRangeKind, u32)> {
        use mind_core::render::drawf::{self, pal};
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(block)?;
        if matches!(
            def.kind,
            BlockKind::MendProjector | BlockKind::OverdriveProjector
        ) {
            const PROJECTOR_RANGES: [(&str, f32, u32); 4] = [
                ("mender", 40.0, 0x84f4_91ff),
                ("mend-projector", 85.0, 0x84f4_91ff),
                ("overdrive-projector", 80.0, 0xfeb3_80ff),
                ("overdrive-dome", 200.0, 0xfeb3_80ff),
            ];
            return PROJECTOR_RANGES
                .iter()
                .find(|(name, ..)| *name == def.name)
                .map(|(_, range, color)| (*range, PlaceRangeKind::Projector, *color));
        }
        let accent = drawf::to_bits(pal::ACCENT);
        let laser_range = match def.name.as_str() {
            "power-node" => 6.0,
            "power-node-large" => 15.0,
            "surge-tower" => 40.0,
            _ => 0.0,
        };
        if laser_range > 0.0 {
            return Some((laser_range * 8.0, PlaceRangeKind::PowerNode, accent));
        }
        let cached = self.place_range_cache.borrow().get(&block).copied();
        let range = match cached {
            Some(range) => range,
            None => {
                // Turret ranges resolve against the *sim runtime* content: it is
                // the registry the live `TurretBehavior` was built from and
                // carries the named turret-ammo fixtures (`install_turret_ammo`),
                // unlike the view snapshot (`create_base_content` only).
                let range = self
                    .sim
                    .block_runtime()
                    .and_then(|runtime| {
                        let runtime_content = runtime.content();
                        let runtime_def = runtime_content.block(block)?;
                        mind_core::world::blocks::defense::turrets::config_for(
                            runtime_content,
                            &runtime_def.name,
                            runtime_content.bullet_names(),
                        )
                    })
                    .map(|config| config.range)
                    .or_else(|| {
                        mind_core::world::blocks::defense::turrets::config_for(
                            content,
                            &def.name,
                            content.bullet_names(),
                        )
                        .map(|config| config.range)
                    });
                self.place_range_cache.borrow_mut().insert(block, range);
                range
            }
        };
        range.map(|range| (range, PlaceRangeKind::Turret, accent))
    }

    /// `LogicBlock.drawPlace` (`LogicBlock.java:185-189`): the processor link
    /// range drawn while placing (`Drawf.circles(.., range)` in `Pal.accent`);
    /// `None` for privileged/world processors and non-logic blocks. Kept apart
    /// from [`Self::block_place_range`] because `LogicBuild.drawSelect` draws
    /// no hover range.
    pub fn logic_place_range(&self, block: BlockId) -> Option<f32> {
        mind_core::logic::blocks::logic_place_range(self.world(), block)
    }

    /// `InputHandler.validBreak(x, y)`: whether the local team may deconstruct
    /// the building at `(x, y)`. Builds `BuildRules` from the world resource
    /// like `SimRuntime::can_place` does. Rust-only seam for the break-rect
    /// preview (G03 T2).
    pub fn preview_can_break(&self, x: i32, y: i32) -> bool {
        let team = self.sim.live_default_team();
        let Some(content) = self.content_registry() else {
            return false;
        };
        let rules = self
            .world()
            .get_resource::<mind_core::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        mind_core::world::build::valid_break(
            content,
            &self.sim.grid,
            Some(self.world()),
            &rules,
            team,
            x,
            y,
        )
    }

    /// `Rules.placeRangeCheck` for the drawOverlapCheck gate
    /// (`InputHandler.java:1500`). Rust-only seam for the placement overlay
    /// (GAPS #47 residual).
    pub fn place_range_check(&self) -> bool {
        self.world()
            .get_resource::<mind_core::world::limits::BuildRules>()
            .is_some_and(|rules| rules.place_range_check)
    }

    /// `Build.getEnemyOverlap` plus `blocker.wasVisible` (`InputHandler.java:
    /// 1500-1502`): the enemy building whose placement-check radius overlaps
    /// `(x, y)`, as `(origin_x, origin_y, block_size, visible)`.
    ///
    /// `visible` folds the port's fog convention (`Building::was_visible` is
    /// only written by the fog pass; with `Rules.fog` off every building is
    /// visible, like Java's block renderer setting `wasVisible = true` for
    /// every drawn building — same rule as `render/shadow.rs`). Rust-only seam
    /// for the placement overlay (GAPS #47 residual).
    pub fn placement_blocker(
        &self,
        x: i32,
        y: i32,
        block: BlockId,
    ) -> Option<(i32, i32, i32, bool)> {
        let world = self.world();
        let def = self.content_snapshot.as_ref()?.block(block)?;
        let rules = world
            .get_resource::<mind_core::world::limits::BuildRules>()?
            .clone();
        let table = world.get_resource::<mind_core::world::block::BlockTable>()?;
        // `Build.getEnemyOverlap(block, player.team(), cursorX, cursorY)`; the
        // rules gate `placeRangeCheck` is the caller's (`drawOverlapCheck`).
        let entity = mind_core::world::build::get_enemy_overlap(
            world,
            table,
            &rules,
            def,
            self.sim.live_default_team(),
            x,
            y,
        )?;
        let building = world.get::<BuildingComp>(entity)?;
        // `Building.wasVisible` (`BuildingComp.java:77`) is tracked on the
        // richer `entities::comp::Building` component the runtime also puts on
        // the entity (`BlockInstance::spawn`).
        let was_visible = world
            .get::<mind_core::entities::comp::Building>(entity)
            .is_none_or(|building| building.was_visible);
        let size = table
            .get(building.block)
            .map(|instance| instance.def.size.max(1))
            .unwrap_or(1);
        Some((
            i32::from(building.pos.x()),
            i32::from(building.pos.y()),
            size,
            was_visible || !rules.fog,
        ))
    }

    /// `InputHandler.drawRebuildSelection`'s derelict half
    /// (`InputHandler.java:1742-1752`): the deduped team-0 ("derelict")
    /// buildings inside `area` that `canRepairDerelict` accepts, as
    /// `(origin_x, origin_y, block, size, offset)` corner draw inputs.
    ///
    /// The scan/dedupe/predicate order is the Godot-free
    /// [`mind_core::input::placement::rebuild_selection_tiles`]; `area` is the
    /// unbounded `Placement.normalizeArea` of the B-selection. Rust-only seam
    /// for the placement overlay (GAPS #47 residual).
    pub fn rebuild_highlight_tiles(
        &self,
        area: mind_core::input::placement::NormalizeResult,
    ) -> Vec<(i32, i32, BlockId, i32, f32)> {
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let ecs = self.world();
        let grid = &self.sim.grid;
        let origins = mind_core::input::placement::rebuild_selection_tiles(
            area,
            |x, y| {
                let entity = grid.tiles.getn(x, y).and_then(|tile| tile.build)?;
                let building = ecs.get::<BuildingComp>(entity)?;
                Some((i32::from(building.pos.x()), i32::from(building.pos.y())))
            },
            |x, y| self.sim.derelict_repair_plan(x, y).is_some(),
        );
        origins
            .into_iter()
            .filter_map(|(x, y)| {
                let entity = grid.tiles.getn(x, y).and_then(|tile| tile.build)?;
                let building = ecs.get::<BuildingComp>(entity)?;
                let def = content.block(building.block)?;
                Some((x, y, building.block, def.size, def.offset))
            })
            .collect()
    }

    // ---- G03 T3 build-plan / beam seams (Rust-only) ----

    /// One player-builder draw state (`BuilderComp.drawBuilding`, G03 T3).
    pub fn player_builder_view(&self) -> Option<BuilderView> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
        use mind_core::sim::builder::BuilderPlans;
        let entity = self.sim.player_unit()?;
        let pos = self.world().get::<Pos>(entity)?;
        let type_comp = self.world().get::<UnitTypeComp>(entity)?;
        let def = self.content_snapshot.as_ref()?.unit(type_comp.type_id)?;
        let update_building = self
            .world()
            .get::<BuilderPlans>(entity)
            .map(|plans| plans.update_building)
            .unwrap_or(false);
        let rotation = self
            .world()
            .get::<UnitCore>(entity)
            .map(|core| core.rotation)
            .unwrap_or(0.0);
        Some(BuilderView {
            x: pos.x,
            y: pos.y,
            rotation,
            beam_offset: def.build_beam_offset,
            build_range: def.build_range,
            draw_beam: def.draw_build_beam,
            update_building,
            plans: self.sim.player_build_plans(),
        })
    }

    /// Whether `(x, y)` holds a construct site building `block`
    /// (`InputHandler.planMatches`, `InputHandler.java:1515-1518`).
    pub fn plan_matches(&self, x: i32, y: i32, block: BlockId) -> bool {
        self.construct_at(x, y)
            .is_some_and(|(current, _, _)| current == block)
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Playtest seam: the player team's ghost rebuild plans
    /// (`[{x,y,block,rotation}]`).
    #[func]
    pub fn team_ghost_plans_json(&self) -> GString {
        let team = self.player_team();
        let plans: Vec<serde_json::Value> = self
            .campaign_runtime()
            .and_then(|campaign| {
                campaign
                    .session
                    .teams
                    .get_or_null(mind_core::ecs::TeamId(team))
            })
            .map(|data| {
                data.plans
                    .iter()
                    .map(|plan| {
                        serde_json::json!({
                            "x": plan.x,
                            "y": plan.y,
                            "block": self.sim.block_name_of(plan.block),
                            "rotation": plan.rotation,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(plans))
    }

    /// The player unit's build-plan queue as JSON (`BuilderComp.plans`, head
    /// = active): `[{x, y, rotation, block, breaking, progress, stuck}]`.
    #[func]
    pub fn player_build_plans_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .sim
            .player_build_plans()
            .iter()
            .map(|plan| {
                serde_json::json!({
                    "x": plan.x,
                    "y": plan.y,
                    "rotation": plan.rotation,
                    "block": self.sim.block_name_of(plan.block),
                    "breaking": plan.breaking,
                    "progress": plan.progress,
                    "stuck": plan.stuck,
                })
            })
            .collect();
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(rows))
    }

    /// Clears the player unit's queued build plans (`Binding.clearBuilding`;
    /// PAR-0098 §2a).
    #[func]
    pub fn clear_player_build_plans(&mut self) {
        self.sim.clear_player_build_plans();
    }

    /// `BuilderComp.updateBuilding` (`InputHandler.update`; PAR-0098 §2b).
    #[func]
    pub fn set_player_building_paused(&mut self, paused: bool) {
        self.sim.set_player_building_paused(paused);
    }

    /// Construct block state at `(x, y)` (`ConstructBuild`): `{block,
    /// progress, deconstructing}`; empty when the tile is not under
    /// construction.
    #[func]
    pub fn construct_progress(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        if let Some((block, progress, deconstructing)) = self.construct_at(x, y) {
            out.set("block", &self.sim.block_name_of(block).to_variant());
            out.set("progress", &f64::from(progress).to_variant());
            out.set("deconstructing", &deconstructing.to_variant());
        }
        out
    }
}
