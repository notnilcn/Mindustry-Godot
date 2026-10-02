## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Styles.java (field inventory),
##         mindustry.graphics.Pal (accent/stat/negstat).
##
## Color/style constants and the manifest group -> Godot base-type mapping used
## by [MindThemeBuilder]. Style *names* are the parity ABI and must never change
## (plan 14 §3.10 rule 5); only this file's presentation values are ours.

class_name MindStyles
extends RefCounted

const ACCENT := Color(1.0, 0.827, 0.498)
const UNLAUNCHED := Color(0.537, 0.510, 0.929)
const HIGHLIGHT := Color(1.0, 0.878, 0.651)
const STAT := Color(1.0, 0.827, 0.498)
const NEGSTAT := Color(0.898, 0.329, 0.329)
const REMOVE := Color(0.898, 0.329, 0.329)

const BLACK := Color(0.0, 0.0, 0.0, 1.0)
const BLACK_3 := Color(0.0, 0.0, 0.0, 0.3)
const BLACK_5 := Color(0.0, 0.0, 0.0, 0.5)
const BLACK_6 := Color(0.0, 0.0, 0.0, 0.6)
const BLACK_8 := Color(0.0, 0.0, 0.0, 0.8)
const BLACK_9 := Color(0.0, 0.0, 0.0, 0.9)
const GRAY_PANEL := Color(0.16, 0.18, 0.21, 0.95)
const GRAY_PANEL_DARK := Color(0.10, 0.11, 0.13, 0.95)

## Manifest group name -> Godot base class for type variations.
const GROUP_BASE_TYPES := {
	"drawables": "PanelContainer",
	"text_buttons": "Button",
	"buttons": "Button",
	"image_buttons": "Button",
	"panes": "PanelContainer",
	"sliders": "HSlider",
	"labels": "RichTextLabel",
	"fields": "LineEdit",
	"checks": "CheckBox",
	"dialogs": "PanelContainer",
	"trees": "Tree",
}
