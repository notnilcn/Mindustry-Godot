## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.ScrollPane (plan 14 §3.3).
##
## Wraps a [ScrollContainer] + [MindTable]; adds the scene2d scroll API
## (`setScrollingDisabled`, `hasScroll`, `requestScroll`, `updateScrollFocus`).

class_name MindScroll
extends ScrollContainer

var _content: MindTable = null


func _ready() -> void:
	if get_child_count() > 0 and get_child(0) is MindTable:
		_content = get_child(0)
	else:
		# code-instantiated: the scroll content table is owned by the dialog that
		# creates this pane; there is no static scene for a generic scroll body.
		_content = MindTable.new()
		_content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_content.size_flags_vertical = Control.SIZE_EXPAND_FILL
		add_child(_content)


## The content table dialogs add cells to.
func content() -> MindTable:
	return _content


func set_scrolling_disabled(x: bool, y: bool) -> void:
	horizontal_scroll_mode = (
		ScrollContainer.SCROLL_MODE_DISABLED if x else ScrollContainer.SCROLL_MODE_AUTO
	)
	vertical_scroll_mode = (
		ScrollContainer.SCROLL_MODE_DISABLED if y else ScrollContainer.SCROLL_MODE_AUTO
	)


func set_fade_scroll_bars(_fade: bool) -> void:
	pass


func has_scroll() -> bool:
	var bar := get_v_scroll_bar()
	return bar != null and bar.max_value > bar.page


func request_scroll(y: float) -> void:
	scroll_vertical = int(y)


## Focus bookkeeping hook (plan 14 §3.3); the plan-15 input layer owns focus.
func update_scroll_focus() -> void:
	pass
