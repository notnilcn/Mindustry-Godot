// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/water.frag + screenspace.vert.
//
// CanvasGroup variant of `water.gdshader` (M-twin water fix): Java's
// `CacheLayer.ShaderLayer` (`CacheLayer.java:97-111`) draws the water floor
// cache into `renderer.effectBuffer` and blits it with `Shaders.water`. The
// port bakes the water floor band inside a `CanvasGroup`; this material runs
// on the group's baked texture via `screen_texture`. Java's `v_texCoords` is
// the effect-buffer coordinate (y up); Godot's `SCREEN_UV` is y-down, so the
// stripe math uses the flipped y. The sampling offset is x-only, so it needs
// no flip. Uniforms follow `Shaders.SurfaceShader.apply` (`Shaders.java:357-360`):
// `u_campos` = camera bottom-left, `u_resolution` = camera size (world units),
// `u_time` = `Time.time` (ticks).

shader_type canvas_item;
render_mode unshaded, blend_mix;

uniform sampler2D screen_texture : hint_screen_texture, repeat_disable, filter_nearest;
uniform vec2 u_campos;
uniform vec2 u_resolution = vec2(1.0);
uniform float u_time;

const float mscl = 40.0;
const float mth = 7.0;

void fragment() {
	vec2 c = vec2(SCREEN_UV.x, 1.0 - SCREEN_UV.y);
	vec2 v = vec2(1.0 / u_resolution.x, 1.0 / u_resolution.y);
	vec2 coords = vec2(c.x / v.x + u_campos.x, c.y / v.y + u_campos.y);

	float stime = u_time / 5.0;

	vec4 sampled = texture(screen_texture, SCREEN_UV + vec2(sin(stime / 3.0 + coords.y / 0.75) * v.x, 0.0));
	vec3 color = sampled.rgb * vec3(0.9, 0.9, 1.0);

	float tester = mod((coords.x + coords.y * 1.1 + sin(stime / 8.0 + coords.x / 5.0 - coords.y / 100.0) * 2.0) +
		sin(stime / 20.0 + coords.y / 3.0) * 1.0 +
		sin(stime / 10.0 - coords.y / 2.0) * 2.0 +
		sin(stime / 7.0 + coords.y / 1.0) * 0.5 +
		sin(coords.x / 3.0 + coords.y / 2.0) +
		sin(stime / 20.0 + coords.x / 4.0) * 1.0, mscl);

	if (tester < mth) {
		color *= 1.2;
	}

	COLOR = vec4(color.rgb, min(sampled.a * 100.0, 1.0));
}
