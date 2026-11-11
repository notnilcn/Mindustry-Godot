// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/ResearchDialog.java (`View` class,
//         lines 441-814; `Styles.nodei` up-drawable rule, connector drawing).
//
//! The pannable/zoomable tech-tree canvas: one 60 px node button per TechNode
//! at the Java `treeLayout` position (Java y-up, flipped here), a two-pass
//! connector layer (gray locks under accent lines) and drag/wheel camera
//! control. Node rows come from `MindCampaign.research_tree_json` (T1a) and are
//! pushed in through [`MindResearchView::set_tree`].

use godot::builtin::{VarArray, VarDictionary};
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{
    Button, Control, IControl, InputEvent, InputEventMouseButton, InputEventMouseMotion, Json,
    StyleBox, StyleBoxTexture,
};
use godot::global::{MouseButton, MouseButtonMask};
use godot::obj::{EngineBitfield, NewAlloc, NewGd, WithBaseField};
use godot::prelude::*;
use mind_core::ui::campaign::{ResearchNodeRow, ResearchTreeView};
use std::collections::HashMap;

use crate::ui::kit;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

/// `ResearchDialog.nodeSize`.
const NODE_SIZE: f32 = 60.0;
/// `button.resizeImage(32f)`.
const IMAGE_SIZE: f32 = 32.0;
/// Java wheel clamp (`View` wheel handler).
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 1.0;
/// Connector stroke (`4 * scale`).
const LINE_WIDTH: f32 = 4.0;
/// `Pal.gray` (#454545) and `Color.gray` (#7f7f7f).
const PAL_GRAY: Color = Color::from_rgba(0.271, 0.271, 0.271, 1.0);
const IMAGE_GRAY: Color = Color::from_rgba(0.498, 0.498, 0.498, 1.0);
/// Ninepatch border margin of the `button*` node styles.
const STYLE_MARGIN: f32 = 12.0;

/// `MindResearchView` — the tech-tree canvas.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindResearchView {
    base: Base<Control>,
    /// `View.panX`/`panY` in Java (y-up) space.
    #[var]
    pub pan: Vector2,
    /// `View.scale`.
    #[var]
    pub scale_factor: f32,
    /// `View.moved`: true between drag-start and the deferred release reset.
    #[var]
    pub moved: bool,
    /// The typed `ResearchTreeView.nodes` rows (depth-first order).
    nodes: Vec<ResearchNodeRow>,
    buttons: Vec<Gd<Button>>,
    /// `ResearchTreeView.bounds` (`[x, y, w, h]`, Java space).
    bounds: [f32; 4],
    dirty: bool,
    last_size: Vector2,
    /// Region name -> cached ninepatch style (shared across views in Java).
    styles: HashMap<String, Gd<StyleBoxTexture>>,
}

#[godot_api]
impl IControl for MindResearchView {
    fn init(base: Base<Control>) -> Self {
        // The base object is not bound yet during `init` (gdext panics on the
        // downcast), so every base mutation happens in `ready` (PORTING.md).
        Self {
            base,
            pan: Vector2::new(0.0, -200.0),
            scale_factor: 1.0,
            moved: false,
            nodes: Vec::new(),
            buttons: Vec::new(),
            bounds: [0.0; 4],
            dirty: true,
            last_size: Vector2::ZERO,
            styles: HashMap::new(),
        }
    }

    fn ready(&mut self) {
        // Java's tree `View` is an unclipped `Group` stacked in `cont`
        // (`ResearchDialog.java:110-114`): nodes and lines run on under the
        // bottom button row, which draws over them.
        self.base_mut().set_clip_contents(false);
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let size = self.base().get_size();
        if !self.dirty && self.last_size == size {
            return;
        }
        self.last_size = size;
        self.dirty = false;
        self.place_buttons();
        self.base_mut().queue_redraw();
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        self.handle_input(event);
    }

    fn draw(&mut self) {
        self.draw_tree();
    }
}

#[godot_api]
impl MindResearchView {
    /// Emitted on a click that is not a drag (`View` button click).
    #[signal]
    fn node_pressed(index: i64);
    /// Emitted when the mouse enters a node button.
    #[signal]
    fn node_hovered(index: i64);
    /// Emitted when the mouse exits a node button.
    #[signal]
    fn node_exited(index: i64);

    /// Replaces the canvas contents with a `research_tree_json` view.
    ///
    /// # ABI shim
    ///
    /// The untyped `#[func]` form is the MCP/test surface; it deserializes into
    /// the typed [`ResearchTreeView`] and renders that.
    #[func]
    pub fn set_tree(&mut self, tree: VarDictionary) {
        let text = Json::stringify(&tree.to_variant());
        match serde_json::from_str::<ResearchTreeView>(&text.to_string()) {
            Ok(view) => self.set_tree_view(&view),
            Err(error) => log::warn!("[ui] research_tree row shape: {error}"),
        }
    }

    /// Replaces the canvas contents with the typed tree payload (T01).
    pub fn set_tree_view(&mut self, tree: &ResearchTreeView) {
        for mut button in self.buttons.drain(..) {
            if button.is_instance_valid() {
                button.queue_free();
            }
        }
        self.nodes = tree.nodes.clone();
        self.bounds = tree.bounds;
        for index in 0..self.nodes.len() {
            let node = self.nodes[index].clone();
            // code-instantiated: one button per tech node (`View.rebuildAll`).
            let mut button = Button::new_alloc();
            button.set_meta("node_index", &(index as i64).to_variant());
            button.set_custom_minimum_size(Vector2::new(NODE_SIZE, NODE_SIZE));
            button.set_size(Vector2::new(NODE_SIZE, NODE_SIZE));
            let style = self.style_for(&node);
            for style_name in ["normal", "hover", "pressed", "disabled"] {
                button.add_theme_stylebox_override(style_name, &style);
            }
            self.base_mut().add_child(&button);
            self.build_button_content(&mut button, &node);
            let mut bound = VarArray::new();
            bound.push(&(index as i64).to_variant());
            button.connect(
                "pressed",
                &self.base().callable("on_button_pressed").bindv(&bound),
            );
            button.connect(
                "mouse_entered",
                &self.base().callable("on_node_hovered").bindv(&bound),
            );
            button.connect(
                "mouse_exited",
                &self.base().callable("on_node_exited").bindv(&bound),
            );
            // Buttons swallow motion events; forward them to the canvas so a
            // drag started on a node still pans.
            button.connect("gui_input", &self.base().callable("forward_input"));
            self.buttons.push(button);
        }
        self.dirty = true;
    }

    /// Alias for [`Self::set_tree`] (the `set_model` call name).
    #[func]
    pub fn set_model(&mut self, tree: VarDictionary) {
        self.set_tree(tree);
    }

    /// `rebuildTree`: pan -200, scale 1.
    #[func]
    pub fn reset_view(&mut self) {
        self.pan = Vector2::new(0.0, -200.0);
        self.scale_factor = 1.0;
        self.dirty = true;
    }

    /// Forces one layout/redraw pass (`_process` without a size change).
    #[func]
    pub fn refresh(&mut self) {
        self.dirty = true;
        self.base_mut().queue_redraw();
    }

    /// Button rect in viewport coordinates (hover popup anchor).
    #[func]
    pub fn button_rect(&self, index: i64) -> Rect2 {
        if index < 0 || index as usize >= self.buttons.len() {
            return Rect2::new(Vector2::ZERO, Vector2::ZERO);
        }
        self.buttons[index as usize].get_global_rect()
    }

    /// The `research_tree_json` node row at `index`.
    ///
    /// # ABI shim
    ///
    /// The typed rows are serialized back to the MCP/test dictionary shape;
    /// Rust callers read [`Self::node_row`] instead.
    #[func]
    pub fn node_at(&self, index: i64) -> VarDictionary {
        let Some(node) = self.node_row(index) else {
            return VarDictionary::new();
        };
        let text = crate::gdutil::json::to_gstring(&node, "{}");
        Json::parse_string(&text)
            .try_to::<VarDictionary>()
            .unwrap_or_default()
    }

    /// The typed node row at `index`, if any.
    pub fn node_row(&self, index: i64) -> Option<ResearchNodeRow> {
        if index < 0 {
            return None;
        }
        self.nodes.get(index as usize).cloned()
    }

    /// `View` button click, filtered by the drag latch.
    #[func]
    pub fn on_button_pressed(&mut self, index: i64) {
        if self.moved {
            return;
        }
        self.base_mut()
            .emit_signal("node_pressed", &[index.to_variant()]);
    }

    /// Mouse entered a node button.
    #[func]
    pub fn on_node_hovered(&mut self, index: i64) {
        self.base_mut()
            .emit_signal("node_hovered", &[index.to_variant()]);
    }

    /// Mouse exited a node button.
    #[func]
    pub fn on_node_exited(&mut self, index: i64) {
        self.base_mut()
            .emit_signal("node_exited", &[index.to_variant()]);
    }

    /// Forwards a node button's `gui_input` to the canvas (`_forward_input`).
    #[func]
    pub fn forward_input(&mut self, event: Gd<InputEvent>) {
        self.handle_input(event);
    }

    fn build_button_content(&mut self, button: &mut Gd<Button>, node: &ResearchNodeRow) {
        if node.selectable {
            let icon_region = node.icon.clone();
            let type_name = node.kind.clone();
            let icon = factory::content_icon(&icon_region, &type_name, IMAGE_SIZE);
            if let Some(mut icon) = icon {
                icon.set_modulate(self.image_color(node));
                icon.set_mouse_filter(MouseFilter::IGNORE);
                icon.set_anchors_preset(LayoutPreset::CENTER);
                icon.set_offset(godot::builtin::Side::LEFT, -IMAGE_SIZE / 2.0);
                icon.set_offset(godot::builtin::Side::TOP, -IMAGE_SIZE / 2.0);
                icon.set_offset(godot::builtin::Side::RIGHT, IMAGE_SIZE / 2.0);
                icon.set_offset(godot::builtin::Side::BOTTOM, IMAGE_SIZE / 2.0);
                button.add_child(&icon);
            } else {
                // Missing non-sector region: Java draws `Icon.lock` only when
                // the node is not selectable; keep the button empty.
                log::warn!("[ui] missing icon region `{icon_region}`");
            }
        } else {
            let mut glyph = factory::glyph_label("lock", 32, self.image_color(node));
            glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            glyph.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&glyph);
        }
    }

    /// `button.getImage().setColor(...)` (ResearchDialog.java:512).
    fn image_color(&self, node: &ResearchNodeRow) -> Color {
        if !node.locked {
            return Color::WHITE;
        }
        if node.selectable {
            return IMAGE_GRAY;
        }
        PAL_GRAY
    }

    /// `button.getStyle().up` rule (ResearchDialog.java:509).
    fn style_for(&mut self, node: &ResearchNodeRow) -> Gd<StyleBox> {
        let locked = node.locked;
        let selectable = node.selectable;
        let can_spend = node.can_spend;
        let key = if !locked {
            "button-over"
        } else if !selectable || !can_spend {
            "button-red"
        } else {
            "button"
        };
        if let Some(style) = self.styles.get(key) {
            return style.clone().upcast();
        }
        let mut style = StyleBoxTexture::new_gd();
        if let Some(texture) = kit::icon_texture(key) {
            style.set_texture(&texture);
        }
        style.set_texture_margin(godot::builtin::Side::LEFT, STYLE_MARGIN);
        style.set_texture_margin(godot::builtin::Side::TOP, STYLE_MARGIN);
        style.set_texture_margin(godot::builtin::Side::RIGHT, STYLE_MARGIN);
        style.set_texture_margin(godot::builtin::Side::BOTTOM, STYLE_MARGIN);
        let stylebox: Gd<StyleBox> = style.clone().upcast();
        self.styles.insert(key.to_string(), style);
        stylebox
    }

    fn handle_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            let mask = motion.get_button_mask();
            if mask.ord() & MouseButtonMask::LEFT.ord() != 0 {
                let relative = motion.get_relative();
                self.pan.x += relative.x / self.scale_factor;
                // Arc dy is y-up; Godot relative.y grows downward.
                self.pan.y -= relative.y / self.scale_factor;
                self.moved = true;
                self.clamp_view();
                self.dirty = true;
                self.base_mut().accept_event();
            }
            return;
        }
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if mouse.is_pressed() {
            let index = mouse.get_button_index();
            if index == MouseButton::WHEEL_DOWN {
                self.zoom(1.0);
                self.base_mut().accept_event();
            } else if index == MouseButton::WHEEL_UP {
                self.zoom(-1.0);
                self.base_mut().accept_event();
            }
        } else if mouse.get_button_index() == MouseButton::LEFT {
            // Deferred: a button's `pressed` fires on this release and must
            // still observe `moved` (Arc `released` runs after the click).
            self.run_deferred(|view| view.moved = false);
        }
    }

    /// Java wheel: `scale = clamp(scale - amountY/10*scale, 0.25, 1)` with
    /// `amountY = +1` on wheel-down.
    fn zoom(&mut self, amount_y: f32) {
        self.scale_factor = clampf(
            self.scale_factor - amount_y / 10.0 * self.scale_factor,
            MIN_SCALE,
            MAX_SCALE,
        );
        self.dirty = true;
    }

    /// `View.clamp` (ResearchDialog.java:539-549), in Java (y-up) space.
    fn clamp_view(&mut self) {
        let pad = NODE_SIZE;
        let size = self.base().get_size();
        let ox = size.x / 2.0;
        let oy = size.y / 2.0;
        let screen = self.base().get_viewport_rect().size;
        let bounds_x = self.bounds[0];
        let bounds_y = self.bounds[1];
        let bounds_w = self.bounds[2];
        let bounds_h = self.bounds[3];
        let rx = clampf(bounds_x + self.pan.x + ox, -bounds_w + pad, screen.x - pad);
        let ry = clampf(self.pan.y + oy + bounds_y, -bounds_h + pad, screen.y - pad);
        self.pan.x = rx - bounds_x - ox;
        self.pan.y = ry - bounds_y - oy;
    }

    fn place_buttons(&mut self) {
        let size = self.base().get_size();
        let viewport = self.base().get_viewport_rect().size;
        // GDScript `floori(size.y) % 2 / 2.0`: half-pixel parity nudge.
        let odd_offset = ((viewport.y.floor() as i64) % 2) as f32 / 2.0;
        let pan = self.pan;
        let scale = self.scale_factor;
        for (node, button) in self.nodes.iter().zip(self.buttons.iter_mut()) {
            button.set_visible(node.visible);
            if !node.visible {
                continue;
            }
            let center = java_point_at(pan, scale, size, node.x, node.y + odd_offset);
            button.set_scale(Vector2::ONE * scale);
            button.set_position(center - Vector2::new(NODE_SIZE, NODE_SIZE) * 0.5 * scale);
        }
    }

    /// Java (y-up) point through pan/zoom into Godot view coordinates.
    fn java_point(&self, java_x: f32, java_y: f32) -> Vector2 {
        java_point_at(
            self.pan,
            self.scale_factor,
            self.base().get_size(),
            java_x,
            java_y,
        )
    }

    fn draw_tree(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let mut segments: Vec<(Vector2, Vector2, Color)> = Vec::new();
        // `Draw.sort`: gray (locked) connectors under the accent ones.
        for pass_index in 0..2 {
            let want_lock = pass_index == 0;
            for node in &self.nodes {
                if !node.visible {
                    continue;
                }
                let node_x = node.x;
                let node_y = node.y;
                let node_locked = node.locked;
                for child_index in &node.children {
                    let Some(child) = self.nodes.get(*child_index) else {
                        continue;
                    };
                    if !child.visible {
                        continue;
                    }
                    let lock = node_locked || child.locked;
                    if lock != want_lock {
                        continue;
                    }
                    let color = if lock { PAL_GRAY } else { styles::ACCENT };
                    let child_x = child.x;
                    let child_y = child.y;
                    let node_point = self.java_point(node_x, node_y);
                    let child_point = self.java_point(child_x, child_y);
                    let manhattan = (node_x - child_x).abs() + (node_y - child_y).abs();
                    let straight = ((node_y - child_y).abs() - (node_x - child_x).abs()).abs()
                        <= 1.0
                        && manhattan <= NODE_SIZE * 3.0;
                    if straight {
                        segments.push((node_point, child_point, color));
                    } else {
                        let corner = self.java_point(child_x, node_y);
                        segments.push((node_point, corner, color));
                        segments.push((corner, child_point, color));
                    }
                }
            }
        }
        let width = LINE_WIDTH * self.scale_factor;
        for (from, to, color) in segments {
            self.base_mut()
                .draw_line_ex(from, to, color)
                .width(width)
                .done();
        }
    }
}

/// Java (y-up) point through pan/zoom into Godot view coordinates.
fn java_point_at(pan: Vector2, scale: f32, size: Vector2, java_x: f32, java_y: f32) -> Vector2 {
    let point = Vector2::new(
        java_x + pan.x + size.x / 2.0,
        size.y - (java_y + pan.y + size.y / 2.0),
    );
    let view_center = size / 2.0;
    view_center + (point - view_center) * scale
}

/// GDScript `clampf` semantics (`min`/`max` are never swapped, so Godot's
/// clamp never panics on a tiny viewport).
fn clampf(value: f32, min: f32, max: f32) -> f32 {
    if min > max {
        return value;
    }
    value.clamp(min, max)
}
