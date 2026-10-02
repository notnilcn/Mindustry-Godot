// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/planetgrid.frag + planetgrid.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded, blend_mix;

uniform mat4 u_proj;
uniform mat4 u_trans;
uniform vec3 u_mouse;

varying vec4 v_col;
varying vec3 v_position;

const vec4 shadow = vec4(0.0, 0.0, 0.0, 0.0);

void vertex() {
	v_col = COLOR;
	v_position = VERTEX;
}

void fragment() {
	ALBEDO = mix(v_col, shadow, distance(u_mouse, v_position)).rgb;
	ALPHA = mix(v_col, shadow, distance(u_mouse, v_position)).a;
}
