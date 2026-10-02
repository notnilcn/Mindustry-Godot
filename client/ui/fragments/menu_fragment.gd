## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/MenuFragment.java (plan 14 §3.5, M0).
##
## M0 shell: a title/logo panel with the About button wired through `MindUi`.
## The complete desktop/mobile menu (play/database/editor/mods/settings,
## submenus, custom buttons) lands in plan 14 M3.

extends Control


func _ready() -> void:
	var about_button := get_node_or_null("Center/Panel/Buttons/AboutButton")
	if about_button != null and not about_button.pressed.is_connected(_on_about):
		about_button.pressed.connect(_on_about)


func _on_about() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", "about", "{}")
