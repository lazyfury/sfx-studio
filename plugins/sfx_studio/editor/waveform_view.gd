@tool
class_name SfxWaveformView
extends Control
## 波形视图：直接读 AudioStreamWAV 的 PCM 数据画柱状图。
## 编辑器插件与游戏里都能用（不依赖 Editor* 类）。

var stream: AudioStreamWAV
var show_loop := true

var _peak := 0.0


func _ready() -> void:
	custom_minimum_size = Vector2(0, 96)
	# Read-only display: let the Inspector scroll when the wheel is over it.
	mouse_filter = Control.MOUSE_FILTER_IGNORE


func set_stream(value: AudioStreamWAV) -> void:
	stream = value
	_peak = 0.0
	if stream != null and not stream.data.is_empty():
		var data := stream.data
		for i in range(0, data.size(), 2):
			_peak = maxf(_peak, absf(float(data.decode_s16(i)) / 32767.0))
	queue_redraw()


func _draw() -> void:
	var bg := _theme_color("dark_color_1", Color(0.08, 0.09, 0.10))
	var grid := _theme_color("font_disabled_color", Color(1, 1, 1, 0.12)).darkened(0.2)
	var wave := _theme_color("accent_color", Color(0.36, 0.72, 0.96))
	var text := _theme_color("font_color", Color(0.85, 0.86, 0.88))
	var font := get_theme_default_font()
	var fs := get_theme_default_font_size()

	draw_rect(Rect2(Vector2.ZERO, size), bg)
	var mid := size.y * 0.5
	draw_line(Vector2(0, mid), Vector2(size.x, mid), grid, 1.0)
	for i in 4:
		var y := size.y * float(i) / 4.0
		draw_line(Vector2(0, y), Vector2(size.x, y), grid, 1.0)

	if stream == null or stream.data.is_empty():
		_draw_badge(font, "无波形", Vector2(6, 6), fs, text, bg)
		return

	var data := stream.data
	var frames := int(data.size() / 2)
	var half := size.y * 0.46
	var width := maxi(int(size.x), 1)

	# 每列像素取该时间片内的 min/max，画一条竖线
	for x in width:
		var first := int(float(x) * float(frames) / float(width))
		var last := int(float(x + 1) * float(frames) / float(width))
		if last <= first:
			last = first + 1
		var lo := 0.0
		var hi := 0.0
		for i in range(first, mini(last, frames)):
			var v := float(data.decode_s16(i * 2)) / 32767.0
			lo = minf(lo, v)
			hi = maxf(hi, v)
		draw_line(Vector2(x + 0.5, mid - hi * half), Vector2(x + 0.5, mid - lo * half), wave, 1.0)

	# 循环点
	if show_loop and stream.loop_mode != AudioStreamWAV.LOOP_DISABLED and frames > 0:
		var loop_color := _theme_color("warning_color", Color(1, 0.65, 0.2))
		for point in [stream.loop_begin, stream.loop_end]:
			if point > 0:
				var px := float(point) / float(frames) * size.x
				draw_line(Vector2(px, 0), Vector2(px, size.y), loop_color, 1.0)

	var info := "%.3f s   %d Hz   峰值 %.2f" % [stream.get_length(), stream.mix_rate, _peak]
	_draw_badge(font, info, Vector2(6, 6), fs, text, bg)


func _draw_badge(font: Font, text: String, pos: Vector2, fs: int, color: Color, bg: Color) -> void:
	var text_size := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs)
	var pad := 6.0
	var rect := Rect2(pos, Vector2(text_size.x + pad * 2.0, fs + pad))
	var style := StyleBoxFlat.new()
	style.bg_color = Color(bg.lightened(0.10).r, bg.lightened(0.10).g, bg.lightened(0.10).b, 0.92)
	style.set_corner_radius_all(3)
	style.set_border_width_all(1)
	style.border_color = Color(color.r, color.g, color.b, 0.16)
	draw_style_box(style, rect)
	draw_string(font, rect.position + Vector2(pad, fs + pad * 0.45), text,
		HORIZONTAL_ALIGNMENT_LEFT, -1, fs, color)


func _theme_color(name: StringName, fallback: Color) -> Color:
	if has_theme_color(name, "Editor"):
		return get_theme_color(name, "Editor")
	return fallback
