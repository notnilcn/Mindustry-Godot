## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.ScrollPane (plan 14 §3.3).
##
## Wraps a [ScrollContainer] + [MindTable]; adds the scene2d scroll API
## (`setScrollingDisabled`, `hasScroll`, `requestScroll`, `updateScrollFocus`).

class_name MindScroll
extends ScrollContainer

## Meta key holding a pane's [ArcWheel] state (see [method hook_arc_wheel]).
const ARC_WHEEL_META := &"_arc_wheel"

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


## Arc `ScrollPane` wheel for every pane (`ui_root.gd` hooks each
## [ScrollContainer] as it enters the tree): Godot's built-in wheel step is
## `page / 8` and jumps, Arc's is `getMouseWheelY()` (`ScrollPane.java:747-750`)
## eased by `act` (`:313-335`).
static func hook_arc_wheel(pane: ScrollContainer) -> void:
	if pane.has_meta(ARC_WHEEL_META):
		return
	var wheel := ArcWheel.new(pane)
	pane.set_meta(ARC_WHEEL_META, wheel)
	pane.gui_input.connect(wheel.on_gui_input)


## Per-pane Arc scroll state: `amountX/Y` (the target) and `visualAmountX/Y`
## (what is shown), stepped once per frame while they differ.
class ArcWheel:
	extends RefCounted

	## `act`: `Math.max(200 * delta, (amount - visual) * 7 * delta)`.
	const MIN_SPEED := 200.0
	const EASE := 7.0

	var _pane: ScrollContainer
	var _amount := Vector2.ZERO
	var _visual := Vector2.ZERO
	## Bar values written by the last step; any other value means something
	## else (a drag, `ensure_control_visible`, `request_scroll`) moved the pane.
	var _written := Vector2.ZERO
	var _animating := false
	var _tree: SceneTree = null

	func _init(pane: ScrollContainer) -> void:
		_pane = pane

	## `ScrollPane.java:197-204` (`scrolled`): `setScrollY(amountY +
	## getMouseWheelY() * sy)` on a vertically scrollable pane (and the same on
	## x); handled when either axis scrolls. `sy` is SDL's `-wheel.y`
	## (`SdlInput.java:84`): +1 per notch down, -1 per notch up. Godot sends a
	## press and a release per notch; only the press is a scroll event.
	func on_gui_input(event: InputEvent) -> void:
		var button := event as InputEventMouseButton
		if button == null or not button.pressed:
			return
		var sx := 0.0
		var sy := 0.0
		match button.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				sy = -button.factor
			MOUSE_BUTTON_WHEEL_DOWN:
				sy = button.factor
			MOUSE_BUTTON_WHEEL_LEFT:
				sx = button.factor
			MOUSE_BUTTON_WHEEL_RIGHT:
				sx = -button.factor
			_:
				return
		var v_bar := _pane.get_v_scroll_bar()
		var h_bar := _pane.get_h_scroll_bar()
		var scroll_y := _scrolls(v_bar, _pane.vertical_scroll_mode)
		var scroll_x := _scrolls(h_bar, _pane.horizontal_scroll_mode)
		if not scroll_x and not scroll_y:
			return
		if not _animating or _moved_elsewhere():
			_amount = Vector2(h_bar.value, v_bar.value)
			_visual = _amount
		# `getMouseWheelY()`: `Math.min(areaHeight, areaHeight * 0.9f / 4)`.
		if scroll_y:
			_amount.y = clampf(
				_amount.y + minf(v_bar.page, v_bar.page * 0.9 / 4.0) * sy,
				0.0, maxf(v_bar.max_value - v_bar.page, 0.0)
			)
		if scroll_x:
			_amount.x = clampf(
				_amount.x + minf(h_bar.page, h_bar.page * 0.9 / 4.0) * sx,
				0.0, maxf(h_bar.max_value - h_bar.page, 0.0)
			)
		_pane.accept_event()
		if not _animating:
			_animating = true
			_written = Vector2(h_bar.value, v_bar.value)
			_tree = _pane.get_tree()
			_tree.process_frame.connect(_step)

	## `scrollY = widgetHeight > areaHeight && !disableY` (`:418`).
	static func _scrolls(bar: ScrollBar, mode: int) -> bool:
		return (
			bar != null
			and mode != ScrollContainer.SCROLL_MODE_DISABLED
			and bar.max_value > bar.page
		)

	func _moved_elsewhere() -> bool:
		var v_bar := _pane.get_v_scroll_bar()
		var h_bar := _pane.get_h_scroll_bar()
		return h_bar.value != _written.x or v_bar.value != _written.y

	## `ScrollPane.act` smooth scroll (`:313-332`): the visual amount chases the
	## target at `max(200 * delta, remaining * 7 * delta)` px, never overshooting.
	func _step() -> void:
		if not is_instance_valid(_pane) or not _pane.is_inside_tree() or _moved_elsewhere():
			_stop()
			return
		var delta := _pane.get_process_delta_time()
		_visual.x = _chase(_visual.x, _amount.x, delta)
		_visual.y = _chase(_visual.y, _amount.y, delta)
		var v_bar := _pane.get_v_scroll_bar()
		var h_bar := _pane.get_h_scroll_bar()
		h_bar.value = _visual.x
		v_bar.value = _visual.y
		_written = Vector2(h_bar.value, v_bar.value)
		if _visual == _amount:
			_stop()

	static func _chase(visual: float, amount: float, delta: float) -> float:
		if visual < amount:
			return minf(amount, visual + maxf(MIN_SPEED * delta, (amount - visual) * EASE * delta))
		if visual > amount:
			return maxf(amount, visual - maxf(MIN_SPEED * delta, (visual - amount) * EASE * delta))
		return visual

	func _stop() -> void:
		_animating = false
		if _tree != null and _tree.process_frame.is_connected(_step):
			_tree.process_frame.disconnect(_step)
		_tree = null
