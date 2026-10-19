// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/planet.frag + planet.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded;

uniform mat4 u_proj;
uniform mat4 u_trans;
uniform vec3 u_lightdir;
uniform vec3 u_camdir;
uniform vec3 u_campos;
uniform vec3 u_ambientColor;
uniform float u_emissive;

varying vec4 v_col;

const vec3 diffuse = vec3(0.01);

void vertex() {
	vec3 normal = NORMAL;
	vec3 specular = vec3(0.0);

	vec3 light_reflect = normalize(reflect(normal, u_lightdir));
	vec3 vertex_eye = normalize(u_campos - (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz);

	float albedo = 1.0 - COLOR.a;

	float specular_factor = dot(vertex_eye, light_reflect);
	if (specular_factor > 0.0) {
		specular = vec3(pow(specular_factor, 40.0)) * albedo;
	}

	vec3 norc = (u_ambientColor + specular) * (diffuse + vec3(clamp((dot(normal, u_lightdir) + 1.0) / 2.0, 0.0, 1.0)));

	// `planet.vert`: `emissive = a_emissive.a * u_emissive * min(pow(max(0, (1 - norc.r) * 1.2), 3), 1.1)`
	float emissive = CUSTOM0.a * u_emissive * min(pow(max(0.0, (1.0 - norc.r) * 1.2), 3.0), 1.1);

	v_col = vec4(mix(COLOR.rgb, CUSTOM0.rgb, emissive), 1.0) * vec4(mix(norc, vec3(1.0), emissive), 1.0);
}

// `planet.frag` writes `v_col` straight to the (gamma-space) framebuffer. A
// Godot 3D viewport treats ALBEDO as linear and encodes it to sRGB on output,
// which lifted the dark (night) side of the globe to a flat grey; decode the
// interpolated colour so the displayed value is Java's `v_col`.
vec3 srgb_to_linear(vec3 c) {
	vec3 lo = c / 12.92;
	vec3 hi = pow((c + vec3(0.055)) / 1.055, vec3(2.4));
	return mix(hi, lo, lessThan(c, vec3(0.04045)));
}

// `planet.vert` always emits alpha 1 (`vec4(..., 1.0) * vec4(..., 1.0)`). Writing
// ALPHA would move the body into Godot's transparent pass, where it stops
// writing depth; Java draws it opaque with `Gl.depthMask(true)`
// (`PlanetRenderer.java:49-50`), so the globe occludes the orbit loop and the
// far-side sector overlays.
void fragment() {
	ALBEDO = srgb_to_linear(max(v_col.rgb, vec3(0.0)));
}
