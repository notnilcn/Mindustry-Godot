// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Placement preview at `Layer.plans` (G03 T2): the cursor ghost with its
//! rotation arrow and `Block.drawPlace` range circle (`DesktopInput.drawBottom`,
//! `DesktopInput.java:151-230`), the dragged line-plan ghosts, and the
//! right-drag break rectangle with its `block-select` corner marks
//! (`InputHandler.drawBreakSelection`, `InputHandler.java:1675-1724`).
//!
//! This pass is also the home of the queued-build-plan ghosts (T3). Java
//! draws plans at this layer through `InputHandler.drawBuildPlans`; the band is
//! `Layer.plans` = 85 (`Layer.java`).
//!
//! Every textured draw goes through [`crate::render::canvas::draw_region`]
//! (the negative-height rect compensating the y-flip camera). `Draw.mixcol`
//! is approximated by a multiply tint (`lerp(WHITE, c, amount)`), which is
//! exact for the valid-ghost white and an accepted approximation for the
//! invalid tint — see the commit body.

use godot::builtin::Color;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D, Texture2D, Time};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::registries::blocks::BlockDef;
use mind_core::content::{BlockId, BlockKind, ContentRegistry};
use mind_core::input::PlaceMode;
use mind_core::render::block_region_candidates;
use mind_core::render::drawf;
use mind_core::render::drawf::pal;

use crate::input::MindInput;
use crate::render::blocks::rotates_sprite;
use crate::render::canvas::{self, RegionCache};
use crate::sim_host::{MindSimHost, PlaceRangeKind};

/// `Geometry.d4` (`ARC .../geom/Geometry.java:9-14`).
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
/// `InputHandler.maxLength` (tiles) — the break-rect span clamp.
const MAX_LENGTH: i32 = 100;

/// One resolved draw command. Textures are resolved while the host bind is
/// held but before `base_mut()`; a `None` texture is skipped (missing region).
enum DrawCmd {
    Region {
        texture: Option<Gd<Texture2D>>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: Color,
    },
    DashCircle {
        x: f32,
        y: f32,
        r: f32,
        color: Color,
    },
    Circles {
        x: f32,
        y: f32,
        r: f32,
        color: Color,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        stroke: f32,
        color: Color,
    },
}

/// Placement-preview pass (`Node2D` under the `Layer.plans` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct PlanOverlay {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    input: Option<Gd<MindInput>>,
    regions: RegionCache,
    /// This node as a `Gd<Node2D>` for the region resolver (`RegionCache::get`
    /// takes a plain node reference; `Base<Node2D>` has no deref).
    node: Option<Gd<Node2D>>,
    /// `BlockRenderer.brokenFade` (G03 T3): 1 while placing/breaking.
    broken_fade: f32,
    /// `destroyedblocks` setting (read once per frame).
    destroyed_blocks: bool,
}

#[godot_api]
impl INode2D for PlanOverlay {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            input: None,
            regions: RegionCache::new(),
            node: None,
            broken_fade: 0.0,
            destroyed_blocks: true,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, delta: f64) {
        // The ghost pulse animates while a plan is visible; the pass is cheap
        // (empty without a selection).
        let Some(input) = self.input.clone() else {
            return;
        };
        // `BlockRenderer.drawDestroyed`: `brokenFade` lerps to 1 while the
        // player is placing or breaking, to 0 otherwise (Arc `lerpDelta(..,
        // 0.1)` on the frame delta in ticks).
        let (placing, breaking) = {
            let view = input.bind().placement_view();
            (
                view.block.is_some_and(|block| block != BlockId::AIR),
                view.mode == PlaceMode::Breaking,
            )
        };
        let target = f32::from(u8::from(placing || breaking));
        let delta_ticks = ((delta * 60.0) as f32).max(0.0);
        let t = 1.0 - (1.0f32 - 0.1).powf(delta_ticks);
        self.broken_fade += (target - self.broken_fade) * t;
        self.destroyed_blocks = self.setting_bool("destroyedblocks");
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        // Take the input view and drop the bind before touching the host
        // (the input bridge binds the host in some calls).
        let view = input.bind().placement_view();

        let mut cmds: Vec<DrawCmd> = Vec::new();
        {
            let guard = host.bind();
            let Some(content) = guard.content_registry() else {
                return;
            };
            // `Time.globalTime` in ticks (`MindFx` uses the same clock).
            let ticks = Time::singleton().get_ticks_msec() as f32 * 0.06;
            let pulse = drawf::absin(ticks, 6.0, 0.28);

            // `DesktopInput.update` draws the break selection before the frame
            // passes; push it first so the ghosts draw over it.
            if let Some(rect) = view.break_rect {
                self.push_break_selection(&mut cmds, &guard, content, rect);
            }

            // Queued build plans (`InputHandler.drawBuildPlans`,
            // `InputHandler.java:1438-1473`): drawn first, under the cursor
            // preview (T3).
            if let Some(builder) = guard.player_builder_view() {
                let head = builder.plans.first().cloned();
                for plan in &builder.plans {
                    if plan.progress > 0.01 {
                        continue;
                    }
                    let is_head = head
                        .as_ref()
                        .is_some_and(|h| h.x == plan.x && h.y == plan.y && h.block == plan.block);
                    if is_head && plan.initialized {
                        let (wx, wy) = (plan.x as f32 * 8.0, plan.y as f32 * 8.0);
                        if (wx - builder.x).hypot(wy - builder.y) < builder.build_range {
                            continue;
                        }
                    }
                    if plan.breaking {
                        // `drawBreaking(plan)` on the current tile block.
                        if let Some(def) =
                            tile_block_def(&guard, content, i32::from(plan.x), i32::from(plan.y))
                        {
                            self.push_selected_corners(
                                &mut cmds,
                                i32::from(plan.x),
                                i32::from(plan.y),
                                def.size,
                                def.offset,
                                canvas::color_f4(pal::REMOVE),
                            );
                        }
                    } else {
                        let valid = guard.preview_can_place(
                            i32::from(plan.x),
                            i32::from(plan.y),
                            plan.block,
                            plan.rotation,
                        ) || guard.plan_matches(
                            i32::from(plan.x),
                            i32::from(plan.y),
                            plan.block,
                        );
                        self.push_ghost(
                            &mut cmds,
                            content,
                            i32::from(plan.x),
                            i32::from(plan.y),
                            plan.rotation,
                            plan.block,
                            valid,
                            pulse,
                        );
                    }
                }
            }

            let selected = view.block.filter(|block| *block != BlockId::AIR);
            if view.mode == PlaceMode::Placing && selected.is_some() {
                // Line branch: one ghost per dragged plan, the arrow on the
                // last (`DesktopInput.java:186-195`).
                let last = view.line_plans.len().saturating_sub(1);
                for (index, plan) in view.line_plans.iter().enumerate() {
                    let valid = guard.preview_can_place(plan.x, plan.y, plan.block, plan.rotation);
                    self.push_ghost(
                        &mut cmds,
                        content,
                        plan.x,
                        plan.y,
                        plan.rotation,
                        plan.block,
                        valid,
                        pulse,
                    );
                    if index == last
                        && let Some(block) = selected
                        && content.block(block).is_some_and(draws_arrow)
                    {
                        let valid = guard.preview_can_place(plan.x, plan.y, block, plan.rotation);
                        self.push_arrow(
                            &mut cmds,
                            content,
                            plan.x,
                            plan.y,
                            plan.rotation,
                            block,
                            valid,
                        );
                    }
                }
            } else if let (Some(block), Some((cx, cy))) = (selected, view.cursor) {
                // Cursor branch (`DesktopInput.java:196-221`).
                if let Some(def) = content.block(block) {
                    let rotation = view.rotation;
                    let valid = guard.preview_can_place(cx, cy, block, rotation);
                    if draws_arrow(def) {
                        self.push_arrow(&mut cmds, content, cx, cy, rotation, block, valid);
                    }
                    self.push_ghost(&mut cmds, content, cx, cy, rotation, block, valid, pulse);
                    // `block.drawPlace`: the range circle of turrets, power
                    // nodes and projectors (G03 T2 change 2).
                    if let Some((range, kind, color)) = guard.block_place_range(block) {
                        let color = canvas::color_u32(color);
                        let (wx, wy) = (cx as f32 * 8.0 + def.offset, cy as f32 * 8.0 + def.offset);
                        match kind {
                            PlaceRangeKind::Turret | PlaceRangeKind::Projector => {
                                cmds.push(DrawCmd::DashCircle {
                                    x: wx,
                                    y: wy,
                                    r: range,
                                    color,
                                });
                            }
                            PlaceRangeKind::PowerNode => {
                                cmds.push(DrawCmd::Circles {
                                    x: wx,
                                    y: wy,
                                    r: range,
                                    color,
                                });
                            }
                        }
                    }
                    // `LogicBlock.drawPlace` (`LogicBlock.java:185-189`):
                    // `Drawf.circles(x * tilesize + offset, .., range)` in the
                    // default `Pal.accent`; privileged processors skip it.
                    if let Some(range) = guard.logic_place_range(block) {
                        cmds.push(DrawCmd::Circles {
                            x: cx as f32 * 8.0 + def.offset,
                            y: cy as f32 * 8.0 + def.offset,
                            r: range,
                            color: canvas::color_u32(drawf::to_bits(pal::ACCENT)),
                        });
                    }
                }
            }

            // Rebuild ghosts (`BlockRenderer.drawDestroyed`,
            // `BlockRenderer.java:454-475`): 33%-alpha white ghosts of the
            // team's broken plans while the player is placing/breaking.
            if self.destroyed_blocks
                && self.broken_fade > 0.001
                && (selected.is_some() || view.mode == PlaceMode::Breaking)
            {
                let alpha = 0.33 * self.broken_fade;
                let color = Color::from_rgba(1.0, 1.0, 1.0, alpha);
                for (x, y, rot, block) in guard.team_block_plans() {
                    let Some(def) = content.block(block) else {
                        continue;
                    };
                    let rotation = if rotates_sprite(def) {
                        rot as f32 * 90.0
                    } else {
                        0.0
                    };
                    for name in block_region_candidates(&def.name, &def.region) {
                        if let Some(texture) = self.region(&name) {
                            let (w, h) = RegionCache::natural_size(&texture);
                            cmds.push(DrawCmd::Region {
                                texture: Some(texture),
                                x: x as f32 * 8.0 + def.offset,
                                y: y as f32 * 8.0 + def.offset,
                                w,
                                h,
                                rot: rotation,
                                color,
                            });
                            break;
                        }
                    }
                }
            }
        }

        let mut base = self.base_mut();
        for cmd in &cmds {
            match cmd {
                DrawCmd::Region {
                    texture: Some(texture),
                    x,
                    y,
                    w,
                    h,
                    rot,
                    color,
                } => {
                    canvas::draw_region(&mut base, texture, *x, *y, *w, *h, *rot, *color);
                }
                DrawCmd::Region { texture: None, .. } => {}
                DrawCmd::DashCircle { x, y, r, color } => {
                    canvas::dash_circle(&mut base, *x, *y, *r, *color);
                }
                DrawCmd::Circles { x, y, r, color } => {
                    canvas::circles(&mut base, *x, *y, *r, *color);
                }
                DrawCmd::Rect {
                    x,
                    y,
                    w,
                    h,
                    stroke,
                    color,
                } => {
                    canvas::rect_line(&mut base, *x, *y, *w, *h, *stroke, *color);
                }
            }
        }
    }
}

impl PlanOverlay {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.input = self
            .base()
            .try_get_node_as::<MindInput>("/root/Spine/Input");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }

    /// Resolves an atlas region through the shared cache.
    fn region(&mut self, name: &str) -> Option<Gd<Texture2D>> {
        let node = self.node.clone()?;
        self.regions.get(&node, name)
    }

    /// `settings_get_bool` from `/root/MindUi` (`None` => false).
    fn setting_bool(&self, key: &str) -> bool {
        self.base()
            .try_get_node_as::<Node>("/root/MindUi")
            .map(|mut ui| {
                ui.call("settings_get_bool", &[GString::from(key).to_variant()])
                    .booleanize()
            })
            .unwrap_or(false)
    }

    /// `Drawf.selected` corner marks for one tile (`InputHandler.drawBreaking`).
    fn push_selected_corners(
        &mut self,
        cmds: &mut Vec<DrawCmd>,
        x: i32,
        y: i32,
        size: i32,
        offset: f32,
        color: Color,
    ) {
        let texture = self.region("block-select");
        for (cx, cy, rot) in drawf::selected_corners(x, y, size, offset) {
            cmds.push(DrawCmd::Region {
                texture: texture.clone(),
                x: cx,
                y: cy,
                w: 8.0,
                h: 8.0,
                rot,
                color,
            });
        }
    }

    /// `Block.drawPlan` ghost: pulsing tint, `fullIcon`-chain region at the
    /// plan centre (`drawx() = x * 8 + offset`), rotated for `rotate` blocks
    /// (`Block.java:843-870`).
    #[allow(clippy::too_many_arguments)] // mirrors the Java drawPlan parameter set
    fn push_ghost(
        &mut self,
        cmds: &mut Vec<DrawCmd>,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        rotation: u8,
        block: BlockId,
        valid: bool,
        pulse: f32,
    ) {
        if block == BlockId::AIR {
            return;
        }
        let Some(def) = content.block(block) else {
            return;
        };
        let color = ghost_color(valid, pulse);
        let rot = if rotates_sprite(def) {
            rotation as f32 * 90.0
        } else {
            0.0
        };
        for name in block_region_candidates(&def.name, &def.region) {
            if let Some(texture) = self.region(&name) {
                let (w, h) = RegionCache::natural_size(&texture);
                cmds.push(DrawCmd::Region {
                    texture: Some(texture),
                    x: x as f32 * 8.0 + def.offset,
                    y: y as f32 * 8.0 + def.offset,
                    w,
                    h,
                    rot,
                    color,
                });
                return;
            }
        }
    }

    /// `InputHandler.drawArrow` (`InputHandler.java:2405-2427`): the
    /// `place-arrow` region at the block's rotation edge, back copy 1 px below.
    #[allow(clippy::too_many_arguments)] // mirrors the Java drawArrow parameter set
    fn push_arrow(
        &mut self,
        cmds: &mut Vec<DrawCmd>,
        content: &ContentRegistry,
        x: i32,
        y: i32,
        rotation: u8,
        block: BlockId,
        valid: bool,
    ) {
        let Some(def) = content.block(block) else {
            return;
        };
        let trns = (def.size / 2) as f32 * 8.0;
        let (dx, dy) = D4[(rotation % 4) as usize];
        let offx = x as f32 * 8.0 + def.offset + dx as f32 * trns;
        let offy = y as f32 * 8.0 + def.offset + dy as f32 * trns;
        let Some(texture) = self.region("place-arrow") else {
            return;
        };
        let (w, h) = RegionCache::natural_size(&texture);
        let rot = rotation as f32 * 90.0 - 90.0;
        let back = if valid {
            pal::ACCENT_BACK
        } else {
            pal::REMOVE_BACK
        };
        let front = if valid { pal::ACCENT } else { pal::REMOVE };
        cmds.push(DrawCmd::Region {
            texture: Some(texture.clone()),
            x: offx,
            y: offy - 1.0,
            w,
            h,
            rot,
            color: canvas::color_f4(back),
        });
        cmds.push(DrawCmd::Region {
            texture: Some(texture),
            x: offx,
            y: offy,
            w,
            h,
            rot,
            color: canvas::color_f4(front),
        });
    }

    /// `InputHandler.drawBreakSelection` (`InputHandler.java:1675-1724`):
    /// `block-select` corners on every building tile in the normalized
    /// rectangle (deduped per building), then the two rectangles.
    fn push_break_selection(
        &mut self,
        cmds: &mut Vec<DrawCmd>,
        host: &MindSimHost,
        content: &ContentRegistry,
        rect: ((i32, i32), (i32, i32)),
    ) {
        // `Placement.normalizeArea(.., snap=false, maxLength=100)`: clamp each
        // axis, then order.
        let clamp = |start: i32, end: i32| -> i32 {
            if (end - start).abs() > MAX_LENGTH {
                start + (end - start).signum() * MAX_LENGTH
            } else {
                end
            }
        };
        let end_x = clamp(rect.0.0, rect.1.0);
        let end_y = clamp(rect.0.1, rect.1.1);
        let (x1, x2) = (rect.0.0.min(end_x), rect.0.0.max(end_x));
        let (y1, y2) = (rect.0.1.min(end_y), rect.0.1.max(end_y));

        let select_tex = self.region("block-select");
        let remove = canvas::color_f4(pal::REMOVE);
        let mut seen = Vec::new();
        for x in x1..=x2 {
            for y in y1..=y2 {
                if !host.preview_can_break(x, y) {
                    continue;
                }
                if !host.grid().tiles.in_bounds(x, y) {
                    continue;
                }
                let tile = host.grid().tile(x, y);
                // Java draws only `world.tileBuilding` tiles (a build entity).
                let Some(entity) = tile.build else {
                    continue;
                };
                // One corner set per building (Java draws one per covered tile;
                // deduping keeps multiblocks readable — documented deviation).
                if seen.contains(&entity) {
                    continue;
                }
                seen.push(entity);
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                for (cx, cy, rot) in drawf::selected_corners(x, y, def.size, def.offset) {
                    cmds.push(DrawCmd::Region {
                        texture: select_tex.clone(),
                        x: cx,
                        y: cy,
                        w: 8.0,
                        h: 8.0,
                        rot,
                        color: remove,
                    });
                }
            }
        }

        // `Lines.rect(x, y - 1, ..)` in `Pal.removeBack`, then `Pal.remove`
        // (`normalizeDrawArea(air, ..)` = `min * 8 - 4` .. `max * 8 + 4`).
        let rx = x1 as f32 * 8.0 - 4.0;
        let ry = y1 as f32 * 8.0 - 4.0;
        let rw = (x2 - x1) as f32 * 8.0 + 8.0;
        let rh = (y2 - y1) as f32 * 8.0 + 8.0;
        cmds.push(DrawCmd::Rect {
            x: rx,
            y: ry - 1.0,
            w: rw,
            h: rh,
            stroke: 2.0,
            color: canvas::color_f4(pal::REMOVE_BACK),
        });
        cmds.push(DrawCmd::Rect {
            x: rx,
            y: ry,
            w: rw,
            h: rh,
            stroke: 2.0,
            color: remove,
        });
    }
}

/// Java `Block.rotate && Block.drawArrow`; `BlockDef` has no `rotate` field, so
/// the port proxies `rotate` with the sprite-rotation set plus the directional
/// kinds whose Java classes set both flags (turrets set `drawArrow = false`,
/// `Turret.java:169-171`).
fn draws_arrow(def: &BlockDef) -> bool {
    rotates_sprite(def)
        || matches!(
            def.kind,
            BlockKind::DirectionalUnloader
                | BlockKind::Duct
                | BlockKind::DuctBridge
                | BlockKind::OverflowDuct
                | BlockKind::DuctRouter
                | BlockKind::StackRouter
                | BlockKind::UnitFactory
                | BlockKind::Reconstructor
                | BlockKind::PayloadConveyor
                | BlockKind::PayloadRouter
        )
}

/// `Draw.mixcol(!valid ? Pal.breakInvalid : white, (!valid ? 0.4 : 0.24) +
/// absin(time, 6, 0.28))` as a multiply tint: `lerp(white, c, amount)`.
fn ghost_color(valid: bool, pulse: f32) -> Color {
    if valid {
        return Color::from_rgba(1.0, 1.0, 1.0, 1.0);
    }
    let amount = 0.4 + pulse;
    let c = canvas::color_f4(pal::BREAK_INVALID);
    Color::from_rgba(
        1.0 + (c.r - 1.0) * amount,
        1.0 + (c.g - 1.0) * amount,
        1.0 + (c.b - 1.0) * amount,
        1.0,
    )
}

/// Block definition of the tile at `(x, y)` (`None` for air/out of bounds).
fn tile_block_def<'a>(
    host: &MindSimHost,
    content: &'a ContentRegistry,
    x: i32,
    y: i32,
) -> Option<&'a BlockDef> {
    if !host.grid().tiles.in_bounds(x, y) {
        return None;
    }
    content.block(host.grid().tile(x, y).block)
}
