@tool
extends EditorPlugin
## SFX Studio (read-only Inspector edition).
##
## No dock, no toolbar button, no editing surface. The plugin adds an
## EditorInspectorPlugin that summarises SfxPatch / SfxLayer / SfxClip /
## SfxMusic / SfxAutomationPoint and draws their waveform. Use the standalone
## SFX LAB client to design sounds.

const InspectorPlugin := preload("res://addons/sfx_studio/editor/sfx_inspector_plugin.gd")

var inspector_plugin: EditorInspectorPlugin


func _enter_tree() -> void:
	inspector_plugin = InspectorPlugin.new()
	add_inspector_plugin(inspector_plugin)


func _exit_tree() -> void:
	if inspector_plugin != null:
		remove_inspector_plugin(inspector_plugin)
		inspector_plugin = null
