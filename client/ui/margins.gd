## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/core/UI.java (`updateMargins`/`uiEdgePadding`).
##
## Safe-area/edge-padding helper for HUD gutters. Reads the authoritative insets
## from `MindUi` (Rust) so fragments never compute margins themselves.

class_name MindUiMargins
extends RefCounted


static func _ui() -> Node:
	var loop := Engine.get_main_loop()
	if loop is SceneTree:
		return (loop as SceneTree).root.get_node_or_null("MindUi")
	return null


static func left() -> float:
	var ui := _ui()
	return float(ui.call("margin_left")) if ui != null else 0.0


static func right() -> float:
	var ui := _ui()
	return float(ui.call("margin_right")) if ui != null else 0.0


static func top() -> float:
	var ui := _ui()
	return float(ui.call("margin_top")) if ui != null else 0.0


static func bottom() -> float:
	var ui := _ui()
	return float(ui.call("margin_bottom")) if ui != null else 0.0
