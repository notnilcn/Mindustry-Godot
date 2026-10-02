// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/clouds.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded, cull_disabled;

uniform mat4 u_proj;
uniform mat4 u_trans;
uniform vec3 u_lightdir;
uniform vec3 u_ambientColor;
uniform float u_alpha;

varying vec4 v_col;

const vec3 diffuse = vec3(0.01);

void vertex() {
	vec3 norc = u_ambientColor * (diffuse + vec3(clamp((dot(NORMAL, u_lightdir) + 1.0) / 2.0, 0.0, 1.0)));
	v_col = COLOR * vec4(norc, u_alpha);
}

void fragment() {
	ALBEDO = v_col.rgb;
	ALPHA = v_col.a;
}
