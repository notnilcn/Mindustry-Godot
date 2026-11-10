// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Effects and particles.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    /// Parses an `Effect` JSON value (`ContentParser.java:72-82`): a string
    /// names an `Fx` field, an array is a `MultiEffect` shorthand, and an object
    /// is an inline effect class (`Effect`/`MultiEffect`/`ExplosionEffect`/
    /// `WaveEffect`/`WrapEffect`) read by field.
    ///
    /// The renderer body is plan 17; this records the data fields on
    /// [`EffectSpec`]. Unknown names/classes warn and leave the field at its
    /// default rather than failing the whole unit.
    pub(super) fn parse_effect_ref(&mut self, file: &str, value: &Value) -> Option<EffectRef> {
        match value {
            Value::String(raw) => match effect_by_name(raw) {
                Some(effect) => Some(EffectRef::Named(effect)),
                None => {
                    self.warn(file, format!("unknown effect `{raw}`"));
                    None
                }
            },
            Value::Array(_) => {
                let mut children = Vec::new();
                if let Value::Array(entries) = value {
                    for entry in entries {
                        if let Some(child) = self.parse_effect_ref(file, entry) {
                            children.push(child);
                        }
                    }
                }
                // `MultiEffect(Effect...)` carries the `Effect` default lifetime;
                // the port's content generator derives the composite lifetime
                // from the longest child (`EffectSpec::multi`).
                let lifetime = children
                    .iter()
                    .map(EffectRef::lifetime)
                    .fold(0.0_f32, f32::max);
                let lifetime = if lifetime > 0.0 { lifetime } else { 50.0 };
                Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                    lifetime, children,
                ))))
            }
            Value::Object(object) => self.parse_inline_effect(file, object),
            _ => {
                self.warn(file, "effect values must be strings, arrays or objects");
                None
            }
        }
    }

    /// Reads one inline effect object (`Effect.class` field parser,
    /// `ContentParser.java:72-82`).
    pub(super) fn parse_inline_effect(
        &mut self,
        file: &str,
        object: &Map<String, Value>,
    ) -> Option<EffectRef> {
        // `ContentParser.java:79`: the default inline class is `ParticleEffect`.
        let class = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("ParticleEffect");
        let Some(tag) = self.classmap.resolve(class, ClassScope::Effect) else {
            self.warn(file, format!("unknown effect type `{class}`"));
            return None;
        };
        let mut spec = match tag {
            "Effect" => EffectSpec::plain(50.0),
            "MultiEffect" => {
                let mut children = Vec::new();
                if let Some(Value::Array(entries)) = object
                    .get("effects")
                    .or_else(|| object.get("children"))
                    .filter(|value| value.is_array())
                {
                    for entry in entries {
                        if let Some(child) = self.parse_effect_ref(file, entry) {
                            children.push(child);
                        }
                    }
                }
                let lifetime = children
                    .iter()
                    .map(EffectRef::lifetime)
                    .fold(0.0_f32, f32::max);
                EffectSpec::multi(if lifetime > 0.0 { lifetime } else { 50.0 }, children)
            }
            "ExplosionEffect" => {
                let mut effect = EffectSpec::explosion();
                // `ExplosionEffect()` (`ExplosionEffect.java:18-20`).
                effect.lifetime = 22.0;
                effect.clip = 100.0;
                if let Some(color) = object.get("waveColor").and_then(parse_color) {
                    effect.wave_color = Some(color);
                }
                if let Some(color) = object.get("smokeColor").and_then(parse_color) {
                    effect.smoke_color = Some(color);
                }
                if let Some(color) = object.get("sparkColor").and_then(parse_color) {
                    effect.spark_color = Some(color);
                }
                set_f32s(
                    object,
                    &mut [
                        ("waveLife", &mut effect.wave_life),
                        ("waveStroke", &mut effect.wave_stroke),
                        ("waveRad", &mut effect.wave_rad),
                        ("waveRadBase", &mut effect.wave_rad_base),
                        ("sparkStroke", &mut effect.spark_stroke),
                        ("sparkRad", &mut effect.spark_rad),
                        ("sparkLen", &mut effect.spark_len),
                        ("smokeSize", &mut effect.smoke_size),
                        ("smokeSizeBase", &mut effect.smoke_size_base),
                    ],
                );
                set_i32s(
                    object,
                    &mut [
                        ("smokes", &mut effect.smokes),
                        ("sparks", &mut effect.sparks),
                    ],
                );
                effect
            }
            "WaveEffect" => {
                let mut effect = EffectSpec::wave();
                effect.lifetime = 50.0;
                if let Some(color) = object.get("colorFrom").and_then(parse_color) {
                    effect.wave_color = Some(color);
                }
                if let Some(color) = object.get("colorTo").and_then(parse_color) {
                    effect.color_to = Some(color);
                }
                set_f32s(
                    object,
                    &mut [
                        ("sizeFrom", &mut effect.size_from),
                        ("sizeTo", &mut effect.size_to),
                        ("strokeFrom", &mut effect.stroke_from),
                        ("strokeTo", &mut effect.stroke_to),
                    ],
                );
                effect
            }
            "WrapEffect" => {
                let child = object
                    .get("effect")
                    .and_then(|value| self.parse_effect_ref(file, value));
                let Some(child) = child else {
                    self.warn(file, "`WrapEffect` requires an `effect`");
                    return None;
                };
                let color = object
                    .get("color")
                    .and_then(parse_color)
                    .unwrap_or(crate::content::Rgba::WHITE);
                // `WrapEffect.rotation` (degrees) replaces the caller's rotation
                // when the child is created (`WrapEffect.java:18,43-46`);
                // `init()` copies the child lifetime (`:29-32`).
                let rotation = field_f32(object, "rotation").unwrap_or(0.0);
                EffectSpec::wrap_rotated(child, color, rotation)
            }
            "ParticleEffect" => {
                let mut effect = EffectSpec::particle();
                self.read_particle_fields(file, object, &mut effect);
                effect
            }
            "RadialEffect" => {
                let mut effect = EffectSpec::radial();
                if let Some(child) = object
                    .get("effect")
                    .and_then(|value| self.parse_effect_ref(file, value))
                {
                    effect.children.push(child);
                }
                set_f32s(
                    object,
                    &mut [
                        ("rotationSpacing", &mut effect.rotation_spacing),
                        ("rotationOffset", &mut effect.rotation_offset),
                        ("effectRotationOffset", &mut effect.effect_rotation_offset),
                        ("lengthOffset", &mut effect.length_offset),
                    ],
                );
                if let Some(value) = field_i32(object, "amount") {
                    effect.amount = value;
                }
                effect
            }
            "SeqEffect" => {
                let mut effect = EffectSpec::seq();
                if let Some(Value::Array(entries)) = object.get("effects") {
                    for entry in entries {
                        if let Some(child) = self.parse_effect_ref(file, entry) {
                            effect.children.push(child);
                        }
                    }
                }
                // `SeqEffect.init()` (`SeqEffect.java:22-29`): lifetime is the
                // sum of the child lifetimes, clip the max child clip.
                effect.lifetime = effect.children.iter().map(EffectRef::lifetime).sum();
                effect.clip = effect
                    .children
                    .iter()
                    .map(|child| match child {
                        EffectRef::Inline(spec) => spec.clip,
                        EffectRef::Named(_) => 0.0,
                    })
                    .fold(100.0_f32, f32::max);
                effect
            }
            "SoundEffect" => {
                let mut effect = EffectSpec::sound_effect();
                if let Some(sound) = self.parse_sound(file, object.get("sound")) {
                    effect.sound = Some(sound);
                }
                if let Some(child) = object
                    .get("effect")
                    .and_then(|value| self.parse_effect_ref(file, value))
                {
                    // `SoundEffect.init()`: unset `startDelay` inherits the child.
                    if effect.start_delay < 0.0 {
                        if let EffectRef::Inline(spec) = &child {
                            effect.start_delay = spec.start_delay;
                        } else {
                            effect.start_delay = 0.0;
                        }
                    }
                    effect.children.push(child);
                }
                set_f32s(
                    object,
                    &mut [
                        ("minPitch", &mut effect.min_pitch),
                        ("maxPitch", &mut effect.max_pitch),
                        ("minVolume", &mut effect.min_volume),
                        ("maxVolume", &mut effect.max_volume),
                    ],
                );
                effect
            }
            "NoiseEffect" => {
                let mut effect = EffectSpec::noise();
                if let Some(path) = object.get("noisePath").and_then(Value::as_str) {
                    effect.noise_path = path.to_owned();
                }
                if let Some(color) = object.get("color").and_then(parse_color) {
                    effect.color = Some(color);
                }
                set_f32s(
                    object,
                    &mut [
                        ("noiseScl", &mut effect.noise_scl),
                        ("opacity", &mut effect.opacity),
                        ("baseSpeed", &mut effect.base_speed),
                        ("intensity", &mut effect.intensity),
                        ("windX", &mut effect.wind_x),
                        ("windY", &mut effect.wind_y),
                        ("layerSpeedMul", &mut effect.layer_speed_mul),
                        ("layerAlphaMul", &mut effect.layer_alpha_mul),
                        ("layerSclMul", &mut effect.layer_scl_mul),
                        ("layerColorMul", &mut effect.layer_color_mul),
                    ],
                );
                if let Some(value) = field_i32(object, "layers") {
                    effect.layers = value;
                }
                effect
            }
            other => {
                self.warn(
                    file,
                    format!("inline effect `{other}` has no port metadata model yet; ignored"),
                );
                return None;
            }
        };
        // `Effect.lifetime`/`clip` are plain fields upstream; readFields lets
        // any inline class override them after construction.
        if let Some(value) = field_f32(object, "lifetime") {
            spec.lifetime = value;
        }
        if let Some(value) = field_f32(object, "clip") {
            spec.clip = value;
        }
        if let Some(value) = field_f32(object, "startDelay") {
            spec.start_delay = value;
        }
        if let Some(value) = field_f32(object, "baseRotation") {
            spec.base_rotation = value;
        }
        // `Effect.rotWithParent`/`followParent` are runtime flags with no
        // renderer half in the port yet; accept them to keep parity warnings
        // quiet (unknown fields warn upstream only with `ignoreUnknownFields`).
        let _ = field_bool(object, "followParent");
        let _ = field_bool(object, "rotWithParent");
        if matches!(spec.kind, InlineEffectKind::WrapEffect) {
            // `WrapEffect.init()` copies the child lifetime.
            if let Some(child) = spec.children.first() {
                spec.lifetime = child.lifetime();
            }
        }
        Some(EffectRef::Inline(Box::new(spec)))
    }

    /// Reads the `ParticleEffect` field set (`ParticleEffect.java:14-65`).
    pub(super) fn read_particle_fields(
        &mut self,
        file: &str,
        object: &Map<String, Value>,
        effect: &mut EffectSpec,
    ) {
        set_f32s(
            object,
            &mut [
                ("cone", &mut effect.cone),
                ("length", &mut effect.length),
                ("baseLength", &mut effect.base_length),
                ("offsetX", &mut effect.offset_x),
                ("offsetY", &mut effect.offset_y),
                ("lightScl", &mut effect.light_scl),
                ("lightOpacity", &mut effect.light_opacity),
                ("spin", &mut effect.particle_spin),
                ("sizeFrom", &mut effect.size_from),
                ("sizeTo", &mut effect.size_to),
                ("sizeChangeStart", &mut effect.size_change_start),
                ("widthChangeStart", &mut effect.width_change_start),
                ("heightChangeStart", &mut effect.height_change_start),
                ("offset", &mut effect.particle_offset),
                ("widthFrom", &mut effect.width_from),
                ("widthTo", &mut effect.width_to),
                ("heightFrom", &mut effect.height_from),
                ("heightTo", &mut effect.height_to),
                ("strokeFrom", &mut effect.stroke_from),
                ("strokeTo", &mut effect.stroke_to),
                ("lenFrom", &mut effect.len_from),
                ("lenTo", &mut effect.len_to),
            ],
        );
        if let Some(value) = field_i32(object, "particles") {
            effect.particles = value;
        }
        set_bools(
            object,
            &mut [
                ("randLength", &mut effect.rand_length),
                ("casingFlip", &mut effect.casing_flip),
                ("useRotation", &mut effect.use_rotation),
                ("line", &mut effect.line),
                ("cap", &mut effect.cap),
            ],
        );
        if let Some(color) = object.get("colorFrom").and_then(parse_color) {
            effect.particle_color_from = Some(color);
        }
        if let Some(color) = object.get("colorTo").and_then(parse_color) {
            effect.color_to = Some(color);
        }
        if let Some(color) = object.get("lightColor").and_then(parse_color) {
            effect.light_color = Some(color);
        }
        if let Some(region) = object.get("region").and_then(Value::as_str) {
            effect.region = region.to_owned();
        }
        if let Some(interp) = object.get("interp").and_then(Value::as_str) {
            effect.interp = parse_interp(interp);
        }
        for (field, target) in [
            ("sizeInterp", &mut effect.size_interp),
            ("colorInterp", &mut effect.color_interp),
            ("widthInterp", &mut effect.width_interp),
            ("heightInterp", &mut effect.height_interp),
        ] {
            if let Some(interp) = object.get(field).and_then(Value::as_str) {
                *target = Some(parse_interp(interp));
            }
        }
        let _ = file;
    }
}
