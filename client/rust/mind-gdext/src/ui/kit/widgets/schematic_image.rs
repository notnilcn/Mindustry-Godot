// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/SchematicsDialog.java
//         (`SchematicImage`, end of file) + game/Schematics.java:200-290
//         `getBuffer` (background, 1-px silhouette, plan regions, border).
//
//! The 200x200 schematic preview: tiled `schematic-background`, a soft black
//! silhouette built from a 1-px-per-tile image stretched with linear filtering,
//! each tile's `block-<name>-full` region (rotated when `rotate`), and the 4 px
//! border (Pal.gray, accent while the parent button is hovered).

use godot::builtin::{VarArray, VarDictionary, VariantType};
use godot::classes::canvas_item::TextureFilter;
use godot::classes::control::MouseFilter;
use godot::classes::image::Format;
use godot::classes::{Control, IControl, Image, ImageTexture, Texture2D};
use godot::prelude::*;

use crate::ui::kit;

/// `SchematicImage.thickness` (border width).
const THICKNESS: f32 = 4.0;
/// `Pal.gray` / `Pal.accent` borders.
const BORDER_GRAY: Color = Color::from_rgba(0.271, 0.271, 0.271, 1.0);
const BORDER_ACCENT: Color = Color::from_rgba(1.0, 0.827, 0.498, 1.0);
/// `Renderer.laserOpacity` setting key (percent).
const LASER_OPACITY_KEY: &str = "lasersopacity";

/// One `SchematicView.tiles` row (`SchematicTileView`).
#[derive(Clone)]
struct SchematicTile {
    /// Block content name (`block-<name>-full` region).
    block: String,
    x: i64,
    y: i64,
    /// Block footprint size (1/2/3/4).
    size: i64,
    /// Rotation 0-3.
    rotation: i64,
    /// `kind_rotates(def.kind)`: draw the plan region rotated.
    rotate: bool,
    /// Power-node laser targets as indices into the tile list.
    lasers: Vec<usize>,
}

/// `MindSchematicImage` — the schematic preview canvas.
#[derive(GodotClass)]
#[class(rename = RustMindSchematicImage, base=Control)]
pub struct MindSchematicImage {
    base: Base<Control>,
    width: i64,
    height: i64,
    tiles: Vec<SchematicTile>,
    hovered: bool,
    silhouette: Option<Gd<ImageTexture>>,
    /// `schematic-background` loaded from the assets dir (the sprite is not
    /// packed into the atlas; `MindAssets.assets_dir()` resolves the root).
    background: Option<Gd<Texture2D>>,
}

#[godot_api]
impl IControl for MindSchematicImage {
    fn init(base: Base<Control>) -> Self {
        // No base mutation here: the extension class is not bound during
        // `init` (PORTING.md); `ready` applies the filters.
        Self {
            base,
            width: 0,
            height: 0,
            tiles: Vec::new(),
            hovered: false,
            silhouette: None,
            background: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().set_texture_filter(TextureFilter::LINEAR);
    }

    fn draw(&mut self) {
        self.draw_image();
    }
}

#[godot_api]
impl MindSchematicImage {
    /// Binds one `SchematicView` row (`width`/`height`/`tiles`).
    #[func]
    pub fn setup(&mut self, schematic: VarDictionary) {
        self.width = dict_i64(&schematic, "width", 0);
        self.height = dict_i64(&schematic, "height", 0);
        self.tiles = parse_tiles(&schematic);
        self.silhouette = None;
        self.base_mut().queue_redraw();
    }

    /// Alias for [`Self::setup`] (the `set_schematic` call name).
    #[func]
    pub fn set_schematic(&mut self, schematic: VarDictionary) {
        self.setup(schematic);
    }

    /// Forces a redraw of the current schematic (`refresh` call name).
    #[func]
    pub fn refresh(&mut self) {
        self.base_mut().queue_redraw();
    }

    #[func]
    pub fn set_hovered(&mut self, value: bool) {
        if self.hovered == value {
            return;
        }
        self.hovered = value;
        self.base_mut().queue_redraw();
    }

    fn draw_image(&mut self) {
        if self.width <= 0 || self.height <= 0 {
            return;
        }
        let size = self.base().get_size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }
        // Tiled `schematic-background` under everything (atlas first, then the
        // raw PNG through `MindAssets.assets_dir()`; a flat dark backing is the
        // last resort).
        let background =
            kit::icon_texture("schematic-background").or_else(|| self.background_texture());
        if self.silhouette.is_none() {
            self.silhouette = self.build_silhouette();
        }
        let silhouette = self.silhouette.clone();
        let opacity = self.laser_opacity();
        let tiles = self.tiles.clone();
        let width = self.width as f32;
        let height = self.height as f32;
        match background {
            Some(texture) => {
                self.base_mut()
                    .draw_texture_rect(&texture, Rect2::new(Vector2::ZERO, size), true)
            }
            None => self.base_mut().draw_rect(
                Rect2::new(Vector2::ZERO, size),
                Color::from_rgba(0.05, 0.05, 0.05, 0.85),
            ),
        }
        // Tile area with one padding cell on each side (`getBuffer` padding 2/2).
        let resolution = (size.x / (width + 2.0)).min(size.y / (height + 2.0));
        let origin = Vector2::new(
            (size.x - width * resolution) / 2.0,
            (size.y - height * resolution) / 2.0,
        );
        if let Some(silhouette) = silhouette {
            // 1-px-per-tile image stretched over the padded area: the linear
            // filter turns each occupied cell into a soft shadow (`getBuffer`).
            self.base_mut().draw_texture_rect(
                &silhouette,
                Rect2::new(
                    origin - Vector2::new(resolution, resolution),
                    Vector2::new(width + 2.0, height + 2.0) * resolution,
                ),
                false,
            );
        }
        // Plan regions (`drawPlanRegion`: `fullIcon` at `plan.drawx()/drawy()`,
        // rotated by `rotation * 90` when the block rotates).
        for tile in &tiles {
            let texture = kit::icon_texture(&format!("block-{}-full", tile.block))
                .or_else(|| kit::icon_texture(&format!("block-{}-ui", tile.block)));
            let Some(texture) = texture else {
                continue;
            };
            let footprint = tile.size as f32;
            let center = tile_center(tile, origin, resolution, self.height);
            let half = Vector2::new(footprint, footprint) * resolution / 2.0;
            let rect = Rect2::new(-half, half * 2.0);
            if tile.rotate && tile.rotation != 0 {
                // Java angles are counter-clockwise in y-up space; Godot's
                // canvas is y-down, so the angle is negated.
                let angle = -godot::global::deg_to_rad(tile.rotation as f64 * 90.0) as f32;
                self.base_mut()
                    .draw_set_transform_ex(center)
                    .rotation(angle)
                    .scale(Vector2::ONE)
                    .done();
                self.base_mut().draw_texture_rect(&texture, rect, false);
                self.base_mut()
                    .draw_set_transform_ex(Vector2::ZERO)
                    .rotation(0.0)
                    .scale(Vector2::ONE)
                    .done();
            } else {
                self.base_mut().draw_texture_rect(
                    &texture,
                    Rect2::new(center - half, half * 2.0),
                    false,
                );
            }
        }
        // `plans.each(req -> req.block.drawPlanConfigTop(req, eachPlans))`:
        // power nodes draw a laser to every linked plan (`PowerNode.java:335-351`).
        self.draw_lasers(&tiles, origin, resolution, opacity);
        // 4 px border: Pal.gray normally, accent while hovered.
        let border = if self.hovered {
            BORDER_ACCENT
        } else {
            BORDER_GRAY
        };
        self.base_mut()
            .draw_rect_ex(Rect2::new(Vector2::ZERO, size), border)
            .filled(false)
            .width(THICKNESS)
            .done();
    }

    /// `PowerNode.drawPlanConfigTop` -> `drawLaser(.., false)` -> `Drawf.laser`
    /// (`PowerNode.java:183-197`, `Drawf.java:521-541`) in world units scaled by
    /// `resolution / tilesize`: endpoints pulled in by `size * 4 - 1.5`, the
    /// `laser` line (stroke `12 * laserScale`) between `laser-end` caps sized
    /// `region * laserScale * Draw.scl`; `setupColor(1)` is `laserColor1`
    /// (white) at `Renderer.laserOpacity`.
    fn draw_lasers(
        &mut self,
        tiles: &[SchematicTile],
        origin: Vector2,
        resolution: f32,
        opacity: f32,
    ) {
        let laser = kit::icon_texture("laser");
        let laser_end = kit::icon_texture("laser-end");
        if laser.is_none() && laser_end.is_none() {
            return;
        }
        let unit = resolution / 8.0;
        let color = Color::from_rgba(1.0, 1.0, 1.0, opacity);
        if color.a <= 0.0 {
            return;
        }
        let height = self.height;
        for tile in tiles {
            if tile.lasers.is_empty() {
                continue;
            }
            let from = tile_center(tile, origin, resolution, height);
            let size1 = tile.size as f32;
            for &target_index in &tile.lasers {
                let Some(other) = tiles.get(target_index) else {
                    continue;
                };
                let to = tile_center(other, origin, resolution, height);
                if from.distance_to(to) < 0.00001 {
                    continue;
                }
                let size2 = other.size as f32;
                let direction = (to - from).normalized();
                let p1 = from + direction * (size1 * 4.0 - 1.5) * unit;
                let p2 = to - direction * (size2 * 4.0 - 1.5) * unit;
                let angle = direction.angle();
                if let Some(laser_end) = &laser_end {
                    let cap = laser_end.get_size() * 0.25 * 0.25 * unit;
                    // `Draw.rect(start, x, y, .., rot + 180)` /
                    // `Draw.rect(end, x2, y2, .., rot)`.
                    self.base_mut()
                        .draw_set_transform_ex(p1)
                        .rotation(angle + std::f32::consts::PI)
                        .scale(Vector2::ONE)
                        .done();
                    self.base_mut()
                        .draw_texture_rect_ex(laser_end, Rect2::new(-cap / 2.0, cap), false)
                        .modulate(color)
                        .done();
                    self.base_mut()
                        .draw_set_transform_ex(p2)
                        .rotation(angle)
                        .scale(Vector2::ONE)
                        .done();
                    self.base_mut()
                        .draw_texture_rect_ex(laser_end, Rect2::new(-cap / 2.0, cap), false)
                        .modulate(color)
                        .done();
                }
                if let Some(laser) = &laser {
                    // `Lines.line(line, x + vx, y + vy, x2 - vx, y2 - vy, false)`,
                    // `vx = cos(rot) * 8 * scale * Draw.scl`.
                    let inset = direction * 0.5 * unit;
                    let start = p1 + inset;
                    let end = p2 - inset;
                    let length = start.distance_to(end);
                    let stroke = 3.0 * unit;
                    self.base_mut()
                        .draw_set_transform_ex(start)
                        .rotation(angle)
                        .scale(Vector2::ONE)
                        .done();
                    self.base_mut()
                        .draw_texture_rect_ex(
                            laser,
                            Rect2::new(
                                Vector2::new(0.0, -stroke / 2.0),
                                Vector2::new(length, stroke),
                            ),
                            false,
                        )
                        .modulate(color)
                        .done();
                }
                self.base_mut()
                    .draw_set_transform_ex(Vector2::ZERO)
                    .rotation(0.0)
                    .scale(Vector2::ONE)
                    .done();
            }
        }
    }

    /// `Renderer.laserOpacity` (`lasersopacity` setting / 100, default 100).
    fn laser_opacity(&self) -> f32 {
        let Some(mut ui) = kit::ui() else {
            return 1.0;
        };
        if !ui.has_method("settings_get_int") {
            return 1.0;
        }
        let value = ui
            .call(
                "settings_get_int",
                &[
                    GString::from(LASER_OPACITY_KEY).to_variant(),
                    100i64.to_variant(),
                ],
            )
            .try_to::<i64>()
            .unwrap_or(100);
        clampf(value as f32 / 100.0, 0.0, 1.0)
    }

    /// Loads `sprites/schematic-background.png` once through the asset root.
    fn background_texture(&mut self) -> Option<Gd<Texture2D>> {
        if let Some(texture) = &self.background {
            return Some(texture.clone());
        }
        let assets = kit::assets()?;
        let dir = assets.bind().assets_dir();
        let path = format!("{dir}/sprites/schematic-background.png");
        let image = Image::load_from_file(&path)?;
        let texture = ImageTexture::create_from_image(&image)?;
        let texture: Gd<Texture2D> = texture.upcast();
        self.background = Some(texture.clone());
        Some(texture)
    }

    /// One black pixel per footprint cell (`getBuffer`: for every `dx, dy` in
    /// the block size, `Fill.square(padding/2 + wx + 0.5, padding/2 + wy + 0.5,
    /// 0.5)` with `wx = x + dx - (size - 1) / 2`). The buffer is y-up, so image
    /// rows count down from the top (`row = height - wy`).
    fn build_silhouette(&self) -> Option<Gd<ImageTexture>> {
        let width = (self.width + 2) as i32;
        let height = (self.height + 2) as i32;
        if width <= 0 || height <= 0 {
            return None;
        }
        let mut image = Image::create(width, height, false, Format::RGBA8)?;
        image.fill(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        for tile in &self.tiles {
            let footprint = tile.size;
            let offset = -((footprint - 1) / 2);
            for dx in 0..footprint {
                for dy in 0..footprint {
                    let x = 1 + tile.x + dx + offset;
                    let y = self.height - (tile.y + dy + offset);
                    if x < 0
                        || y < 0
                        || x >= image.get_width() as i64
                        || y >= image.get_height() as i64
                    {
                        continue;
                    }
                    image.set_pixel(x as i32, y as i32, Color::from_rgba(0.0, 0.0, 0.0, 1.0));
                }
            }
        }
        ImageTexture::create_from_image(&image)
    }
}

/// `BuildPlan.drawx()/drawy()` in widget pixels: the tile centre plus
/// `Block.offset` (`(size + 1) % 2 * tilesize / 2`, half a tile for even
/// sizes). Java's preview is y-up (`Schematics.getBuffer`), so rows count up
/// from the bottom of the tile area.
fn tile_center(tile: &SchematicTile, origin: Vector2, resolution: f32, height: i64) -> Vector2 {
    let footprint = tile.size;
    let offset = if footprint % 2 == 0 { 0.5 } else { 0.0 };
    let tx = tile.x as f32 + 0.5 + offset;
    let ty = tile.y as f32 + 0.5 + offset;
    origin + Vector2::new(tx, height as f32 - ty) * resolution
}

/// Parses `schematic.tiles` into [`SchematicTile`] rows.
fn parse_tiles(schematic: &VarDictionary) -> Vec<SchematicTile> {
    let mut tiles = Vec::new();
    let Some(value) = schematic.get("tiles") else {
        return tiles;
    };
    let Ok(rows) = value.try_to::<VarArray>() else {
        return tiles;
    };
    for row in rows.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        let mut lasers = Vec::new();
        if let Some(value) = row.get("lasers")
            && let Ok(entries) = value.try_to::<VarArray>()
        {
            for entry in entries.iter_shared() {
                let index = variant_i64(&entry);
                if index >= 0 {
                    lasers.push(index as usize);
                }
            }
        }
        tiles.push(SchematicTile {
            block: dict_string(&row, "block", ""),
            x: dict_i64(&row, "x", 0),
            y: dict_i64(&row, "y", 0),
            size: dict_i64(&row, "size", 1),
            rotation: dict_i64(&row, "rotation", 0),
            rotate: dict_bool(&row, "rotate", false),
            lasers,
        });
    }
    tiles
}

/// GDScript `clampf` semantics (`min`/`max` are never swapped).
fn clampf(value: f32, min: f32, max: f32) -> f32 {
    if min > max {
        return value;
    }
    value.clamp(min, max)
}

/// `row.get(key, default)` as an integer.
fn dict_i64(dict: &VarDictionary, key: &str, fallback: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(fallback)
}

/// `row.get(key, default)` as a bool.
fn dict_bool(dict: &VarDictionary, key: &str, fallback: bool) -> bool {
    match dict.get(key) {
        Some(value) if value.get_type() == VariantType::BOOL => {
            value.try_to::<bool>().unwrap_or(fallback)
        }
        _ => fallback,
    }
}

/// `str(row.get(key, default))`.
fn dict_string(dict: &VarDictionary, key: &str, fallback: &str) -> String {
    match dict.get(key) {
        Some(value) if value.get_type() == VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_else(|_| fallback.to_string()),
        _ => fallback.to_string(),
    }
}

/// GDScript `int(value)` for JSON numbers.
fn variant_i64(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        _ => 0,
    }
}
