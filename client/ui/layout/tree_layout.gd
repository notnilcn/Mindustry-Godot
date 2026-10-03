## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/layout/TreeLayout.java (plan 14 M5 §3.1 layout/).
##
## Shared tree-layout math: positions nodes in depth-ordered rows. The concrete
## layouts (`row`/`branch`/`radial`) extend this; sizes are Scl-scaled pixels.

class_name MindTreeLayout
extends Control

## Node positions in local pixels (set by `layout_entries`).
var positions: Dictionary = {}
## Horizontal gap between sibling nodes.
var gap := 16.0
## Seed used by the radial/branch variants for deterministic jitter.
var seed := 0


## Computes `{name: Vector2}` positions for the entry depth tree.
func layout_entries(entries: Array) -> Dictionary:
	positions.clear()
	var cursor := Vector2.ZERO
	for entry in entries:
		var name := str((entry as Dictionary).get("name", ""))
		positions[name] = cursor
		cursor.x += (entry as Dictionary).get("width", 120.0)
		cursor.x += gap
	return positions
