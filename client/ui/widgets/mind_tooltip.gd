## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.Tooltip (plan 14 §3.3).
##
## Tooltip helper. Godot's native `tooltip_text` renders the black8+margins look
## through the theme; `allow_mobile` and the top-left container variant are
## recorded as metadata for the mobile branches (plan 14 §3.3/M7).

class_name MindTooltip
extends RefCounted


## Attaches a tooltip to `control` (`Tooltip(background, textProvider)`).
static func attach(control: Control, provider, allow_mobile: bool = false) -> void:
	control.tooltip_text = _resolve(provider)
	control.set_meta("mind_tooltip_allow_mobile", allow_mobile)


## Re-resolves the tooltip text from its provider.
static func refresh(control: Control, provider) -> void:
	control.tooltip_text = _resolve(provider)


## `setContainerPosition` top-left variant for `addDescTooltip`.
static func set_container_position(control: Control, top_left: bool = true) -> void:
	control.set_meta("mind_tooltip_top_left", top_left)


static func _resolve(provider) -> String:
	if provider is Callable:
		var callable: Callable = provider
		if callable.is_valid():
			return str(callable.call())
		return ""
	if provider == null:
		return ""
	return str(provider)
