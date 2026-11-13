@tool
class_name SfxSynth
extends RefCounted
## 程序化音频合成内核（纯函数，运行时/编辑器通用）。
##
## 背景音乐、波形采样与包络工具。单条音效的合成在 [SfxLayer] 里完成。
##
## 约定：
##   - 频率单位 Hz，时间单位秒，增益 0~1。
##   - 扫频用累加相位，避免 `sin(TAU*f*t)` 在变频时产生相位跳变（咔哒声）。
##   - 噪声固定种子，保证每次启动音色一致。


## 背景音乐：每小节"贝斯 8 分 + 琶音 16 分 + 底鼓 2 下 + 反拍踩镲"，无缝循环。
static func music(
	rate: int,
	bpm: float,
	bars: int,
	bass_notes: PackedFloat64Array,
	chords: Array,
	seed_value: int,
	bass_gain: float,
	bass_harsh: float,
	bass_decay: float,
	arp_gain: float,
	arp_harsh: float,
	arp_decay: float,
	kick_gain: float,
	hat_gain: float,
	normalize: bool = true,
	bass_wave: int = 0,
	arp_wave: int = 0,
	swing: float = 0.0,
	use_patterns: bool = false,
	arp_pattern: PackedInt32Array = PackedInt32Array(),
	bass_pattern: PackedInt32Array = PackedInt32Array(),
	kick_steps: int = 0,
	hat_steps: int = 0
) -> AudioStreamWAV:
	var beat := 60.0 / bpm
	var bar := beat * 4.0
	var total := int(round(float(rate) * bar * float(bars)))
	var buf := PackedFloat32Array()
	buf.resize(total)
	if bass_notes.is_empty() or chords.is_empty():
		return _to_wav(rate, PackedByteArray())

	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value

	for b in bars:
		var t0 := float(b) * bar
		var root: float = bass_notes[b % bass_notes.size()]
		var chord: PackedFloat64Array = _as_float64_array(chords[b % chords.size()])

		if use_patterns:
			# 步进音序器：16 步 = 1 小节
			for step in 16:
				var tone_index := arp_pattern[step] if step < arp_pattern.size() else -1
				if tone_index >= 0 and not chord.is_empty():
					var arp_freq := chord[tone_index % chord.size()] * 2.0
					var swing_offset := swing * beat * 0.125 if step % 2 == 1 else 0.0
					_music_note(buf, rate, t0 + float(step) * beat * 0.25 + swing_offset,
						beat * 0.20, arp_freq, arp_gain, arp_harsh, arp_decay, arp_wave)
				var bass_index := bass_pattern[step] if step < bass_pattern.size() else -1
				if bass_index >= 0:
					var bass_freq := root * (2.0 if bass_index == 1 else 1.0)
					_music_note(buf, rate, t0 + float(step) * beat * 0.25, beat * 0.46,
						bass_freq, bass_gain, bass_harsh, bass_decay, bass_wave)
			for step in 16:
				if kick_steps & (1 << step):
					_music_kick(buf, rate, t0 + float(step) * beat * 0.25, kick_gain)
				if hat_steps & (1 << step):
					_music_hat(buf, rate, rng, t0 + float(step) * beat * 0.25, hat_gain)
		else:
			# 默认：贝斯 8 分音符，每 4 个音升一次八度
			for i in 8:
				var freq := root * (2.0 if i % 4 == 3 else 1.0)
				_music_note(buf, rate, t0 + float(i) * beat * 0.5, beat * 0.46,
					freq, bass_gain, bass_harsh, bass_decay, bass_wave)

			# 琶音：16 分音符
			if not chord.is_empty():
				for i in 16:
					var freq := chord[i % chord.size()] * 2.0
					var swing_offset := swing * beat * 0.125 if i % 2 == 1 else 0.0
					_music_note(buf, rate, t0 + float(i) * beat * 0.25 + swing_offset, beat * 0.20,
						freq, arp_gain, arp_harsh, arp_decay, arp_wave)

			_music_kick(buf, rate, t0, kick_gain)
			_music_kick(buf, rate, t0 + beat * 2.0, kick_gain)
			for i in 4:
				_music_hat(buf, rate, rng, t0 + beat * (float(i) + 0.5), hat_gain)

	var scale := 1.0
	if normalize:
		var peak := 0.0
		for i in total:
			peak = maxf(peak, absf(buf[i]))
		if peak > 0.85:
			scale = 0.85 / peak

	var data := PackedByteArray()
	data.resize(total * 2)
	for i in total:
		data.encode_s16(i * 2, _s16(buf[i] * scale))

	var wav := _to_wav(rate, data)
	wav.loop_mode = AudioStreamWAV.LOOP_FORWARD
	wav.loop_begin = 0
	wav.loop_end = total
	return wav


static func _music_note(
	buf: PackedFloat32Array, rate: int, t0: float, dur: float,
	freq: float, amp: float, harsh: float, decay: float, wave: int = 0
) -> void:
	var first := int(t0 * float(rate))
	var count := int(dur * float(rate))
	var size := buf.size()
	var phase := 0.0
	for i in count:
		var index := first + i
		if index < 0 or index >= size:
			break
		var t := float(i) / float(rate)
		var k := float(i) / float(maxi(count, 1))
		var env := exp(-decay * k) * minf(1.0, t * 1200.0)
		phase += TAU * freq / float(rate)
		var s := sin(phase)
		if wave != 0:
			s = wave_sample(wave, phase, 0.5, harsh)
		elif harsh > 0.0:
			s = lerpf(s, signf(s), harsh)
		buf[index] += s * env * amp


static func _music_kick(buf: PackedFloat32Array, rate: int, t0: float, gain: float) -> void:
	var first := int(t0 * float(rate))
	var count := int(0.16 * float(rate))
	var size := buf.size()
	var phase := 0.0
	for i in count:
		var index := first + i
		if index < 0 or index >= size:
			break
		var k := float(i) / float(count)
		phase += TAU * lerpf(135.0, 48.0, k) / float(rate)
		buf[index] += sin(phase) * exp(-7.0 * k) * gain


static func _music_hat(
	buf: PackedFloat32Array, rate: int, rng: RandomNumberGenerator,
	t0: float, gain: float
) -> void:
	var first := int(t0 * float(rate))
	var count := int(0.05 * float(rate))
	var size := buf.size()
	var prev := 0.0
	for i in count:
		var index := first + i
		if index < 0 or index >= size:
			break
		var noise := rng.randf_range(-1.0, 1.0)
		var hp := noise - prev
		prev = noise
		var env := exp(-38.0 * float(i) / float(count))
		buf[index] += hp * env * gain


static func _as_float64_array(value: Variant) -> PackedFloat64Array:
	if value is PackedFloat64Array:
		return value
	if value is Array:
		var out := PackedFloat64Array()
		for v in value:
			out.append(float(v))
		return out
	return PackedFloat64Array()


## 单个波形的采样值（phase 用弧度，wave 传 SfxLayer.Wave 的整数）。
static func wave_sample(wave: int, phase: float, duty: float = 0.5, harsh: float = 0.0) -> float:
	var p := fposmod(phase / TAU, 1.0)
	match wave:
		1:  # TRIANGLE
			return 1.0 - 4.0 * absf(p - 0.5)
		2:  # SAW
			return 2.0 * p - 1.0
		3:  # PULSE
			return 1.0 if p < clampf(duty, 0.05, 0.95) else -1.0
		4:  # SQUARE
			return 1.0 if p < 0.5 else -1.0
		_:
			var s := sin(phase)
			if harsh > 0.0:
				s = lerpf(s, signf(s), harsh)
			return s


## ADSR 包络。t / duration / a / d / r 单位为秒，s 为 0~1。
static func adsr(t: float, duration: float, a: float, d: float, s_level: float, r: float) -> float:
	if t < 0.0:
		return 0.0
	if a > 0.0 and t < a:
		return t / a
	var sustain_start := a + maxf(d, 0.0)
	if t < sustain_start:
		var k := (t - a) / maxf(d, 0.0001)
		return lerpf(1.0, s_level, k)
	var release_start := maxf(duration - r, sustain_start)
	if t < release_start:
		return s_level
	if r <= 0.0:
		return s_level
	var k2 := (t - release_start) / r
	return s_level * maxf(0.0, 1.0 - k2)


static func _s16(sample: float) -> int:
	return int(clampf(sample, -1.0, 1.0) * 32767.0)


static func _to_wav(rate: int, data: PackedByteArray) -> AudioStreamWAV:
	var wav := AudioStreamWAV.new()
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.mix_rate = rate
	wav.stereo = false
	wav.data = data
	return wav
