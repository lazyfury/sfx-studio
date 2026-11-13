extends Node
## SFX Studio 无头自检：资源完整性、合成、音乐循环、存取往返、烘焙、Autoload。
##
## 用法（任意接入了插件的工程）：
##   godot --headless --path <工程> res://addons/sfx_studio/tools/self_test.tscn
## 或 tools/run_test.sh sfx
##
## 退出码：0 = 全过，1 = 有失败。

const PatchScript := preload("res://addons/sfx_studio/core/sfx_patch.gd")
const MusicScript := preload("res://addons/sfx_studio/core/sfx_music.gd")
const LayerScript := preload("res://addons/sfx_studio/core/sfx_layer.gd")

const TMP := "user://sfx_studio_selftest"

var _failures := 0
var _checks := 0


func _ready() -> void:
	await get_tree().process_frame
	_test_layers()
	_test_clips()
	_test_patches()
	_test_music()
	_test_roundtrip()
	_test_bake()
	_test_player()
	_cleanup()

	print("")
	if _failures == 0:
		print("SFX Studio 自检通过 ✅（%d 项）" % _checks)
	else:
		printerr("%d 项失败 / %d 项检查 ❌" % [_failures, _checks])
	get_tree().quit(1 if _failures > 0 else 0)


func _check(condition: bool, label: String) -> void:
	_checks += 1
	if condition:
		print("  [OK]   ", label)
	else:
		_failures += 1
		printerr("  [FAIL] ", label)


# =====================================================================
# 2. 图层与全部波形
# =====================================================================

func _test_layers() -> void:
	print("== 图层与波形 ==")
	var names := ["Sine", "Triangle", "Saw", "Pulse", "Square", "Noise", "Arp",
		"FM", "Pluck", "Sample", "Kick", "Snare", "Hat", "Clap"]
	for wave in range(names.size()):
		var layer: SfxLayer = LayerScript.new()
		layer.wave = wave
		layer.duration = 0.2
		if wave == SfxLayer.Wave.SAMPLE:
			layer.sample = _make_tone_stream(22050, 440.0, 0.2, 0.5)
		if wave == SfxLayer.Wave.ARP:
			layer.duration = 0.18
		var buf := layer.build(22050)
		_check(buf.size() > 0, "%s 波形非空" % names[wave])
		var finite := true
		var peak := 0.0
		for v in buf:
			if not is_finite(v):
				finite = false
			peak = maxf(peak, absf(v))
		_check(finite, "%s 采样有限" % names[wave])
		_check(peak > 0.0, "%s 峰值 > 0" % names[wave])

	# 多层叠加
	var l1: SfxLayer = LayerScript.new()
	l1.wave = SfxLayer.Wave.PULSE
	l1.freq_start = 900.0
	l1.freq_end = 200.0
	l1.duration = 0.12
	l1.gain = 0.4
	var l2: SfxLayer = LayerScript.new()
	l2.wave = SfxLayer.Wave.NOISE
	l2.duration = 0.2
	l2.gain = 0.2
	l2.lowpass_from = 0.6
	l2.lowpass_to = 0.1
	var stack: Array[SfxLayer] = [l1, l2]
	var p: SfxPatch = PatchScript.new()
	p.layers = stack
	_check(p.uses_layers(), "patch 使用图层")
	var stream := p.build()
	_check(stream != null and stream.data.size() > 0, "多层叠加可合成")
	var expected := int(0.2 * 22050.0) * 2
	_check(absf(stream.get_length() - 0.2) < 0.01, "多层时长 = 最长层")
	_check(stream.data.size() >= expected - 4, "多层采样数正确")


# =====================================================================
# 3. Clip / 变调 / 自动化
# =====================================================================

func _test_clips() -> void:
	print("== Clip / 变调 / 自动化 ==")
	var layer := SfxLayer.new()
	layer.wave = SfxLayer.Wave.SAW
	layer.duration = 0.4
	var clip := SfxClip.new()
	clip.start = 0.0
	clip.duration = 0.3
	clip.pitch = 12.0
	clip.gain_points.append(_point(0.0, 0.0))
	clip.gain_points.append(_point(0.3, 1.0))
	clip.cutoff_points.append(_point(0.0, 300.0))
	clip.cutoff_points.append(_point(0.3, 12000.0))
	clip.clamp_points()
	layer.clips.append(clip)
	var patch := SfxPatch.new()
	patch.rate = 22050
	patch.layers.append(layer)
	var stream := patch.build()
	_check(stream != null and not stream.data.is_empty(), "带 clip 的 patch 可合成")
	_check(absf(patch.end_time() - 0.3) < 0.01, "end_time 使用 clip")
	_check(absf(stream.get_length() - 0.3) < 0.01, "clip 时长正确")
	_check(clip.gain_at(0.15) > 0.1, "gain 自动化插值")
	_check(clip.cutoff_at(0.15) > 1000.0, "cutoff 自动化插值")
	var transposed := layer.with_pitch(12.0)
	_check(absf(transposed.freq_start - layer.freq_start * 2.0) < 0.01, "with_pitch +12st")
	_check(layer.clips.size() == 1, "with_pitch 不改原层")


	# Resource roundtrip: clips, pitch and automation must survive .tres.
	_ensure_dir(TMP)
	var path := TMP.path_join("clips.tres")
	_check(ResourceSaver.save(patch, path) == OK, "保存带 clip 的 patch")
	var back: SfxPatch = ResourceLoader.load(path)
	_check(back != null and back.layers.size() == 1, "重新加载带 clip 的 patch")
	if back != null and not back.layers.is_empty():
		var back_layer: SfxLayer = back.layers[0]
		_check(back_layer.clips.size() == 1, "clip 数量保留")
		if not back_layer.clips.is_empty():
			var back_clip: SfxClip = back_layer.clips[0]
			_check(is_equal_approx(back_clip.pitch, 12.0), "clip pitch 保留")
			_check(back_clip.gain_points.size() == 2, "gain 自动化点保留")
			_check(back_clip.cutoff_points.size() == 2, "cutoff 自动化点保留")
		_check(back.build().data == patch.build().data, "clip 重建波形一致")


func _point(time: float, value: float) -> SfxAutomationPoint:
	var point := SfxAutomationPoint.new()
	point.time = time
	point.value = value
	return point


# =====================================================================
# 4. 工程内资源
# =====================================================================

func _patch_dir() -> String:
	return str(ProjectSettings.get_setting("sfx_studio/patch_dir", "res://audio/sfx"))


func _music_path() -> String:
	return str(ProjectSettings.get_setting("sfx_studio/music_path", "res://audio/music.tres"))


func _test_patches() -> void:
	print("== 工程音效资源 ==")
	var dir := DirAccess.open(_patch_dir())
	_check(dir != null, "音效目录存在：%s" % _patch_dir())
	if dir == null:
		return

	var files := 0
	for file in dir.get_files():
		if not (file.ends_with(".tres") or file.ends_with(".res")):
			continue
		files += 1
		var path := _patch_dir().path_join(file)
		var patch := ResourceLoader.load(path)
		_check(patch is SfxPatch, "%s 类型正确" % file)
		if not (patch is SfxPatch):
			continue
		var p: SfxPatch = patch
		var stream: AudioStreamWAV = p.build()
		_check(stream != null and not stream.data.is_empty(), "%s 波形非空" % file)
		if stream == null:
			continue
		_check(stream.data.size() % 2 == 0, "%s 16bit 对齐" % file)
		_check(stream.format == AudioStreamWAV.FORMAT_16_BITS, "%s 格式 16bit" % file)
		_check(stream.mix_rate == p.rate, "%s 采样率 = %d" % [file, p.rate])
		var expected := 0.0
		for layer in p.layers:
			if layer != null:
				expected = maxf(expected, layer.end_time())
		_check(absf(stream.get_length() - expected) < 0.02, "%s 时长 %.3fs" % [file, expected])
		var peak := _peak(stream)
		_check(peak > 0.0 and peak <= 1.0, "%s 峰值 %.2f" % [file, peak])
	_check(files > 0, "至少有一条音效")


func _test_music() -> void:
	print("== 工程背景音乐 ==")
	var path := _music_path()
	_check(FileAccess.file_exists(path), "音乐资源存在：%s" % path)
	if not FileAccess.file_exists(path):
		return
	var music := ResourceLoader.load(path)
	_check(music is SfxMusic, "音乐类型正确")
	if not (music is SfxMusic):
		return
	var m: SfxMusic = music
	var stream: AudioStreamWAV = m.build()
	_check(stream != null and stream.get_length() > 0.0, "音乐可合成")
	if stream == null:
		return
	_check(stream.loop_mode == AudioStreamWAV.LOOP_FORWARD, "循环模式 = LOOP_FORWARD")
	_check(stream.loop_begin == 0, "循环起点 = 0")
	_check(stream.loop_end == int(stream.data.size() / 2), "循环终点 = 采样数")
	var expected := 60.0 / m.bpm * 4.0 * float(m.bars)
	_check(absf(stream.get_length() - expected) < 0.05, "音乐时长 %.2fs" % expected)
	var peak := _peak(stream)
	_check(peak > 0.0 and peak < 0.95, "音乐峰值 %.2f（不削波）" % peak)

	var variant: SfxMusic = MusicScript.new()
	variant.bars = 1
	variant.bass_wave = SfxMusic.Wave.SAW
	variant.arp_wave = SfxMusic.Wave.TRIANGLE
	variant.swing = 0.3
	var variant_stream := variant.build()
	_check(variant_stream != null and variant_stream.data.size() > 0, "音乐波形 / swing 可合成")

	variant.use_patterns = true
	variant.arp_pattern = PackedInt32Array([0, -1, 1, -1, 2, -1, 1, -1, 0, -1, 1, -1, 2, -1, 1, -1])
	variant.bass_pattern = PackedInt32Array([0, -1, 0, -1, 1, -1, 0, -1, 0, -1, 0, -1, 1, -1, 0, -1])
	variant.kick_steps = 0x0101
	variant.hat_steps = 0x4444
	var pattern_stream := variant.build()
	_check(pattern_stream != null and pattern_stream.data.size() > 0, "16 步音序器可合成")


# =====================================================================
# 3. 资源存取往返
# =====================================================================

func _test_roundtrip() -> void:
	print("== 资源存取往返 ==")
	_ensure_dir(TMP)

	var p: SfxPatch = PatchScript.new()
	p.rate = 16000
	p.group = "selftest"
	var arp_layer: SfxLayer = LayerScript.new()
	arp_layer.wave = SfxLayer.Wave.ARP
	arp_layer.arp_freqs = PackedFloat64Array([261.63, 329.63, 392.0])
	arp_layer.step = 0.07
	arp_layer.duration = 0.21
	arp_layer.gain = 0.37
	arp_layer.harsh = 0.42
	arp_layer.decay = 6.5
	arp_layer.attack = 1.0 / 800.0
	var arp_arr: Array[SfxLayer] = [arp_layer]
	p.layers = arp_arr
	var patch_path := TMP.path_join("patch.tres")
	_check(ResourceSaver.save(p, patch_path) == OK, "保存 patch.tres")
	var p2: SfxPatch = ResourceLoader.load(patch_path)
	_check(p2 != null, "重新加载 patch.tres")
	if p2 != null and not p2.layers.is_empty():
		var back_layer: SfxLayer = p2.layers[0]
		_check(p2.rate == p.rate and p2.group == "selftest", "标量字段一致")
		_check(back_layer.wave == SfxLayer.Wave.ARP, "图层类型一致")
		_check(back_layer.arp_freqs == arp_layer.arp_freqs, "频率数组一致")
		_check(is_equal_approx(back_layer.step, arp_layer.step) and is_equal_approx(back_layer.gain, arp_layer.gain), "浮点字段一致")
		_check(p2.build().data == p.build().data, "重建波形一致")
		_check(FileAccess.get_file_as_bytes(patch_path).size() < 4096, "参数文件 < 4KB")

	var layered: SfxPatch = PatchScript.new()
	var la: SfxLayer = LayerScript.new()
	la.wave = SfxLayer.Wave.FM
	la.freq_start = 660.0
	la.duration = 0.2
	var lb: SfxLayer = LayerScript.new()
	lb.wave = SfxLayer.Wave.NOISE
	lb.delay = 0.05
	lb.duration = 0.15
	var layer_arr: Array[SfxLayer] = [la, lb]
	layered.layers = layer_arr
	var layered_path := TMP.path_join("layered.tres")
	_check(ResourceSaver.save(layered, layered_path) == OK, "保存多图层 patch")
	var layered2: SfxPatch = ResourceLoader.load(layered_path)
	_check(layered2 != null and layered2.layers.size() == 2, "多图层重新加载")
	if layered2 != null:
		_check(layered2.build().data == layered.build().data, "多图层重建一致")
		_check(layered2.layers[0].wave == SfxLayer.Wave.FM, "图层波形字段一致")

	var m: SfxMusic = MusicScript.new()
	m.bpm = 101.0
	m.bars = 2
	m.rate = 16000
	m.bass_notes = PackedFloat64Array([130.81, 98.0])
	m.chords = [
		PackedFloat64Array([261.63, 329.63, 392.0]),
		PackedFloat64Array([196.0, 246.94, 293.66]),
	]
	m.arp_gain = 0.12
	var music_path := TMP.path_join("music.tres")
	_check(ResourceSaver.save(m, music_path) == OK, "保存 music.tres")
	var m2: SfxMusic = ResourceLoader.load(music_path)
	_check(m2 != null, "重新加载 music.tres")
	if m2 != null:
		_check(m2.bpm == m.bpm and m2.bars == m.bars, "音乐结构字段一致")
		_check(m2.bass_notes == m.bass_notes, "音乐低音数组一致")
		_check(m2.chords.size() == 2 and m2.chords[0] == m.chords[0], "音乐和弦数组一致")
		_check(is_equal_approx(m2.arp_gain, m.arp_gain), "音乐音量一致")
	_check(FileAccess.get_file_as_bytes(music_path).size() < 8192, "音乐参数文件 < 8KB")


# =====================================================================
# 4. 烘焙
# =====================================================================

func _test_bake() -> void:
	print("== 烘焙 ==")
	_ensure_dir(TMP)
	var stream := _make_tone_stream(22050, 880.0, 0.12, 0.4, 220.0)

	var wav_path := TMP.path_join("bake.wav")
	_check(stream.save_to_wav(wav_path) == OK, "保存 .wav")
	var wav_bytes := FileAccess.get_file_as_bytes(wav_path)
	_check(wav_bytes.size() > 44, "WAV 文件非空")
	if wav_bytes.size() >= 4:
		_check(wav_bytes.slice(0, 4).get_string_from_ascii() == "RIFF", "WAV 头正确")

	var res_path := TMP.path_join("bake.res")
	_check(ResourceSaver.save(stream, res_path) == OK, "保存 .res")
	var back: AudioStreamWAV = ResourceLoader.load(res_path)
	_check(back != null and back.data == stream.data, "重新加载 .res 数据一致")

	# 音乐循环点必须保留
	var music: SfxMusic = MusicScript.new()
	music.bars = 1
	var music_stream := music.build()
	var loop_path := TMP.path_join("music_loop.res")
	_check(ResourceSaver.save(music_stream, loop_path) == OK, "保存音乐 .res")
	var loop_back: AudioStreamWAV = ResourceLoader.load(loop_path)
	_check(loop_back != null and loop_back.loop_mode == AudioStreamWAV.LOOP_FORWARD, "循环模式保留")
	if loop_back != null:
		_check(loop_back.loop_end == int(music_stream.data.size() / 2), "循环终点保留")
		_check(loop_back.data == music_stream.data, "音乐数据一致")

	# baked 字段存的是外部引用，不应把采样塞进 .tres
	var p: SfxPatch = PatchScript.new()
	p.baked = back
	var baked_path := TMP.path_join("patch_baked.tres")
	_check(ResourceSaver.save(p, baked_path) == OK, "保存带 baked 的 patch")
	_check(FileAccess.get_file_as_bytes(baked_path).size() < 4096, "baked 为外部引用（文件 < 4KB）")
	var p2: SfxPatch = ResourceLoader.load(baked_path)
	_check(p2 != null and p2.baked != null and p2.baked.data == stream.data, "baked 重新加载一致")


# =====================================================================
# 5. Autoload
# =====================================================================

func _test_player() -> void:
	print("== Autoload Sfx ==")
	_check(Sfx != null, "Autoload Sfx 存在")
	if Sfx == null:
		return
	var names: Array = Sfx.sound_names()
	_check(names.size() > 0, "已加载 %d 条音效" % names.size())
	if names.size() > 0:
		var first: String = names[0]
		_check(Sfx.has_sound(first), "has_sound(%s)" % first)
		var stream: AudioStream = Sfx.get_stream(first)
		_check(stream is AudioStreamWAV, "get_stream(%s) 是 WAV" % first)
		Sfx.play(first)
		_check(true, "play(%s) 不报错" % first)

	Sfx.play_music()
	_check(Sfx.music_length() > 0.0, "music_length() > 0")
	var was_muted: bool = Sfx.is_muted()
	Sfx.set_muted(true)
	_check(Sfx.is_muted(), "可静音")
	Sfx.set_muted(was_muted)
	_check(Sfx.is_muted() == was_muted, "静音状态已还原")
	Sfx.set_music_paused(true)
	Sfx.set_music_paused(false)


# =====================================================================
# 小工具
# =====================================================================

func _make_tone_stream(rate: int, f0: float, duration: float, gain: float, f1 := -1.0) -> AudioStreamWAV:
	var layer: SfxLayer = LayerScript.new()
	layer.wave = SfxLayer.Wave.SINE
	layer.freq_start = f0
	layer.freq_end = f0 if f1 < 0.0 else f1
	layer.duration = duration
	layer.gain = gain
	var patch: SfxPatch = PatchScript.new()
	patch.rate = rate
	var arr: Array[SfxLayer] = [layer]
	patch.layers = arr
	return patch.build()


func _peak(stream: AudioStreamWAV) -> float:
	var peak := 0.0
	var data := stream.data
	for i in range(0, data.size(), 2):
		peak = maxf(peak, absf(float(data.decode_s16(i)) / 32767.0))
	return peak


func _ensure_dir(path: String) -> void:
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(path))


func _cleanup() -> void:
	for file in ["patch.tres", "music.tres", "bake.wav", "bake.res", "music_loop.res", "patch_baked.tres", "layered.tres", "clips.tres"]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(TMP.path_join(file)))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(TMP))
