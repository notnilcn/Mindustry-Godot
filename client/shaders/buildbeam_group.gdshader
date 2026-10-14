// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/buildbeam.frag + screenspace.vert.
//
// CanvasGroup variant of `buildbeam.gdshader` (H03 T11): the beam pass draws
// Java's opaque beam geometry inside a `CanvasGroup`; this material runs on
// the group's baked texture, sampled via `screen_texture`, and applies the
// base-alpha modulation + moving stripe. The y-flip mapping matches
// `shield_group.gdshader`.

shader_type canvas_item;
render_mode unshaded, blend_mix;

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
	vec4 color = texture(screen_texture, SCREEN_UV);

	color.a *= (0.37 + abs(sin(u_time / 15.0)) * 0.05 + 0.2 * (step(mod(coords.x / u_dp + coords.y / u_dp + u_time / 4.0, 10.0), 3.0)));
	COLOR = color;
}
