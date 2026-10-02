// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapMarkers` — the live objective-marker containers (plan 12 M6).
//!
//! Ported from `core/src/mindustry/game/MapMarkers.java`. The marker **data**
//! (all 8 classes + `TextureHolder` + the legacy `Minimap` alias) is plan 04's
//! [`crate::io::json::objectives`]; this module owns the three index vectors
//! (`world_markers`/`map_markers`/`light_markers`), the `-1` sentinel index
//! fix-up, the logic `control()` port and the save-region `write`/`read`.
//!
//! Java stores marker object references in the vectors and mutates the marker's
//! stored index in place. Rust cannot alias, so the vectors hold `u32` ids into
//! the `map` (keyed by marker id, exactly the upstream `IntMap` key) and the
//! index lives on the marker value. Ordering is ascending-id for determinism
//! (HLP §9, Rust↔Rust only). Rendering is plan 16; the textures/regions are
//! resolved there.

use std::collections::BTreeMap;

use crate::content::Rgba;
use crate::io::json::JsonIo;
use crate::io::json::objectives::{ObjectiveMarker, TextureHolder};
use crate::io::typeio::MAX_BYTE_ARRAY;
use crate::io::wire::{WireReader, WireWriter};
use crate::io::{IoError, IoResult};

use super::map_objectives::ObjectiveLocale;

/// `WorldLabel.flagOutline`.
pub const LABEL_FLAG_OUTLINE: i8 = 1 << 1;
/// `WorldLabel.flagBackground`.
pub const LABEL_FLAG_BACKGROUND: i8 = 1 << 0;

/// `MapObjectives.allMarkerTypeNames` / `registerMarker` order.
pub const ALL_MARKER_TYPE_NAMES: [&str; 8] = [
    "shapeText",
    "point",
    "shape",
    "text",
    "line",
    "texture",
    "quad",
    "light",
];

/// `registerLegacyMarker("Minimap", PointMarker::new)`.
pub const MARKER_LEGACY_ALIASES: [(&str, &str); 1] = [("minimap", "point")];

/// Resolves a marker type name (including the legacy alias) to its canonical
/// camelized name.
pub fn marker_name_to_type(name: &str) -> Option<&'static str> {
    if let Some((_, canonical)) = MARKER_LEGACY_ALIASES
        .iter()
        .copied()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
    {
        return Some(canonical);
    }
    ALL_MARKER_TYPE_NAMES
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// Which of the three index vectors a marker occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerKind {
    /// `worldMarkers`.
    World,
    /// `mapMarkers` (minimap).
    Minimap,
    /// `lightMarkers`.
    Light,
}

impl MarkerKind {
    /// Every kind in upstream order.
    pub const ALL: [MarkerKind; 3] = [MarkerKind::World, MarkerKind::Minimap, MarkerKind::Light];
}

/// Reads a marker's stored index for `kind` (`ObjectiveMarker.world/minimap/light`).
pub fn marker_index(marker: &ObjectiveMarker, kind: MarkerKind) -> i32 {
    match marker {
        ObjectiveMarker::Point(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::ShapeText(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::Shape(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::Text(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::Line(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::Texture(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::Quad(m) => kind_index(kind, m.world, m.minimap, m.light),
        ObjectiveMarker::Light(m) => kind_index(kind, m.world, m.minimap, m.light),
    }
}

fn kind_index(kind: MarkerKind, world: i32, minimap: i32, light: i32) -> i32 {
    match kind {
        MarkerKind::World => world,
        MarkerKind::Minimap => minimap,
        MarkerKind::Light => light,
    }
}

/// Writes a marker's stored index for `kind`.
pub fn set_marker_index(marker: &mut ObjectiveMarker, kind: MarkerKind, value: i32) {
    let (world, minimap, light) = match marker {
        ObjectiveMarker::Point(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::ShapeText(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::Shape(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::Text(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::Line(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::Texture(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::Quad(m) => (&mut m.world, &mut m.minimap, &mut m.light),
        ObjectiveMarker::Light(m) => (&mut m.world, &mut m.minimap, &mut m.light),
    };
    match kind {
        MarkerKind::World => *world = value,
        MarkerKind::Minimap => *minimap = value,
        MarkerKind::Light => *light = value,
    }
}

/// `MapMarkers` (`MapMarkers.java`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapMarkers {
    /// `IntMap<ObjectiveMarker>` keyed by marker id (ascending iteration).
    pub map: BTreeMap<u32, ObjectiveMarker>,
    /// `worldMarkers`.
    pub world_markers: Vec<u32>,
    /// `mapMarkers`.
    pub map_markers: Vec<u32>,
    /// `lightMarkers`.
    pub light_markers: Vec<u32>,
}

impl MapMarkers {
    /// Empty container.
    pub fn new() -> Self {
        Self::default()
    }

    /// `MapMarkers.clear()`.
    pub fn clear(&mut self) {
        self.world_markers.clear();
        self.map_markers.clear();
        self.light_markers.clear();
        self.map.clear();
    }

    /// `MapMarkers.add(id, marker)`.
    pub fn add(&mut self, id: u32, marker: ObjectiveMarker) {
        let prev = self.map.insert(id, marker);
        for kind in MarkerKind::ALL {
            self.set_marker(kind, id, prev.as_ref());
        }
    }

    /// `MapMarkers.remove(id)`.
    pub fn remove(&mut self, id: u32) {
        if !self.map.contains_key(&id) {
            return;
        }
        for kind in MarkerKind::ALL {
            let index = marker_index(&self.map[&id], kind);
            if index == -1 {
                continue;
            }
            let list = Self::list_for(
                &mut self.world_markers,
                &mut self.map_markers,
                &mut self.light_markers,
                kind,
            );
            if let Some(&last) = list.last()
                && let Some(last_marker) = self.map.get_mut(&last)
            {
                set_marker_index(last_marker, kind, index);
            }
            list.remove(index as usize);
        }
        self.map.remove(&id);
    }

    /// `MapMarkers.get(id)`.
    pub fn get(&self, id: u32) -> Option<&ObjectiveMarker> {
        self.map.get(&id)
    }

    /// `MapMarkers.has(id)`.
    pub fn has(&self, id: u32) -> bool {
        self.map.contains_key(&id)
    }

    /// `MapMarkers.size()`.
    pub fn size(&self) -> usize {
        self.map.len()
    }

    /// `MapMarkers.setMarker` for one kind.
    fn set_marker(&mut self, kind: MarkerKind, id: u32, prev: Option<&ObjectiveMarker>) {
        let curr_index = marker_index(&self.map[&id], kind);
        let prev_index = prev.map(|marker| marker_index(marker, kind)).unwrap_or(-1);

        if prev.is_some() && prev_index != -1 {
            let list = Self::list_for(
                &mut self.world_markers,
                &mut self.map_markers,
                &mut self.light_markers,
                kind,
            );
            if curr_index != -1 {
                if let Some(marker) = self.map.get_mut(&id) {
                    set_marker_index(marker, kind, prev_index);
                }
                list[prev_index as usize] = id;
            } else {
                if let Some(&last) = list.last()
                    && let Some(last_marker) = self.map.get_mut(&last)
                {
                    set_marker_index(last_marker, kind, prev_index);
                }
                list.remove(prev_index as usize);
            }
        } else if curr_index != -1 {
            let list = Self::list_for(
                &mut self.world_markers,
                &mut self.map_markers,
                &mut self.light_markers,
                kind,
            );
            if let Some(marker) = self.map.get_mut(&id) {
                set_marker_index(marker, kind, list.len() as i32);
            }
            list.push(id);
        }
    }

    fn list_for<'a>(
        world: &'a mut Vec<u32>,
        map: &'a mut Vec<u32>,
        light: &'a mut Vec<u32>,
        kind: MarkerKind,
    ) -> &'a mut Vec<u32> {
        match kind {
            MarkerKind::World => world,
            MarkerKind::Minimap => map,
            MarkerKind::Light => light,
        }
    }

    /// `MapMarkers.updateMarker`: toggles `visible` and fixes the index.
    pub fn update_marker(&mut self, kind: MarkerKind, id: u32, visible: bool) {
        let Some(current) = self.map.get(&id) else {
            return;
        };
        let index = marker_index(current, kind);
        if (index != -1) == visible {
            return;
        }
        let list = Self::list_for(
            &mut self.world_markers,
            &mut self.map_markers,
            &mut self.light_markers,
            kind,
        );
        if !visible {
            if let Some(&last) = list.last()
                && let Some(last_marker) = self.map.get_mut(&last)
            {
                set_marker_index(last_marker, kind, index);
            }
            list.remove(index as usize);
            if let Some(marker) = self.map.get_mut(&id) {
                set_marker_index(marker, kind, -1);
            }
        } else {
            let size = list.len() as i32;
            list.push(id);
            if let Some(marker) = self.map.get_mut(&id) {
                set_marker_index(marker, kind, size);
            }
        }
    }

    /// `ObjectiveMarker.control(LMarkerControl, p1, p2, p3)`.
    ///
    /// Delegates the index toggles to [`Self::update_marker`] and the remaining
    /// fields to the concrete marker (NaN p1 returns immediately, like upstream).
    pub fn control(
        &mut self,
        id: u32,
        kind: crate::logic::enums::LMarkerControl,
        p1: f64,
        p2: f64,
        p3: f64,
    ) {
        use crate::logic::enums::LMarkerControl as C;
        if p1.is_nan() {
            return;
        }
        match kind {
            C::World => self.update_marker(MarkerKind::World, id, !approx_zero(p1)),
            C::Minimap => self.update_marker(MarkerKind::Minimap, id, !approx_zero(p1)),
            C::Light => self.update_marker(MarkerKind::Light, id, !approx_zero(p1)),
            C::Autoscale => {
                if let Some(marker) = self.map.get_mut(&id) {
                    set_autoscale(marker, !approx_zero(p1));
                }
            }
            C::DrawLayer => {
                if let Some(marker) = self.map.get_mut(&id) {
                    set_draw_layer(marker, p1 as f32);
                }
            }
            _ => {}
        }
        if let Some(marker) = self.map.get_mut(&id) {
            apply_marker_control(marker, kind, p1, p2, p3);
        }
    }

    /// `ObjectiveMarker.setText(text, fetch)`.
    pub fn set_text(&mut self, id: u32, text: &str, fetch: bool, locale: &dyn ObjectiveLocale) {
        let resolved = if fetch {
            locale.fetch_text(text)
        } else {
            text.to_owned()
        };
        if let Some(marker) = self.map.get_mut(&id) {
            match marker {
                ObjectiveMarker::ShapeText(m) => m.text = resolved,
                ObjectiveMarker::Text(m) => m.text = resolved,
                _ => {}
            }
        }
    }

    /// `ObjectiveMarker.setTexture(texture)`.
    pub fn set_texture(&mut self, id: u32, value: TextureValue) {
        if let Some(marker) = self.map.get_mut(&id) {
            match marker {
                ObjectiveMarker::Texture(m) => m.texture = value.into_holder(),
                ObjectiveMarker::Quad(m) => m.texture = value.into_holder(),
                _ => {}
            }
        }
    }

    /// `ObjectiveMarker.worldMarkers` count.
    pub fn world_count(&self) -> usize {
        self.world_markers.len()
    }

    /// `ObjectiveMarker.mapMarkers` count.
    pub fn map_count(&self) -> usize {
        self.map_markers.len()
    }

    /// `ObjectiveMarker.lightMarkers` count.
    pub fn light_count(&self) -> usize {
        self.light_markers.len()
    }
}

/// `ObjectiveMarker.fetchText(text)`.
pub fn fetch_text(text: &str, locale: &dyn ObjectiveLocale) -> String {
    locale.fetch_text(text)
}

/// `MapMarkers.write` payload: `i32` count + (`i32` id, `i32` JSON length, bytes).
///
/// Java writes an UBJson `IntMap` (`JsonIO.writeBytes`); plan 04 deliberately
/// did not port UBJson (deviation 5), so the Rust payload is the self-consistent
/// JSON-per-entry form. The region framing (count header + skip-to-end for
/// unknown readers) is unchanged.
impl crate::io::save::state::MarkersIo for MapMarkers {
    fn write_markers(&self, w: &mut WireWriter) -> IoResult<()> {
        w.i(self.map.len() as i32);
        for (id, marker) in &self.map {
            let json = JsonIo::write(marker)?;
            w.i(*id as i32);
            w.i(json.len() as i32);
            w.bytes(json.as_bytes());
        }
        Ok(())
    }
}

impl crate::io::save::state::MarkersSink for MapMarkers {
    fn read_markers(&mut self, r: &mut WireReader) -> IoResult<()> {
        self.clear();
        let count = r.i()?;
        if count < 0 {
            return Err(IoError::corrupt(format!("invalid marker count: {count}")));
        }
        for _ in 0..count {
            let id = r.i()? as u32;
            let length = r.i()?;
            if length < 0 || length as usize > MAX_BYTE_ARRAY {
                return Err(IoError::corrupt("objective marker too long"));
            }
            let string = std::str::from_utf8(r.bytes(length as usize)?)?;
            let marker: ObjectiveMarker = JsonIo::read(string)?;
            self.map.insert(id, marker);
        }
        self.reindex_from_sentinels();
        Ok(())
    }
}

impl MapMarkers {
    /// Rebuilds the three index vectors after a read, using the stored
    /// `world/minimap/light != -1` sentinels (`MapMarkers.read` second pass).
    fn reindex_from_sentinels(&mut self) {
        let ids: Vec<u32> = self.map.keys().copied().collect();
        for id in ids {
            let mut push = [false; 3];
            if let Some(marker) = self.map.get_mut(&id) {
                for (slot, kind) in MarkerKind::ALL.iter().enumerate() {
                    if marker_index(marker, *kind) != -1 {
                        push[slot] = true;
                    }
                }
            }
            if push[0] {
                let size = self.world_markers.len() as i32;
                if let Some(marker) = self.map.get_mut(&id) {
                    set_marker_index(marker, MarkerKind::World, size);
                }
                self.world_markers.push(id);
            }
            if push[1] {
                let size = self.map_markers.len() as i32;
                if let Some(marker) = self.map.get_mut(&id) {
                    set_marker_index(marker, MarkerKind::Minimap, size);
                }
                self.map_markers.push(id);
            }
            if push[2] {
                let size = self.light_markers.len() as i32;
                if let Some(marker) = self.map.get_mut(&id) {
                    set_marker_index(marker, MarkerKind::Light, size);
                }
                self.light_markers.push(id);
            }
        }
    }
}

/// `TextureHolder` setter value (`ObjectiveMarker.setTexture`).
#[derive(Debug, Clone, PartialEq)]
pub enum TextureValue {
    /// Atlas region/asset name.
    String(String),
    /// Unlockable content name.
    Content(String),
    /// Packed building position.
    Building(i32),
}

impl TextureValue {
    fn into_holder(self) -> TextureHolder {
        match self {
            TextureValue::String(value) => TextureHolder {
                string: Some(value),
                content: None,
                building: None,
            },
            TextureValue::Content(value) => TextureHolder {
                string: None,
                content: Some(value),
                building: None,
            },
            TextureValue::Building(value) => TextureHolder {
                string: None,
                content: None,
                building: Some(value),
            },
        }
    }
}

/// `Double`/`Mathf.equal(x, 0f)`.
fn approx_zero(value: f64) -> bool {
    (value as f32) == 0.0
}

/// `Pack.bitmask(flags, mask, value)`.
fn bitmask(flags: i8, mask: i8, value: bool) -> i8 {
    if value { flags | mask } else { flags & !mask }
}

/// `Color.fromDouble(value)` (`Color.rgba8888` of the low 32 raw bits).
fn color_from_double(value: f64) -> crate::io::json::ColorHex {
    let bits = value.to_bits() as u32;
    crate::io::json::ColorHex(Rgba::from_rgba8888(bits))
}

fn set_autoscale(marker: &mut ObjectiveMarker, value: bool) {
    match marker {
        ObjectiveMarker::Point(m) => m.autoscale = value,
        ObjectiveMarker::ShapeText(m) => m.autoscale = value,
        ObjectiveMarker::Shape(m) => m.autoscale = value,
        ObjectiveMarker::Text(m) => m.autoscale = value,
        ObjectiveMarker::Line(m) => m.autoscale = value,
        ObjectiveMarker::Texture(m) => m.autoscale = value,
        ObjectiveMarker::Quad(m) => m.autoscale = value,
        ObjectiveMarker::Light(m) => m.autoscale = value,
    }
}

fn set_draw_layer(marker: &mut ObjectiveMarker, value: f32) {
    match marker {
        ObjectiveMarker::Point(m) => m.draw_layer = value,
        ObjectiveMarker::ShapeText(m) => m.draw_layer = value,
        ObjectiveMarker::Shape(m) => m.draw_layer = value,
        ObjectiveMarker::Text(m) => m.draw_layer = value,
        ObjectiveMarker::Line(m) => m.draw_layer = value,
        ObjectiveMarker::Texture(m) => m.draw_layer = value,
        ObjectiveMarker::Quad(m) => m.draw_layer = value,
        ObjectiveMarker::Light(m) => m.draw_layer = value,
    }
}

/// Per-class `ObjectiveMarker.control` field updates.
fn apply_marker_control(
    marker: &mut ObjectiveMarker,
    kind: crate::logic::enums::LMarkerControl,
    p1: f64,
    p2: f64,
    p3: f64,
) {
    use crate::logic::enums::LMarkerControl as C;
    let tile = crate::config::TILESIZE as f32;
    match marker {
        ObjectiveMarker::Point(m) => match kind {
            C::Radius => m.radius = p1 as f32,
            C::Stroke => m.stroke = p1 as f32,
            C::Color => m.color = color_from_double(p1),
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            _ => {}
        },
        ObjectiveMarker::ShapeText(m) => match kind {
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            C::FontSize => m.font_size = p1 as f32,
            C::TextHeight => m.text_height = p1 as f32,
            C::TextAlign => m.text_align = p1 as i32,
            C::LineAlign => m.line_align = p1 as i32,
            C::Outline => m.flags = bitmask(m.flags, LABEL_FLAG_OUTLINE, !approx_zero(p1)),
            C::LabelFlags => {
                m.flags = bitmask(m.flags, LABEL_FLAG_BACKGROUND, !approx_zero(p1));
                if !p2.is_nan() {
                    m.flags = bitmask(m.flags, LABEL_FLAG_OUTLINE, !approx_zero(p2));
                }
            }
            C::Radius => m.radius = p1 as f32,
            C::Rotation => m.rotation = p1 as f32,
            C::Color => m.color = color_from_double(p1),
            C::Shape => m.sides = p1 as i32,
            _ => {}
        },
        ObjectiveMarker::Shape(m) => match kind {
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            C::Radius => m.radius = p1 as f32,
            C::Stroke => m.stroke = p1 as f32,
            C::Outline => m.outline = !approx_zero(p1),
            C::Rotation => m.rotation = p1 as f32,
            C::Color => m.color = color_from_double(p1),
            C::Shape => {
                m.sides = p1 as i32;
                if !p2.is_nan() {
                    m.fill = !approx_zero(p2);
                }
                if !p3.is_nan() {
                    m.outline = !approx_zero(p3);
                }
            }
            C::Arc => {
                m.start_angle = p1 as f32;
                if !p2.is_nan() {
                    m.end_angle = p2 as f32;
                }
            }
            _ => {}
        },
        ObjectiveMarker::Text(m) => match kind {
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            C::FontSize => m.font_size = p1 as f32,
            C::TextAlign => m.text_align = p1 as i32,
            C::LineAlign => m.line_align = p1 as i32,
            C::Outline => m.flags = bitmask(m.flags, LABEL_FLAG_OUTLINE, !approx_zero(p1)),
            C::LabelFlags => {
                m.flags = bitmask(m.flags, LABEL_FLAG_BACKGROUND, !approx_zero(p1));
                if !p2.is_nan() {
                    m.flags = bitmask(m.flags, LABEL_FLAG_OUTLINE, !approx_zero(p2));
                }
            }
            _ => {}
        },
        ObjectiveMarker::Line(m) => match kind {
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            C::EndPos => {
                if !p1.is_nan() {
                    m.end_pos.x = p1 as f32 * tile;
                }
                if !p2.is_nan() {
                    m.end_pos.y = p2 as f32 * tile;
                }
            }
            C::Stroke => m.stroke = p1 as f32,
            C::Color => {
                let color = color_from_double(p1);
                m.color1 = color;
                m.color2 = color;
            }
            C::Outline => m.outline = !approx_zero(p1),
            C::Posi => {
                if !p2.is_nan() {
                    match p1 as i32 {
                        0 => m.pos.x = p2 as f32 * tile,
                        1 => m.end_pos.x = p2 as f32 * tile,
                        _ => {}
                    }
                }
                if !p3.is_nan() {
                    match p1 as i32 {
                        0 => m.pos.y = p3 as f32 * tile,
                        1 => m.end_pos.y = p3 as f32 * tile,
                        _ => {}
                    }
                }
            }
            C::Colori if !p2.is_nan() => {
                let color = color_from_double(p2);
                match p1 as i32 {
                    0 => m.color1 = color,
                    1 => m.color2 = color,
                    _ => {}
                }
            }
            _ => {}
        },
        ObjectiveMarker::Texture(m) => match kind {
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            C::Rotation => m.rotation = p1 as f32,
            C::TextureSize => {
                m.width = p1 as f32 * tile;
                if !p2.is_nan() {
                    m.height = p2 as f32 * tile;
                }
            }
            C::Color => m.color = color_from_double(p1),
            _ => {}
        },
        ObjectiveMarker::Quad(m) => match kind {
            C::Color => {
                let float_bits = f32::from_bits(color_from_double(p1).0.to_rgba8888());
                for index in 0..4 {
                    m.vertices[index * 6 + 2] = float_bits;
                }
            }
            C::Pos => {
                m.vertices[0] = p1 as f32 * tile;
                if !p2.is_nan() {
                    m.vertices[1] = p2 as f32 * tile;
                }
            }
            C::Posi => {
                // QuadMarker stores vertices only; pos index 0 sets x0/y0.
                if (p1 as i32) == 0 {
                    if !p2.is_nan() {
                        m.vertices[0] = p2 as f32 * tile;
                    }
                    if !p3.is_nan() {
                        m.vertices[1] = p3 as f32 * tile;
                    }
                }
            }
            C::Colori => {
                if (p1 as i32) == 0
                    && !p2.is_nan()
                    && let Some(vertex) = m.vertices.get_mut(2)
                {
                    *vertex = f32::from_bits(color_from_double(p2).0.to_rgba8888());
                }
            }
            C::Uvi => {
                // UV mapping requires a resolved texture region (plan 16).
            }
            _ => {}
        },
        ObjectiveMarker::Light(m) => match kind {
            C::Pos => pos_control(&mut m.pos, p1, p2, tile),
            C::Radius => m.radius = p1 as f32,
            C::Color => m.color = color_from_double(p1),
            _ => {}
        },
    }
}

/// `PosMarker.control` position update (`pos` is in tiles).
fn pos_control(pos: &mut crate::io::json::objectives::Vec2, p1: f64, p2: f64, tile: f32) {
    if !p1.is_nan() {
        pos.x = p1 as f32 * tile;
    }
    if !p2.is_nan() {
        pos.y = p2 as f32 * tile;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::enums::LMarkerControl as C;

    fn point(world: i32, minimap: i32, light: i32) -> ObjectiveMarker {
        ObjectiveMarker::Point(crate::io::json::objectives::PointMarker {
            world,
            minimap,
            light,
            ..Default::default()
        })
    }

    #[test]
    fn add_fills_index_vectors_and_stores_indices() {
        let mut markers = MapMarkers::new();
        markers.add(10, point(1, -1, -1));
        markers.add(20, point(-1, 1, -1));
        markers.add(30, point(1, 1, 1));

        assert_eq!(markers.world_markers, vec![10, 30]);
        assert_eq!(markers.map_markers, vec![20, 30]);
        assert_eq!(markers.light_markers, vec![30]);
        assert_eq!(marker_index(markers.get(10).unwrap(), MarkerKind::World), 0);
        assert_eq!(marker_index(markers.get(30).unwrap(), MarkerKind::World), 1);
        assert_eq!(
            marker_index(markers.get(30).unwrap(), MarkerKind::Minimap),
            1
        );
    }

    #[test]
    fn remove_moves_last_into_gap_and_clears_sentinel() {
        let mut markers = MapMarkers::new();
        markers.add(1, point(1, -1, -1));
        markers.add(2, point(1, -1, -1));
        markers.add(3, point(1, -1, -1));
        // Remove the middle; marker 3 moves into index 1.
        markers.remove(2);
        assert_eq!(markers.world_markers, vec![1, 3]);
        assert_eq!(marker_index(markers.get(3).unwrap(), MarkerKind::World), 1);
        assert!(!markers.has(2));
        markers.remove(1);
        assert_eq!(markers.world_markers, vec![3]);
        assert_eq!(marker_index(markers.get(3).unwrap(), MarkerKind::World), 0);
    }

    #[test]
    fn update_marker_toggles_without_duplicating() {
        let mut markers = MapMarkers::new();
        markers.add(1, point(-1, -1, -1));
        assert!(markers.world_markers.is_empty());
        markers.update_marker(MarkerKind::World, 1, true);
        assert_eq!(markers.world_markers, vec![1]);
        // Idempotent when already visible.
        markers.update_marker(MarkerKind::World, 1, true);
        assert_eq!(markers.world_markers, vec![1]);
        markers.update_marker(MarkerKind::World, 1, false);
        assert!(markers.world_markers.is_empty());
        assert_eq!(marker_index(markers.get(1).unwrap(), MarkerKind::World), -1);
    }

    #[test]
    fn control_updates_base_and_pos_marker_fields() {
        let mut markers = MapMarkers::new();
        markers.add(7, point(1, -1, -1));
        markers.control(7, C::Pos, 3.0, 4.0, f64::NAN);
        markers.control(7, C::Radius, 9.0, f64::NAN, f64::NAN);
        let ObjectiveMarker::Point(p) = markers.get(7).unwrap() else {
            panic!("expected point");
        };
        assert_eq!(p.pos.x, 24.0);
        assert_eq!(p.pos.y, 32.0);
        assert_eq!(p.radius, 9.0);

        // World toggle off via control.
        markers.control(7, C::World, 0.0, f64::NAN, f64::NAN);
        assert!(markers.world_markers.is_empty());

        // NaN p1 returns immediately (no mutation).
        markers.control(7, C::Radius, f64::NAN, f64::NAN, f64::NAN);
    }

    #[test]
    fn write_read_roundtrip_rebuilds_vectors() {
        let mut markers = MapMarkers::new();
        markers.add(1, point(1, -1, -1));
        markers.add(2, point(-1, 1, 1));
        let mut buffer = Vec::new();
        {
            let mut writer = WireWriter::new(&mut buffer);
            crate::io::save::state::MarkersIo::write_markers(&markers, &mut writer).unwrap();
        }
        let mut restored = MapMarkers::new();
        {
            let mut reader = WireReader::new(&buffer);
            crate::io::save::state::MarkersSink::read_markers(&mut restored, &mut reader).unwrap();
        }
        assert_eq!(restored, markers);
    }

    #[test]
    fn type_name_registry_and_legacy_alias() {
        assert_eq!(marker_name_to_type("Minimap"), Some("point"));
        assert_eq!(marker_name_to_type("shapeText"), Some("shapeText"));
        assert_eq!(marker_name_to_type("nope"), None);
        assert_eq!(ALL_MARKER_TYPE_NAMES.len(), 8);
    }

    #[test]
    fn texture_holder_setters_round_trip_variants() {
        let mut markers = MapMarkers::new();
        markers.add(
            5,
            ObjectiveMarker::Texture(crate::io::json::objectives::TextureMarker {
                world: 1,
                minimap: -1,
                light: -1,
                ..Default::default()
            }),
        );
        markers.set_texture(5, TextureValue::String("white".to_owned()));
        let ObjectiveMarker::Texture(t) = markers.get(5).unwrap() else {
            panic!("expected texture");
        };
        assert_eq!(t.texture.string.as_deref(), Some("white"));
    }
}
