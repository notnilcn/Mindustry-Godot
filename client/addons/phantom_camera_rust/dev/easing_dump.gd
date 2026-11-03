@tool
extends SceneTree

## `Tween.interpolate_value` / easing parity dump for the Rust math core.
##
## This is a **dev/parity tool**, not shipped addon logic. It is the source of
## truth for `client/rust/phantom-camera-rust/src/math.rs`: the oracle
## (`parity/pcam_oracle/phantom_camera/scripts/phantom_camera_host/phantom_camera_host.gd:1221-1229`)
## forwards Phantom Camera's own `TransitionType`/`EaseType` integers straight to
## `Tween.interpolate_value`, so the Rust port has to reproduce Godot's Penner
## easing table (and the container paths) exactly instead of guessing formulas.
##
## Usage (repo root):
##   godot4 --headless --path client --script res://addons/phantom_camera_rust/dev/easing_dump.gd -- <out.json>
##
## Writes (default `res://addons/phantom_camera_rust/dev/easing_dump.json`):
## * `scalar`          — `interpolate_value(0.0, 1.0, t_grid[i], 1.0, trans, ease)`,
##                       i.e. `apply_easing(t, trans, ease)` itself;
##                       `scalar[trans][ease][i]`, transitions 0..=10, eases 0..=3.
## * `dense`           — the same on `0.00 .. 1.00` step 0.01 (formula fitting).
## * `outside`         — normalized times outside `[0, 1]` (hold behaviour).
## * `duration_zero`   — `duration == 0.0` (the oracle guards this itself).
## * `scalar_edges`    — from/delta combinations that are not `(0, 1)`.
## * `probe_t`         — times that are not exactly representable in float32,
##                       which discriminate double-vs-float easing internals.
## * `duration_probes` — `elapsed != duration` (where the division is rounded).
## * `vector2`/`vector3` — container semantics of the Variant arithmetic path.
## * `quaternion`      — `interpolate_value(q_from, q_delta, ...)` for a fixed
##                       non-trivial pair, plus identity-delta and
##                       negative-dot (`dot < 0`) probes.
##
## Floats are emitted with `full_precision` so a Rust test can embed the values
## and compare at 1e-12 without the dump itself being the error source.

const T_GRID := [0.0, 0.01, 0.05, 0.1, 0.25, 0.333, 0.5, 0.75, 0.9, 0.99, 1.0]
const DENSE_STEP := 0.01
const OUTSIDE_T := [-2.0, -1.0, -0.5, -0.001, 0.5, 1.001, 1.5, 2.0]

## Times that are *not* exactly representable in float32: they discriminate
## "easing computed in double, result rounded to `real_t`" (Godot's shape) from
## "easing computed in float32 throughout", which differ by ~1 ULP.
const PROBE_T := [
	0.001, 0.002, 0.007, 0.123456789, 0.137, 0.2718281828, 0.31415926535,
	0.4242424242, 0.6180339887, 0.7777777, 0.8642, 0.999, 0.9999,
]
## (elapsed, duration) pairs whose `elapsed / duration` is not exact in either
## precision, to pin down where the division happens.
const DURATION_PROBES := [[1.0, 3.0], [0.5, 3.0], [0.7, 0.3], [2.5, 5.0], [0.1, 0.7]]

## Transition/ease pairs dumped for the container probes (index into 0..=10 / 0..=3):
## LINEAR/IN, QUAD/IN, QUAD/OUT, ELASTIC/OUT, BACK/OUT, EXPO/IN_OUT.
const CONTAINER_PAIRS := [[0, 0], [4, 0], [4, 1], [6, 1], [10, 1], [5, 2]]


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var out_path := "res://addons/phantom_camera_rust/dev/easing_dump.json"
	if args.size() > 0:
		out_path = args[0]

	var report := {
		"godot_version": str(Engine.get_version_info().get("string", "")),
		"t_grid": T_GRID,
		"scalar": _dump_scalar(),
		"dense": _dump_dense(),
		"outside": _dump_outside(),
		"duration_zero": _dump_duration_zero(),
		"scalar_edges": _dump_scalar_edges(),
		"probe_t": _dump_probe_t(),
		"duration_probes": _dump_duration_probes(),
		"vector2": _dump_vector2(),
		"vector3": _dump_vector3(),
		"quaternion": _dump_quaternion(),
	}

	var file := FileAccess.open(out_path, FileAccess.WRITE)
	if file == null:
		printerr("easing_dump: cannot write ", out_path)
		quit(1)
		return
	# `full_precision` keeps every float round-trippable; the table is compared
	# numerically against the Rust port, so a lossy dump would be useless.
	file.store_string(JSON.stringify(report, "\t", false, true))
	file.close()
	print("easing_dump: wrote ", out_path)

	# Optional exact-text mirror (`-- <out.json> <out.txt>`): some toolchain JSON
	# readers round doubles to 16 digits, which is enough to hide a 1-ULP
	# mismatch. `String.num(v, 20)` round-trips every double we emit.
	if args.size() > 1:
		_write_text(report, args[1])

	quit(0)


## Flattened `path value` lines, 20 decimals, one number per line.
func _write_text(report: Dictionary, path: String) -> void:
	var lines := PackedStringArray()
	_flatten("", report, lines)
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		printerr("easing_dump: cannot write ", path)
		return
	file.store_string("\n".join(lines))
	file.close()
	print("easing_dump: wrote ", path, " (", lines.size(), " values)")


func _flatten(prefix: String, value: Variant, out: PackedStringArray) -> void:
	match typeof(value):
		TYPE_DICTIONARY:
			for key: Variant in value:
				_flatten(prefix + "/" + str(key), value[key], out)
		TYPE_ARRAY:
			for i in value.size():
				_flatten(prefix + "/" + str(i), value[i], out)
		TYPE_FLOAT:
			out.append(prefix + " " + String.num(value, 20))
		TYPE_INT:
			out.append(prefix + " " + str(value))
		TYPE_VECTOR2:
			_flatten(prefix + "/x", value.x, out)
			_flatten(prefix + "/y", value.y, out)
		TYPE_VECTOR3:
			_flatten(prefix + "/x", value.x, out)
			_flatten(prefix + "/y", value.y, out)
			_flatten(prefix + "/z", value.z, out)
		TYPE_QUATERNION:
			_flatten(prefix + "/x", value.x, out)
			_flatten(prefix + "/y", value.y, out)
			_flatten(prefix + "/z", value.z, out)
			_flatten(prefix + "/w", value.w, out)
		_:
			out.append(prefix + " " + str(value))


## `scalar[trans][ease]` -> 11 values: `apply_easing(t, trans, ease)`.
func _dump_scalar() -> Array:
	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for t: float in T_GRID:
				values.append(Tween.interpolate_value(0.0, 1.0, t, 1.0, trans, ease))
			by_ease.append(values)
		rows.append(by_ease)
	return rows


func _dump_dense() -> Array:
	var steps := int(round(1.0 / DENSE_STEP))
	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for i in range(steps + 1):
				var t := float(i) * DENSE_STEP
				values.append(Tween.interpolate_value(0.0, 1.0, t, 1.0, trans, ease))
			by_ease.append(values)
		rows.append(by_ease)
	return rows


## Normalized time outside `[0, 1]`: reveals whether easing is clamped/holds.
func _dump_outside() -> Array:
	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for t: float in OUTSIDE_T:
				values.append(Tween.interpolate_value(0.0, 1.0, t, 1.0, trans, ease))
			by_ease.append(values)
		rows.append(by_ease)
	return rows


## `duration == 0.0`: the oracle short-circuits a zero duration
## (`phantom_camera_host.gd:694`), so this only documents Godot's own answer.
func _dump_duration_zero() -> Array:
	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for t: float in [0.0, 0.5, 1.0]:
				values.append(Tween.interpolate_value(0.0, 1.0, t, 0.0, trans, ease))
			by_ease.append(values)
		rows.append(by_ease)
	return rows


## Non-`(0, 1)` from/delta pairs: confirms `from + delta * eased`.
func _dump_scalar_edges() -> Array:
	var cases := [
		[5.0, -3.0],
		[-2.5, 0.0],
		[100.0, 1.0],
	]
	var rows := []
	for case: Array in cases:
		var from: float = case[0]
		var delta: float = case[1]
		var by_ease := []
		for pair: Array in CONTAINER_PAIRS:
			var values := []
			for t: float in T_GRID:
				values.append(Tween.interpolate_value(from, delta, t, 1.0, pair[0], pair[1]))
			by_ease.append(values)
		rows.append({"from": from, "delta": delta, "values": by_ease})
	return rows


## Awkward (non-float32-exact) times: discriminates the precision of the easing
## evaluation itself, not just of the returned `real_t`.
func _dump_probe_t() -> Dictionary:
	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for t: float in PROBE_T:
				values.append(Tween.interpolate_value(0.0, 1.0, t, 1.0, trans, ease))
			by_ease.append(values)
		rows.append(by_ease)
	return {"t": PROBE_T, "values": rows}


## `elapsed != duration`: where `elapsed / duration` is rounded.
func _dump_duration_probes() -> Dictionary:
	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for pair: Array in DURATION_PROBES:
				values.append(Tween.interpolate_value(0.0, 1.0, pair[0], pair[1], trans, ease))
			by_ease.append(values)
		rows.append(by_ease)
	return {"elapsed_duration": DURATION_PROBES, "values": rows}


func _dump_vector2() -> Dictionary:
	var from := Vector2(10.0, -20.0)
	var to := Vector2(13.0, -12.5)
	var rows := []
	for pair: Array in CONTAINER_PAIRS:
		var values := []
		for t: float in T_GRID:
			values.append(Tween.interpolate_value(from, to - from, t, 1.0, pair[0], pair[1]))
		rows.append(values)
	return {"from": from, "delta": to - from, "pairs": CONTAINER_PAIRS, "values": rows}


func _dump_vector3() -> Dictionary:
	var from := Vector3(1.5, -2.0, 4.0)
	var to := Vector3(2.5, 6.0, -1.0)
	var rows := []
	for pair: Array in CONTAINER_PAIRS:
		var values := []
		for t: float in T_GRID:
			values.append(Tween.interpolate_value(from, to - from, t, 1.0, pair[0], pair[1]))
		rows.append(values)
	return {"from": from, "delta": to - from, "pairs": CONTAINER_PAIRS, "values": rows}


## Quaternion path. The oracle feeds `prev_quat` and `prev_quat.inverse() * new_quat`
## (`phantom_camera_host.gd:1009-1018`); this dumps what Godot does with that pair.
func _dump_quaternion() -> Dictionary:
	var q_from := Quaternion(0.2, 0.4, -0.1, 0.8).normalized()
	var q_to := Quaternion(Vector3(0.3, 1.0, 0.2).normalized(), 0.6).normalized()
	var q_delta: Quaternion = q_from.inverse() * q_to

	var rows := []
	for trans in range(11):
		var by_ease := []
		for ease in range(4):
			var values := []
			for t: float in T_GRID:
				var q: Quaternion = Tween.interpolate_value(q_from, q_delta, t, 1.0, trans, ease)
				values.append([q.x, q.y, q.z, q.w])
			by_ease.append(values)
		rows.append(by_ease)

	# Identity delta: `from.slerp(from * identity, w) == from` for every weight.
	var identity_rows := []
	for pair: Array in CONTAINER_PAIRS:
		var values := []
		for t: float in T_GRID:
			var q: Quaternion = Tween.interpolate_value(q_from, Quaternion.IDENTITY, t, 1.0, pair[0], pair[1])
			values.append([q.x, q.y, q.z, q.w])
		identity_rows.append(values)

	# dot(from, from * delta) < 0: does Godot take the short path (negate) or not?
	var neg_delta: Quaternion = (q_from.inverse() * (-q_to)).normalized()
	var neg_rows := []
	for pair: Array in CONTAINER_PAIRS:
		var values := []
		for t: float in T_GRID:
			var q: Quaternion = Tween.interpolate_value(q_from, neg_delta, t, 1.0, pair[0], pair[1])
			values.append([q.x, q.y, q.z, q.w])
		neg_rows.append(values)

	return {
		"from": [q_from.x, q_from.y, q_from.z, q_from.w],
		"delta": [q_delta.x, q_delta.y, q_delta.z, q_delta.w],
		"neg_delta": [neg_delta.x, neg_delta.y, neg_delta.z, neg_delta.w],
		"dot_from_to": q_from.dot(q_to),
		"pairs": CONTAINER_PAIRS,
		"pair_values": _dump_quaternion_pairs(q_from, q_delta),
		"values": rows,
		"identity_delta": identity_rows,
		"neg_delta_values": neg_rows,
	}


## Quaternion results restricted to `CONTAINER_PAIRS` (the committed test subset).
func _dump_quaternion_pairs(q_from: Quaternion, q_delta: Quaternion) -> Array:
	var rows := []
	for pair: Array in CONTAINER_PAIRS:
		var values := []
		for t: float in T_GRID:
			var q: Quaternion = Tween.interpolate_value(q_from, q_delta, t, 1.0, pair[0], pair[1])
			values.append([q.x, q.y, q.z, q.w])
		rows.append(values)
	return rows
