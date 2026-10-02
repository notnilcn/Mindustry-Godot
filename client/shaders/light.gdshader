// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/light.frag + screenspace.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode unshaded, blend_add;

uniform sampler2D u_texture;
uniform vec4 u_ambient;

varying vec2 v_texCoords;

void vertex() {
	v_texCoords = UV;
}

void fragment() {
	vec4 color = texture(u_texture, v_texCoords);
	COLOR = clamp(vec4(mix(u_ambient.rgb, color.rgb, color.a), u_ambient.a - color.a), 0.0, 1.0);
}
