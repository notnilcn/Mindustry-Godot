// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
// Ported from: core/assets/bloomshaders/alpha_threshold.frag +
// alpha_gaussian.frag + blurspace.vert + alpha_bloom.frag (Arc
// `arc.graphics.g2d.Bloom`, `Bloom.java:128-168`; `Renderer.java:391-394`).
//
// CanvasGroup variant of the world bloom (J10 T2): `MindWorldRenderer` bakes
// the `Layer.bullet..Layer.effect` band nodes through a `CanvasGroup`; this
// material runs on the group's baked texture (via `hint_screen_texture`, like
// `shield_group.gdshader`) and applies Java's alpha-threshold, gaussian blur
// and composite maths in one pass.
//
// Deviations (recorded in docs/J10-twin-reverify-residuals_summary.md):
// - Java runs the 5-tap separable blur `blurPasses` times over a quarter-res
//   FBO pair; this pass runs one wider separable kernel whose tap offsets
//   scale with `sqrt(passes)` (equal stddev growth, same visual direction).
// - Java captures from `Layer.bullet - 0.02` into an FBO, so everything drawn
//   after that point is captured; the port captures the bullet/effect band
//   nodes only (the FX host at `/root/Spine/MindFx` is a sibling of the world
//   renderer and is composited un-bloomed above the group).

shader_type canvas_item;
render_mode unshaded, blend_mix;

// The group's bake (`CanvasGroup`); Java's captured frame buffer.
uniform sampler2D screen_texture : hint_screen_texture, filter_linear;
// One screen pixel in UV units (the renderer keeps this current).
uniform vec2 u_screen_px;
// `settings.getBool("bloom")` (`Renderer.java:391` gates the whole pass).
uniform bool u_enabled;
// `bloom.setBloomIntensity(settings.getInt("bloomintensity", 6) / 4f + 1f)`.
uniform float u_bloom_intensity;
// `bloom.setOriginalIntensity(1f)` (`Bloom.java:86`).
uniform float u_original_intensity;
// `bloom.blurPasses = settings.getInt("bloomblur", 1)`; sqrt-scaled tap radius.
uniform float u_blur_radius;

// `alpha_gaussian.frag` weights.
const float W_FAR = 0.0702702703;
const float W_CLOSE = 0.3162162162;
const float W_CENTER = 0.2270270270;
// `blurspace.vert` offsets (`futher`/`closer`) in quarter-res FBO texels,
// converted to screen pixels (the FBOs are 1/4 scale, `Bloom.java:57-66`).
const float OFF_FAR = 3.2307692308 * 4.0;
const float OFF_CLOSE = 1.3846153846 * 4.0;

vec4 thresholded(vec2 uv) {
	vec4 c = texture(screen_texture, uv);
	// `alpha_threshold.frag`: `if(color.r + color.g + color.b > 0.5 * 3.0)`.
	return (c.r + c.g + c.b) > 1.5 ? c : vec4(0.0);
}

void fragment() {
	if (!u_enabled) {
		// `renderer.toggleBloom(false)`: no capture, draw the bands as-is.
		COLOR = texture(screen_texture, SCREEN_UV);
	} else {
		vec2 uv = SCREEN_UV;
		vec2 near_px = u_screen_px * (OFF_CLOSE * u_blur_radius);
		vec2 far_px = u_screen_px * (OFF_FAR * u_blur_radius);
		float w[5] = float[5](W_FAR, W_CLOSE, W_CENTER, W_CLOSE, W_FAR);
		vec2 o[5] = vec2[5](-far_px, -near_px, vec2(0.0), near_px, far_px);
		// `alpha_gaussian.frag` run H then V (`Bloom.java:145-159`), folded
		// into the 2D double sum; threshold applies to every source tap.
		vec4 blurred = vec4(0.0);
		for (int i = 0; i < 5; i++) {
			for (int j = 0; j < 5; j++) {
				blurred += w[i] * w[j] * thresholded(uv + o[i] + o[j]);
			}
		}
		// `alpha_bloom.frag`.
		vec4 original = texture(screen_texture, uv) * u_original_intensity;
		vec4 bloom = blurred * u_bloom_intensity;
		original *= (vec4(1.0) - bloom);
		vec4 combined = original + bloom;
		float mx = min(max(combined.r, max(combined.g, combined.b)), 1.0);
		COLOR = vec4(combined.rgb / max(mx, 0.0001), mx);
	}
}
