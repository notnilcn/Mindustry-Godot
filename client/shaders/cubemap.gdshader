// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/cubemap.frag + cubemap.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never;

uniform mat4 u_proj;
uniform samplerCube u_cubemap;

varying vec3 v_texCoords;

const float SCALE = 50.0;

void vertex() {
	v_texCoords = VERTEX;
	VERTEX *= SCALE;
}

void fragment() {
	ALBEDO = texture(u_cubemap, v_texCoords).rgb;
}
