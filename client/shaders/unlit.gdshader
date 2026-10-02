// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/unlit.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded;

uniform mat4 u_proj;
uniform mat4 u_trans;

varying vec4 v_col;

void vertex() {
	v_col = COLOR;
}

void fragment() {
	ALBEDO = v_col.rgb;
	ALPHA = v_col.a;
}
