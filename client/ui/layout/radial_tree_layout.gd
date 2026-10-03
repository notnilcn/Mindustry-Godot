## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/layout/RadialTreeLayout.java (plan 14 M5).
##
## Radial layout: places nodes on concentric circles by depth. Used by the
## sector-select hex/radial mode.

class_name MindRadialTreeLayout
extends MindTreeLayout

## Radius in pixels added per depth level.
var radius_step := 96.0


## `{name: Vector2}` on concentric rings by `depth`.
func radial_positions(entries: Array) -> Dictionary:
	positions.clear()
	var by_depth: Dictionary = {}
	for entry in entries:
		var depth := int((entry as Dictionary).get("depth", 0))
		var row: Array = by_depth.get(depth, [])
		row.append(entry)
		by_depth[depth] = row
	for depth in by_depth.keys():
		var row: Array = by_depth[depth]
		var radius := radius_step * float(depth)
		var step := TAU / maxf(1.0, float(row.size()))
		for index in row.size():
			var node: Dictionary = row[index]
			var name := str(node.get("name", node.get("content", "")))
			positions[name] = Vector2(cos(step * index), sin(step * index)) * radius
	return positions
