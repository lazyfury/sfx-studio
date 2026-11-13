@tool
class_name SfxLayer
extends Resource
## 一个音效图层：振荡器 + ADSR + 滤波 + 图层延迟/增益。
##
## 一条 SfxPatch 可以叠任意多层，例如：
##   激光 = 脉冲扫频层 + 噪声层（低通下扫）
##   爆炸 = 噪声层 + 低频正弦层
##   硬币 = 两个不同音高的脉冲层（先后触发）

enum Wave {
	SINE, TRIANGLE, SAW, PULSE, SQUARE, NOISE, ARP,
	FM, PLUCK, SAMPLE, KICK, SNARE, HAT, CLAP,
}
enum FilterMode { NONE, LOWPASS, HIGHPASS, BANDPASS }
enum EnvShape { EXP, SINE }
enum ToneEnv { ADSR, EXP }

@export_group("图层")
@export var wave: Wave = Wave.SINE
@export var enabled := true
@export_range(0.0, 2.0, 0.001, "suffix:s") var delay := 0.0
@export_range(0.005, 3.0, 0.001, "suffix:s") var duration := 0.15
@export_range(0.0, 1.0, 0.001) var gain := 0.3
## Empty = one implicit clip using delay/duration. Non-empty = explicit clips.
@export var clips: Array[SfxClip] = []

@export_group("音高")
@export_range(20.0, 12000.0, 1.0, "suffix:Hz") var freq_start := 440.0
@export_range(20.0, 12000.0, 1.0, "suffix:Hz") var freq_end := 440.0
## 仅 SINE：0 = 纯正弦，1 = 方波
@export_range(0.0, 1.0, 0.01) var harsh := 0.0
@export_range(0.05, 0.95, 0.01) var duty := 0.5
@export_range(0.0, 20.0, 0.1, "suffix:Hz") var vibrato_rate := 0.0
@export_range(0.0, 12.0, 0.01, "suffix:st") var vibrato_depth := 0.0

@export_group("包络")
## ADSR = 常规线性包络；EXP = 指数衰减（适合短促的扫频音）。
@export var tone_env: ToneEnv = ToneEnv.ADSR
@export_range(0.0, 2.0, 0.0001, "suffix:s") var attack := 0.002
@export_range(0.0, 3.0, 0.001, "suffix:s") var decay := 0.06
@export_range(0.0, 1.0, 0.001) var sustain := 0.0
@export_range(0.0, 3.0, 0.001, "suffix:s") var release := 0.03
## 仅 EXP 包络：指数衰减速度，越大衰减越快。
@export_range(0.5, 30.0, 0.1) var exp_decay := 4.0

@export_group("滤波")
@export var filter_mode: FilterMode = FilterMode.NONE
@export_range(20.0, 18000.0, 1.0, "suffix:Hz") var cutoff_start := 12000.0
@export_range(20.0, 18000.0, 1.0, "suffix:Hz") var cutoff_end := 12000.0
@export_range(0.5, 5.0, 0.01) var resonance := 0.707

@export_group("噪声 / Arp")
@export var arp_freqs := PackedFloat64Array([523.25, 659.25, 783.99])
@export_range(0.01, 0.5, 0.001, "suffix:s") var step := 0.06
@export_range(0.01, 1.0, 0.01) var lowpass_from := 0.35
@export_range(0.01, 1.0, 0.01) var lowpass_to := 0.35
@export_range(0.0, 1.0, 0.01) var noise_gain := 0.85
@export_range(0.0, 1.0, 0.01) var sweep_gain := 0.45
@export var env_shape: EnvShape = EnvShape.EXP
@export var seed_value := 20240607

@export_group("FM / 拨弦")
@export_range(0.25, 12.0, 0.01) var fm_ratio := 2.0
@export_range(0.0, 20.0, 0.1) var fm_index := 2.0
@export_range(0.5, 30.0, 0.1) var fm_decay := 4.0
@export_range(0.9, 0.9999, 0.0001) var pluck_damping := 0.996
@export_range(0.0, 1.0, 0.01) var pluck_brightness := 0.5

@export_group("采样")
@export var sample: AudioStreamWAV
@export_range(-24.0, 24.0, 0.1, "suffix:st") var sample_pitch := 0.0
@export var sample_loop := false

@export_group("鼓")
@export_range(0.25, 2.0, 0.01) var drum_tone := 1.0
@export_range(0.0, 2.0, 0.01) var drum_snap := 1.0


static func wave_name(value: Wave) -> String:
	match value:
		Wave.SINE: return "正弦 Sine"
		Wave.TRIANGLE: return "三角 Triangle"
		Wave.SAW: return "锯齿 Saw"
		Wave.PULSE: return "脉冲 Pulse"
		Wave.SQUARE: return "方波 Square"
		Wave.NOISE: return "噪声 Noise"
		Wave.ARP: return "琶音 Arp"
		Wave.FM: return "FM 金属"
		Wave.PLUCK: return "拨弦 Pluck"
		Wave.SAMPLE: return "采样 Sample"
		Wave.KICK: return "底鼓 Kick"
		Wave.SNARE: return "军鼓 Snare"
		Wave.HAT: return "踩镲 Hat"
		Wave.CLAP: return "拍手 Clap"
	return "未知"


func end_time() -> float:
	if clips.is_empty():
		return delay + duration
	var end := 0.0
	for clip in clips:
		if clip != null:
			end = maxf(end, clip.end_time())
	return end


func ensure_clips() -> void:
	if clips.is_empty():
		var clip := SfxClip.new()
		clip.start = delay
		clip.duration = duration
		clip.gain = 1.0
		clips.append(clip)


## Transposed copy of the instrument (clips are cleared; pitch is per clip).
func with_pitch(semitones: float) -> SfxLayer:
	var copy: SfxLayer = duplicate(true)
	copy.clips = []
	if absf(semitones) < 0.001:
		return copy
	var factor := pow(2.0, semitones / 12.0)
	copy.freq_start = maxf(copy.freq_start * factor, 20.0)
	copy.freq_end = maxf(copy.freq_end * factor, 20.0)
	for i in copy.arp_freqs.size():
		copy.arp_freqs[i] = maxf(copy.arp_freqs[i] * factor, 20.0)
	copy.drum_tone = clampf(copy.drum_tone * factor, 0.25, 4.0)
	if copy.wave == Wave.SAMPLE:
		copy.sample_pitch = clampf(copy.sample_pitch + semitones, -24.0, 24.0)
	return copy


## 生成本层波形（float32 单声道，不含 delay）。
func build(rate: int) -> PackedFloat32Array:
	if not enabled or duration <= 0.0 or rate <= 0:
		return PackedFloat32Array()
	match wave:
		Wave.NOISE:
			return _build_noise(rate)
		Wave.ARP:
			return _build_arp(rate)
		Wave.FM:
			return _build_fm(rate)
		Wave.PLUCK:
			return _build_pluck(rate)
		Wave.SAMPLE:
			return _build_sample(rate)
		Wave.KICK:
			return _build_kick(rate)
		Wave.SNARE:
			return _build_snare(rate)
		Wave.HAT:
			return _build_hat(rate)
		Wave.CLAP:
			return _build_clap(rate)
		_:
			return _build_tone(rate)


## 给 Inspector 用：按波形隐藏无关参数。
func _validate_property(property: Dictionary) -> void:
	var name: String = property.name
	var hide := false
	if wave in [Wave.SINE, Wave.TRIANGLE, Wave.SAW, Wave.PULSE, Wave.SQUARE]:
		hide = name in ["lowpass_from", "lowpass_to", "noise_gain", "sweep_gain",
			"env_shape", "tone_env", "exp_decay", "seed_value", "arp_freqs", "step",
			"fm_ratio", "fm_index", "fm_decay", "pluck_damping", "pluck_brightness",
			"sample", "sample_pitch", "sample_loop", "drum_tone", "drum_snap"]
		if wave != Wave.SINE:
			hide = hide or name == "harsh"
		if wave != Wave.PULSE:
			hide = hide or name == "duty"
	elif wave == Wave.NOISE:
		hide = name in ["harsh", "duty", "vibrato_rate", "vibrato_depth", "attack",
			"decay", "sustain", "release", "tone_env", "exp_decay",
			"filter_mode", "cutoff_start", "cutoff_end",
			"resonance", "arp_freqs", "step", "fm_ratio", "fm_index", "fm_decay",
			"pluck_damping", "pluck_brightness", "sample", "sample_pitch", "sample_loop",
			"drum_tone", "drum_snap"]
	elif wave == Wave.ARP:
		hide = name in ["harsh", "duty", "vibrato_rate", "vibrato_depth", "sustain",
			"tone_env", "exp_decay", "filter_mode", "cutoff_start", "cutoff_end", "resonance",
			"lowpass_from", "lowpass_to", "noise_gain", "sweep_gain", "env_shape",
			"fm_ratio", "fm_index", "fm_decay", "pluck_damping", "pluck_brightness",
			"sample", "sample_pitch", "sample_loop", "drum_tone", "drum_snap"]
	elif wave == Wave.FM:
		hide = name in ["harsh", "duty", "lowpass_from", "lowpass_to", "noise_gain",
			"sweep_gain", "env_shape", "tone_env", "exp_decay", "seed_value", "arp_freqs", "step",
			"pluck_damping", "pluck_brightness", "sample", "sample_pitch", "sample_loop",
			"drum_tone", "drum_snap"]
	elif wave == Wave.PLUCK:
		hide = name in ["harsh", "duty", "lowpass_from", "lowpass_to", "noise_gain",
			"sweep_gain", "env_shape", "tone_env", "exp_decay", "seed_value", "arp_freqs", "step",
			"fm_ratio", "fm_index", "fm_decay", "sample", "sample_pitch", "sample_loop",
			"drum_tone", "drum_snap"]
	elif wave == Wave.SAMPLE:
		hide = name in ["harsh", "duty", "freq_start", "freq_end", "vibrato_rate",
			"vibrato_depth", "lowpass_from", "lowpass_to", "noise_gain", "sweep_gain",
			"env_shape", "tone_env", "exp_decay", "seed_value", "arp_freqs", "step", "fm_ratio", "fm_index",
			"fm_decay", "pluck_damping", "pluck_brightness", "drum_tone", "drum_snap",
			"filter_mode", "cutoff_start", "cutoff_end", "resonance"]
	elif wave in [Wave.KICK, Wave.SNARE, Wave.HAT, Wave.CLAP]:
		hide = name in ["harsh", "duty", "freq_start", "freq_end", "vibrato_rate",
			"vibrato_depth", "attack", "decay", "sustain", "release",
			"tone_env", "exp_decay", "filter_mode", "cutoff_start", "cutoff_end", "resonance",
			"lowpass_from", "lowpass_to", "noise_gain", "sweep_gain", "env_shape",
			"seed_value", "arp_freqs", "step", "fm_ratio", "fm_index", "fm_decay",
			"pluck_damping", "pluck_brightness", "sample", "sample_pitch", "sample_loop"]
	if name == "exp_decay" and tone_env == ToneEnv.ADSR:
		hide = true
	if hide:
		property.usage &= ~PROPERTY_USAGE_EDITOR


# =====================================================================
# 波形
# =====================================================================

func _build_tone(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var phase := 0.0
	var low := 0.0
	var band := 0.0
	for i in count:
		var t := float(i) / float(rate)
		var k := t / duration
		var freq := lerpf(freq_start, freq_end, k)
		if vibrato_depth > 0.0 and vibrato_rate > 0.0:
			freq *= pow(2.0, sin(TAU * vibrato_rate * t) * vibrato_depth / 12.0)
		phase += TAU * freq / float(rate)
		var s := SfxSynth.wave_sample(wave, phase, duty, harsh)
		var vars := _filter_state(s, low, band, k, rate)
		low = vars.x
		band = vars.y
		var env := SfxSynth.adsr(t, duration, attack, decay, sustain, release)
		if tone_env == ToneEnv.EXP:
			env = exp(-exp_decay * k) * minf(1.0, t / maxf(attack, 0.0001))
		out[i] = vars.z * env * gain
	return out


func _build_noise(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var low := 0.0
	for i in count:
		var k := float(i) / float(maxi(count, 1))
		var t := float(i) / float(rate)
		var noise := rng.randf_range(-1.0, 1.0)
		low = lerpf(low, noise, lerpf(lowpass_from, lowpass_to, k))
		var sweep := sin(TAU * lerpf(freq_start, freq_end, k) * t)
		var env := sin(PI * k) if env_shape == EnvShape.SINE else exp(-decay * k)
		out[i] = (low * noise_gain + sweep * sweep_gain) * env * gain
	return out


func _build_arp(rate: int) -> PackedFloat32Array:
	if arp_freqs.is_empty():
		return PackedFloat32Array()
	var count := int(rate * step * float(arp_freqs.size()))
	var out := PackedFloat32Array()
	out.resize(count)
	var fade := maxf(attack, 0.0001)
	for i in count:
		var t := float(i) / float(rate)
		var index := mini(int(t / step), arp_freqs.size() - 1)
		var local := t - float(index) * step
		var freq := arp_freqs[index]
		var s := sin(TAU * freq * t)
		if harsh > 0.0:
			s = lerpf(s, signf(s), harsh)
		var env := exp(-decay * local / step) * minf(1.0, local / fade)
		out[i] = s * env * gain
	return out


func _build_fm(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var carrier := 0.0
	var modulator := 0.0
	var low := 0.0
	var band := 0.0
	for i in count:
		var t := float(i) / float(rate)
		var k := float(i) / float(maxi(count, 1))
		var freq := lerpf(freq_start, freq_end, k)
		if vibrato_depth > 0.0 and vibrato_rate > 0.0:
			freq *= pow(2.0, sin(TAU * vibrato_rate * t) * vibrato_depth / 12.0)
		modulator += TAU * freq * fm_ratio / float(rate)
		carrier += TAU * freq / float(rate)
		var index := fm_index * exp(-fm_decay * k)
		var s := sin(carrier + sin(modulator) * index)
		var vars := _filter_state(s, low, band, k, rate)
		low = vars.x
		band = vars.y
		out[i] = vars.z * SfxSynth.adsr(t, duration, attack, decay, sustain, release) * gain
	return out


func _build_pluck(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var n := maxi(int(float(rate) / maxf(freq_start, 20.0)), 2)
	var line := PackedFloat32Array()
	line.resize(n)
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var previous := 0.0
	for i in n:
		var noise := rng.randf_range(-1.0, 1.0)
		previous = lerpf(previous, noise, clampf(pluck_brightness, 0.02, 1.0))
		line[i] = previous
	var pos := 0
	for i in count:
		var t := float(i) / float(rate)
		var current := line[pos]
		var next := line[(pos + 1) % n]
		line[pos] = (current + next) * 0.5 * pluck_damping
		pos = (pos + 1) % n
		out[i] = current * SfxSynth.adsr(t, duration, attack, decay, sustain, release) * gain
	return out


func _build_sample(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	if sample == null or sample.data.is_empty():
		return out
	var channels := 2 if sample.stereo else 1
	var frame_bytes := 2 * channels
	var frames := int(sample.data.size() / frame_bytes)
	if frames <= 0:
		return out
	var ratio := (float(sample.mix_rate) / float(rate)) * pow(2.0, sample_pitch / 12.0)
	var pos := 0.0
	for i in count:
		if not sample_loop and pos >= float(frames - 1):
			break
		var index := int(pos)
		var frac := pos - float(index)
		var s := _sample_at(index, channels, frame_bytes, frames)
		if index + 1 < frames:
			s = lerpf(s, _sample_at(index + 1, channels, frame_bytes, frames), frac)
		var t := float(i) / float(rate)
		out[i] = s * SfxSynth.adsr(t, duration, attack, decay, sustain, release) * gain
		pos += ratio
		if sample_loop:
			pos = fposmod(pos, float(frames))
	return out


func _sample_at(index: int, channels: int, frame_bytes: int, frames: int) -> float:
	var at := (index % frames) * frame_bytes
	var s := float(sample.data.decode_s16(at)) / 32767.0
	if channels == 2:
		s = (s + float(sample.data.decode_s16(at + 2)) / 32767.0) * 0.5
	return s


# =====================================================================
# 鼓
# =====================================================================

func _build_kick(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var phase := 0.0
	var start := 135.0 * drum_tone
	var end := 48.0 * drum_tone
	for i in count:
		var k := float(i) / float(maxi(count, 1))
		phase += TAU * lerpf(start, end, k) / float(rate)
		out[i] = sin(phase) * exp(-7.0 * k) * gain
	return out


func _build_snare(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var previous := 0.0
	for i in count:
		var k := float(i) / float(maxi(count, 1))
		var t := float(i) / float(rate)
		var noise := rng.randf_range(-1.0, 1.0)
		var high := noise - previous
		previous = noise
		var body := sin(TAU * 185.0 * drum_tone * t) * 0.45
		out[i] = (high * 0.8 * drum_snap + body) * exp(-11.0 * k) * gain
	return out


func _build_hat(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var previous := 0.0
	for i in count:
		var k := float(i) / float(maxi(count, 1))
		var noise := rng.randf_range(-1.0, 1.0)
		var high := noise - previous
		previous = noise
		out[i] = high * exp(-38.0 * k) * gain * (0.4 + drum_snap * 0.6)
	return out


func _build_clap(rate: int) -> PackedFloat32Array:
	var count := int(rate * duration)
	var out := PackedFloat32Array()
	out.resize(count)
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var previous := 0.0
	for i in count:
		var k := float(i) / float(maxi(count, 1))
		var noise := rng.randf_range(-1.0, 1.0)
		var high := noise - previous
		previous = noise
		var env := 0.0
		for burst in 3:
			var start := float(burst) * 0.11
			if k >= start:
				env = maxf(env, exp(-30.0 * (k - start)))
		out[i] = high * env * gain * (0.5 + drum_snap * 0.5)
	return out


# =====================================================================
# 滤波
# =====================================================================

## 返回 (low, band, out)。用 Vector3 避免数组分配。
func _filter_state(s: float, low: float, band: float, k: float, rate: int) -> Vector3:
	var cutoff := lerpf(cutoff_start, cutoff_end, k)
	var f := clampf(2.0 * sin(PI * clampf(cutoff, 20.0, float(rate) * 0.45) / float(rate)), 0.0, 0.9)
	var q := 1.0 / maxf(resonance, 0.5)
	low += f * band
	var high := s - low - q * band
	band += f * high
	var out := s
	match filter_mode:
		FilterMode.LOWPASS:
			out = low
		FilterMode.HIGHPASS:
			out = high
		FilterMode.BANDPASS:
			out = band
	return Vector3(low, band, out)

