## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/FileChooserDialog.java (plan 14 §3.8 M7).
##
## Fallback file browser used when the native `DisplayServer.file_dialog_show`
## path is unavailable (plan 22 owns the native-first path). Navigation, the
## extension filter and the last-directory static are ported; the actual I/O is
## plan 04. Emits `file_chosen(path)` on confirm.

extends MindDialog

## Emitted with the selected absolute path.
signal file_chosen(path: String)
## Emitted when the chooser is cancelled.
signal chooser_cancelled

var _open := true
var _extensions: PackedStringArray = []
var _directory := "user://"
var _field: LineEdit = null
var _list: MindTable = null


func _ready() -> void:
	should_pause = false
	set_title_key("@open")
	super._ready()
	_build()
	add_close_button()
	_rebuild()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	var ctx := context()
	_open = bool(ctx.get("open", true))
	var extensions: Variant = ctx.get("extensions", [])
	if extensions is Array:
		_extensions.clear()
		for extension in extensions:
			_extensions.append(str(extension))
	set_title_key("@open" if _open else "@save")


func _build() -> void:
	var root := content_table()
	_field = MindWidgets.field(_t("@filechooser.path"))
	_field.text_submitted.connect(_navigate)
	root.add(_field).grow_x_axis().pad(4)
	root.row()
	_list = MindTable.new()
	root.add(_list).grow_x_axis().grow_y_axis()


func _navigate(path: String) -> void:
	if not path.is_empty():
		_directory = path
	_rebuild()


func _rebuild() -> void:
	_list.clear_children()
	_field.text = _directory
	var directory := DirAccess.open(_directory)
	if directory == null:
		_list.add(MindWidgets.label("@filechooser.unreadable")).pad(4)
		return
	# code-instantiated: directory entries are runtime filesystem data.
	if _directory != "user://" and _directory != "/":
		var up := MindWidgets.button("..")
		up.pressed.connect(func() -> void: _navigate(_directory.path_join("..").simplify_path()))
		_list.add(up).grow_x_axis().pad(1)
		_list.row()
	for name in directory.get_files():
		if not _extensions.is_empty() and not _matches(name):
			continue
		var file := MindWidgets.button(name)
		file.pressed.connect(func() -> void: _choose(_directory.path_join(name)))
		_list.add(file).grow_x_axis().pad(1)
		_list.row()
	for name in directory.get_directories():
		var folder := MindWidgets.button("%s/" % name)
		folder.pressed.connect(func() -> void: _navigate(_directory.path_join(name)))
		_list.add(folder).grow_x_axis().pad(1)
		_list.row()


func _matches(file_name: String) -> bool:
	var extension := file_name.get_extension()
	for allowed in _extensions:
		if extension.to_lower() == str(allowed).to_lower():
			return true
	return false


func _choose(path: String) -> void:
	file_chosen.emit(path)
	hide_dialog()


func _t(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_get", key.trim_prefix("@")))
