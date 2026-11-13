@tool
class_name SfxMusic
extends Resource
## 背景音乐参数（保存为 .tres）。运行时 `build()` 合成一段带循环点的 AudioStreamWAV。
##
## 每小节 = 贝斯 8 分 × 8 + 琶音 16 分 × 16 + 底鼓 × 2 + 反拍踩镲 × 4
## 每小节一个根音（bass_notes）和一个和弦（chords[b]）。

enum Wave { SINE, TRIANGLE, SAW, PULSE, SQUARE }

@export_group("结构")
@export_range(40.0, 240.0, 1.0) var bpm := 132.0
@export_range(1, 16, 1) var bars := 4
@export_range(8000, 48000, 100, "suffix:Hz") var rate := 11025
@export_range(-40.0, 0.0, 0.5, "suffix:dB") var volume_db := -17.0
@export var seed_value := 20240607

@export_group("音色")
@export var bass_wave: Wave = Wave.SINE
@export var arp_wave: Wave = Wave.SINE
## 0 = 平均，0.5 = 明显摇摆（只影响琶音的奇数 16 分音符）
@export_range(0.0, 0.5, 0.01) var swing := 0.0

@export_group("节奏（步进音序器）")
## 打开后使用下面的 16 步 pattern，而不是默认的 8 分贝斯 + 16 分琶音。
@export var use_patterns := false
## 16 步：-1 = 休止，0..3 = 和弦音序号
@export var arp_pattern := PackedInt32Array([0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0])
## 16 步：-1 = 休止，0 = 根音，1 = 高八度
@export var bass_pattern := PackedInt32Array([0, -1, 0, -1, 0, -1, 1, -1, 0, -1, 0, -1, 0, -1, 1, -1])
## 16 位掩码：第 0 / 8 步底鼓
@export var kick_steps := 0x0101
## 16 位掩码：第 2 / 6 / 10 / 14 步踩镲
@export var hat_steps := 0x4444

@export_group("和声")
## 每小节低音根音（Hz）。长度不足时循环取用。
@export var bass_notes := PackedFloat64Array([110.0, 87.31, 130.81, 98.0])
## 每小节琶音音高（Hz）。内层数组长度可变（3 音 / 七和弦都行）。
@export var chords: Array[PackedFloat64Array] = [
	PackedFloat64Array([220.0, 261.63, 329.63]),
	PackedFloat64Array([174.61, 220.0, 261.63]),
	PackedFloat64Array([261.63, 329.63, 392.0]),
	PackedFloat64Array([196.0, 246.94, 293.66]),
]

@export_group("分层音量")
@export_range(0.0, 1.0, 0.01) var bass_gain := 0.26
@export_range(0.0, 1.0, 0.01) var bass_harsh := 1.0
@export_range(0.5, 30.0, 0.1) var bass_decay := 2.2
@export_range(0.0, 1.0, 0.01) var arp_gain := 0.085
@export_range(0.0, 1.0, 0.01) var arp_harsh := 0.25
@export_range(0.5, 40.0, 0.1) var arp_decay := 9.0
@export_range(0.0, 1.0, 0.01) var kick_gain := 0.45
@export_range(0.0, 1.0, 0.01) var hat_gain := 0.055
@export var normalize := true

@export_group("烘焙")
## 可选：烘焙后的音乐。留空则运行时合成（4 小节约 6ms）。
@export var baked: AudioStreamWAV


func build() -> AudioStreamWAV:
	if baked != null:
		return baked
	return SfxSynth.music(rate, bpm, bars, bass_notes, chords, seed_value,
		bass_gain, bass_harsh, bass_decay,
		arp_gain, arp_harsh, arp_decay,
		kick_gain, hat_gain, normalize,
		bass_wave, arp_wave, swing,
		use_patterns, arp_pattern, bass_pattern, kick_steps, hat_steps)


func build_live() -> AudioStreamWAV:
	var backup := baked
	baked = null
	var stream := build()
	baked = backup
	return stream
