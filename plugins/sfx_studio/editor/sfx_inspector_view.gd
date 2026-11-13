@tool
extends VBoxContainer
## Compact read-only summary for SfxPatch / SfxLayer / SfxClip / SfxMusic.

const WaveformView := preload("res://addons/sfx_studio/editor/waveform_view.gd")
const PatchScript := preload("res://addons/sfx_studio/core/sfx_patch.gd")
const LayerScript := preload("res://addons/sfx_studio/core/sfx_layer.gd")

var _object: Resource


func setup(value: Resource) -> void:
	_object = value
	add_theme_constant_override("separation", 6)
	_clear()
	_line("只读 · Read-only。编辑请使用 SFX LAB 客户端或直接修改 .tres。", true)
	if value is SfxPatch:
		_patch(value)
	elif value is SfxLayer:
		_layer(value)
	elif value is SfxClip:
		_clip(value)
	elif value is SfxAutomationPoint:
		_point(value)
	elif value is SfxMusic:
		_music(value)
	else:
		_line("不支持的 SFX 资源类型。")


func _clear() -> void:
	for child in get_children():
		remove_child(child)
		child.queue_free()


func _patch(patch: SfxPatch) -> void:
	var name := patch.resource_path.get_file().get_basename()
	_title(name if not name.is_empty() else "SfxPatch")
	if not patch.group.strip_edges().is_empty():
		_line("分组：%s" % patch.group.strip_edges())
	_line("采样率：%d Hz" % patch.rate)
	_line("时长：%.3f s" % patch.end_time())
	_line("图层：%d · 归一化：%s" % [patch.layers.size(), "开" if patch.normalize_layers else "关"])
	if patch.baked != null:
		_line("已烘焙：%s" % patch.baked.resource_path)
	_waveform(patch.build_live())

	_section("图层")
	if patch.layers.is_empty():
		_line("没有图层。", true)
		return
	for i in patch.layers.size():
		var layer: SfxLayer = patch.layers[i]
		if layer == null:
			continue
		var clip_count := layer.clips.size()
		var points := 0
		for clip in layer.clips:
			points += clip.gain_points.size() + clip.cutoff_points.size()
		_line("%d. %s · %.3f s · gain %.2f%s" % [
			i + 1,
			SfxLayer.wave_name(layer.wave),
			layer.duration,
			layer.gain,
			"" if layer.enabled else " · 已禁用",
		])
		if clip_count > 0:
			_line("   %d clip · %d 自动化点" % [clip_count, points], true)
			for j in layer.clips.size():
				var clip: SfxClip = layer.clips[j]
				_line("   %02d  起点 %.3fs · 时长 %.3fs · 增益 %.2f · 变调 %+.1fst" % [
					j + 1, clip.start, clip.duration, clip.gain, clip.pitch,
				], true)
		_layer_detail(layer, "   ")


func _layer(layer: SfxLayer) -> void:
	_title("图层 · %s" % SfxLayer.wave_name(layer.wave))
	_line("启用：%s · 延迟 %.3fs · 时长 %.3fs · 增益 %.2f" % [
		"是" if layer.enabled else "否", layer.delay, layer.duration, layer.gain,
	])
	if layer.wave == SfxLayer.Wave.SAMPLE:
		_line("采样：%s" % (layer.sample.resource_path if layer.sample != null else "未选择"))
	else:
		_line("频率：%.1f → %.1f Hz" % [layer.freq_start, layer.freq_end])
	if layer.clips.is_empty():
		_line("隐式 clip（使用图层的延迟/时长）。", true)
	else:
		var points := 0
		for clip in layer.clips:
			points += clip.gain_points.size() + clip.cutoff_points.size()
		_line("Clip：%d · 自动化点：%d" % [layer.clips.size(), points])
	_layer_detail(layer)
	var temp: SfxPatch = PatchScript.new()
	temp.rate = 22050
	var layers: Array[SfxLayer] = [layer]
	temp.layers = layers
	_waveform(temp.build_live())


func _clip(clip: SfxClip) -> void:
	_title("Clip")
	_line("起点：%.3f s · 时长：%.3f s" % [clip.start, clip.duration])
	_line("增益：%.2f · 变调：%+.1f st" % [clip.gain, clip.pitch])
	_line("GAIN 自动化点：%d · CUTOFF 自动化点：%d" % [
		clip.gain_points.size(), clip.cutoff_points.size(),
	])


func _point(point: SfxAutomationPoint) -> void:
	_title("自动化点")
	_line("时间：%.3f s" % point.time)
	_line("值：%.3f" % point.value)


func _music(music: SfxMusic) -> void:
	_title("SfxMusic")
	_line("BPM：%.1f · 小节：%d · 采样率：%d Hz" % [music.bpm, music.bars, music.rate])
	var seconds := 60.0 / maxf(music.bpm, 1.0) * 4.0 * float(music.bars)
	_line("时长：%.2f s · 音量：%.1f dB" % [seconds, music.volume_db])
	_line("低音：%d 个 · 和弦：%d 个" % [music.bass_notes.size(), music.chords.size()])
	_line("波形：贝斯 %d · 琶音 %d · Swing %.2f" % [
		music.bass_wave, music.arp_wave, music.swing,
	])
	_line("音量：贝斯 %.2f · 琶音 %.2f · 底鼓 %.2f · 踩镲 %.2f" % [
		music.bass_gain, music.arp_gain, music.kick_gain, music.hat_gain,
	])
	_line("16 步音序器：%s" % ("开" if music.use_patterns else "关"))
	if music.baked != null:
		_line("已烘焙：%s" % music.baked.resource_path)
	_waveform(music.build_live())


func _layer_detail(layer: SfxLayer, prefix := "") -> void:
	if layer.wave in [
		SfxLayer.Wave.SINE, SfxLayer.Wave.TRIANGLE, SfxLayer.Wave.SAW,
		SfxLayer.Wave.PULSE, SfxLayer.Wave.SQUARE,
	]:
		_line(prefix + "ADSR：起 %.3fs · 衰 %.3fs · 延 %.2f · 释 %.3fs" % [
			layer.attack, layer.decay, layer.sustain, layer.release,
		], true)
		_line(prefix + "滤波：%s · %.1f → %.1f Hz · Q %.2f" % [
			_filter_name(layer.filter_mode), layer.cutoff_start, layer.cutoff_end,
			layer.resonance,
		], true)
	elif layer.wave == SfxLayer.Wave.NOISE:
		_line(prefix + "低通：%.2f → %.2f · 噪声 %.2f · 扫频 %.2f" % [
			layer.lowpass_from, layer.lowpass_to, layer.noise_gain, layer.sweep_gain,
		], true)
		_line(prefix + "包络：%s · 衰减 %.2f · 种子 %d" % [
			"正弦" if layer.env_shape == SfxLayer.EnvShape.SINE else "指数",
			layer.decay, layer.seed_value,
		], true)
	elif layer.wave == SfxLayer.Wave.ARP:
		_line(prefix + "步长：%.3fs · 衰减 %.2f · 起音 %.4fs" % [
			layer.step, layer.decay, layer.attack,
		], true)
		_line(prefix + "频率：%s" % _join(layer.arp_freqs), true)
	elif layer.wave == SfxLayer.Wave.FM:
		_line(prefix + "FM：比率 %.2f · 深度 %.2f · 调制衰减 %.2f" % [
			layer.fm_ratio, layer.fm_index, layer.fm_decay,
		], true)
	elif layer.wave == SfxLayer.Wave.PLUCK:
		_line(prefix + "拨弦：阻尼 %.4f · 亮度 %.2f" % [
			layer.pluck_damping, layer.pluck_brightness,
		], true)
	elif layer.wave == SfxLayer.Wave.SAMPLE:
		_line(prefix + "采样：变调 %+.1fst · 循环 %s" % [
			layer.sample_pitch, "开" if layer.sample_loop else "关",
		], true)
	else:
		_line(prefix + "鼓：音高 %.2f · 力度 %.2f" % [layer.drum_tone, layer.drum_snap], true)


func _filter_name(mode: int) -> String:
	match mode:
		SfxLayer.FilterMode.LOWPASS: return "低通"
		SfxLayer.FilterMode.HIGHPASS: return "高通"
		SfxLayer.FilterMode.BANDPASS: return "带通"
		_: return "关闭"


func _join(values: PackedFloat64Array) -> String:
	var parts := PackedStringArray()
	for value in values:
		parts.append("%.1f" % value)
	return " · ".join(parts)


func _waveform(stream: AudioStreamWAV) -> void:
	if stream == null:
		return
	var view := WaveformView.new()
	add_child(view)
	view.custom_minimum_size = Vector2(0, 120)
	view.set_stream(stream)


func _section(text: String) -> void:
	add_child(HSeparator.new())
	_title(text)


func _title(text: String) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	var mark := ColorRect.new()
	mark.color = _accent_color()
	mark.custom_minimum_size = Vector2(3, 14)
	mark.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(mark)
	var label := Label.new()
	label.text = text
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if has_theme_font("bold_font", "EditorFonts"):
		label.add_theme_font_override("font", get_theme_font("bold_font", "EditorFonts"))
	row.add_child(label)
	add_child(row)


func _line(text: String, muted := false) -> void:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	if muted:
		label.add_theme_color_override("font_color", _muted_color())
	add_child(label)


func _muted_color() -> Color:
	if has_theme_color("font_disabled_color", "Editor"):
		return get_theme_color("font_disabled_color", "Editor")
	return Color(0.65, 0.68, 0.66)


func _accent_color() -> Color:
	if has_theme_color("accent_color", "Editor"):
		return get_theme_color("accent_color", "Editor")
	return Color(0.45, 0.85, 0.65)
