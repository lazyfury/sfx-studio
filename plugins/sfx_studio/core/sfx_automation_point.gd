@tool
class_name SfxAutomationPoint
extends Resource
## One automation point: time relative to the clip start, value in the lane's units.

@export_range(0.0, 3.0, 0.001, "suffix:s") var time := 0.0
@export var value := 0.0
