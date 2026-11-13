@tool
extends EditorInspectorPlugin
## Read-only Inspector for SFX Studio resources.
##
## The plugin deliberately has no editing surface: sound design happens in the
## standalone SFX LAB client or by editing the `.tres` parameters directly.
## This inspector only summarises the resource and draws its waveform.

const View := preload("res://addons/sfx_studio/editor/sfx_inspector_view.gd")


func _can_handle(object: Object) -> bool:
	return object is SfxPatch \
		or object is SfxLayer \
		or object is SfxClip \
		or object is SfxAutomationPoint \
		or object is SfxMusic


func _parse_begin(object: Object) -> void:
	var view := View.new()
	add_custom_control(view)
	view.setup(object)


func _parse_property(
	object: Object, _type: Variant.Type, _name: String, _hint_type: PropertyHint,
	_hint_string: String, _usage_flags: int, _wide: bool
) -> bool:
	# Everything is read-only: hide the default property editors.
	return _can_handle(object)
