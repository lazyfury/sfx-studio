@tool
class_name SfxPatch
extends Resource
## 一条程序化音效的参数（保存为 .tres，运行时按需合成）。
##
## 只有参数、没有采样数据，所以文件很小、可以进 git、可以手改。
## 一条音效由一个或多个 [SfxLayer] 叠加而成；需要"零合成开销"时，
## 可以用 dock 里的「烘焙…」把它烘成 AudioStreamWAV（.res/.wav），
## 填到 `baked` 字段后运行时会直接播放烘焙结果。

@export_group("图层")
## 叠加的图层。每个图层可以有多个 clip，分别控制起点 / 时长 / 增益 / 变调 / 自动化。
@export var layers: Array[SfxLayer] = []
## 多图层叠加后是否归一化到 0.95，避免削波。
@export var normalize_layers := true

@export_group("音效库")
## 音效库里的分组名；留空表示「未分组」。
@export var group := ""

@export_group("输出")
@export_range(8000, 48000, 100, "suffix:Hz") var rate := 22050
## 可选：烘焙后的波形。留空表示运行时实时合成。
@export var baked: AudioStreamWAV


## 是否至少有一层可用。
func uses_layers() -> bool:
	return not layers.is_empty()


## 按参数生成波形。baked 有值时直接返回它。
func build() -> AudioStreamWAV:
	if baked != null:
		return baked
	return _build_layers()


## 所有图层里最晚结束的时间（秒）。
func end_time() -> float:
	var end := 0.0
	for layer in layers:
		if layer != null and layer.enabled:
			end = maxf(end, layer.end_time())
	return end


func _build_layers() -> AudioStreamWAV:
	var total := 0
	for layer in layers:
		if layer == null or not layer.enabled:
			continue
		total = maxi(total, int(layer.end_time() * float(rate)) + 1)
	if total <= 0:
		return SfxSynth._to_wav(rate, PackedByteArray())

	var buf := PackedFloat32Array()
	buf.resize(total)
	for layer in layers:
		if layer == null or not layer.enabled:
			continue
		if layer.clips.is_empty():
			var part := layer.build(rate)
			var offset := int(layer.delay * float(rate))
			for i in part.size():
				var index := offset + i
				if index >= total:
					break
				buf[index] += part[i]
			continue

		for clip in layer.clips:
			if clip == null or clip.gain <= 0.0:
				continue
			var source := layer
			if absf(clip.pitch) > 0.001:
				source = layer.with_pitch(clip.pitch)
			var part := source.build(rate)
			var count := mini(int(clip.duration * float(rate)), part.size())
			var temp := PackedFloat32Array()
			temp.resize(count)
			for i in count:
				var t := float(i) / float(rate)
				temp[i] = part[i] * clip.gain * clip.gain_at(t)
			if not clip.cutoff_points.is_empty():
				temp = _apply_cutoff(temp, clip, layer.resonance)
			var offset := int(clip.start * float(rate))
			for i in count:
				var index := offset + i
				if index >= total:
					break
				buf[index] += temp[i]

	var scale := 1.0
	if normalize_layers:
		var peak := 0.0
		for i in total:
			peak = maxf(peak, absf(buf[i]))
		if peak > 0.95:
			scale = 0.95 / peak

	var data := PackedByteArray()
	data.resize(total * 2)
	for i in total:
		data.encode_s16(i * 2, int(clampf(buf[i] * scale, -1.0, 1.0) * 32767.0))
	return SfxSynth._to_wav(rate, data)


## State-variable low-pass used by CUTOFF automation.
func _apply_cutoff(samples: PackedFloat32Array, clip: SfxClip, resonance: float) -> PackedFloat32Array:
	var low := 0.0
	var band := 0.0
	var q := 1.0 / maxf(resonance, 0.5)
	for i in samples.size():
		var t := float(i) / float(rate)
		var cutoff := clip.cutoff_at(t)
		var f := clampf(2.0 * sin(PI * clampf(cutoff, 20.0, float(rate) * 0.45) / float(rate)), 0.0, 0.9)
		low += f * band
		var high := samples[i] - low - q * band
		band += f * high
		samples[i] = low
	return samples


## 只返回参数合成结果，忽略 baked（编辑器波形预览用）。
func build_live() -> AudioStreamWAV:
	var backup := baked
	baked = null
	var stream := build()
	baked = backup
	return stream
