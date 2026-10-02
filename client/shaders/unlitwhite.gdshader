// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/unlitwhite.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded;

uniform mat4 u_proj;
uniform mat4 u_trans;
uniform vec4 u_color;

varying vec4 v_col;

void vertex() {
	v_col = vec4(1.0);
}

void fragment() {
	ALBEDO = v_col.rgb * u_color.rgb;
	ALPHA = v_col.a * u_color.a;
}
