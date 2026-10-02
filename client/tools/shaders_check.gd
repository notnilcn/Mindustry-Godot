# SPDX-License-Identifier: GPL-3.0-only
#
# Headless shader compile check (plan 16 M8 oracle).
#
# Godot's `--headless --editor --quit` import does not compile unreferenced
# `.gdshader` files, so this script loads every `res://shaders/*.gdshader`,
# assigns it to a `ShaderMaterial` (which triggers the shader compiler
# front-end even under the dummy driver) and reports parse/compile errors.
#
# Usage:
#   godot4 --headless --path client --script res://tools/shaders_check.gd
#
# Exit code 0 when every shader compiles, 1 otherwise.

extends SceneTree

const SHADER_DIR := "res://shaders"


func _declared_uniform_count(source: String) -> int:
	var count := 0
	for raw in source.split("\n"):
		var line := raw.strip_edges()
		if line.begins_with("uniform "):
			count += 1
	return count


func _init() -> void:
	var dir := DirAccess.open(SHADER_DIR)
	if dir == null:
		printerr("shaders_check: cannot open ", SHADER_DIR)
		quit(1)
		return
	var files := dir.get_files()
	files.sort()
	var total := 0
	var failed := 0
	for file in files:
		if not file.ends_with(".gdshader"):
			continue
		total += 1
		var path := SHADER_DIR.path_join(file)
		var file_access := FileAccess.open(path, FileAccess.READ)
		var source := "" if file_access == null else file_access.get_as_text()
		var shader: Shader = load(path)
		if shader == null:
			printerr("SHADERCHECK: LOAD FAILED ", path)
			failed += 1
			continue
		var material := ShaderMaterial.new()
		material.shader = shader
		# `get_shader_uniform_list()` forces compilation; a shader that fails to
		# compile reports zero uniforms even though the source declares some.
		var compiled := shader.get_shader_uniform_list().size()
		var declared := _declared_uniform_count(source)
		if declared > 0 and compiled == 0:
			printerr("SHADERCHECK: COMPILE FAILED ", file, " (declared=", declared, " compiled=0)")
			failed += 1
			continue
		print("SHADERCHECK: ", file, " uniforms=", compiled)
	print("SHADERCHECK: total=", total, " failed=", failed)
	quit(1 if failed > 0 else 0)
