// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/slag.frag + screenspace.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode unshaded, blend_mix;

// shades of slag
#define S2 vec3(100.0, 93.0, 49.0) / 100.0
#define S1 vec3(100.0, 60.0, 25.0) / 100.0
#define NSCALE 200.0 / 2.0

uniform sampler2D u_texture;
uniform sampler2D u_noise;

uniform vec2 u_campos;
uniform vec2 u_resolution;
uniform float u_time;

varying vec2 v_texCoords;

void vertex() {
	v_texCoords = UV;
}

void fragment() {
	vec2 coords = v_texCoords * u_resolution + u_campos;

	float btime = u_time / 5000.0;
	vec4 orig = texture(u_texture, v_texCoords);
	float noise = (texture(u_noise, (coords) / NSCALE + vec2(btime) * vec2(-0.9, 0.8)).r + texture(u_noise, (coords) / NSCALE + vec2(btime * 1.1) * vec2(0.8, -1.0)).r) / 2.0;

	vec2 c = v_texCoords + (vec2(
		texture(u_noise, (coords) / 170.0 + vec2(btime) * vec2(-0.9, 0.8)).r,
		texture(u_noise, (coords) / 170.0 + vec2(btime * 1.1) * vec2(0.8, -1.0)).r
	) - vec2(0.5)) * 8.0 / u_resolution;

	vec4 color = texture(u_texture, c);
	if (color.a < 0.95) {
		color = orig;
	}

	if (noise > 0.6) {
		color.rgb = S2;
	} else if (noise > 0.54) {
		color.rgb = S1;
	}

	COLOR = color;
}
