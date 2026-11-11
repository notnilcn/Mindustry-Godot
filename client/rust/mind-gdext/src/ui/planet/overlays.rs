// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Sector overlays for the planet view: markers, arcs, icons, hover, highlight and label.
//! Upstream: core/src/mindustry/ui/dialogs/PlanetDialog.java + core/src/mindustry/graphics/g3d/PlanetRenderer.java

use std::f32::consts::{PI, TAU};

use godot::classes::base_material_3d::{
    CullMode, Flags, ShadingMode, TextureFilter, TextureParam, Transparency,
};
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, Font, Label3D, Mesh, MeshInstance3D, Node3D, QuadMesh, Sprite3D, StandardMaterial3D,
    Texture2D, Time,
};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::mesh::{arc_ribbon, gamma_shadow_alpha, new_arrays, overlay_material};
use super::rows::{faded_alpha, row_f32, with_alpha};
use crate::ui::kit::styles;
use mind_core::ui::campaign::SectorView;

use super::*;

impl MindPlanetView {
    // -----------------------------------------------------------------------
    // Sector markers (`renderSectors` + `renderProjections`)
    // -----------------------------------------------------------------------

    pub(super) fn rebuild_markers(&mut self) {
        if self.markers.is_none() {
            // code-instantiated: marker layer content is driven by the sector
            // rows.
            let mut markers = Node3D::new_alloc();
            markers.set_name("SectorMarkers");
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&markers);
            }
            self.markers = Some(markers);
        }
        if let Some(markers) = self.markers.clone() {
            for child in markers.get_children().iter_shared() {
                let mut child = child;
                child.queue_free();
            }
        }
        if self.tile_corners.is_empty() {
            return;
        }
        self.built_ui_alpha = self.ui_alpha;
        self.built_pulse = self.preset_pulse();
        let arad = row_f32(&self.data, "outline", 1.17) + 0.001;
        let sectors = self.sector_views();
        let ui_alpha = self.ui_alpha;
        let mut verts: Vec<Vector3> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();
        for sector in &sectors {
            let tile = self.sector_tile(sector);
            if tile < 0 || tile as usize >= self.tile_corners.len() || tile == self.selected_tile {
                continue;
            }
            let color = self.status_color(sector);
            if sector.selectable || sector.unlocked {
                if let Some(color) = color {
                    let ring =
                        Color::from_rgba(color.r * 0.8, color.g * 0.8, color.b * 0.8, ui_alpha);
                    self.append_ring(&mut verts, &mut colors, tile, arad, 0.026, ring);
                }
            } else {
                let shadow = Color::from_rgba(
                    SHADOW.r,
                    SHADOW.g,
                    SHADOW.b,
                    gamma_shadow_alpha(SHADOW.a * ui_alpha),
                );
                self.append_fill(&mut verts, &mut colors, tile, arad, shadow);
            }
            self.add_icon(sector, tile);
        }
        // `PlanetDialog.java:481-486`: the sector being played is filled with
        // `hoverColor` (`Pal.accent` at 50%) at `-0.001f`.
        let current_fill =
            Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, faded_alpha(0.5, ui_alpha));
        for sector in &sectors {
            if !sector.being_played {
                continue;
            }
            let tile = self.sector_tile(sector);
            if tile < 0 || tile as usize >= self.tile_corners.len() {
                continue;
            }
            self.append_fill(&mut verts, &mut colors, tile, arad - 0.002, current_fill);
        }
        self.arc_specs = self.build_arc_specs(&sectors, ui_alpha);
        if !verts.is_empty() {
            let mut arrays = new_arrays();
            arrays.set(
                ArrayType::VERTEX.ord() as usize,
                &PackedVector3Array::from(verts).to_variant(),
            );
            arrays.set(
                ArrayType::COLOR.ord() as usize,
                &PackedColorArray::from(colors).to_variant(),
            );
            let mut mesh = ArrayMesh::new_gd();
            mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_mesh(&mesh);
            let mut fill_material = overlay_material();
            // Depth-tested so the globe disc hides the far hemisphere.
            fill_material.set_flag(Flags::DISABLE_DEPTH_TEST, false);
            fill_material.set_render_priority(5);
            instance.set_material_override(&fill_material);
            if let Some(mut markers) = self.markers.clone() {
                markers.add_child(&instance);
            }
        }
        self.rebuild_arcs();
    }

    /// `PlanetDialog.renderOverProjections` (`PlanetDialog.java:519-543`): the
    /// shield arc, the `sectorInvasion` attack arcs and the selected sector's
    /// import/export arcs, all in the mesh frame the port renders in.
    pub(super) fn build_arc_specs(&self, sectors: &[SectorView], ui_alpha: f32) -> Vec<ArcSpec> {
        let mut specs = Vec::new();
        if ui_alpha <= 0.001 {
            return specs;
        }
        let tile_of = |id: i64| -> Option<i32> {
            let row = sectors.iter().find(|row| i64::from(row.id) == id)?;
            let tile = self.sector_tile(row);
            (tile >= 0).then_some(tile)
        };
        let selected_id = sectors
            .iter()
            .find(|row| self.sector_tile(row) == self.selected_tile)
            .map(|row| i64::from(row.id))
            .filter(|id| *id >= 0);
        let selected_row =
            selected_id.and_then(|id| sectors.iter().find(|row| i64::from(row.id) == id));
        let selected_has_base = selected_row.is_some_and(|row| row.has_base);
        let selected_destination =
            selected_row.and_then(|row| row.export_destination.map(i64::from));
        let selected_exports = selected_row.is_some_and(|row| row.any_exports);
        for row in sectors {
            let id = i64::from(row.id);
            let Some(tile) = tile_of(id) else {
                continue;
            };
            // `PlanetDialog.java:519-521`: `Team.crux.color` fading to half
            // alpha, height 0.3, 110-tick cycle, stroke 0.006.
            if let Some(target) = row.shield_arc.map(i64::from)
                && let Some(target_tile) = tile_of(target)
            {
                specs.push(ArcSpec {
                    a: tile,
                    b: target_tile,
                    from: with_alpha(CRUX, ui_alpha),
                    to: with_alpha(CRUX, ui_alpha * 0.5),
                    length: 0.3,
                    time_scale: 110.0,
                    stroke: 0.006,
                });
            }
            if !row.has_base {
                continue;
            }
            // `PlanetDialog.java:525-531`: crux to clear, height 0.24, stroke
            // 0.005.
            for source in row.attack_sources.iter().map(|id| i64::from(*id)) {
                let Some(source_tile) = tile_of(source) else {
                    continue;
                };
                specs.push(ArcSpec {
                    a: source_tile,
                    b: tile,
                    from: with_alpha(CRUX, ui_alpha),
                    to: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    length: 0.24,
                    time_scale: 110.0,
                    stroke: 0.005,
                });
            }
            // `PlanetDialog.java:534-542`: imports into the selected sector
            // (`Color.gray` -> accent) and exports from it (`Pal.place` ->
            // accent), height 0.4, 90-tick cycle.
            let Some(selected) = selected_id.filter(|selected| *selected != id) else {
                continue;
            };
            if !selected_has_base {
                continue;
            }
            if row.any_exports
                && row.export_destination.map(i64::from) == Some(selected)
                && let Some(selected_tile) = tile_of(selected)
            {
                specs.push(ArcSpec {
                    a: tile,
                    b: selected_tile,
                    from: with_alpha(ARC_GRAY, ui_alpha),
                    to: with_alpha(ACCENT, ui_alpha),
                    length: 0.4,
                    time_scale: 90.0,
                    stroke: 0.0,
                });
            }
            if selected_exports
                && selected_destination == Some(id)
                && let Some(selected_tile) = tile_of(selected)
            {
                specs.push(ArcSpec {
                    a: selected_tile,
                    b: tile,
                    from: with_alpha(PLACE, ui_alpha),
                    to: with_alpha(ACCENT, ui_alpha),
                    length: 0.4,
                    time_scale: 90.0,
                    stroke: 0.0,
                });
            }
        }
        specs
    }

    /// Rebuilds the arc mesh with the current `Time.globalTime` phase
    /// (`Planet.drawArc*` animates the gradient every frame).
    pub(super) fn rebuild_arcs(&mut self) {
        let Some(mut instance) = self.ensure_arcs() else {
            return;
        };
        if self.arc_specs.is_empty() {
            instance.set_mesh(Gd::<Mesh>::null_arg());
            return;
        }
        let time = self.global_time as f32;
        let mut verts: Vec<Vector3> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();
        for spec in &self.arc_specs {
            self.append_arc(&mut verts, &mut colors, spec, time);
        }
        if verts.is_empty() {
            instance.set_mesh(Gd::<Mesh>::null_arg());
            return;
        }
        let mut arrays = new_arrays();
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(verts).to_variant(),
        );
        arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(colors).to_variant(),
        );
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        let mut material = overlay_material();
        // Depth-tested so the far hemisphere/behind-the-globe arcs are hidden.
        material.set_flag(Flags::DISABLE_DEPTH_TEST, false);
        material.set_render_priority(6);
        instance.set_mesh(&mesh);
        instance.set_material_override(&material);
    }

    pub(super) fn ensure_arcs(&mut self) -> Option<Gd<MeshInstance3D>> {
        if self.arcs.is_none() {
            // code-instantiated: one dynamic mesh for the projection arcs.
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_name("SectorArcs");
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&instance);
            }
            self.arcs = Some(instance);
        }
        self.arcs.clone()
    }

    /// One quadratic-Bezier ribbon (`Planet.drawArcLine`, `Planet.java:663-690`)
    /// between two sector tiles; the geometry/colour walk lives in
    /// [`arc_ribbon`] so it is unit-testable without a Godot instance.
    pub(super) fn append_arc(
        &self,
        verts: &mut Vec<Vector3>,
        colors: &mut Vec<Color>,
        spec: &ArcSpec,
        time: f32,
    ) {
        if spec.a < 0 || spec.b < 0 {
            return;
        }
        let a = self.tile_vec(spec.a).normalized();
        let b = self.tile_vec(spec.b).normalized();
        if a.length_squared() < 1e-8 || b.length_squared() < 1e-8 {
            return;
        }
        // The port renders the unit sphere, so `outlineRad * radius` is just
        // the outline shell radius.
        let outline = row_f32(&self.data, "outline", 1.17);
        let (ribbon, rainbow) = arc_ribbon(
            [a.x, a.y, a.z],
            [b.x, b.y, b.z],
            outline,
            spec.length,
            spec.time_scale,
            spec.stroke,
            [spec.from.r, spec.from.g, spec.from.b, spec.from.a],
            [spec.to.r, spec.to.g, spec.to.b, spec.to.a],
            time,
        );
        verts.extend(ribbon.iter().map(|v| Vector3::new(v[0], v[1], v[2])));
        colors.extend(
            rainbow
                .iter()
                .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3])),
        );
    }

    /// `renderSectors` ring color (None = no ring).
    pub(super) fn status_color(&self, sector: &SectorView) -> Option<Color> {
        if sector.has_base {
            let mix = if sector.has_enemy_base { 0.5 } else { 0.0 };
            return Some(SHARDED.lerp(CRUX, mix));
        }
        let preset = sector.preset.as_deref().unwrap_or("");
        if !preset.is_empty() && sector.require_unlock {
            // `derelict.lerp(white, absin(time, 10, 1))` (`PlanetDialog.java:463`).
            return Some(if sector.preset_unlocked {
                DERELICT.lerp(Color::WHITE, f64::from(self.preset_pulse()))
            } else {
                styles::ARC_GRAY
            });
        }
        if sector.has_enemy_base {
            return Some(CRUX);
        }
        None
    }

    pub(super) fn tile_outer(&self, tile: i32, arad: f32) -> Vec<Vector3> {
        self.tile_ids(tile)
            .iter()
            .map(|id| self.corner(*id) * arad)
            .collect()
    }

    pub(super) fn append_fill(
        &self,
        verts: &mut Vec<Vector3>,
        colors: &mut Vec<Color>,
        tile: i32,
        arad: f32,
        color: Color,
    ) {
        let outers = self.tile_outer(tile, arad);
        let center = self.tile_vec(tile) * arad;
        for index in 0..outers.len() {
            verts.push(center);
            verts.push(outers[index]);
            verts.push(outers[(index + 1) % outers.len()]);
            for _ in 0..3 {
                colors.push(color);
            }
        }
    }

    pub(super) fn append_ring(
        &self,
        verts: &mut Vec<Vector3>,
        colors: &mut Vec<Color>,
        tile: i32,
        arad: f32,
        stroke: f32,
        color: Color,
    ) {
        let outers = self.tile_outer(tile, arad);
        let center = self.tile_vec(tile) * arad;
        let mut inners = Vec::with_capacity(outers.len());
        for outer in &outers {
            let offset = *outer - center;
            inners.push(center + offset.normalized() * (offset.length() - stroke));
        }
        for index in 0..outers.len() {
            let next = (index + 1) % outers.len();
            verts.push(outers[index]);
            verts.push(outers[next]);
            verts.push(inners[next]);
            verts.push(outers[index]);
            verts.push(inners[next]);
            verts.push(inners[index]);
            for _ in 0..6 {
                colors.push(color);
            }
        }
    }

    /// `renderProjections` icon for one sector.
    ///
    /// Java has no facing cull here: each icon lies flat on the sector's plane
    /// (`planets.drawPlane` -> `Planet.setPlane`), so icons near the limb are
    /// seen edge-on and those behind the globe fail the depth test.
    pub(super) fn add_icon(&mut self, sector: &SectorView, tile: i32) {
        let preset = sector.preset.as_deref().unwrap_or("");
        let has_base = sector.has_base;
        let mut glyph = String::new();
        let mut region = String::new();
        let mut color = Color::WHITE;
        if sector.attacked {
            glyph = String::from("warn");
            color = SHARDED;
        } else if !has_base
            && !preset.is_empty()
            && sector.require_unlock
            && sector.preset_unlocked
            && sector.icon.as_deref().unwrap_or("").is_empty()
            && sector.content_icon.as_deref().unwrap_or("").is_empty()
        {
            region = format!("sector-{preset}");
        } else if sector.lock_icon {
            glyph = String::from("lock");
        } else {
            // `Sector.icon()`: contentIcon, info.icon, else an unlocked preset's
            // icon.
            let content_icon = sector.content_icon.as_deref().unwrap_or("");
            let icon = sector.icon.as_deref().unwrap_or("");
            if !content_icon.is_empty() {
                region = content_icon.to_owned();
            } else if !icon.is_empty() {
                glyph = icon.to_owned();
            } else if !preset.is_empty() && sector.require_unlock && sector.preset_unlocked {
                region = format!("sector-{preset}");
            }
        }
        let plane = self.sector_plane(tile);
        let ui_alpha = self.ui_alpha;
        if !region.is_empty() {
            let mut texture = kit::icon_texture(&region);
            if texture.is_none() && !region.starts_with("sector-") {
                for prefix in ["item", "block", "unit", "liquid"] {
                    texture = kit::icon_texture(&format!("{prefix}-{region}-ui"));
                    if texture.is_some() {
                        break;
                    }
                }
            }
            let Some(texture) = texture else {
                return;
            };
            // code-instantiated: one sector-plane sprite per marked sector.
            let mut sprite = Sprite3D::new_alloc();
            sprite.set_texture(&texture);
            let width = (texture.get_width() as f32).max(1.0);
            sprite.set_pixel_size(SECTOR_ICON_SIZE / width);
            sprite.set_texture_filter(TextureFilter::LINEAR);
            sprite.set_render_priority(ICON_PRIORITY);
            sprite.set_transform(plane);
            sprite.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, ui_alpha));
            if let Some(mut markers) = self.markers.clone() {
                markers.add_child(&sprite);
            }
            return;
        }
        let text = kit::glyph(&glyph);
        if text.is_empty() {
            return;
        }
        // code-instantiated: one icon-font glyph per marked sector.
        let mut label = Label3D::new_alloc();
        label.set_text(&text);
        if let Some(assets) = kit::assets()
            && let Some(font) = assets.bind().icon_font()
        {
            let font: Gd<Font> = font.upcast();
            label.set_font(&font);
        }
        label.set_font_size(64);
        label.set_outline_size(0);
        label.set_pixel_size(SECTOR_ICON_SIZE / 64.0);
        label.set_modulate(Color::from_rgba(
            color.r,
            color.g,
            color.b,
            color.a * ui_alpha,
        ));
        label.set_render_priority(ICON_PRIORITY);
        label.set_transform(plane);
        if let Some(mut markers) = self.markers.clone() {
            markers.add_child(&label);
        }
    }

    /// `Planet.setPlane` (`Planet.java:700-712`) in the mesh frame the port
    /// renders in (Java's `rotate(Y, -getRotation())` and `+ position` cancel
    /// out): origin `tile.v * (outlineRad + 0.01) * radius`; up = `Vec3.Y`
    /// projected onto the sector plane (`Sector.plane` through corners 0/2/4,
    /// `Sector.java:271`) minus `tile.v * radius`; right = up turned by Arc's
    /// `rotate(tile.v, 90)` (a clockwise quarter turn seen from outside). The
    /// local +Z faces outward, so a Sprite3D/Label3D/QuadMesh drawn in its XY
    /// plane matches `PlaneBatch3D`.
    pub(super) fn sector_plane(&self, tile: i32) -> Transform3D {
        let radius = row_f32(&self.data, "radius", 1.0);
        let v = self.tile_vec(tile);
        let origin = v.normalized() * (row_f32(&self.data, "outline", 1.17) + 0.01 * radius);
        let mut up = Vector3::UP;
        let ids = self.tile_ids(tile);
        if ids.len() >= 5 {
            let p1 = self.corner(ids[0]);
            let p2 = self.corner(ids[2]);
            let p3 = self.corner(ids[4]);
            let normal = (p1 - p2).cross(p2 - p3).normalized();
            let mut top = v + Vector3::UP;
            top -= normal * (normal.dot(top) - p1.dot(normal));
            up = top - v * radius;
        }
        if up.length_squared() < 1e-10 {
            up = Vector3::UP;
        }
        let up = up.normalized();
        // Arc `Vec3.rotate(axis, 90)` is Godot's `rotated(axis, -PI / 2)`.
        let right = up.rotated(v.normalized(), -PI * 0.5).normalized();
        let basis = Basis::from_cols(right, up, right.cross(up).normalized());
        Transform3D::new(basis, origin)
    }

    // -----------------------------------------------------------------------
    // Hover fill / selection highlight / sector icon
    // -----------------------------------------------------------------------

    /// `PlanetDialog.draw`: `planets.fill(hovered, hoverColor.write().a(0.5f),
    /// -0.003f)` paints the hovered hex with the accent at 50% just under the
    /// shell.
    pub(super) fn rebuild_hover_fill(&mut self) {
        if let Some(mut hover) = self.hover_fill.clone() {
            hover.set_mesh(Gd::<Mesh>::null_arg());
        }
        if self.hover_tile < 0 || self.hover_tile as usize >= self.tile_corners.len() {
            return;
        }
        let ids = self.tile_ids(self.hover_tile);
        let arad = row_f32(&self.data, "outline", 1.17) - 0.003;
        let center = if (self.hover_tile as usize) < self.tiles.len() {
            self.tile_vec(self.hover_tile) * arad
        } else {
            Vector3::ZERO
        };
        let corners: Vec<Vector3> = ids.iter().map(|id| self.corner(*id) * arad).collect();
        // `hoverColor = Pal.accent.a(0.5f)` (`PlanetRenderer.java:19`) faded by
        // `mulA(state.uiAlpha)` (`PlanetDialog.java:489`).
        let accent_fill = Color::from_rgba(
            ACCENT.r,
            ACCENT.g,
            ACCENT.b,
            faded_alpha(0.5, self.ui_alpha),
        );
        let mut fan: Vec<Vector3> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();
        for index in 0..corners.len() {
            fan.push(center);
            fan.push(corners[index]);
            fan.push(corners[(index + 1) % corners.len()]);
            for _ in 0..3 {
                colors.push(accent_fill);
            }
        }
        let mut arrays = new_arrays();
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(fan).to_variant(),
        );
        arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(colors).to_variant(),
        );
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        let mut material = overlay_material();
        material.set_render_priority(5);
        if let Some(mut hover) = self.hover_fill.clone() {
            hover.set_mesh(&mesh);
            hover.set_material_override(&material);
        }
    }

    pub(super) fn rebuild_highlight(&mut self) {
        self.clear_highlight();
        if self.selected_tile < 0 || self.selected_tile as usize >= self.tile_corners.len() {
            return;
        }
        let ids = self.tile_ids(self.selected_tile);
        // `PlanetRenderer.drawSelection(selected, uiAlpha)` draws a stroke band
        // `(outlineRad + 0.001) * radius` wide `0.04` toward the tile center, in
        // Pal.accent; `drawBorders` re-draws the hex outline in accent at 30%.
        let arad = row_f32(&self.data, "outline", 1.17) + 0.001;
        let center = if (self.selected_tile as usize) < self.tiles.len() {
            self.tile_vec(self.selected_tile) * arad
        } else {
            Vector3::ZERO
        };
        let stroke = 0.04;
        let mut outers: Vec<Vector3> = Vec::new();
        let mut inners: Vec<Vector3> = Vec::new();
        for id in &ids {
            let outer = self.corner(*id) * arad;
            outers.push(outer);
            let offset = outer - center;
            inners.push(center + offset.normalized() * (offset.length() - stroke));
        }

        // Band ring (`Planet.drawSelection`: quad(outer_i, outer_i+1,
        // inner_i+1, inner_i)).
        let mut band: Vec<Vector3> = Vec::new();
        let mut band_colors: Vec<Color> = Vec::new();
        // `drawSelection(selected, state.uiAlpha)` (`PlanetDialog.java:495`)
        // tints the band with `Pal.accent` at alpha `uiAlpha`.
        let band_color = Color::from_rgba(
            ACCENT.r,
            ACCENT.g,
            ACCENT.b,
            faded_alpha(1.0, self.ui_alpha),
        );
        for index in 0..outers.len() {
            let next = (index + 1) % outers.len();
            band.push(outers[index]);
            band.push(outers[next]);
            band.push(inners[next]);
            band.push(outers[index]);
            band.push(inners[next]);
            band.push(inners[index]);
            band_colors.extend(std::iter::repeat_n(band_color, 6));
        }
        let mut band_arrays = new_arrays();
        band_arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(band).to_variant(),
        );
        band_arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(band_colors).to_variant(),
        );
        let mut band_mesh = ArrayMesh::new_gd();
        band_mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &band_arrays);
        if let Some(mut selection) = self.selection.clone() {
            selection.set_mesh(&band_mesh);
            selection.set_material_override(&overlay_material());
        }
        self.rebuild_sector_icon();

        // Outline loop (`drawBorders`, `borderColor` = accent at 30%),
        // `borderColor.write.cpy().a((base.a + 0.3f) * alpha)` with
        // `alpha = state.uiAlpha` (`Planet.java:563-564`, `PlanetDialog.java:496`).
        let mut outline_loop: Vec<Vector3> = Vec::new();
        let mut loop_colors: Vec<Color> = Vec::new();
        let edge = Color::from_rgba(
            ACCENT.r,
            ACCENT.g,
            ACCENT.b,
            faded_alpha(0.3, self.ui_alpha),
        );
        for index in 0..outers.len() {
            outline_loop.push(outers[index]);
            outline_loop.push(outers[(index + 1) % outers.len()]);
            for _ in 0..2 {
                loop_colors.push(edge);
            }
        }
        let mut loop_arrays = new_arrays();
        loop_arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(outline_loop).to_variant(),
        );
        loop_arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(loop_colors).to_variant(),
        );
        let mut loop_mesh = ArrayMesh::new_gd();
        loop_mesh.add_surface_from_arrays(PrimitiveType::LINES, &loop_arrays);
        if let Some(mut outline) = self.outline.clone() {
            outline.set_mesh(&loop_mesh);
            outline.set_material_override(&overlay_material());
        }
    }

    pub(super) fn clear_highlight(&mut self) {
        if let Some(mut selection) = self.selection.clone() {
            selection.set_mesh(Gd::<Mesh>::null_arg());
        }
        if let Some(mut outline) = self.outline.clone() {
            outline.set_mesh(Gd::<Mesh>::null_arg());
        }
        if let Some(mut icon) = self.sector_icon.clone() {
            icon.set_mesh(Gd::<Mesh>::null_arg());
        }
    }

    /// The selected sector's icon (`PlanetDialog.renderProjections`: every
    /// visible sector draws its preset icon; the port draws the selected one).
    pub(super) fn rebuild_sector_icon(&mut self) {
        if self.sector_icon.is_none() {
            // code-instantiated: one data-driven sector-plane quad for the
            // selected sector.
            let mut icon = MeshInstance3D::new_alloc();
            icon.set_name("SectorIcon");
            icon.set_mesh(Gd::<Mesh>::null_arg());
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&icon);
            }
            self.sector_icon = Some(icon);
        }
        let mut texture: Option<Gd<Texture2D>> = None;
        if !self.sector_icon_preset.is_empty() {
            texture = kit::icon_texture(&format!("sector-{}", self.sector_icon_preset));
        }
        let missing = texture.is_none()
            || self.selected_tile < 0
            || self.selected_tile as usize >= self.tiles.len();
        if missing {
            if let Some(mut icon) = self.sector_icon.clone() {
                icon.set_mesh(Gd::<Mesh>::null_arg());
            }
            return;
        }
        let Some(texture) = texture else {
            return;
        };
        // `renderProjections` `iw = 64f/4f` at the `PlaneBatch3D` scaling
        // (1/150), flat on the sector plane (`Planet.setPlane`) like every other
        // icon.
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(SECTOR_ICON_SIZE, SECTOR_ICON_SIZE));
        let plane = self.sector_plane(self.selected_tile);
        let Some(mut icon) = self.sector_icon.clone() else {
            return;
        };
        icon.set_mesh(&quad);
        icon.set_transform(plane);
        let mut material = StandardMaterial3D::new_gd();
        material.set_shading_mode(ShadingMode::UNSHADED);
        material.set_transparency(Transparency::ALPHA);
        material.set_texture(TextureParam::ALBEDO, &texture);
        material.set_texture_filter(TextureFilter::LINEAR);
        material.set_cull_mode(CullMode::DISABLED);
        material.set_render_priority(ICON_PRIORITY);
        icon.set_material_override(&material);
    }

    // -----------------------------------------------------------------------
    // Label / tile geometry
    // -----------------------------------------------------------------------

    /// `Mathf.absin(Time.time, 10f, 1f)` (`PlanetDialog.java:463`): a 10 s
    /// `|sin|` pulse for unlocked, still-locked presets.
    pub(super) fn preset_pulse(&self) -> f32 {
        let seconds = Time::singleton().get_ticks_msec() as f32 / 1000.0;
        (seconds / 10.0 * TAU).sin().abs()
    }

    pub(super) fn update_label(&mut self) {
        let Some(mut label) = self.sector_label.clone() else {
            return;
        };
        // `PlanetDialog.draw`: the label exists only for `hovered != null`; the
        // selected sector alone draws no name.
        let point = if self.hover_tile >= 0 && !self.sector_label_text.is_empty() {
            self.tile_to_screen_point(self.hover_tile)
        } else {
            None
        };
        let Some(point) = point else {
            label.set_visible(false);
            return;
        };
        // `PlanetDialog.draw`: `[accent][lb] [white]<name>[accent] ]` — gold
        // brackets with the name in white (`Styles.outlineLabel` carries the
        // black outline via the scene's theme overrides). The dialog's
        // `set_sector_label_markup` hands over the finished hover text instead.
        let text = if self.sector_label_markup {
            self.sector_label_text.to_string()
        } else {
            format!(
                "[color=#ffd37f][lb] [color=white]{}[color=#ffd37f] ]",
                self.sector_label_text
            )
        };
        label.set_text(&GString::from(text.as_str()));
        label.set_visible(true);
        label.reset_size();
        let size = label.get_size();
        label.set_position(point - size * 0.5);
    }

    pub(super) fn tile_to_screen_point(&self, tile: i32) -> Option<Vector2> {
        if tile < 0 || tile as usize >= self.tiles.len() {
            return None;
        }
        let camera = self.camera.as_ref()?;
        let point = camera.unproject_position(self.tile_position(tile));
        let viewport_size = camera.get_viewport()?.get_visible_rect().size;
        if point.x < 0.0 || point.y < 0.0 || point.x > viewport_size.x || point.y > viewport_size.y
        {
            return None;
        }
        Some(point)
    }

    /// The typed sector rows the overlays render.
    pub(super) fn sector_views(&self) -> Vec<SectorView> {
        self.sector_views.clone()
    }

    /// The remapped grid tile of one typed sector marker.
    pub(super) fn sector_tile(&self, sector: &SectorView) -> i32 {
        self.view_tile_index(i64::from(sector.id), sector.preset.as_deref().unwrap_or(""))
    }

    pub(super) fn tile_vec(&self, tile: i32) -> Vector3 {
        if tile < 0 {
            return Vector3::ZERO;
        }
        self.tiles.get(tile as usize).unwrap_or(Vector3::ZERO)
    }

    pub(super) fn corner(&self, id: i32) -> Vector3 {
        if id < 0 {
            return Vector3::ZERO;
        }
        self.corners.get(id as usize).unwrap_or(Vector3::ZERO)
    }

    pub(super) fn tile_ids(&self, tile: i32) -> Vec<i32> {
        if tile < 0 {
            return Vec::new();
        }
        let Some(value) = self.tile_corners.get(tile as usize) else {
            return Vec::new();
        };
        value
            .try_to::<PackedInt32Array>()
            .map(|ids| ids.to_vec())
            .unwrap_or_default()
    }

    pub(super) fn load_loose(&self, key: &str, cutout: bool) -> Option<Gd<Texture2D>> {
        let mut assets = kit::assets()?;
        let mut guard = assets.bind_mut();
        let texture = if cutout {
            guard.find_loose_cutout(GString::from(key))?
        } else {
            guard.find_loose(GString::from(key))?
        };
        Some(texture.upcast())
    }
}
