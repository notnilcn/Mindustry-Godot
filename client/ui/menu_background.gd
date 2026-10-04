## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/MenuFragment.java (`parent.fill((x,y,w,h)
## -> renderer.render())`) and graphics/MenuRenderer.java.
##
## Full-screen menu backdrop. Asks `MindRender.build_menu_texture` for the
## procedurally generated menu world baked to a texture and darkens it by
## `MenuRenderer.darkness` (0.3). The texture covers the viewport (nearest/linear
## scale) behind the menu fragment.

extends TextureRect

## `1 - MenuRenderer.darkness`.
const DARKNESS := 0.7


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	modulate = Color(DARKNESS, DARKNESS, DARKNESS, 1.0)
	# Deferred so the `MindWorldRenderer`/`MindSimHost` autoloads have finished
	# `_ready` and loaded their atlas + content snapshot.
	call_deferred("_generate")


func _generate() -> void:
	var render := get_node_or_null("/root/Spine/MindRender")
	if render == null:
		return
	var mobile := false
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		mobile = bool(ui.call("is_mobile"))
	var generated: Variant = render.call("build_menu_texture", randi(), mobile)
	if generated is Texture2D:
		texture = generated
	else:
		push_warning("[menu] background bake unavailable")
