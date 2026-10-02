// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/default.frag + default.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode blend_mix;

uniform sampler2D u_texture;
uniform mat4 u_projTrans;
uniform vec2 u_viewportInverse;

varying vec4 v_color;
varying vec4 v_mix_color;
varying vec2 v_texCoords;

void vertex() {
	v_texCoords = UV;
	v_color = COLOR;
	v_mix_color = vec4(0.0);
}

void fragment() {
	vec4 c = texture(u_texture, v_texCoords);
	COLOR = v_color * mix(c, vec4(v_mix_color.rgb, c.a), v_mix_color.a);
}
