// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/fog.frag + default.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode unshaded, blend_mix;

#define QUANT 0.3

uniform sampler2D u_texture;

varying vec4 v_color;
varying vec2 v_texCoords;

void vertex() {
	v_texCoords = UV;
	v_color = COLOR;
}

void fragment() {
	vec4 color = texture(u_texture, v_texCoords.xy);
	COLOR = vec4(1.0, 1.0, 1.0, (1.0 - floor(color.r / QUANT) * QUANT) * (step(color.r, 0.99))) * v_color;
}
