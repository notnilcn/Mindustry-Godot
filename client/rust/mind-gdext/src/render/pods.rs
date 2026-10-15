// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch pods and landing-pad arrivals (H03 T13).
//!
//! Java draws the in-flight pod in three layers: the engine glow + 4 flares at
//! `Layer.effect + 0.001` (`Fill.light` + `Drawf.tri`), the pod region at
//! `Layer.weather - 1` (with `Drawf.shadow` for the landing descent) and a
//! dark copy at `Layer.flyingUnit + 1` offset `trns(225, ...)`
//! (`LaunchPayloadComp.draw`, `LaunchPad.java:227-280`;
//! `LandingPadBuild.draw`, `LandingPad.java:205-265`). Idle launch pads draw
//! their pod, a shadow and four corner progress lights
//! (`LaunchPadBuild.draw`, `LaunchPad.java:105-135`).
//!
//! One `PodsRenderer` instance per band/mode (`render.rs` creates them):
//! `Engine` (Layer.effect), `Pod` (Layer.weather, under the weather nodes),
//! `Shadow` (Layer.flyingUnit, above the unit sprites) and `PadStatic`
//! (Layer.block, above the block meshes).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D, Texture2D};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use mind_core::content::BlockId;
use mind_core::render::drawf;
use mind_core::render::drawf::pal;

use crate::render::canvas::{self, RegionCache};
use crate::sim_host::{MindSimHost, PadDraw};

/// `Pal.engine` (`ffbb64`).
const ENGINE: u32 = 0xffbb_64ff;
/// `LaunchPad.lightColor` default (`eab678`).
const LIGHT_COLOR: u32 = 0xeab6_78ff;
/// `Pal.metalGrayDark` (the unlit corner-light colour).
const METAL_GRAY_DARK: u32 = 0x6e70_80ff;
/// `LaunchPad.lightSteps` / `lightStep` defaults.
const LIGHT_STEPS: i32 = 3;
const LIGHT_STEP: f32 = 1.0;
/// `Geometry.d8edge` (`Geometry.java:35-40`).
const D8EDGE: [(f32, f32); 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

/// Which slice of the pod draws this node runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PodMode {
    /// Engine glow + flares (`Layer.effect + 0.001`).
    Engine,
    /// Pod region at `Layer.weather - 1` (+ landing shadows).
    Pod,
    /// Dark pod copy at `Layer.flyingUnit + 1`.
    Shadow,
    /// Pad pod, shadow and corner lights at `Layer.block`.
    PadStatic,
}

/// A colour with a replaced alpha (`Color.with_alpha` does not exist).
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha)
}

/// Pod/pad draw pass (`Node2D` in its band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct PodsRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    regions: RegionCache,
    /// `build_floor` assigns the mode before the node enters the tree.
    mode: PodMode,
}

#[godot_api]
impl INode2D for PodsRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            regions: RegionCache::new(),
            mode: PodMode::Engine,
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

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // Pods move every tick; the cooldown/lights change without a
            // world-revision bump.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let rows = {
            let mut bound = host.clone();
            bound.bind_mut().pad_draws()
        };
        if rows.is_empty() {
            return;
        }
        let mode = self.mode;
        let node = self.to_gd().upcast::<Node2D>();
        let mut cmds: Vec<Cmd> = Vec::new();
        for row in rows {
            match row {
                PadDraw::Launch {
                    id,
                    x,
                    y,
                    time,
                    lifetime,
                    block,
                } => {
                    let fin = (time / lifetime.max(f32::EPSILON)).clamp(0.0, 1.0);
                    // `alpha = fout(pow5Out)` (`LaunchPad.java:228`): the pod
                    // starts opaque and fades out along the flight.
                    let alpha = drawf::pow5_out(1.0 - fin);
                    let scale = (1.0 - alpha) * 1.3 + 1.0;
                    let (cx, cy, rotation) = drawf::launch_pod_path(
                        x,
                        y,
                        fin,
                        drawf::random_seed_range(i64::from(id) + 3, 4.0),
                        drawf::random_seed_range(i64::from(id) + 2, 30.0),
                        drawf::random_seed_range(i64::from(id), 50.0),
                    );
                    let rad = 0.2 + drawf::slope(fin);
                    match mode {
                        PodMode::Engine => {
                            push_engine(&mut cmds, cx, cy, alpha, rad, scale, rotation);
                        }
                        PodMode::Pod => {
                            let texture = pod_texture(self, &node, block, "launchpod");
                            cmds.push(Cmd::Region {
                                texture,
                                cx,
                                cy,
                                scale,
                                rotation,
                                tint: with_alpha(Color::WHITE, alpha),
                            });
                        }
                        PodMode::Shadow => {
                            let (ox, oy) = drawf::trns(225.0, drawf::pow3_in(fin) * 250.0);
                            let texture = pod_texture(self, &node, block, "launchpod");
                            cmds.push(Cmd::Region {
                                texture,
                                cx: cx + ox,
                                cy: cy + oy,
                                scale,
                                rotation,
                                tint: Color::from_rgba(0.0, 0.0, 0.0, 0.22 * alpha),
                            });
                        }
                        PodMode::PadStatic => {}
                    }
                }
                PadDraw::Landing {
                    id,
                    x,
                    y,
                    size,
                    arriving,
                    arriving_timer,
                    cooldown,
                    block,
                } => {
                    if arriving.is_some() {
                        let fin = arriving_timer.clamp(0.0, 1.0);
                        let fout = 1.0 - fin;
                        let alpha = drawf::pow5_out(fin);
                        let scale = (1.0 - alpha) * 1.3 + 1.0;
                        let (cx, cy, rotation) = drawf::landing_pod_path(
                            x,
                            y,
                            fin,
                            drawf::random_seed_range(i64::from(id) + 2, 30.0),
                            drawf::random_seed_range(i64::from(id), 50.0),
                        );
                        let rad = 0.15 + drawf::pow5_out(drawf::slope(fin));
                        match mode {
                            PodMode::Engine => {
                                push_engine(&mut cmds, cx, cy, alpha, rad, scale, rotation);
                            }
                            PodMode::Pod => {
                                // `Drawf.shadow(cx, cy, size * tilesize, fin)`.
                                cmds.push(Cmd::Shadow {
                                    texture: self.regions.get(&node, "circle-shadow"),
                                    cx,
                                    cy,
                                    size: size as f32 * 8.0,
                                    alpha: 0.4 * fin,
                                });
                                let texture =
                                    pod_texture(self, &node, block, "advanced-launch-pad-pod");
                                cmds.push(Cmd::Region {
                                    texture,
                                    cx,
                                    cy,
                                    scale,
                                    rotation,
                                    tint: with_alpha(Color::WHITE, alpha),
                                });
                            }
                            PodMode::Shadow => {
                                let (ox, oy) = drawf::trns(225.0, drawf::pow3_in(fout) * 250.0);
                                let texture =
                                    pod_texture(self, &node, block, "advanced-launch-pad-pod");
                                cmds.push(Cmd::Region {
                                    texture,
                                    cx: cx + ox,
                                    cy: cy + oy,
                                    scale,
                                    rotation,
                                    tint: Color::from_rgba(0.0, 0.0, 0.0, 0.22 * alpha),
                                });
                            }
                            PodMode::PadStatic => {}
                        }
                    } else if cooldown > 0.0 && mode == PodMode::PadStatic {
                        // `else if(cooldown > 0f)`: shadow, alpha(cooldown),
                        // `mixcol(accent, 1 - cooldown)`, pod at the centre.
                        cmds.push(Cmd::Shadow {
                            texture: self.regions.get(&node, "circle-shadow"),
                            cx: x,
                            cy: y,
                            size: size as f32 * 8.0,
                            alpha: 0.4 * cooldown,
                        });
                        let accent = canvas::color_f4(pal::ACCENT);
                        let mix = 1.0 - cooldown;
                        let tint = with_alpha(
                            Color::from_rgba(
                                1.0 + (accent.r - 1.0) * mix,
                                1.0 + (accent.g - 1.0) * mix,
                                1.0 + (accent.b - 1.0) * mix,
                                1.0,
                            ),
                            cooldown,
                        );
                        let texture = pod_texture(self, &node, block, "advanced-launch-pad-pod");
                        cmds.push(Cmd::Region {
                            texture,
                            cx: x,
                            cy: y,
                            scale: 1.0,
                            rotation: 0.0,
                            tint,
                        });
                    }
                }
                PadDraw::Pad {
                    id: _,
                    x,
                    y,
                    size,
                    progress,
                    block,
                } => {
                    if mode != PodMode::PadStatic {
                        continue;
                    }
                    // `Drawf.shadow(x, y, size * tilesize)` then the pod.
                    cmds.push(Cmd::Shadow {
                        texture: self.regions.get(&node, "circle-shadow"),
                        cx: x,
                        cy: y,
                        size: size as f32 * 8.0,
                        alpha: 0.4,
                    });
                    cmds.push(Cmd::Region {
                        texture: pod_texture(self, &node, block, "launchpod"),
                        cx: x,
                        cy: y,
                        scale: 1.0,
                        rotation: 0.0,
                        tint: Color::WHITE,
                    });
                    // Corner progress lights (`<name>-light`, `lightSteps` x 4).
                    let light = self
                        .block_name(&host, block)
                        .and_then(|name| self.regions.get(&node, &format!("{name}-light")));
                    if let Some(texture) = light {
                        for i in 0..4 {
                            for j in 0..LIGHT_STEPS {
                                let alpha = drawf::curve(
                                    progress,
                                    j as f32 / LIGHT_STEPS as f32,
                                    (j + 1) as f32 / LIGHT_STEPS as f32,
                                );
                                let offset = -(j as f32 - 1.0) * LIGHT_STEP;
                                let (dx, dy) = D8EDGE[(i as usize) % 4];
                                cmds.push(Cmd::LightRegion {
                                    texture: texture.clone(),
                                    cx: x + dx * offset,
                                    cy: y + dy * offset,
                                    rotation: i as f32 * 90.0,
                                    alpha,
                                });
                            }
                        }
                    }
                }
            }
        }
        let mut base = self.base_mut();
        for cmd in cmds {
            match cmd {
                Cmd::Light {
                    x,
                    y,
                    radius,
                    center,
                    edge,
                } => {
                    if radius > 0.0 {
                        canvas::fill_light(&mut base, x, y, 10, radius, center, edge);
                    }
                }
                Cmd::Tri { tri, color } => {
                    canvas::fill_tri(&mut base, tri, color);
                }
                Cmd::Shadow {
                    texture,
                    cx,
                    cy,
                    size,
                    alpha,
                } => {
                    if let Some(texture) = texture {
                        canvas::draw_region(
                            &mut base,
                            &texture,
                            cx,
                            cy,
                            size,
                            size,
                            0.0,
                            Color::from_rgba(0.0, 0.0, 0.0, alpha),
                        );
                    }
                }
                Cmd::Region {
                    texture,
                    cx,
                    cy,
                    scale,
                    rotation,
                    tint,
                } => {
                    if let Some(texture) = texture {
                        let (w, h) = RegionCache::natural_size(&texture);
                        canvas::draw_region(
                            &mut base,
                            &texture,
                            cx,
                            cy,
                            w * scale,
                            h * scale,
                            rotation,
                            tint,
                        );
                    }
                }
                Cmd::LightRegion {
                    texture,
                    cx,
                    cy,
                    rotation,
                    alpha,
                } => {
                    let (w, h) = RegionCache::natural_size(&texture);
                    let light = canvas::color_u32(LIGHT_COLOR);
                    let dark = canvas::color_u32(METAL_GRAY_DARK);
                    let color = Color::from_rgba(
                        dark.r + (light.r - dark.r) * alpha,
                        dark.g + (light.g - dark.g) * alpha,
                        dark.b + (light.b - dark.b) * alpha,
                        dark.a + (light.a - dark.a) * alpha,
                    );
                    canvas::draw_region(&mut base, &texture, cx, cy, w, h, rotation, color);
                }
            }
        }
    }
}

/// Engine glow + 4 flares (`Fill.light` + `Drawf.tri`).
fn push_engine(
    cmds: &mut Vec<Cmd>,
    cx: f32,
    cy: f32,
    alpha: f32,
    rad: f32,
    scale: f32,
    rotation: f32,
) {
    let engine = canvas::color_u32(ENGINE);
    cmds.push(Cmd::Light {
        x: cx,
        y: cy,
        radius: 25.0 * (rad + scale - 1.0),
        center: with_alpha(engine, alpha),
        edge: with_alpha(engine, 0.0),
    });
    for i in 0..4 {
        cmds.push(Cmd::Tri {
            tri: drawf::tri(
                cx,
                cy,
                6.0,
                40.0 * (rad + scale - 1.0),
                i as f32 * 90.0 + rotation,
            ),
            color: with_alpha(engine, alpha),
        });
    }
}

/// The pad's `<name>-pod` region, else the fallback (`LaunchPad.find("@-pod",
/// fallback)`).
fn pod_texture(
    pass: &mut PodsRenderer,
    node: &Gd<Node2D>,
    block: BlockId,
    fallback: &str,
) -> Option<Gd<Texture2D>> {
    let name = pass.block_name(&pass.host.clone()?, block)?;
    pass.regions
        .get(node, &format!("{name}-pod"))
        .or_else(|| pass.regions.get(node, fallback))
}

/// One deferred draw command (textures resolved before the canvas borrow).
enum Cmd {
    Light {
        x: f32,
        y: f32,
        radius: f32,
        center: Color,
        edge: Color,
    },
    Tri {
        tri: [(f32, f32); 3],
        color: Color,
    },
    Shadow {
        texture: Option<Gd<Texture2D>>,
        cx: f32,
        cy: f32,
        size: f32,
        alpha: f32,
    },
    Region {
        texture: Option<Gd<Texture2D>>,
        cx: f32,
        cy: f32,
        scale: f32,
        rotation: f32,
        tint: Color,
    },
    LightRegion {
        texture: Gd<Texture2D>,
        cx: f32,
        cy: f32,
        rotation: f32,
        alpha: f32,
    },
}

impl PodsRenderer {
    /// Installs the draw mode (called from `MindWorldRenderer::build_floor`).
    pub(crate) fn set_mode(&mut self, mode: PodMode) {
        self.mode = mode;
    }

    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
    }

    /// Block content name for a `BlockId` (region prefix lookup).
    fn block_name(&self, host: &Gd<MindSimHost>, block: BlockId) -> Option<String> {
        let host = host.clone();
        let host = host.bind();
        let content = host.content_registry()?;
        content.block(block).map(|def| def.name.clone())
    }
}
