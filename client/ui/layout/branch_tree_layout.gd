## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/layout/BranchTreeLayout.java (plan 14 M5).
##
## Branch layout: positions nodes by depth and sibling index. Used by the
## research tree; the interactive node graph is plan 16's draw layer.

class_name MindBranchTreeLayout
extends MindTreeLayout


## `{name: Vector2}` where `y` follows depth and `x` the sibling order.
func branch_positions(entries: Array) -> Dictionary:
	positions.clear()
	var per_depth: Dictionary = {}
	for entry in entries:
		var node: Dictionary = entry
		var depth := int(node.get("depth", 0))
		var row: Array = per_depth.get(depth, [])
		row.append(node)
		per_depth[depth] = row
	var row_y := 0.0
	for depth in per_depth.keys():
		var row: Array = per_depth[depth]
		row.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
			return str(a.get("content", "")) < str(b.get("content", "")))
		var x := 0.0
		for node_variant in row:
			var node: Dictionary = node_variant
			positions[str(node.get("content", ""))] = Vector2(x, row_y)
			x += float(node.get("width", 120.0)) + gap
		row_y += 64.0
	return positions
