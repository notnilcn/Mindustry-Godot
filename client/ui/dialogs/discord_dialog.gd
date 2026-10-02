## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/DiscordDialog.java (plan 14 §3.4, M3).
##
## Invite banner + copy/open link buttons. `openURI` is the plan-22 platform
## hook; on desktop `OS.shell_open` stands in, and a failed open copies the link
## (upstream `DiscordDialog` behavior).

extends MindDialog

const DISCORD_URL := "https://discord.gg/mindustry"
## Ported from `DiscordDialog`'s banner color.
const BANNER_COLOR := Color("7289da")


func _ready() -> void:
	set_title_key("@discord")
	should_pause = false
	super._ready()
	_build()


func _build() -> void:
	var table := content_table()
	var banner := MindWidgets.label("[accent]%s" % _t("@discord"))
	banner.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	table.add(banner).grow_x_axis().pad(12)
	table.row()

	add_close_button()
	# code-instantiated: the button row is composed per dialog (`DiscordDialog`
	# builds back/copy/open at runtime).
	var copy := MindWidgets.button(_t("@copylink"))
	copy.pressed.connect(_copy_link)
	buttons.add_child(copy)
	var open := MindWidgets.button(_t("@openlink"))
	open.pressed.connect(_open_link)
	buttons.add_child(open)


func _copy_link() -> void:
	DisplayServer.clipboard_set(DISCORD_URL)
	_info("@copied")


func _open_link() -> void:
	if OS.shell_open(DISCORD_URL) != OK:
		_info("@linkfail")
		DisplayServer.clipboard_set(DISCORD_URL)


func _info(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t(key))
