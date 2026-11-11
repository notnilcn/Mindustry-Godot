// SPDX-License-Identifier: GPL-3.0-only

//! Light-source collection for the light pass.
//! Upstream: core/src/mindustry/core/Renderer.java, core/src/mindustry/graphics/LightRenderer.java

use super::*;

/// Laser/bridge visible-rect growth: the longest link is a surge tower's
/// 40 tiles = 320 px (`Blocks.java:2502`; G03 T7 pitfall).
pub(super) const POWER_GROW: f32 = 320.0;

/// Bridge links reach `range 4` tiles (`Blocks.java`); 64 px covers it.
pub(super) const BRIDGE_GROW: f32 = 64.0;

/// Light sources are stamped from outside the view too (`LightRenderer`
/// quads), so grow the collection rect by the largest radius (G03 T12).
pub(super) const LIGHT_GROW: f32 = 200.0;

/// One `LightRenderer.add` source (G03 T12): world centre, radius, RGBA
/// colour and the per-source opacity the pass multiplies into the alpha.
pub struct LightSource {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// World-pixel radius.
    pub radius: f32,
    /// RGBA colour `[r, g, b, a]`.
    pub color: [f32; 4],
    /// Accumulated-alpha multiplier (`LightRenderer.add` opacity).
    pub opacity: f32,
}

/// Unpacks a packed `rgba8888` into float RGBA.
pub(super) fn rgba_f32(packed: u32) -> [f32; 4] {
    [
        ((packed >> 24) & 0xff) as f32 / 255.0,
        ((packed >> 16) & 0xff) as f32 / 255.0,
        ((packed >> 8) & 0xff) as f32 / 255.0,
        (packed & 0xff) as f32 / 255.0,
    ]
}

/// Building sprite centre (`anchor * 8 + Block.offset`; Java `Building.x`).
pub(super) fn building_center(
    x: i32,
    y: i32,
    def: &mind_core::content::registries::blocks::BlockDef,
) -> (f32, f32) {
    let anchor_x = (x + (def.size - 1) / 2) as f32 * 8.0 + def.offset;
    let anchor_y = (y + (def.size - 1) / 2) as f32 * 8.0 + def.offset;
    (anchor_x, anchor_y)
}

/// `BulletType.drawLight`'s default circle light for a bullet def, or `None`
/// when the kind overrides it with a line light.
///
/// Java's default body is
/// `if(lightOpacity <= 0f || lightRadius <= 0f) return; Drawf.light(b,
/// lightRadius, lightColor, lightOpacity)` (`BulletType.java:708-711`), with
/// `lightRadius` derived in `init` (`Math.max(18, hitSize * 5f)`,
/// `BulletType.java:862-863`). Four beam kinds replace it with
/// `Drawf.light(x, y, x2, y2, stroke, tint, alpha)` in `draw` and empty
/// `drawLight` bodies (`LaserBulletType.java:121-128`,
/// `ContinuousLaserBulletType.java:70-76`,
/// `ContinuousFlameBulletType.java:91-104`, `SapBulletType.java:58-59`); the
/// host stamps those as `LightRenderer.line` primitives in `render::light`, so
/// emitting the default circle too would double-light the beam.
pub(super) fn bullet_circle_light(
    def: &mind_core::content::registries::bullets::BulletDef,
) -> Option<(f32, [f32; 4], f32)> {
    use mind_core::content::registries::bullets::BulletKind;
    if matches!(
        def.kind,
        BulletKind::Laser
            | BulletKind::ContinuousLaser
            | BulletKind::ContinuousFlame
            | BulletKind::Sap
    ) {
        return None;
    }
    if def.light_opacity <= 0.0 || def.light_radius <= 0.0 {
        return None;
    }
    let color = def.light_color;
    Some((
        def.light_radius,
        [color.r, color.g, color.b, color.a],
        def.light_opacity,
    ))
}

/// `Block.drawEnvironmentLight` (`Block.java:493-495`):
/// `Drawf.light(tile.worldx(), tile.worldy(), lightRadius, lightColor,
/// lightColor.a)` — the per-block environment light from the `BlockDef`
/// fields (PAR-0467). `x`/`y` are the Java anchor (building centre for a
/// building, tile centre for a floor).
pub(super) fn environment_light(
    def: &mind_core::content::registries::blocks::BlockDef,
    x: f32,
    y: f32,
) -> LightSource {
    let color = def.light_color;
    LightSource {
        x,
        y,
        radius: def.light_radius,
        color: [color.r, color.g, color.b, color.a],
        // `drawEnvironmentLight` passes `lightColor.a` as the opacity.
        opacity: color.a,
    }
}

/// `BlockRenderer.java:843-855`: floor/overlay environment lights.
///
/// Java emits one light per visible tile whose block does not obstruct light:
/// the overlay when its floor lets light through, then the floor itself
/// (`forceDrawLight` is not carried on `BlockDef`; every vanilla floor that
/// sets it also clears `obstructsLight`, so the plain predicate is
/// equivalent). The light sits at `Tile.worldx()/worldy()` — the tile centre
/// in Mindustry's coordinate convention (tile `x` spans
/// `[x*8 - 4, x*8 + 4)`, `World.toTile` rounds).
pub(super) fn floor_light_sources(
    grid: &WorldGrid,
    content: &ContentRegistry,
    view: &CameraView,
) -> Vec<LightSource> {
    let size = mind_core::config::TILESIZE as f32;
    let grow = size * 2.0;
    let [bx, by, bw, bh] = view.bounds();
    let min_x = ((bx - grow) / size).floor().max(0.0) as i32;
    let min_y = ((by - grow) / size).floor().max(0.0) as i32;
    let max_x = (((bx + bw + grow) / size).ceil() as i32).min(grid.width() - 1);
    let max_y = (((by + bh + grow) / size).ceil() as i32).min(grid.height() - 1);
    let mut out = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let tile = grid.tile(x, y);
            // `if(!tile.block().obstructsLight)` — `air` clears the flag
            // (`AirBlock extends Floor`).
            if content
                .block(tile.block)
                .is_none_or(|block| block.obstructs_light)
            {
                continue;
            }
            let floor = content.block(tile.floor);
            let overlay = content.block(tile.overlay);
            let (lx, ly) = (x as f32 * size, y as f32 * size);
            // `if(!floor.obstructsLight && overlay.emitLight)`.
            if let (Some(floor), Some(overlay)) = (floor, overlay)
                && !floor.obstructs_light
                && overlay.emit_light
            {
                out.push(environment_light(overlay, lx, ly));
            }
            // `if(floor.forceDrawLight || (!overlay.obstructsLight &&
            // floor.emitLight))`.
            if let Some(floor) = floor
                && overlay.is_none_or(|overlay| !overlay.obstructs_light)
                && floor.emit_light
            {
                out.push(environment_light(floor, lx, ly));
            }
        }
    }
    out
}

impl MindSimHost {
    /// `LightRenderer` sources in the (grown) camera rect (G03 T12): cores
    /// (`CoreBlock.java:158,739-741`), illuminators (`LightBlock.java:40,105,
    /// 126-129`), static `emitLight` blocks (`Block.lightRadius` default 60),
    /// units when `rules.unitLight` (`UnitType.java:954,1800-1804`) and bullets
    /// (`BulletType.drawLight` defaults `max(18, hitSize*5)`,
    /// `Pal.powerLight`, 0.3 — `BulletDef` has no light fields yet).
    pub fn light_sources(&self, rect: (f32, f32, f32, f32)) -> Vec<LightSource> {
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let rules = self.render_rules_or_default();
        let view = self.grown_view(rect, LIGHT_GROW);
        let mut out = Vec::new();
        self.block_lights(content, &view, &mut out);
        // `BlockRenderer.java:846-855`: floor/overlay environment lights for
        // tiles whose block does not obstruct light (PAR-0467).
        out.extend(floor_light_sources(&self.sim.grid, content, &view));
        self.hazard_lights(rect, &mut out);
        self.unit_lights(content, &rules, &mut out);
        self.bullet_lights(content, &mut out);
        out
    }

    /// Block light sources: cores (`CoreBlock.java:158,739-741`), illuminators
    /// (`LightBlock.java:40,105,126-129`) and static `emitLight` blocks
    /// (`Block.lightRadius` default 60), in `visible_blocks` order.
    fn block_lights(
        &self,
        content: &ContentRegistry,
        view: &CameraView,
        out: &mut Vec<LightSource>,
    ) {
        use mind_core::render::drawf::pal;
        use mind_core::world::behavior::light::LightState;
        for tile in visible_blocks(&self.sim.grid, content, view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            // `BlockRenderer.indexBlock` indexes `tile.isCenter()` only and
            // `procLights.add(tile.build.pos())` dedupes multiblocks, so a
            // placed building contributes exactly one light at its centre
            // (PAR-0467: the covered proxy tiles used to emit shifted copies).
            if let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build
                && let Some(building) = self
                    .world()
                    .get::<mind_core::entities::comp::Building>(entity)
                && building.tile != TilePos::new(tile.x as i16, tile.y as i16)
            {
                continue;
            }
            let (cx, cy) = building_center(tile.x, tile.y, def);
            match def.kind {
                BlockKind::CoreBlock => {
                    // `CoreBlock.java:739-741`: `0.65 + absin(time, 20, 0.1)`
                    // over `Pal.accent` at the block's `lightRadius`
                    // (`CoreBlock.java:158`, PAR-0467).
                    let ticks = godot::classes::Time::singleton().get_ticks_msec() as f32 * 0.06;
                    out.push(LightSource {
                        x: cx,
                        y: cy,
                        radius: def.light_radius,
                        color: pal::ACCENT,
                        opacity: 0.65 + mind_core::render::drawf::absin(ticks, 20.0, 0.1),
                    });
                }
                BlockKind::LightBlock => {
                    let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                        continue;
                    };
                    let state = self.world().get::<LightState>(entity);
                    let color = state.map_or(0xffd3_7fff_u32, |s| s.color);
                    let smooth = state.map_or(1.0, |s| s.smooth_time).min(2.0);
                    out.push(LightSource {
                        x: cx,
                        y: cy,
                        // `LightBlock.java:39,128`: `lightRadius * min(smoothTime,
                        // 2f)` (`illuminator` 140 * 2.5 = 350, PAR-0467).
                        radius: def.light_radius * smooth,
                        color: rgba_f32(color),
                        opacity: 0.75,
                    });
                }
                _ if def.emit_light => out.push(environment_light(def, cx, cy)),
                _ => {}
            }
        }
    }

    /// Fires and puddles light the map (`FireComp.draw` -> `Drawf.light`,
    /// `Liquid.drawPuddle`; H03 T7).
    fn hazard_lights(&self, rect: (f32, f32, f32, f32), out: &mut Vec<LightSource>) {
        use mind_core::render::drawf::pal;
        let ticks = godot::classes::Time::singleton().get_ticks_msec() as f32 * 0.06;
        for fire in self.fire_draws(rect) {
            out.push(LightSource {
                x: fire.x,
                y: fire.y,
                radius: 50.0 + mind_core::render::drawf::absin(ticks, 5.0, 5.0),
                color: pal::LIGHT_FLAME,
                opacity: 0.6 * fire.alpha,
            });
        }
        for puddle in self.puddle_draws(rect) {
            if let Some((color, opacity)) = puddle.light {
                out.push(LightSource {
                    x: puddle.x,
                    y: puddle.y,
                    radius: 30.0 * puddle.f,
                    color,
                    opacity,
                });
            }
        }
    }

    /// Unit lights when `rules.unitLight` (`UnitType.java:954,1800-1804`).
    fn unit_lights(&self, content: &ContentRegistry, rules: &Rules, out: &mut Vec<LightSource>) {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        if rules.unit_light
            && let Some(runtime) = self.sim.unit_runtime()
        {
            for &entity in &runtime.units {
                let Some(pos) = self.world().get::<Pos>(entity) else {
                    continue;
                };
                let Some(type_comp) = self.world().get::<UnitTypeComp>(entity) else {
                    continue;
                };
                let Some(def) = content.unit(type_comp.type_id) else {
                    continue;
                };
                let radius = if def.light_radius > 0.0 {
                    def.light_radius
                } else {
                    60.0f32.max(def.hit_size * 2.3)
                };
                let opacity = if def.light_opacity > 0.0 {
                    def.light_opacity
                } else {
                    0.6
                };
                let c = def.light_color;
                out.push(LightSource {
                    x: pos.x,
                    y: pos.y,
                    radius,
                    color: [c.r, c.g, c.b, c.a],
                    opacity,
                });
            }
        }
    }

    /// Bullet lights (`BulletType.drawLight` defaults `max(18, hitSize*5)`,
    /// `Pal.powerLight`, 0.3 — `BulletDef` has no light fields yet).
    fn bullet_lights(&self, content: &ContentRegistry, out: &mut Vec<LightSource>) {
        use mind_core::combat::bullet::Bullet;
        use mind_core::entities::comp::Pos;
        if let Some(runtime) = self.sim.unit_runtime() {
            for &entity in &runtime.bullets {
                let Some(pos) = self.world().get::<Pos>(entity) else {
                    continue;
                };
                let Some(bullet) = self.world().get::<Bullet>(entity) else {
                    continue;
                };
                if bullet.finished() {
                    continue;
                }
                // `BulletType.drawLight` (`BulletType.java:708-711`) reads the
                // def's light fields; the four beam kinds override it with a
                // line light that `render::light` stamps separately, so they
                // must not also cast the default circle.
                let Some(def) = content.bullet(bullet.def) else {
                    continue;
                };
                let Some((radius, color, opacity)) = bullet_circle_light(def) else {
                    continue;
                };
                out.push(LightSource {
                    x: pos.x,
                    y: pos.y,
                    radius,
                    color,
                    opacity,
                });
            }
        }
    }

    /// `rules.borderDarkness` for the darkness/minimap passes (G03 T11):
    /// live campaign rules, else the last `load_sector` rules, else `true`.
    pub fn border_darkness_enabled(&self) -> bool {
        self.campaign_runtime()
            .map(|runtime| runtime.session.rules.border_darkness)
            .or_else(|| {
                self.sector_rules
                    .as_ref()
                    .map(|rules| rules.border_darkness)
            })
            .unwrap_or(true)
    }

    /// `tile.getLinkedTiles` of the building covering world `(wx, wy)` with
    /// each tile's floor (`CoreBuild.updateLaunch`, `CoreBlock.java:665-669`):
    /// `(worldx, worldy, floor block id, registry mapColor)`. The registry
    /// `mapColor` is the `block_colors.png` entry for the floor id
    /// (`ContentLoader.loadColors` in the content boot). Empty without a
    /// building.
    pub fn linked_tile_floors(
        &self,
        wx: f32,
        wy: f32,
    ) -> Vec<(f32, f32, usize, Option<mind_core::content::Rgba>)> {
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        // `World.toTile`: `Math.round(coord / tilesize)`.
        let tx = (wx / TILESIZE_F).round() as i32;
        let ty = (wy / TILESIZE_F).round() as i32;
        let Some(entity) = self.tile_at(tx, ty).and_then(|tile| tile.build) else {
            return Vec::new();
        };
        let Some(building) = self
            .world()
            .get::<mind_core::entities::comp::building::Building>(entity)
        else {
            return Vec::new();
        };
        let size = content
            .block(building.block)
            .map(|def| def.size.max(1))
            .unwrap_or(1);
        // `Block.sizeOffset = -(size - 1) / 2`.
        let offset = -(size - 1) / 2;
        let (cx, cy) = (i32::from(building.tile.0), i32::from(building.tile.1));
        let mut out = Vec::with_capacity((size * size) as usize);
        for dx in 0..size {
            for dy in 0..size {
                let (x, y) = (cx + dx + offset, cy + dy + offset);
                let Some(tile) = self.tile_at(x, y) else {
                    continue;
                };
                let color = content.block(tile.floor).and_then(|def| def.map_color);
                out.push((
                    x as f32 * TILESIZE_F,
                    y as f32 * TILESIZE_F,
                    tile.floor.raw() as usize,
                    color,
                ));
            }
        }
        out
    }

    // ---- G03 T7 power/bridge link seams (Rust-only) ----

    /// Power-node laser links in the (grown) visible rect (`PowerNode.draw`,
    /// `PowerNode.java:496-512`): each node pair once (the other node's entity
    /// index must be below ours), derelicts skipped.
    pub fn power_lasers(&self, rect: (f32, f32, f32, f32)) -> Vec<LaserLink> {
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let view = self.grown_view(rect, POWER_GROW);
        let mut out = Vec::new();
        for tile in visible_blocks(&self.sim.grid, content, &view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            // `PowerSource extends PowerNode` (`PowerSource.java:6`): it draws
            // its node lasers too.
            if !matches!(def.kind, BlockKind::PowerNode | BlockKind::PowerSource) {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                continue;
            };
            let Some(module) = self.world().get::<PowerModule>(entity) else {
                continue;
            };
            let satisfaction = self
                .world()
                .get_resource::<PowerGrids>()
                .and_then(|grids| grids.graph(module.graph))
                .map(|graph| graph.get_satisfaction())
                .unwrap_or(1.0);
            let (x1, y1) = building_center(tile.x, tile.y, def);
            for packed in &module.links {
                let (lx, ly) = unpack(*packed);
                if !self.sim.grid.tiles.in_bounds(lx as i32, ly as i32) {
                    continue;
                }
                let Some(other) = self.sim.grid.entity_at(TilePos::new(lx, ly)) else {
                    continue;
                };
                if other == entity {
                    continue;
                }
                if self.building_team(lx as i32, ly as i32) == Some(0) {
                    continue;
                }
                let Some(other_def) = self
                    .world()
                    .get::<BuildingComp>(other)
                    .and_then(|comp| content.block(comp.block))
                else {
                    continue;
                };
                // Each node pair once (`link.id >= id` skip on the other node).
                if matches!(
                    other_def.kind,
                    BlockKind::PowerNode | BlockKind::PowerSource
                ) && other.index() >= entity.index()
                {
                    continue;
                }
                let (x2, y2) = building_center(lx as i32, ly as i32, other_def);
                out.push(LaserLink {
                    x1,
                    y1,
                    size1: def.size,
                    x2,
                    y2,
                    size2: other_def.size,
                    satisfaction,
                    block: tile.block,
                });
            }
        }
        out
    }

    /// `BeamNode.BeamNodeBuild.draw` links (`BeamNode.java:207-233`; I02 T5):
    /// one row per drawn beam, already inset by `tilesize / 2` on the source
    /// (by `size * d4`) and destination (by `d4`) sides, with the source
    /// graph's satisfaction for `setupColor`.
    pub fn beam_node_lasers(&self, rect: (f32, f32, f32, f32)) -> Vec<BeamLaserLink> {
        use mind_core::entities::comp::Building;
        use mind_core::world::blocks::power::nodes::BeamNodeState;
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let view = self.grown_view(rect, POWER_GROW);
        let mut out = Vec::new();
        for tile in visible_blocks(&self.sim.grid, content, &view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if def.kind != BlockKind::BeamNode {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                continue;
            };
            // `team == Team.derelict` never draws (`BeamNode.java:213`).
            if self.building_team(tile.x, tile.y) == Some(0) {
                continue;
            }
            let Some(state) = self.world().get::<BeamNodeState>(entity).copied() else {
                continue;
            };
            let satisfaction = self
                .world()
                .get::<PowerModule>(entity)
                .and_then(|module| {
                    self.world()
                        .get_resource::<PowerGrids>()
                        .and_then(|grids| grids.graph(module.graph))
                        .map(|graph| graph.get_satisfaction())
                })
                .unwrap_or(1.0);
            let (x1, y1) = building_center(tile.x, tile.y, def);
            // `Geometry.d4`: right, up, left, down.
            const D4: [(f32, f32); 4] = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)];
            for (i, link) in state.links.iter().enumerate() {
                let Some(other) = *link else {
                    continue;
                };
                let Some(other_build) = self.world().get::<Building>(other) else {
                    continue;
                };
                let Some(other_def) = content.block(other_build.block) else {
                    continue;
                };
                let (ox, oy) = building_center(
                    i32::from(other_build.tile.x()),
                    i32::from(other_build.tile.y()),
                    other_def,
                );
                // Don't draw lasers for adjacent blocks (`dst > 1 + size/2`).
                let dst = (i32::from(other_build.tile.x()) - tile.x)
                    .abs()
                    .max((i32::from(other_build.tile.y()) - tile.y).abs());
                if dst <= 1 + def.size / 2 {
                    continue;
                }
                // Beam-to-beam dedupe (`BeamNode.java:219-221`): the larger
                // range draws; equal ranges break the tie by entity index.
                if other_def.kind == BlockKind::BeamNode {
                    let other_range = self
                        .world()
                        .get::<BeamNodeState>(other)
                        .map(|state| state.range)
                        .unwrap_or(0);
                    if state.range < other_range
                        || (state.range == other_range && entity.index() < other.index())
                    {
                        continue;
                    }
                }
                let (dx, dy) = D4[i % 4];
                let poff = 4.0;
                out.push(BeamLaserLink {
                    x1: x1 + poff * def.size as f32 * dx,
                    y1: y1 + poff * def.size as f32 * dy,
                    x2: ox - poff * dx,
                    y2: oy - poff * dy,
                    satisfaction,
                    block: tile.block,
                });
            }
        }
        out
    }

    /// Item-bridge links in the (grown) visible rect (`ItemBridge.draw`,
    /// `ItemBridge.java:384-432`).
    pub fn bridge_links(&self, rect: (f32, f32, f32, f32)) -> Vec<BridgeLink> {
        use mind_core::world::blocks::distribution::item_bridge::ItemBridgeBuild;
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let view = self.grown_view(rect, BRIDGE_GROW);
        let mut out = Vec::new();
        for tile in visible_blocks(&self.sim.grid, content, &view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if !matches!(
                def.kind,
                BlockKind::ItemBridge | BlockKind::BufferedItemBridge
            ) {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                continue;
            };
            let Some(bridge) = self.world().get::<ItemBridgeBuild>(entity) else {
                continue;
            };
            if bridge.link < 0 {
                continue;
            }
            let (lx, ly) = unpack(bridge.link);
            if !self.sim.grid.tiles.in_bounds(lx as i32, ly as i32) {
                continue;
            }
            let Some(other) = self.sim.grid.entity_at(TilePos::new(lx, ly)) else {
                continue;
            };
            let linked_same = self
                .world()
                .get::<BuildingComp>(other)
                .is_some_and(|comp| comp.block == tile.block);
            if !linked_same {
                continue;
            }
            out.push(BridgeLink {
                x: tile.x,
                y: tile.y,
                ox: lx as i32,
                oy: ly as i32,
                warmup: bridge.warmup,
                time: bridge.time,
                has_power: def.has_power,
                block: tile.block,
            });
        }
        out
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Player-team colour as packed `rgba8888` (landing flame circles,
    /// `Draw.color(team.color)`, `CoreBlock.java:505-512`; G03 T18): the real
    /// `Team.color` (sharded = `Pal.accent` `ffd37f`).
    #[func]
    pub fn player_team_color(&self) -> i64 {
        i64::from(
            mind_core::game::team::Team::get(self.player_team())
                .color
                .to_rgba8888(),
        )
    }
}
