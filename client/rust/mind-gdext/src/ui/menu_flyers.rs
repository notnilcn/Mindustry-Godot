// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/graphics/MenuRenderer.java (`drawFlyers`, `flyers`).

//! Flyer squadron over the baked menu background (`MenuFlyers`).
//!
//! `MindRender.menu_flyers_json` returns one flyer type's sprites in the bake's
//! texture-pixel space (32 px per tile, `MenuRenderer.flyerRot = 45`); this node
//! maps them through the same stretched/centred transform the `MenuBackground`
//! `TextureRect` uses and draws each sprite rotated like Java's flyer.

use godot::classes::{Control, IControl};
use godot::prelude::*;

use crate::ui::kit;

/// `1 - MenuRenderer.darkness`: Java darkens the whole frame (background *and*
/// flyers) after `drawFlyers`, so the squadron carries the same modulate.
const DARKNESS: f32 = 0.7;

/// `MindMenuFlyers` — the animated flyer overlay.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindMenuFlyers {
    base: Base<Control>,
}

#[godot_api]
impl IControl for MindMenuFlyers {
    fn ready(&mut self) {
        self.base_mut()
            .set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        self.base_mut()
            .set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        self.base_mut()
            .set_modulate(Color::from_rgba(DARKNESS, DARKNESS, DARKNESS, 1.0));
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible_in_tree() {
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(mut render) = self.base().get_node_or_null("/root/Spine/MindRender") else {
            return;
        };
        let Some(assets) = kit::assets() else {
            return;
        };
        let payload = render.call("menu_flyers_json", &[]);
        let Ok(payload) = payload.try_to::<GString>() else {
            return;
        };
        let parsed = godot::classes::Json::parse_string(&payload);
        let Ok(dictionary) = parsed.try_to::<VarDictionary>() else {
            return;
        };
        let rows = dictionary
            .get("rows")
            .map(|value| value.to::<VarArray>())
            .unwrap_or_default();
        if rows.is_empty() {
            return;
        }
        // The bake is displayed by the parent `TextureRect` with
        // `STRETCH_KEEP_ASPECT_COVERED`: scale to cover this rect, centred.
        let texture_size = Vector2::new(
            dictionary
                .get("width")
                .map(|value| value.to::<f32>())
                .unwrap_or(1.0),
            dictionary
                .get("height")
                .map(|value| value.to::<f32>())
                .unwrap_or(1.0),
        );
        let rect = self.base().get_size();
        let draw_scale = (rect.x / texture_size.x).max(rect.y / texture_size.y);
        let offset = (rect - texture_size * draw_scale) * 0.5;
        // A unit sprite is authored facing up and Java draws it at
        // `rotation - 90` for an up-right `flyerRot`; in Godot's y-down space
        // the clockwise-positive rotation that faces the sprite up-right is
        // `+flyerRot`.
        let rotation = dictionary
            .get("rot")
            .map(|value| value.to::<f32>())
            .unwrap_or(45.0);
        let draw_angle = rotation.to_radians();
        let scale_vector = Vector2::new(draw_scale, draw_scale);
        // `MindAssets` is a user class (no Rust Deref to `Node`): upcast once
        // so the dynamic `call` surface is available.
        let mut assets: Gd<Node> = assets.upcast();
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarArray>() else {
                continue;
            };
            if row.len() < 3 {
                continue;
            }
            let Some(name) = row.get(2).map(|value| value.to::<GString>()) else {
                continue;
            };
            let geometry = assets
                .call("region_geometry", &[name.to_variant()])
                .try_to::<PackedInt32Array>();
            let Ok(geometry) = geometry else {
                continue;
            };
            if geometry.len() < 5 {
                continue;
            }
            let page_index = geometry.as_slice()[4];
            let page = assets
                .call("page_texture", &[page_index.to_variant()])
                .try_to::<Gd<godot::classes::Texture2D>>();
            let Ok(page) = page else {
                continue;
            };
            let x = row
                .get(0)
                .map(|value| value.to::<f32>())
                .unwrap_or_default();
            let y = row
                .get(1)
                .map(|value| value.to::<f32>())
                .unwrap_or_default();
            let center = offset + Vector2::new(x, y) * draw_scale;
            self.base_mut()
                .draw_set_transform_ex(center)
                .rotation(draw_angle)
                .scale(scale_vector)
                .done();
            let values = geometry.as_slice();
            let (source_x, source_y, width, height) = (
                values[0] as f32,
                values[1] as f32,
                values[2] as f32,
                values[3] as f32,
            );
            self.base_mut().draw_texture_rect_region(
                &page,
                Rect2::new(
                    Vector2::new(-width * 0.5, -height * 0.5),
                    Vector2::new(width, height),
                ),
                Rect2::new(
                    Vector2::new(source_x, source_y),
                    Vector2::new(width, height),
                ),
            );
        }
        self.base_mut()
            .draw_set_transform_ex(Vector2::ZERO)
            .rotation(0.0)
            .scale(Vector2::ONE)
            .done();
    }
}
