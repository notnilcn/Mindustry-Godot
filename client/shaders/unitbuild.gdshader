// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/unitbuild.frag + default.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode unshaded, blend_mix;

uniform sampler2D u_texture;

uniform float u_time;
uniform float u_progress;
uniform vec4 u_color;
uniform vec2 u_uv;
uniform vec2 u_uv2;
uniform vec2 u_texsize;

varying vec4 v_color;
varying vec2 v_texCoords;

void vertex() {
	v_texCoords = UV;
	v_color = COLOR;
}

void fragment() {
	vec2 coords = (v_texCoords - u_uv) / (u_uv2 - u_uv);

	vec4 c = texture(u_texture, v_texCoords);
	float alpha = c.a;

	c.a *= u_progress;

	if (c.a > 0.01) {
		float f = step(abs(sin(coords.x * 2.0 + u_time)), 0.9);
		c = mix(c, u_color, f * u_color.a);
	}

	c.a *= alpha;

	COLOR = c * v_color;
}
