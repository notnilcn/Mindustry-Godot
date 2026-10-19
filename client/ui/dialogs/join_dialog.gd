## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/JoinDialog.java (plan 14 §3.4, I08 T3).
##
## Server browser over SpacetimeDB addresses. Saved servers persist under the
## Java `servers` settings JSON (`JoinDialog.java:691`/`:774`) as
## `[{"ip": "...", "port": N}]`; each server row probes its address through
## `MindNet.ping_server`/`server_probe_json` and lists the public `all_matches`
## rows (Java's `net.pingHost` + `setupServer`). "Community" is the active
## server's match list: the port never fetches the Java community URLs because
## they serve Java servers the relay cannot talk to (I08 deviation).

extends MindDialog

signal connect_requested(address: String)
signal join_requested(match_id: int)

## Java `JoinDialog.java:341`/`:349`: name and colour persist in the shared
## settings store (`Core.settings "name"` / `"color-0"`), max 40 chars.
const NAME_SETTING := "name"
const COLOR_SETTING := "color-0"
const MAX_NAME_LENGTH := 40
## Java `Core.settings.getJson("servers")` key and the add-panel `ip` memory.
const SERVERS_SETTING := "servers"
const IP_SETTING := "ip"
## The local relay address (`tools/mp-local.sh up`); Java's LAN discovery has
## no STDB equivalent, so "local" is the default server (`JoinDialog.java:418-424`).
const DEFAULT_SERVER_ADDRESS := "127.0.0.1:3000/mindustry"
const DEFAULT_DB_NAME := "mindustry"
const DEFAULT_PORT := 3000
## Probe rows re-poll every N frames (the transport is async).
const PROBE_POLL_FRAMES := 10

## Player color shown in the header swatch (upstream `player.color()`).
var _color := Color.WHITE
var _table: MindTable = null
var _name_field: LineEdit = null
var _swatch_tint: ColorRect = null
var _search_field: LineEdit = null
var _add_panel: Control = null
var _ip_field: LineEdit = null
var _local_body: Control = null
var _remote_body: Control = null
var _global_list: MindTable = null
## Header/search/body controls of the community section (`_build_sections`).
var _global_nodes: Array[Control] = []
## Persisted servers (`[{"ip": String, "port": int}]`, Java `Server` shape).
var _servers: Array = []
## Signature of the last rendered probe state; rows rebuild when it changes.
var _server_signature := ""
## Add-panel target: -1 = append, else the server index being edited.
var _editing_index := -1
## Pending delete confirmation target (Java `ui.showConfirm("@confirm", "@server.delete", ...)`).
var _pending_delete := -1
var _server_search := ""
var _poll_counter := 0


func _ready() -> void:
	set_title_key("@joingame")
	should_pause = false
	full_dialog = true
	# `makeButtonOverlay()` (`JoinDialog.java:59`): the padded title cell.
	button_overlay = true
	super._ready()
	_load_servers()
	_build()
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm_result):
		ui.connect("confirm_result", _on_confirm_result)


func _process(_delta: float) -> void:
	if not visible:
		return
	_poll_counter = (_poll_counter + 1) % PROBE_POLL_FRAMES
	if _poll_counter != 0:
		return
	# The probes are async: poll, and rebuild the rows only when the rendered
	# state actually changed.
	var signature := _poll_server_signature()
	if signature != _server_signature:
		_server_signature = signature
		_rebuild_servers()


func _build() -> void:
	_table = content_table()
	_table.add_theme_constant_override("separation", 8)
	_build_header()
	_build_add_panel()
	_build_sections()
	add_button(_t("@back"), _close, "left", 210.0)
	add_button(_t("@server.add"), _toggle_add, "add")
	add_button("?", _show_info, "", 60.0)


## Header row: `@name` label + name field + square color swatch (`JoinDialog.setup`).
func _build_header() -> void:
	# code-instantiated: the name/color header row is data-driven and has no
	# static scene counterpart.
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 6)
	header.add_child(MindWidgets.label(_t("@name")))
	_name_field = MindWidgets.field(_t("@name"))
	_name_field.max_length = MAX_NAME_LENGTH
	# `JoinDialog.java:341`: the field opens with the remembered name.
	_name_field.text = _stored_name()
	_name_field.text_changed.connect(_store_name)
	_name_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(_name_field)
	# code-instantiated: the square color swatch is tinted at runtime by the
	# color chosen in the `picker` dialog.
	var swatch := Button.new()
	swatch.theme_type_variation = "squarei"
	swatch.custom_minimum_size = Vector2(54, 54)
	_swatch_tint = ColorRect.new()
	_color = _stored_color()
	_swatch_tint.color = _color
	_swatch_tint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	swatch.add_child(_swatch_tint)
	_swatch_tint.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	swatch.pressed.connect(_open_picker)
	header.add_child(swatch)
	_table.add(header).grow_x_axis().pad(4)
	_table.row()


## Inline add/edit-server form (`JoinDialog.add`, `JoinDialog.java:77-112`).
func _build_add_panel() -> void:
	# code-instantiated: the inline add-server form is revealed by `@server.add`
	# and toggled at runtime.
	_add_panel = VBoxContainer.new()
	_add_panel.visible = false
	_add_panel.add_theme_constant_override("separation", 4)
	var ip_row := HBoxContainer.new()
	ip_row.add_theme_constant_override("separation", 6)
	ip_row.add_child(MindWidgets.label(_t("@joingame.ip")))
	_ip_field = MindWidgets.field(_t("@joingame.ip"))
	_ip_field.max_length = 100
	_ip_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `JoinDialog.java:81-83`: the field remembers the last address typed.
	_ip_field.text_changed.connect(_store_ip)
	ip_row.add_child(_ip_field)
	_add_panel.add_child(ip_row)
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 6)
	var ok := MindWidgets.button(_t("@ok"))
	ok.pressed.connect(_submit_add)
	actions.add_child(ok)
	var cancel := MindWidgets.button(_t("@cancel"))
	cancel.pressed.connect(_hide_add)
	actions.add_child(cancel)
	_add_panel.add_child(actions)
	_table.add(_add_panel).grow_x_axis().pad(4)
	_table.row()


## The three server groups (`JoinDialog.section`): local, remote, global.
func _build_sections() -> void:
	var local := _add_section("@servers.local", false)
	# code-instantiated: the local section body is the default relay server row
	# (plain containers; `MindTable` only places its own direct children).
	_local_body = local

	var remote := _add_section("@servers.remote", false)
	# code-instantiated: remote rows rebuild from the persisted `servers` list.
	_remote_body = remote
	_rebuild_servers()

	var first_global := _table.get_child_count()
	var global := _add_section("@servers.global", true)
	# The community section's header/search/body nodes, hidden while
	# `communityservers` is off (`JoinDialog.java:329-331`).
	_global_nodes.clear()
	for index in range(first_global, _table.get_child_count()):
		var node := _table.get_child(index) as Control
		if node != null:
			_global_nodes.append(node)
	# code-instantiated: the community body lists the active server's public
	# `all_matches` rows; it is Java's community list minus the remote fetch.
	_global_list = MindTable.new()
	_global_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	global.add_child(_global_list)
	_rebuild_public()


## Builds one section header (accent title + collapse glyph, optional search row)
## and returns its collapsible body container.
func _add_section(label_key: String, with_search: bool) -> Control:
	# code-instantiated: section header + collapsible body repeat for the three
	# server groups and are toggled at runtime.
	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", 4)
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	var title := MindWidgets.styled_label(_tm(label_key), "outlineLabel")
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	title.add_theme_color_override("default_color", MindStyles.ACCENT)
	header.add_child(title)
	var toggle := Button.new()
	toggle.theme_type_variation = "emptyi"
	toggle.custom_minimum_size = Vector2(40, 40)
	# code-instantiated: the collapse glyph swaps between up-open/down-open.
	var toggle_glyph := MindWidgets.glyph("up-open", 22, MindStyles.ACCENT)
	toggle.add_child(toggle_glyph)
	toggle.pressed.connect(_toggle_section.bind(body, toggle_glyph))
	header.add_child(toggle)
	_table.add(header).grow_x_axis().pad(4)
	_table.row()

	if with_search:
		# code-instantiated: the community search/refresh row is data-driven
		# (`JoinDialog.java:417-424`).
		var search_row := HBoxContainer.new()
		search_row.add_theme_constant_override("separation", 6)
		search_row.add_child(MindWidgets.label(_t("@search")))
		_search_field = MindWidgets.field(_t("@search"))
		_search_field.max_length = 50
		_search_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_search_field.text_changed.connect(_on_search_changed)
		search_row.add_child(_search_field)
		var zoom := MindWidgets.icon_button("zoom", "", "emptyi")
		zoom.custom_minimum_size = Vector2(54, 54)
		zoom.pressed.connect(_rebuild_public)
		search_row.add_child(zoom)
		_table.add(search_row).grow_x_axis().pad(4)
		_table.row()

	_table.add(body).grow_x_axis().pad(4)
	_table.row()
	return body


func _toggle_section(body: Control, glyph: Label) -> void:
	body.visible = not body.visible
	glyph.text = MindIcons.glyph("up-open" if body.visible else "down-open")


func shown() -> void:
	_rebuild_servers()
	# `setupRemote`/`refreshAll` (`JoinDialog.java:143-150,329-331`): the
	# community (`@servers.global`) section and its refresh exist only while
	# `Core.settings.getBool("communityservers", true)`.
	var community := _community_servers()
	for node in _global_nodes:
		if is_instance_valid(node):
			node.visible = community
	if community:
		_rebuild_public()


## `Core.settings.getBool("communityservers", true)` (Game settings row).
func _community_servers() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_bool"):
		return true
	return bool(ui.call("settings_get_bool", "communityservers"))


## Rebuilds both server sections from the live probe data (one container row
## per entry).
func _rebuild_servers() -> void:
	if _local_body != null:
		_clear_container(_local_body)
		_build_server_row(_local_body, DEFAULT_SERVER_ADDRESS, -1)
	if _remote_body != null:
		_clear_container(_remote_body)
		for index in _servers.size():
			var entry: Dictionary = _servers[index]
			_build_server_row(_remote_body, str(entry.get("ip", "")), index)
	# the section bodies grew/shrank; re-place the dialog table
	call_deferred("_resort_tables")


## Removes + frees every child of a container (immediate rebuilds must not
## stack deferred frees).
func _clear_container(container: Node) -> void:
	for child in container.get_children():
		container.remove_child(child)
		child.queue_free()


## Polls every server row's probe and returns one signature over their rendered
## state (`server_probe_json`).
func _poll_server_signature() -> String:
	var signature := ""
	for address in [DEFAULT_SERVER_ADDRESS] + _server_addresses():
		signature += "%s=%s;" % [address, _probe_json(address)]
	return signature


## The address of every persisted server, in list order.
func _server_addresses() -> Array:
	var addresses := []
	for entry in _servers:
		addresses.append(str((entry as Dictionary).get("ip", "")))
	return addresses


## One address's probe state as compact JSON (empty when `MindNet` is absent).
func _probe_json(address: String) -> String:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net == null or not net.has_method("server_probe_json"):
		return "{}"
	return str(net.call("server_probe_json", address))


## One server row: address header + Java's action buttons (`JoinDialog.java:150-204`)
## followed by the probe content (`@server.refreshing`/`@host.invalid`/matches).
func _build_server_row(parent: Control, address: String, index: int) -> void:
	# code-instantiated: one server row per saved server plus the local default;
	# rows rebuild whenever a probe's state changes.
	var row := VBoxContainer.new()
	row.add_theme_constant_override("separation", 2)
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	var label := MindWidgets.styled_label(address, "outlineLabel")
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	label.add_theme_color_override("default_color", MindStyles.ACCENT)
	header.add_child(label)
	if index >= 0:
		header.add_child(_row_action("up-open", _move_server.bind(index, -1)))
		header.add_child(_row_action("down-open", _move_server.bind(index, 1)))
	header.add_child(_row_action("refresh", _refresh_server.bind(address)))
	if index >= 0:
		header.add_child(_row_action("pencil", _edit_server.bind(index)))
		header.add_child(_row_action("trash", _remove_server.bind(index)))
	row.add_child(header)
	_build_server_content(row, address)
	parent.add_child(row)


## One row action button (`Styles.emptyi` + icon glyph).
func _row_action(icon: String, action: Callable) -> Button:
	# code-instantiated: per-server row action buttons (reorder/refresh/edit/remove).
	var button := MindWidgets.icon_button(icon, "", "emptyi")
	button.custom_minimum_size = Vector2(40, 40)
	button.pressed.connect(action)
	return button


## Appends one server's probe content to its row.
func _build_server_content(parent: Control, address: String) -> void:
	var parsed: Variant = JSON.parse_string(_probe_json(address))
	if not (parsed is Dictionary):
		return
	var probe: Dictionary = parsed
	var state := str(probe.get("state", "none"))
	var ping := int(probe.get("ping_ms", -1))
	var matches: Array = probe.get("matches", [])
	match state:
		"ready":
			if ping > 0:
				# `buildServer` shows `host.ping + "ms"` under the host info.
				parent.add_child(MindWidgets.styled_label("%dms" % ping, "outlineLabel"))
			if matches.is_empty():
				parent.add_child(MindWidgets.styled_label(_tm("@hosts.none"), "outlineLabel"))
			else:
				for match_variant in matches:
					if not (match_variant is Dictionary):
						continue
					var match: Dictionary = match_variant
					var match_id := int(match.get("match_id", 0))
					if match_id <= 0:
						continue
					var label := "%s  %s  %d/%d" % [
						str(match.get("map_id", "")),
						str(match.get("mode_name", "")),
						int(match.get("player_count", 0)),
						int(match.get("max_players", 0)),
					]
					var button := MindWidgets.button(label)
					button.pressed.connect(_join_on_server.bind(address, match_id))
					parent.add_child(button)
		"connecting":
			# `refreshServer`: `@server.refreshing` while the ping is in flight.
			parent.add_child(MindWidgets.styled_label(_tm("@server.refreshing"), "outlineLabel"))
		_:
			# `refreshServer` failure: `@host.invalid`.
			parent.add_child(MindWidgets.styled_label(_tm("@host.invalid"), "outlineLabel"))


## Global group body: the active server's public `all_matches` rows, filtered by
## the community search field.
func _rebuild_public() -> void:
	if _global_list == null:
		return
	_global_list.clear_children()
	var matches := _public_matches()
	for row_variant in matches:
		# code-instantiated: public match rows come from the server view.
		var row: Dictionary = row_variant
		var match_id := int(row.get("match_id", 0))
		if match_id <= 0:
			continue
		var label := "%s  %s  %d/%d" % [
			str(row.get("map_id", "")),
			str(row.get("mode_name", "")),
			int(row.get("player_count", 0)),
			int(row.get("max_players", 0)),
		]
		if not _server_search.is_empty() and not label.to_lower().contains(_server_search):
			continue
		var button := MindWidgets.button(label)
		button.pressed.connect(_join.bind(match_id))
		_global_list.add(button).grow_x_axis().pad(3)
		_global_list.row()
	if _global_list.get_child_count() == 0:
		# code-instantiated: empty-state row carries the refresh control.
		var empty := HBoxContainer.new()
		empty.add_theme_constant_override("separation", 6)
		var empty_label := MindWidgets.styled_label(_tm("@hosts.none"), "outlineLabel")
		empty_label.autowrap_mode = TextServer.AUTOWRAP_OFF
		empty_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		empty.add_child(empty_label)
		var refresh := MindWidgets.icon_button("refresh", "", "emptyi")
		refresh.custom_minimum_size = Vector2(70, 70)
		refresh.pressed.connect(_rebuild_public)
		empty.add_child(refresh)
		_global_list.add(empty).grow_x_axis().pad(4)
		_global_list.row()
	_global_list.sort_now()
	_global_list.call_deferred("sort_now")


## Live `all_matches` rows from `MindNet` (empty when absent or offline).
func _public_matches() -> Array:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net == null or not net.has_method("get_public_matches_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(net.call("get_public_matches_json")))
	return parsed if parsed is Array else []


func _on_search_changed(text: String) -> void:
	# `JoinDialog.java:419-421`: trim, collapse spaces, lowercase.
	_server_search = " ".join(text.strip_edges().to_lower().split(" ", false))
	_rebuild_public()


## `JoinDialog.safeConnect` intent: join the picked public match.
func _join(match_id: int) -> void:
	_push_identity()
	join_requested.emit(match_id)
	_close()


## Clicking a match under a server row: connect to that server first unless the
## active connector already points there (`JoinDialog.java:150-158`).
func _join_on_server(address: String, match_id: int) -> void:
	if match_id <= 0:
		return
	_push_identity()
	var net := get_node_or_null("/root/Spine/MindNet")
	var current := ""
	if net != null and net.has_method("current_server_address"):
		current = str(net.call("current_server_address"))
	if current != _normalized_address(address):
		connect_requested.emit(address)
	join_requested.emit(match_id)
	_close()


## Normalizes an address to `host[:port]/db` (no scheme), matching
## `MindNet.current_server_address` and the Rust parser defaults.
func _normalized_address(address: String) -> String:
	var value := address.strip_edges().trim_prefix("http://").trim_prefix("https://")
	var parts := value.split("/", false, 1)
	var authority := parts[0]
	var db := DEFAULT_DB_NAME
	if parts.size() > 1 and not parts[1].is_empty():
		db = parts[1].split("/", false, 1)[0]
	if not authority.contains(":"):
		authority += ":%d" % DEFAULT_PORT
	return "%s/%s" % [authority, db]


func _open_picker() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("open_dialog", "picker", "{}")
	var picker := get_tree().root.find_child("picker", true, false)
	if picker != null and picker.has_signal("color_selected") and not picker.is_connected("color_selected", _on_color_selected):
		picker.connect("color_selected", _on_color_selected)


func _on_color_selected(color: Color) -> void:
	_store_color(color)


## `Core.settings.getString("name")` (`JoinDialog.java:341`).
func _stored_name() -> String:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_text"):
		return ""
	return str(ui.call("settings_get_text", NAME_SETTING, ""))


## `Core.settings.getInt("color-0")` as `Color.rgba8888()`; unset (0) keeps the
## palette default (`JoinDialog.java:349`).
func _stored_color() -> Color:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_int"):
		return Color.WHITE
	var rgba := int(ui.call("settings_get_int", COLOR_SETTING, 0))
	if rgba == 0:
		return Color.WHITE
	return Color8((rgba >> 24) & 0xFF, (rgba >> 16) & 0xFF, (rgba >> 8) & 0xFF, rgba & 0xFF)


## `JoinDialog.java:342-346`: every name edit updates `player.name` and the
## `name` setting immediately.
func _store_name(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", NAME_SETTING, text)
	_push_identity()


## `JoinDialog.java:348-354`: picking a colour updates `player.color()` and the
## `color-0` setting immediately.
func _store_color(color: Color) -> void:
	_color = color
	if _swatch_tint != null:
		_swatch_tint.color = color
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_int"):
		ui.call("settings_put_int", COLOR_SETTING, color.to_rgba32())
	_push_identity()


## Pushes `player.name`/`player.color` to the relay identity reducers; the
## facade queues them and sends on connect (I08 T2).
func _push_identity() -> void:
	if _name_field == null:
		return
	var net := get_node_or_null("/root/Spine/MindNet")
	if net != null and net.has_method("set_player_identity"):
		net.call("set_player_identity", _name_field.text, _color.to_rgba32())


## `JoinDialog.java:83`: the add panel remembers the address field.
func _store_ip(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", IP_SETTING, text)


func _toggle_add() -> void:
	if _add_panel == null:
		return
	if _add_panel.visible:
		_hide_add()
		return
	_editing_index = -1
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_get_text"):
		_ip_field.text = str(ui.call("settings_get_text", IP_SETTING, ""))
	_add_panel.visible = true
	if _ip_field != null:
		_ip_field.grab_focus()


func _edit_server(index: int) -> void:
	if index < 0 or index >= _servers.size() or _add_panel == null:
		return
	_editing_index = index
	_ip_field.text = str((_servers[index] as Dictionary).get("ip", ""))
	_add_panel.visible = true
	_ip_field.grab_focus()


func _hide_add() -> void:
	if _add_panel != null:
		_add_panel.visible = false
	_editing_index = -1


## `JoinDialog.java:92-101` OK handler: append or replace, save, rebuild rows.
func _submit_add() -> void:
	var address := ""
	if _ip_field != null:
		address = _ip_field.text.strip_edges()
	if address.is_empty():
		return
	if _editing_index >= 0 and _editing_index < _servers.size():
		var entry: Dictionary = _servers[_editing_index]
		entry["ip"] = address
	else:
		_servers.append({"ip": address, "port": 0})
	_save_servers()
	_rebuild_servers()
	_hide_add()


## Java `moveRemote` (`JoinDialog.java:206-222`): reorder and persist.
func _move_server(index: int, sign: int) -> void:
	var target := index + sign
	if target < 0 or target >= _servers.size():
		return
	var entry: Variant = _servers[index]
	_servers.remove_at(index)
	_servers.insert(target, entry)
	_save_servers()
	_rebuild_servers()


## Java confirm flow (`JoinDialog.java:194-202`) then remove + persist.
func _remove_server(index: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_confirm"):
		_delete_server(index)
		return
	_pending_delete = index
	ui.call("show_confirm", _t("@server.delete"))


func _on_confirm_result(confirmed: bool) -> void:
	var index := _pending_delete
	_pending_delete = -1
	if confirmed:
		_delete_server(index)


func _delete_server(index: int) -> void:
	if index < 0 or index >= _servers.size():
		return
	_servers.remove_at(index)
	_save_servers()
	_rebuild_servers()


## `JoinDialog.refresh` row action: drop the probe so the next poll re-pings.
func _refresh_server(address: String) -> void:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net != null and net.has_method("refresh_server"):
		net.call("refresh_server", address)
	# force the next poll to rebuild the rows
	_server_signature = ""


## `loadServers` (`JoinDialog.java:689-694`): `Core.settings.getJson("servers")`.
func _load_servers() -> void:
	_servers.clear()
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_text"):
		return
	var text := str(ui.call("settings_get_text", SERVERS_SETTING, ""))
	if text.is_empty():
		return
	var parsed: Variant = JSON.parse_string(text)
	if not (parsed is Array):
		return
	for entry in parsed:
		if entry is Dictionary and not str((entry as Dictionary).get("ip", "")).is_empty():
			_servers.append({
				"ip": str(entry.get("ip", "")),
				"port": int(entry.get("port", 0)),
			})


## `saveServers` (`JoinDialog.java:773-775`): `Core.settings.putJson`.
func _save_servers() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", SERVERS_SETTING, JSON.stringify(_servers))


func _show_info() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t("@join.info"))


## Closes through `MindUi` so the dialog stack and pause governor stay in sync.
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "join")
	else:
		hide_dialog()
