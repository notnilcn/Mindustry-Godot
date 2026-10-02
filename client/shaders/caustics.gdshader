// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/caustics.frag + screenspace.vert (plan 16 §3.9).

shader_type canvas_item;
render_mode unshaded, blend_add;

#define NSCALE 200.0 / 1.8

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
	vec2 c = v_texCoords.xy;
	vec2 coords = vec2(c.x * u_resolution.x + u_campos.x, c.y * u_resolution.y + u_campos.y);

	float btime = u_time / 3400.0;
	vec4 noise1 = texture(u_noise, (coords) / NSCALE + vec2(btime) * vec2(-0.9, 0.8));
	vec4 noise2 = texture(u_noise, (coords) / NSCALE + vec2(btime * 1.1) * vec2(0.8, -1.0));

	COLOR = vec4(vec3(min(noise1.r, noise2.r)), 0.2);
}
