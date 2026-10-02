// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/space.frag + screenspace.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode unshaded, blend_mix;

#define NSCALE 2700.0
#define CAMSCALE (NSCALE * 10.0)

uniform sampler2D u_texture;
uniform sampler2D u_stars;

uniform vec2 u_campos;
uniform vec2 u_ccampos;
uniform vec2 u_resolution;
uniform float u_time;

varying vec2 v_texCoords;

void vertex() {
	v_texCoords = UV;
}

void fragment() {
	vec2 c = v_texCoords.xy;
	vec2 coords = vec2(c.x * u_resolution.x, c.y * u_resolution.y);

	vec4 color = texture(u_texture, c);
	color.rgb = texture(u_stars, coords / NSCALE + vec2(-0.1, -0.1) + u_ccampos / CAMSCALE).rgb;

	COLOR = color;
}
