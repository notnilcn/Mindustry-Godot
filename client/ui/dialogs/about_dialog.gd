## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/AboutDialog.java,
##         core/src/mindustry/ui/Links.java (plan 14 §3.4, M0 slice).
##
## The M0 vertical slice: links grid + credits/close buttons. Titles/descriptions
## resolve through the plan-03 bundle (`link.<name>.title`/`.description`); the
## `@about.button` title key is unchanged from upstream (parity ABI).

extends MindDialog

## `Links.getLinks()` entries (name, link, icon region, color hex). Order and
## names are the upstream ABI; bannedItems (google-play/itch.io/dev-builds/
## f-droid) render only when the platform is not iOS/Steam (M3).
const LINKS := [
	{"name": "discord", "link": "https://discord.gg/mindustry", "icon": "discord", "color": "7289da"},
	{"name": "changelog", "link": "https://github.com/Anuken/Mindustry/releases", "icon": "list", "color": "ffd37f"},
	{"name": "trello", "link": "https://trello.com/b/aE2tcUwF", "icon": "trello", "color": "026aa7"},
	{"name": "wiki", "link": "https://mindustrygame.github.io/wiki/", "icon": "book", "color": "0f142f"},
	{"name": "suggestions", "link": "https://github.com/Anuken/Mindustry-Suggestions/issues/new/choose/", "icon": "add", "color": "ebebeb"},
	{"name": "reddit", "link": "https://www.reddit.com/r/Mindustry/", "icon": "redditAlien", "color": "ee593b"},
	{"name": "itch.io", "link": "https://anuke.itch.io/mindustry", "icon": "itchio", "color": "fa5c5c"},
	{"name": "google-play", "link": "https://play.google.com/store/apps/details?id=io.anuke.mindustry", "icon": "googleplay", "color": "689f38"},
	{"name": "f-droid", "link": "https://f-droid.org/packages/io.anuke.mindustry/", "icon": "android", "color": "026aa7"},
	{"name": "github", "link": "https://github.com/Anuken/Mindustry/", "icon": "github", "color": "24292e"},
	{"name": "dev-builds", "link": "https://github.com/Anuken/MindustryBuilds", "icon": "githubSquare", "color": "fafbfc"},
	{"name": "bug", "link": "https://github.com/Anuken/Mindustry/issues/new", "icon": "wrench", "color": "cbd97f"},
]

var _grid: MindTable = null


func _ready() -> void:
	title_text = _t("@about.button")
	title_color = MindStyles.ACCENT
	super._ready()
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	# code-instantiated: link rows come from the Links.getLinks() data table, which
	# is not known at scene-authoring time (plan 14 §3.4 AboutDialog.setup()).
	_grid = MindTable.new()
	_grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_grid.add_theme_constant_override("separation", 2)
	cont.add_child(_grid)
	for link in LINKS:
		_add_link(link)
	add_close_button()
	add_button(_t("@credits"), _show_credits, "", 200.0)


## Upstream `AboutDialog.showCredits` opens the credits dialog.
func _show_credits() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", "credits", "{}")


func _add_link(link: Dictionary) -> void:
	var button := Button.new()
	button.text = _link_title(str(link.name))
	button.icon = MindWidgets.icon_texture(str(link.icon))
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	button.theme_type_variation = "flatBordert"
	button.pressed.connect(_open_link.bind(str(link.link)))
	_grid.add(button).grow_x_axis().pad(2)
	_grid.row()
	var description := MindWidgets.label(_t("@link.%s.description" % str(link.name)))
	description.add_theme_color_override("default_color", Color(0.8, 0.8, 0.8))
	_grid.add(description).grow_x_axis().pad(1)
	_grid.row()


## `Links.LinkEntry.title`: bundle override, else the `Strings.capitalize`
## fallback over the hyphen-replaced name (`Links.java`). The bundle has no
## `link.*.title` keys, so the fallback is what the Java reference renders
## ("Google play", "F droid", "Dev builds").
func _link_title(name: String) -> String:
	var key := "link.%s.title" % name
	var value := _t("@" + key)
	if value != key:
		return value
	return _capitalize(name.replace("-", " "))


## Arc `Strings.capitalize`: upper-cases the first character and any character
## following `_`/`-` (which themselves become spaces).
static func _capitalize(text: String) -> String:
	var out := ""
	var upper_next := true
	for character in text:
		if character == "_" or character == "-":
			out += " "
			upper_next = true
			continue
		out += character.to_upper() if upper_next else character
		upper_next = false
	return out


func _open_link(url: String) -> void:
	if OS.shell_open(url) != OK:
		var ui := get_node_or_null("/root/MindUi")
		if ui != null:
			ui.call("show_info", _t("@linkfail"))


## Bundle lookup with the key echoed back (plan 03 `Bundle.get` semantics).
func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))
