@tool
class_name SfxClip
extends Resource
## A placed instance of a layer's instrument. Multiple clips share the same
## oscillator/envelope/filter parameters but have their own start/length/gain,
## pitch and automation.

enum Lane { GAIN, CUTOFF }

@export_range(0.0, 8.0, 0.001, "suffix:s") var start := 0.0
@export_range(0.005, 3.0, 0.001, "suffix:s") var duration := 0.15
@export_range(0.0, 2.0, 0.005) var gain := 1.0
@export_range(-24.0, 24.0, 0.1, "suffix:st") var pitch := 0.0
@export var gain_points: Array[SfxAutomationPoint] = []
@export var cutoff_points: Array[SfxAutomationPoint] = []


func end_time() -> float:
	return start + duration


func has_automation() -> bool:
	return not gain_points.is_empty() or not cutoff_points.is_empty()


func lane_points(lane: Lane) -> Array[SfxAutomationPoint]:
	return gain_points if lane == Lane.GAIN else cutoff_points


func add_point(lane: Lane, time: float, value: float) -> int:
	var point := SfxAutomationPoint.new()
	point.time = clampf(time, 0.0, duration)
	point.value = value
	var points: Array[SfxAutomationPoint] = lane_points(lane)
	var index := points.size()
	for i in points.size():
		if points[i].time > point.time:
			index = i
			break
	points.insert(index, point)
	if lane == Lane.GAIN:
		gain_points = points
	else:
		cutoff_points = points
	return index


func remove_point(lane: Lane, index: int) -> void:
	var points: Array[SfxAutomationPoint] = lane_points(lane)
	if index < 0 or index >= points.size():
		return
	points.remove_at(index)
	if lane == Lane.GAIN:
		gain_points = points
	else:
		cutoff_points = points


func gain_at(t: float) -> float:
	if gain_points.is_empty():
		return 1.0
	var time := clampf(t, 0.0, maxf(duration, 0.0))
	var first: SfxAutomationPoint = gain_points[0]
	if time <= first.time:
		return first.value
	for i in range(1, gain_points.size()):
		var a: SfxAutomationPoint = gain_points[i - 1]
		var b: SfxAutomationPoint = gain_points[i]
		if time <= b.time:
			var span := maxf(b.time - a.time, 0.0001)
			var k := clampf((time - a.time) / span, 0.0, 1.0)
			return lerpf(a.value, b.value, k)
	return gain_points[gain_points.size() - 1].value


func cutoff_at(t: float) -> float:
	if cutoff_points.is_empty():
		return 18000.0
	var time := clampf(t, 0.0, maxf(duration, 0.0))
	var first: SfxAutomationPoint = cutoff_points[0]
	if time <= first.time:
		return first.value
	for i in range(1, cutoff_points.size()):
		var a: SfxAutomationPoint = cutoff_points[i - 1]
		var b: SfxAutomationPoint = cutoff_points[i]
		if time <= b.time:
			var span := maxf(b.time - a.time, 0.0001)
			var k := clampf((time - a.time) / span, 0.0, 1.0)
			return lerpf(a.value, b.value, k)
	return cutoff_points[cutoff_points.size() - 1].value


func clamp_points() -> void:
	for points in [gain_points, cutoff_points]:
		for point in points:
			point.time = clampf(point.time, 0.0, duration)
		points.sort_custom(func(a: SfxAutomationPoint, b: SfxAutomationPoint) -> bool:
			return a.time < b.time)
