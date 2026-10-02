// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/shaders/atmosphere.frag + atmosphere.vert (plan 16 §3.9).

shader_type spatial;
render_mode unshaded, blend_add, cull_disabled;

uniform float u_innerRadius;
uniform float u_outerRadius;
uniform vec3 u_color;
uniform vec2 u_resolution;
uniform float u_time;
uniform vec3 u_campos;
uniform vec3 u_rcampos;
uniform mat4 u_invproj;
uniform vec3 u_light;
uniform mat4 u_model;
uniform mat4 u_projection;

varying vec3 v_world;

const float MAX = 10000.0;

const float PEAK = 0.1;
const float FLARE = 0.0025;
const float INTENSITY = 14.3;
const float G_M = -0.85;

const int numOutScatter = 3;
const float fNumOutScatter = 3.0;
const int numInScatter = 3;
const float fNumInScatter = 3.0;

vec2 rayIntersection(vec3 p, vec3 dir, float radius) {
	float b = dot(p, dir);
	float c = dot(p, p) - radius * radius;

	float d = b * b - c;
	if (d < 0.0) {
		return vec2(MAX, -MAX);
	}
	d = sqrt(d);

	float near = -b - d;
	float far = -b + d;
	return vec2(near, far);
}

float miePhase(float g, float c, float cc) {
	float gg = g * g;
	float a = (1.0 - gg) * (1.0 + cc);
	float b = 1.0 + gg - 2.0 * g * c;
	b *= sqrt(b);
	b *= 2.0 + gg;
	return 1.5 * a / b;
}

float rayleighPhase(float cc) {
	return 0.75 * (1.0 + cc);
}

float density(vec3 p) {
	return exp(-(length(p) - u_innerRadius) * (4.0 / (u_outerRadius - u_innerRadius)));
}

float optic(vec3 p, vec3 q) {
	vec3 stepv = (q - p) / fNumOutScatter;
	vec3 v = p + stepv * 0.5;
	float sum = 0.0;
	for (int i = 0; i < numOutScatter; i++) {
		sum += density(v);
		v += stepv;
	}
	sum *= length(stepv) * (1.0 / (u_outerRadius - u_innerRadius));
	return sum;
}

vec3 inScatter(vec3 o, vec3 dir, vec2 e, vec3 l) {
	float len = (e.y - e.x) / fNumInScatter;
	vec3 stepv = dir * len;
	vec3 p = o + dir * e.x;
	vec3 v = p + dir * (len * 0.5);

	vec3 sum = vec3(0.0);
	for (int i = 0; i < numInScatter; i++) {
		vec2 f = rayIntersection(v, l, u_outerRadius);
		vec3 u = v + l * f.y;
		float n = (optic(p, v) + optic(v, u)) * (PI * 4.0);
		sum += density(v) * exp(-n * (PEAK * u_color + FLARE));
		v += stepv;
	}
	sum *= len * (1.0 / (u_outerRadius - u_innerRadius));
	float c = dot(dir, -l);
	float cc = c * c;
	return sum * (PEAK * u_color * rayleighPhase(cc) + FLARE * miePhase(G_M, c, cc)) * INTENSITY;
}

void vertex() {
	v_world = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec3 dir = normalize(v_world - u_campos);
	vec3 eye = u_rcampos;
	vec3 l = u_light;

	vec2 e = rayIntersection(eye, dir, u_outerRadius);
	vec2 f = rayIntersection(eye, dir, u_innerRadius);
	e.y = min(e.y, f.x);

	vec3 result = inScatter(eye, dir, e, l);
	ALBEDO = result;
	ALPHA = 1.0;
}
