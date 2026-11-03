@tool
extends SceneTree

## ABI dump for the Phantom Camera addon and its Rust port.
##
## Usage (repo root):
##   godot4 --headless --path client --script res://addons/phantom_camera_rust/dev/abi_dump.gd -- <out.json>
##
## Emits, per class: base type, exported properties (name/type/hint/hint_string/
## usage/default), signals (name + arg names/types) and public methods
## (name + arg names/types + return type). Exported property names, signal
## signatures and enum values are the scene ABI: `.tscn` files name classes and
## store properties as strings, so a Rust port must reproduce this table exactly.
##
## The GD addon is dumped only when present; a missing GDScript class is reported
## as `"missing": true` so the same run can dump the Rust port alone.

## class name -> gdscript path (empty = native class looked up in ClassDB only)
const CLASSES := {
	"PhantomCamera2D": "res://addons/phantom_camera/scripts/phantom_camera/phantom_camera_2d.gd",
	"PhantomCamera3D": "res://addons/phantom_camera/scripts/phantom_camera/phantom_camera_3d.gd",
	"PhantomCameraHost": "res://addons/phantom_camera/scripts/phantom_camera_host/phantom_camera_host.gd",
	"PhantomCameraNoiseEmitter2D": "res://addons/phantom_camera/scripts/phantom_camera/phantom_camera_noise_emitter_2d.gd",
	"PhantomCameraNoiseEmitter3D": "res://addons/phantom_camera/scripts/phantom_camera/phantom_camera_noise_emitter_3d.gd",
	"PhantomCameraTweenDirector": "res://addons/phantom_camera/scripts/phantom_camera/phantom_camera_tween_director.gd",
	"PhantomCameraTween": "res://addons/phantom_camera/scripts/resources/tween_resource.gd",
	"Camera3DResource": "res://addons/phantom_camera/scripts/resources/camera_3d_resource.gd",
	"PhantomCameraNoise2D": "res://addons/phantom_camera/scripts/resources/phantom_camera_noise_2d.gd",
	"PhantomCameraNoise3D": "res://addons/phantom_camera/scripts/resources/phantom_camera_noise_3d.gd",
	"TweenDirectorResource": "res://addons/phantom_camera/scripts/resources/tween_director_resource.gd",
	"PhantomCameraManager": "res://addons/phantom_camera/scripts/managers/phantom_camera_manager.gd",
}


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var out_path := "user://pcam_abi.json"
	if args.size() > 0:
		out_path = args[0]

	var report := {}
	for cls_name: String in CLASSES:
		report[cls_name] = _dump_class(cls_name, CLASSES[cls_name])

	var file := FileAccess.open(out_path, FileAccess.WRITE)
	if file == null:
		printerr("abi_dump: cannot write ", out_path)
		quit(1)
		return
	file.store_string(JSON.stringify(report, "\t", false))
	file.close()
	print("abi_dump: wrote ", out_path, " (", report.size(), " classes)")
	quit(0)


func _dump_class(cls_name: String, gd_path: String) -> Dictionary:
	var entry := {"name": cls_name, "source": "", "missing": false}
	var instance: Object = null
	var script: Script = null

	if gd_path != "" and ResourceLoader.exists(gd_path):
		script = load(gd_path)
		if script != null:
			instance = script.new()
			entry["source"] = "gdscript"
			entry["script_path"] = gd_path

	if instance == null:
		if ClassDB.class_exists(cls_name):
			entry["source"] = "native"
			instance = ClassDB.instantiate(cls_name)
		else:
			entry["missing"] = true
			return entry

	var base_name: String = instance.get_class()
	if script != null:
		for global_class: Dictionary in ProjectSettings.get_global_class_list():
			if global_class.get("class", "") == cls_name:
				base_name = global_class.get("base", base_name)
				break
	entry["base"] = base_name

	# Own-surface diff: everything the base class already declares is engine
	# plumbing, not this addon's API, so it is dropped from the report.
	var base_props := {}
	var base_signals := {}
	var base_methods := {}
	if base_name != "" and ClassDB.class_exists(base_name):
		var base_instance: Object = ClassDB.instantiate(base_name)
		if base_instance != null:
			for bp: Dictionary in base_instance.get_property_list():
				base_props[bp.get("name", "")] = true
			for bs: Dictionary in base_instance.get_signal_list():
				base_signals[bs.get("name", "")] = true
			for bm: Dictionary in base_instance.get_method_list():
				base_methods[bm.get("name", "")] = true

	var props := []
	for p: Dictionary in instance.get_property_list():
		if base_props.has(p.get("name", "")):
			continue
		var usage: int = p.get("usage", 0)
		# Skip editor-internal plumbing; keep everything script/scene-visible.
		if not (usage & PROPERTY_USAGE_SCRIPT_VARIABLE) and not (usage & PROPERTY_USAGE_EDITOR) \
			and not (usage & PROPERTY_USAGE_STORAGE):
			continue
		props.append({
			"name": p.get("name", ""),
			"type": p.get("type", 0),
			"type_name": type_string(p.get("type", 0)),
			"hint": p.get("hint", 0),
			"hint_string": p.get("hint_string", ""),
			"usage": usage,
			"is_script_variable": bool(usage & PROPERTY_USAGE_SCRIPT_VARIABLE),
			"default": _default_of(instance, p.get("name", "")),
		})
	entry["properties"] = props

	var signals := []
	for s: Dictionary in instance.get_signal_list():
		var signal_args := []
		for a: Dictionary in s.get("args", []):
			signal_args.append({"name": a.get("name", ""), "type": a.get("type", 0), "type_name": type_string(a.get("type", 0))})
		signals.append({"name": s.get("name", ""), "args": signal_args})
	entry["signals"] = signals

	var methods := []
	for m: Dictionary in instance.get_method_list():
		if m.get("flags", 0) & METHOD_FLAG_VIRTUAL:
			continue
		var m_args := []
		for a: Dictionary in m.get("args", []):
			m_args.append({"name": a.get("name", ""), "type": a.get("type", 0), "type_name": type_string(a.get("type", 0))})
		methods.append({
			"name": m.get("name", ""),
			"args": m_args,
			"return_type": m.get("return", {}).get("type", 0),
			"return_type_name": type_string(m.get("return", {}).get("type", 0)),
		})
	entry["methods"] = methods

	var constants := {}
	var enums := {}
	if script != null:
		for c_name: String in script.get_script_constant_map():
			var value: Variant = script.get_script_constant_map()[c_name]
			if typeof(value) == TYPE_DICTIONARY:
				enums[c_name] = value
			else:
				constants[c_name] = value
	entry["constants"] = constants
	entry["enums"] = enums

	return entry


## Default value of one property on a fresh instance (script variables only;
## engine properties report their class default).
func _default_of(instance: Object, prop_name: String) -> Variant:
	if prop_name.is_empty():
		return null
	var value: Variant = instance.get(prop_name)
	match typeof(value):
		TYPE_OBJECT, TYPE_CALLABLE, TYPE_RID, TYPE_SIGNAL:
			return null
		TYPE_ARRAY:
			return value.duplicate()
		TYPE_DICTIONARY:
			return value.duplicate()
		_:
			return value
