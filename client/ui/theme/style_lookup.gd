## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/builder/UiStyleLookup.java (plan 14 §3.6).
##
## Resolves an MSUI `style:` name against `styles_manifest.json`, matching the
## Rust `mind_core::ui::builder::style_lookup` half. Names are the parity ABI.

class_name MindStyleLookup
extends RefCounted


## Returns `name` when it belongs to `kind`, else "" (unknown styles are a no-op).
static func get_style(manifest: Dictionary, kind: String, name: String) -> String:
	for entry in manifest.get(kind, []):
		if str(entry) == name:
			return name
	return ""


## The manifest group key for a Godot base type (inverse of the theme mapping).
static func kind_for_base_type(base_type: String) -> String:
	match base_type:
		"Button":
			return "text_buttons"
		"PanelContainer", "Panel":
			return "panes"
		"HSlider":
			return "sliders"
		"RichTextLabel", "Label":
			return "labels"
		"LineEdit":
			return "fields"
		"CheckBox":
			return "checks"
		"Tree":
			return "trees"
		_:
			return "drawables"
