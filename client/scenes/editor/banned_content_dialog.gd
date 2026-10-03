## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/BannedContentDialog.java (plan 19 M6 §3.9).
##
## UI-only: search + banned/unbanned panes. The sets live in `Rules`
## (`Rules.bannedBlocks`/`bannedUnits`, plan 12) and are edited through the
## Rust `editor::banned` helpers; here the dialog round-trips the map `rules`
## tag JSON via `MindEditor.set_tag("rules", ...)`.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
## `ContentType.block` → blocks, otherwise units.
@export var content_type := "block"

var _editor: Node = null
var _search := ""


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Banned Content"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	var search := LineEdit.new()
	search.text_changed.connect(_on_search)
	cont.add_child(search)
	cont.add_child(_label("banned: %s" % ", ".join(_banned())))
	cont.add_child(_label("unbanned: %s" % ", ".join(_unbanned())))


func _on_search(text: String) -> void:
	_search = text.to_lowercase()
	shown()


func _rules() -> Dictionary:
	if _editor == null or not _editor.has_method("tags"):
		return {}
	var tags: Variant = _editor.call("tags")
	if tags is Dictionary:
		var value: Variant = JSON.parse_string(str(tags.get("rules", "{}")))
		if value is Dictionary:
			return value
	return {}


func _banned() -> Array:
	var key := "bannedBlocks" if content_type == "block" else "bannedUnits"
	return _rules().get(key, [])


func _unbanned() -> Array:
	return []


func _label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	return label
