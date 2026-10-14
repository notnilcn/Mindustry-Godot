// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/shield.frag + screenspace.vert.
//
// CanvasGroup variant of `shield.gdshader` (H03 T11): the pass node draws the
// shield geometry (opaque `team.color.lerp(white, hit)` fills) inside a
// `CanvasGroup`; this material runs on the group's baked texture, sampled via
// `screen_texture`. Java's `v_texCoords` is the effect-buffer coordinate
// (y up); Godot's `SCREEN_UV` is y-down, so the y-flip is applied for the
// `coords` stripe math and every sampling offset negates its y component.

shader_type canvas_item;
render_mode unshaded, blend_mix;

#define ALPHA 0.18
#define step 2.0

uniform sampler2D screen_texture : hint_screen_texture, filter_nearest;
uniform vec2 u_texsize;
uniform vec2 u_invsize;
uniform float u_time;
uniform float u_dp;
uniform vec2 u_offset;

void fragment() {
	// `screenspace.vert`: `vec2 T = vec2(v_texCoords.x, 1.0 - v_texCoords.y)`.
	vec2 T = vec2(SCREEN_UV.x, 1.0 - SCREEN_UV.y);
	vec2 coords = (T * u_texsize) + u_offset;

	// `T += vec2(sin(coords.y / 3.0 + u_time / 20.0), sin(coords.x / 3.0 + u_time / 20.0)) / u_texsize`.
	vec2 wavy = vec2(sin(coords.y / 3.0 + u_time / 20.0), -sin(coords.x / 3.0 + u_time / 20.0)) / u_texsize;
	vec2 vu = SCREEN_UV + wavy;

	vec4 color = texture(screen_texture, vu);
	vec2 v = u_invsize;

	// `T + vec2(0, +/-step) * v` with the y axis negated.
	vec4 maxed = max(max(max(
		texture(screen_texture, vu + vec2(0.0, -step) * v),
		texture(screen_texture, vu + vec2(0.0, step) * v)),
		texture(screen_texture, vu + vec2(step, 0.0) * v)),
		texture(screen_texture, vu + vec2(-step, 0.0) * v));

	if (texture(screen_texture, vu).a < 0.9 && maxed.a > 0.9) {
		// Bright outline: `vec4(maxed.rgb, maxed.a * 100.0)`.
		COLOR = vec4(maxed.rgb, maxed.a * 100.0);
	} else {
		if (color.a > 0.0) {
			if (mod(coords.x / u_dp + coords.y / u_dp + sin(coords.x / u_dp / 5.0) * 3.0 + sin(coords.y / u_dp / 5.0) * 3.0 + u_time / 4.0, 10.0) < 2.0) {
				color *= 1.65;
			}

			color.a = ALPHA;
		}

		COLOR = color;
	}
}
