## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ModBrowserDialog.java (plan 20 §3.9, M6).
##
## Remote mod browser: fetches the community `Mods.json` listings
## (`Vars.modJsonURLs`), filters/sorts them, and installs a repo by downloading its
## GitHub zipball into `user://modcache` and calling `MindMods.import_mod`.
## Listing parsing and version matching stay in `mind-core::mods::listing`
## (reached through `MindMods.parse_listing`/`MindMods.matching_release`); this
## script owns only HTTP and presentation (the documented Godot-side seam).

extends MindDialog

## Ported from `Vars.modJsonURLs` (tried in order).
const MOD_JSON_URLS := [
	"https://raw.githubusercontent.com/Anuken/MindustryMods/master/mods.json",
	"https://cdn.jsdelivr.net/gh/anuken/mindustrymods/mods.json",
]
## Ported from `Vars.ghApi`.
const GH_API := "https://api.github.com"
## TEMP download directory for zipballs (deleted after import).
const CACHE_PATH := "user://modcache"
## `Version.build` is surfaced by plan 22/23; until then version-specific release
## selection degrades to the untagged release (documented M6 seam).
const GAME_BUILD := 0
## `Version.revision` (see `GAME_BUILD`).
const GAME_REVISION := 0

var _search: LineEdit = null
var _repo_field: LineEdit = null
var _sort_button: Button = null
var _status: MindLabel = null
var _list: MindTable = null

var _listings: Array = []
var _installed_repos: Dictionary = {}
var _order_date := true
var _open_repo := ""
var _queue: Array = []
var _pending_repo := ""
var _pending_url_index := 0

var _list_http: HTTPRequest = null
var _api_http: HTTPRequest = null
var _zip_http: HTTPRequest = null


func _ready() -> void:
	set_title_key("@mods.browser")
	should_pause = false
	super._ready()
	_add_http()
	_build()
	add_close_button()
	_connect_mods()


func shown() -> void:
	# Lazy fetch on first open (upstream `shown(this::rebuildBrowser)`); `UiRoot`
	# eagerly instantiates every manifest dialog at boot, so no network at startup.
	_fetch_listings()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	var repo := str(context().get("repo", ""))
	if not repo.is_empty() and _repo_field != null:
		_repo_field.text = repo
		_install_repo(repo)


## Batch reinstall queue (upstream's `@mods.update.all` sequential import).
func install_repos(repos: Array) -> void:
	_queue = repos.duplicate()
	_order_next()


func _build() -> void:
	var root := content_table()
	root.add_theme_constant_override("separation", 6)

	# Search + sort row (`ModBrowserDialog` search field / order toggle).
	var search_row := HBoxContainer.new()
	_search = MindWidgets.field(_t("@search"))
	_search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search.text_changed.connect(func(_value: String) -> void: _rebuild())
	search_row.add_child(_search)
	_sort_button = MindWidgets.image_button("list", "emptyi")
	_sort_button.tooltip_text = _t("@mods.browser.sortdate")
	_sort_button.pressed.connect(_toggle_order)
	search_row.add_child(_sort_button)
	root.add(search_row).grow_x_axis().pad(4)
	root.row()

	# Direct GitHub repo import fallback (`@mod.import.github`).
	var repo_row := HBoxContainer.new()
	_repo_field = MindWidgets.field(_t("@mods.github.open"))
	_repo_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_repo_field.text_submitted.connect(_install_repo)
	repo_row.add_child(_repo_field)
	var install := MindWidgets.button(_t("@mods.browser.add"))
	install.pressed.connect(func() -> void: _install_repo(_repo_field.text))
	repo_row.add_child(install)
	root.add(repo_row).grow_x_axis().pad(4)
	root.row()

	_status = MindWidgets.label("")
	_status.visible = false
	root.add(_status).grow_x_axis().pad(2)
	root.row()

	_list = MindTable.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(_list).grow().pad(4)


func _fetch_listings() -> void:
	if not _listings.is_empty():
		_rebuild()
		return
	_status.text = _t("@loading")
	_status.visible = true
	_fetch_url_index(0)


func _fetch_url_index(index: int) -> void:
	if index >= MOD_JSON_URLS.size():
		_status.text = _fmt("@connectfail", ["mods.json"])
		_rebuild()
		return
	_pending_url_index = index
	if _list_http.request(MOD_JSON_URLS[index]) != OK:
		_fetch_url_index(index + 1)


func _on_list_completed(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
	if result == HTTPRequest.RESULT_SUCCESS and code == 200:
		var parsed: Variant = JSON.parse_string(body.get_string_from_utf8())
		if parsed is Array:
			_parse_listings(parsed)
			return
	_fetch_url_index(_pending_url_index + 1)


## Normalizes each raw listing through `mind-core`'s `ModListing` (camelCase +
## defaults) so the dialog never re-implements the listing model.
func _parse_listings(raw: Array) -> void:
	_listings.clear()
	var mods := _mods()
	for entry in raw:
		if not (entry is Dictionary):
			continue
		var listing: Variant = entry
		if mods != null:
			var parsed: Dictionary = mods.call("parse_listing", JSON.stringify(entry))
			if bool(parsed.get("ok", false)):
				listing = parsed.get("listing", entry)
		_listings.append(listing)
	_status.visible = false
	_refresh_installed()
	_rebuild()


func _refresh_installed() -> void:
	_installed_repos.clear()
	var mods := _mods()
	if mods != null:
		for entry in mods.call("list"):
			if not (entry is Dictionary):
				continue
			var name := str(entry.get("name", ""))
			var details: Dictionary = mods.call("details", name)
			var repo := str(details.get("repo", ""))
			if not repo.is_empty():
				_installed_repos[repo] = name
	_publish_updates()


## Pushes `internal name -> remote version` to the ModsDialog (`withUpdates`).
func _publish_updates() -> void:
	var mods_dialog := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/mods")
	if mods_dialog == null or not mods_dialog.has_method("set_updates"):
		return
	var mods := _mods()
	var updates := {}
	for listing in _listings:
		var repo := str(listing.get("repo", ""))
		if not _installed_repos.has(repo):
			continue
		var name := str(_installed_repos[repo])
		var details: Dictionary = mods.call("details", name) if mods != null else {}
		var latest := str(listing.get("version", ""))
		if not latest.is_empty() and latest != str(details.get("version", "")):
			updates[name] = latest
	mods_dialog.call("set_updates", updates)


func _rebuild() -> void:
	if _list == null:
		return
	_list.clear_children()
	if _listings.is_empty():
		_list.add(MindWidgets.label(_t("@mods.none"))).pad(8)
		return
	var query := _search.text.to_lower() if _search != null else ""
	var visible: Array = []
	for listing in _listings:
		if not (listing is Dictionary):
			continue
		var entry: Dictionary = listing
		if query.is_empty() or str(entry.get("name", "")).to_lower().contains(query) \
				or str(entry.get("repo", "")).to_lower().contains(query):
			visible.append(entry)
	if _order_date:
		visible.sort_custom(_sort_by_date)
	else:
		visible.sort_custom(_sort_by_stars)
	if visible.is_empty():
		_list.add(MindWidgets.label(_t("@none.found"))).pad(8)
		return
	for listing in visible:
		_add_listing_row(listing)


func _add_listing_row(listing: Dictionary) -> void:
	var repo := str(listing.get("repo", ""))
	var name := str(listing.get("name", repo))
	# code-instantiated: browser rows are data-driven from the fetched `ModListing`s.
	var box := HBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var info := Button.new()
	info.theme_type_variation = "flatBordert"
	info.alignment = HORIZONTAL_ALIGNMENT_LEFT
	info.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	info.text = "%s\n%s %s  %s %s" % [
		name,
		_t("@mods.browser.latest"),
		str(listing.get("version", "")),
		"\u2605",
		str(listing.get("stars", 0)),
	]
	info.pressed.connect(_toggle_repo.bind(repo))
	box.add_child(info)

	var installed := _installed_repos.has(repo)
	var add := MindWidgets.button(_t("@mods.browser.reinstall") if installed else _t("@mods.browser.add"))
	add.pressed.connect(_install_repo.bind(repo))
	box.add_child(add)

	_list.add(box).grow_x_axis().pad(3)
	_list.row()
	if _open_repo == repo:
		_add_listing_details(listing)


func _add_listing_details(listing: Dictionary) -> void:
	# code-instantiated: details come from the fetched `ModListing` (GitHub model).
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var author := str(listing.get("author", ""))
	if not author.is_empty():
		box.add_child(MindWidgets.label("%s %s" % [_t("@editor.author"), author]))
	var description := str(listing.get("description", ""))
	if not description.is_empty():
		var body := MindWidgets.label(description)
		body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		box.add_child(body)
	var min_version := str(listing.get("min_game_version", listing.get("minGameVersion", "")))
	if not min_version.is_empty():
		box.add_child(MindWidgets.label(_fmt("@mod.requiresversion", [min_version])))
	var tags: Array = listing.get("tags", [])
	if not tags.is_empty():
		box.add_child(MindWidgets.label(", ".join(_string_list(tags))))

	var release := _matching_release(listing)
	if not release.is_empty():
		box.add_child(MindWidgets.label("%s %s" % [_t("@mods.browser.releases"), str(release.get("version", release.get("id", "")))]))
	var repo := str(listing.get("repo", ""))
	var actions := HBoxContainer.new()
	var open_repo := MindWidgets.button(_t("@mods.github.open"))
	open_repo.pressed.connect(_open_url.bind("https://github.com/%s" % repo))
	actions.add_child(open_repo)
	var add := MindWidgets.button(_t("@mods.browser.add"))
	add.pressed.connect(_install_repo.bind(repo))
	actions.add_child(add)
	box.add_child(actions)
	_list.add(box).grow_x_axis().pad(6)
	_list.row()


## `ModListing.getMatchingRelease` via `mind-core` (`parseVersion`/`matchesGameVersion`).
func _matching_release(listing: Dictionary) -> Dictionary:
	var mods := _mods()
	if mods == null:
		return {}
	var result: Dictionary = mods.call(
		"matching_release", JSON.stringify(listing), GAME_BUILD, GAME_REVISION
	)
	return result if bool(result.get("found", false)) else {}


func _sort_by_date(a: Dictionary, b: Dictionary) -> bool:
	return str(a.get("last_updated", a.get("lastUpdated", ""))) > str(b.get("last_updated", b.get("lastUpdated", "")))


func _sort_by_stars(a: Dictionary, b: Dictionary) -> bool:
	return int(a.get("stars", 0)) > int(b.get("stars", 0))


func _toggle_order() -> void:
	_order_date = not _order_date
	if _sort_button != null:
		_sort_button.tooltip_text = _t("@mods.browser.sortdate") if _order_date else _t("@mods.browser.sortstars")
	_rebuild()


func _toggle_repo(repo: String) -> void:
	_open_repo = "" if _open_repo == repo else repo
	_rebuild()


## Downloads `<repo>`'s default-branch zipball, then imports it via `MindMods`.
func _install_repo(repo: String) -> void:
	var cleaned := repo.strip_edges().replace(" ", "")
	if cleaned.begins_with("https://github.com/"):
		cleaned = cleaned.trim_prefix("https://github.com/")
	cleaned = cleaned.trim_suffix("/").trim_suffix(".git")
	if cleaned.is_empty() or cleaned.find("/") == -1:
		return
	_pending_repo = cleaned
	_status.text = _fmt("@mods.downloading", [cleaned])
	_status.visible = true
	var error := _api_http.request("%s/repos/%s" % [GH_API, cleaned])
	if error != OK:
		_install_failed(str(error))


func _on_api_completed(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
	if result != HTTPRequest.RESULT_SUCCESS or code != 200:
		_install_failed(_fmt("@connectfail", [str(code)]))
		return
	var parsed: Variant = JSON.parse_string(body.get_string_from_utf8())
	var branch := "master"
	if parsed is Dictionary:
		branch = str(parsed.get("default_branch", "master"))
	if _zip_http.request("%s/repos/%s/zipball/%s" % [GH_API, _pending_repo, branch]) != OK:
		_install_failed(_fmt("@connectfail", ["zipball"]))


func _on_zip_completed(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
	if result != HTTPRequest.RESULT_SUCCESS or code != 200:
		_install_failed(_fmt("@connectfail", [str(code)]))
		return
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(CACHE_PATH))
	var path := "%s/%s.zip" % [CACHE_PATH, _pending_repo.replace("/", "_")]
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		_install_failed(_t("@mod.delete.error"))
		return
	file.store_buffer(body)
	file.close()
	var mods := _mods()
	var imported: Dictionary = mods.call("import_mod", path) if mods != null else {}
	DirAccess.remove_absolute(ProjectSettings.globalize_path(path))
	if mods != null and not bool(imported.get("ok", false)):
		_install_failed(str(imported.get("error", _t("@mod.delete.error"))))
		return
	_status.visible = false
	_refresh_installed()
	_rebuild()
	_order_next()


func _install_failed(message: String) -> void:
	_status.text = message
	_status.visible = true
	var ui := _ui()
	if ui != null:
		ui.call("show_info", message)
	_order_next()


func _order_next() -> void:
	if _queue.is_empty():
		return
	_install_repo(str(_queue.pop_front()))


func _on_import_progress(_path: String, ratio: float) -> void:
	if _status == null or not _status.visible:
		return
	_status.text = "%s %d%%" % [_fmt("@mods.downloading", [_pending_repo]), int(ratio * 100.0)]


func _add_http() -> void:
	# code-instantiated: one HTTPRequest per concurrent step (list/api/zip) since a
	# single request node cannot multiplex the sequential GitHub download flow.
	_list_http = HTTPRequest.new()
	_list_http.request_completed.connect(_on_list_completed)
	add_child(_list_http)
	_api_http = HTTPRequest.new()
	_api_http.request_completed.connect(_on_api_completed)
	add_child(_api_http)
	_zip_http = HTTPRequest.new()
	_zip_http.request_completed.connect(_on_zip_completed)
	add_child(_zip_http)


func _connect_mods() -> void:
	var mods := _mods()
	if mods != null and mods.has_signal("mod_import_progress") \
			and not mods.is_connected("mod_import_progress", Callable(self, "_on_import_progress")):
		mods.connect("mod_import_progress", Callable(self, "_on_import_progress"))


func _open_url(url: String) -> void:
	OS.shell_open(url)


func _mods() -> Node:
	return get_node_or_null("/root/MindMods")


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")


## Bundle lookup with `{0}`-style substitution (`Bundle.format`).
func _fmt(key: String, args: Array) -> String:
	var result := _t(key)
	for index in args.size():
		result = result.replace("{%d}" % index, str(args[index]))
	return result


func _string_list(value: Variant) -> PackedStringArray:
	var result := PackedStringArray()
	if value is Array:
		for item in value:
			result.append(str(item))
	return result
