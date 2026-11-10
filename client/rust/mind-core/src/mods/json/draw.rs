// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block drawers and unit draw parts.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    // ---- shared helpers ----

    /// Parses the `Block.drawer` JSON value (`ContentParser.java:182-198`):
    /// a class-name string, an array shorthand for `DrawMulti`, or an object
    /// with `type` (default `DrawDefault`) and fields.
    pub(super) fn parse_drawer(&mut self, file: &str, value: &Value) -> Option<DrawerSpec> {
        match value {
            Value::String(class) => match self.resolve_drawer_class(class) {
                Some(kind) => Some(DrawerSpec::for_kind(kind)),
                None => {
                    self.warn(file, format!("unknown drawer `{class}`"));
                    None
                }
            },
            Value::Array(entries) => {
                let mut spec = DrawerSpec::for_kind(DrawerKind::Multi);
                for entry in entries {
                    if let Some(child) = self.parse_drawer(file, entry) {
                        spec.children.push(child);
                    }
                }
                Some(spec)
            }
            Value::Object(object) => {
                let class = object
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("DrawDefault");
                let Some(kind) = self.resolve_drawer_class(class) else {
                    self.warn(file, format!("unknown drawer `{class}`"));
                    return None;
                };
                let mut spec = DrawerSpec::for_kind(kind);
                for (field, target) in [
                    ("spinSprite", &mut spec.spin_sprite),
                    ("drawPlan", &mut spec.draw_plan),
                    ("buildingRotate", &mut spec.building_rotate),
                ] {
                    if let Some(value) = field_bool(object, field) {
                        *target = value;
                    }
                }
                for (field, target) in [
                    ("x", &mut spec.x),
                    ("y", &mut spec.y),
                    ("rotation", &mut spec.rotation),
                    ("rotateSpeed", &mut spec.rotate_speed),
                    ("layer", &mut spec.layer),
                    ("alpha", &mut spec.alpha),
                    ("scale", &mut spec.scale),
                    ("pulse", &mut spec.pulse),
                    ("pulseScl", &mut spec.pulse_scl),
                    ("flameRadius", &mut spec.flame_radius),
                ] {
                    if let Some(value) = field_f32(object, field) {
                        *target = value;
                    }
                }
                if let Some(value) = field_f32(object, "heatPulse") {
                    spec.pulse = value;
                }
                if let Some(value) = field_f32(object, "heatPulseScl") {
                    spec.pulse_scl = value;
                }
                if let Some(color) = object.get("color").and_then(parse_color) {
                    spec.color = Some(color);
                }
                if let Some(color) = object.get("flameColor").and_then(parse_color) {
                    spec.flame_color = Some(color);
                }
                if let Some(value) = object.get("suffix").and_then(Value::as_str) {
                    spec.suffix = value.to_owned();
                }
                if let Some(value) = object.get("name").and_then(Value::as_str) {
                    spec.name = Some(value.to_owned());
                }
                if let Some(value) = object.get("basePrefix").and_then(Value::as_str) {
                    spec.base_prefix = value.to_owned();
                }
                if let Some(value) = object
                    .get("liquid")
                    .or_else(|| object.get("drawLiquid"))
                    .and_then(Value::as_str)
                {
                    spec.liquid = Some(value.to_owned());
                }
                if let Some(drawers) = object.get("drawers") {
                    match drawers {
                        Value::Array(entries) => {
                            for entry in entries {
                                if let Some(child) = self.parse_drawer(file, entry) {
                                    spec.children.push(child);
                                }
                            }
                        }
                        _ => self.warn(file, "`drawers` must be an array"),
                    }
                }
                if let Some(parts) = object.get("parts") {
                    spec.parts = self.parse_draw_parts(file, parts);
                }
                Some(spec)
            }
            _ => {
                self.warn(file, "`drawer` must be a string, array or object");
                None
            }
        }
    }

    /// Resolves a drawer class name. Most aliases live in the `Drawer` scope;
    /// the committed ClassMap lists a few `world/draw` classes under the
    /// `DrawPart` scope (J09 T1), so both are accepted.
    pub(super) fn resolve_drawer_class(&self, class: &str) -> Option<DrawerKind> {
        self.classmap
            .resolve(class, ClassScope::Drawer)
            .and_then(resolve_drawer_kind)
            .or_else(|| {
                self.classmap
                    .resolve(class, ClassScope::DrawPart)
                    .and_then(resolve_drawer_kind)
            })
    }

    /// Parses a `DrawPart[]` (`Weapon.parts`, `DrawBlockParts.parts`,
    /// `DrawTurret.parts`; `ContentParser.java:212-218`).
    pub(super) fn parse_draw_parts(&mut self, file: &str, value: &Value) -> Vec<DrawPartSpec> {
        let Value::Array(entries) = value else {
            self.warn(file, "`parts` must be an array");
            return Vec::new();
        };
        let mut parts = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(object) = entry.as_object() else {
                self.warn(file, "draw-part entries must be objects");
                continue;
            };
            parts.push(self.parse_draw_part(file, object));
        }
        parts
    }

    /// Reads one `DrawPart` (`entities/part/*`). The class defaults come from
    /// [`DrawPartSpec::for_kind`]; fields not modelled yet are ignored like
    /// upstream `ignoreUnknownFields`.
    pub(super) fn parse_draw_part(
        &mut self,
        file: &str,
        object: &Map<String, Value>,
    ) -> DrawPartSpec {
        let class = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("RegionPart");
        let kind = self
            .classmap
            .resolve(class, ClassScope::Part)
            .and_then(resolve_draw_part_kind)
            .unwrap_or_else(|| {
                self.warn(
                    file,
                    format!("unknown draw part `{class}`; using RegionPart"),
                );
                DrawPartKind::RegionPart
            });
        let mut part = DrawPartSpec::for_kind(kind);
        for (field, target) in [
            ("under", &mut part.under),
            ("turretShading", &mut part.turret_shading),
            ("mirror", &mut part.mirror),
            ("outline", &mut part.outline),
            ("drawRegion", &mut part.draw_region),
            ("heatLight", &mut part.heat_light),
            ("clampProgress", &mut part.clamp_progress),
            ("replaceOutline", &mut part.replace_outline),
            ("followRotation", &mut part.follow_rotation),
            ("circle", &mut part.circle),
            ("hollow", &mut part.hollow),
            ("tri", &mut part.tri),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("weaponIndex", &mut part.weapon_index),
            ("recoilIndex", &mut part.recoil_index),
            ("sides", &mut part.sides),
            ("circles", &mut part.circles),
            ("shapes", &mut part.shapes),
        ] {
            if let Some(value) = field_i32(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("layer", &mut part.layer),
            ("layerOffset", &mut part.layer_offset),
            ("heatLayerOffset", &mut part.heat_layer_offset),
            ("outlineLayerOffset", &mut part.outline_layer_offset),
            ("x", &mut part.x),
            ("y", &mut part.y),
            ("xScl", &mut part.x_scl),
            ("yScl", &mut part.y_scl),
            ("rotation", &mut part.rotation),
            ("originX", &mut part.origin_x),
            ("originY", &mut part.origin_y),
            ("moveX", &mut part.move_x),
            ("moveY", &mut part.move_y),
            ("growX", &mut part.grow_x),
            ("growY", &mut part.grow_y),
            ("moveRot", &mut part.move_rot),
            ("heatLightOpacity", &mut part.heat_light_opacity),
            ("radius", &mut part.radius),
            ("radiusTo", &mut part.radius_to),
            ("stroke", &mut part.stroke),
            ("strokeTo", &mut part.stroke_to),
            ("rotateSpeed", &mut part.rotate_speed),
            ("phase", &mut part.phase),
            ("minStroke", &mut part.min_stroke),
            ("innerScl", &mut part.inner_scl),
            ("innerRadScl", &mut part.inner_rad_scl),
            ("rotMove", &mut part.rot_move),
            ("spinSpeed", &mut part.spin_speed),
            ("triLength", &mut part.tri_length),
            ("triLengthTo", &mut part.tri_length_to),
            ("haloRadius", &mut part.halo_radius),
            ("haloRadiusTo", &mut part.halo_radius_to),
            ("shapeRotation", &mut part.shape_rotation),
            ("shapeMoveRot", &mut part.shape_move_rot),
            ("haloRotateSpeed", &mut part.halo_rotate_speed),
            ("haloRotation", &mut part.halo_rotation),
        ] {
            if let Some(value) = field_f32(object, field) {
                *target = value;
            }
        }
        for (field, target) in [
            ("heatColor", &mut part.heat_color),
            ("color", &mut part.color),
            ("colorTo", &mut part.color_to),
            ("mixColor", &mut part.mix_color),
            ("mixColorTo", &mut part.mix_color_to),
        ] {
            if let Some(color) = object.get(field).and_then(parse_color) {
                *target = Some(color);
            }
        }
        if let Some(color) = object.get("color1").and_then(parse_color) {
            part.color1 = color;
        }
        if let Some(color) = object.get("color2").and_then(parse_color) {
            part.color2 = color;
        }
        if let Some(suffix) = object.get("suffix").and_then(Value::as_str) {
            part.suffix = suffix.to_owned();
        }
        if let Some(name) = object.get("name").and_then(Value::as_str) {
            part.name = Some(name.to_owned());
        }
        if let Some(blending) = object.get("blending").and_then(Value::as_str) {
            part.blending = match blending {
                "additive" => BlendingKind::Additive,
                "normal" => BlendingKind::Normal,
                other => {
                    self.warn(file, format!("unknown blending `{other}`; using normal"));
                    BlendingKind::Normal
                }
            };
        }
        for (field, target) in [
            ("progress", &mut part.progress),
            ("growProgress", &mut part.grow_progress),
            ("heatProgress", &mut part.heat_progress),
        ] {
            if let Some(value) = object.get(field) {
                *target = self.parse_part_progress(file, value);
            }
        }
        if let Some(value) = object.get("children") {
            part.children = self.parse_draw_parts(file, value);
        }
        if let Some(Value::Array(moves)) = object.get("moves") {
            for entry in moves {
                let Some(map) = entry.as_object() else {
                    continue;
                };
                let progress = map
                    .get("progress")
                    .map_or(PartProgressSpec::Warmup, |value| {
                        self.parse_part_progress(file, value)
                    });
                part.moves.push(PartMoveSpec {
                    progress,
                    x: field_f32(map, "x").unwrap_or(0.0),
                    y: field_f32(map, "y").unwrap_or(0.0),
                    gx: field_f32(map, "gx").unwrap_or(0.0),
                    gy: field_f32(map, "gy").unwrap_or(0.0),
                    rot: field_f32(map, "rot").unwrap_or(0.0),
                });
            }
        }
        part
    }

    /// Reads a `PartProgress` (`ContentParser.parseProgressOp`,
    /// `ContentParser.java:1174-1200`): a constant name, a number
    /// (`PartProgress.constant`), or a base `type` plus `operation`/`op` or an
    /// `operations`/`ops` chain.
    pub(super) fn parse_part_progress(&mut self, file: &str, value: &Value) -> PartProgressSpec {
        match value {
            Value::String(name) => part_progress_constant(name).unwrap_or_else(|| {
                self.warn(file, format!("unknown PartProgress `{name}`; using warmup"));
                PartProgressSpec::Warmup
            }),
            Value::Number(number) => {
                PartProgressSpec::Constant(number.as_f64().unwrap_or(0.0) as f32)
            }
            Value::Object(object) => {
                let Some(base_name) = object.get("type").and_then(Value::as_str) else {
                    self.warn(file, "PartProgress objects need a `type`");
                    return PartProgressSpec::Warmup;
                };
                let mut progress = part_progress_constant(base_name).unwrap_or_else(|| {
                    self.warn(
                        file,
                        format!("unknown PartProgress `{base_name}`; using warmup"),
                    );
                    PartProgressSpec::Warmup
                });
                if let Some(operation) = object
                    .get("operation")
                    .or_else(|| object.get("op"))
                    .and_then(Value::as_str)
                {
                    progress = self.apply_part_progress_op(file, progress, operation, object);
                } else if let Some(ops) = object.get("operations").or_else(|| object.get("ops")) {
                    if let Value::Array(entries) = ops {
                        for entry in entries {
                            let Some(map) = entry.as_object() else {
                                continue;
                            };
                            let Some(operation) = map
                                .get("operation")
                                .or_else(|| map.get("op"))
                                .and_then(Value::as_str)
                            else {
                                continue;
                            };
                            progress = self.apply_part_progress_op(file, progress, operation, map);
                        }
                    } else {
                        self.warn(file, "chained PartProgress operations must be an array");
                    }
                }
                progress
            }
            _ => {
                self.warn(file, "PartProgress must be a string, number or object");
                PartProgressSpec::Warmup
            }
        }
    }

    /// Applies one `PartProgress` operation (`ContentParser.parseProgressOp`).
    pub(super) fn apply_part_progress_op(
        &mut self,
        file: &str,
        base: PartProgressSpec,
        operation: &str,
        object: &Map<String, Value>,
    ) -> PartProgressSpec {
        let amount = |key: &str| field_f32(object, key).unwrap_or(0.0);
        let other = |parser: &mut Self, object: &Map<String, Value>| {
            object
                .get("other")
                .map_or(PartProgressSpec::Warmup, |value| {
                    parser.parse_part_progress(file, value)
                })
        };
        let boxed = Box::new(base);
        match operation {
            "inv" => PartProgressSpec::Inv(boxed),
            "slope" => PartProgressSpec::Slope(boxed),
            "clamp" => PartProgressSpec::Clamp(boxed),
            "delay" => PartProgressSpec::Delay(boxed, amount("amount")),
            "sustain" => PartProgressSpec::Sustain(
                boxed,
                amount("offset"),
                amount("grow"),
                amount("sustain"),
            ),
            "shorten" => PartProgressSpec::Shorten(boxed, amount("amount")),
            "compress" => PartProgressSpec::Compress(boxed, amount("start"), amount("end")),
            "add" => {
                if object.contains_key("other") {
                    PartProgressSpec::AddProgress(boxed, Box::new(other(self, object)))
                } else {
                    PartProgressSpec::Add(boxed, amount("amount"))
                }
            }
            "blend" => {
                PartProgressSpec::Blend(boxed, Box::new(other(self, object)), amount("amount"))
            }
            "mul" => {
                if object.contains_key("other") {
                    PartProgressSpec::MulProgress(boxed, Box::new(other(self, object)))
                } else {
                    PartProgressSpec::Mul(boxed, amount("amount"))
                }
            }
            "min" => PartProgressSpec::Min(boxed, Box::new(other(self, object))),
            "sin" => {
                let offset = if object.contains_key("offset") {
                    amount("offset")
                } else {
                    0.0
                };
                PartProgressSpec::Sin(boxed, offset, amount("scl"), amount("mag"))
            }
            "absin" => PartProgressSpec::Absin(boxed, amount("scl"), amount("mag")),
            "mod" => PartProgressSpec::Mod(boxed, amount("amount")),
            "loop" => PartProgressSpec::Loop(boxed, amount("time")),
            "curve" => {
                if let Some(interp) = object.get("interp").and_then(Value::as_str) {
                    PartProgressSpec::CurveInterp(boxed, parse_interp(interp))
                } else {
                    PartProgressSpec::CurveRange(boxed, amount("offset"), amount("duration"))
                }
            }
            other => {
                self.warn(
                    file,
                    format!("unknown PartProgress operation `{other}`; ignored"),
                );
                *boxed
            }
        }
    }
}
