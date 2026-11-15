//! Deterministic synthesis. Mirrors the Godot `sfx_synth.gd` / `sfx_layer.gd`
//! formulas so a project sounds the same in the app and in-game.

use crate::model::{EnvShape, FilterMode, Layer, Music, MusicWave, Patch, ToneEnv, Wave};

const TAU: f32 = std::f32::consts::TAU;

/// Preview/export render rate. Audio output resamples from this rate.
pub const PREVIEW_RATE: u32 = 48_000;

/// Bucket count for the cached waveform envelope.
pub const ENVELOPE_BINS: usize = 2048;

/// Per-track render switches. `LEGACY` reproduces DSP v2 exactly (the Godot
/// golden); `HD` adds clip fades and the band-limited / v3 voice path.
#[derive(Clone, Copy, Debug)]
pub struct TrackOptions {
    pub clip_adsr: bool,
    pub clip_fade: bool,
    pub hq: bool,
}

impl TrackOptions {
    pub const LEGACY: Self = Self {
        clip_adsr: true,
        clip_fade: false,
        hq: false,
    };
    pub const HD: Self = Self {
        clip_adsr: true,
        clip_fade: true,
        hq: true,
    };
}

pub fn render_patch(patch: &Patch, rate: u32) -> Vec<f32> {
    let rate = rate.clamp(8_000, 192_000);
    let mut total = 0usize;
    for layer in &patch.layers {
        if layer.enabled {
            total = total.max(((layer.end() * rate as f32) as usize) + 1);
        }
    }
    if total == 0 {
        return Vec::new();
    }
    // Defensive cap: a hand-edited / corrupt project must not allocate for
    // minutes of audio on the UI thread.
    let total = total.min((rate as f32 * 30.0) as usize);
    let options = TrackOptions {
        clip_adsr: patch.dsp_version >= 2,
        ..TrackOptions::LEGACY
    };
    let mut buf = vec![0.0f32; total];
    for layer in &patch.layers {
        if !layer.enabled {
            continue;
        }
        for (slot, sample) in buf.iter_mut().zip(render_layer_track(layer, rate, options)) {
            *slot += sample;
        }
    }
    if patch.normalize {
        let peak = peak(&buf);
        if peak > 0.95 {
            let scale = 0.95 / peak;
            for sample in &mut buf {
                *sample *= scale;
            }
        }
    }
    buf
}

/// One layer rendered onto its own mono timeline (delay + clip placement,
/// including the v2 clip-length ADSR). Shared by the legacy mono renderer and
/// the v3 stereo/master chain so both place audio identically. `clip_fade`
/// is only enabled for v3; v2 must stay sample-identical to Godot.
pub fn render_layer_track(layer: &Layer, rate: u32, options: TrackOptions) -> Vec<f32> {
    let rate = rate.clamp(8_000, 192_000);
    if !layer.enabled {
        return Vec::new();
    }
    let frames = ((layer.end() * rate as f32) as usize) + 1;
    let mut out = vec![0.0f32; frames];
    if layer.clips.is_empty() {
        let source = if options.hq {
            render_layer_hq(layer, rate)
        } else {
            render_layer(layer, rate)
        };
        let offset = (layer.delay * rate as f32) as usize;
        for (index, sample) in source.iter().enumerate() {
            if let Some(slot) = out.get_mut(offset + index) {
                *slot += *sample;
            }
        }
        return out;
    }
    for clip in &layer.clips {
        if clip.gain <= 0.0 {
            continue;
        }
        let mut source_layer = if clip.pitch.abs() > 0.001 {
            layer.with_pitch(clip.pitch)
        } else {
            layer.clone()
        };
        // Godot flattens clips to layers with the clip duration, so dsp v2
        // follows the clip; v1 keeps the old layer-length ADSR.
        source_layer.duration = if options.clip_adsr {
            clip.duration.min(layer.duration)
        } else {
            layer.duration
        };
        let source = if options.hq {
            render_layer_hq(&source_layer, rate)
        } else {
            render_layer(&source_layer, rate)
        };
        let start = (clip.start * rate as f32) as usize;
        let count = ((clip.duration * rate as f32) as usize).min(source.len());
        let mut temp = vec![0.0f32; count];
        for (index, slot) in temp.iter_mut().enumerate() {
            let env = clip.gain_at(index as f32 / rate as f32);
            *slot = source[index] * clip.gain * env;
        }
        if clip.has_cutoff_automation() {
            apply_cutoff(&mut temp, clip, layer.resonance, rate);
        }
        if options.clip_fade {
            fade_edges(&mut temp, rate, 2.0, 4.0);
        }
        for (index, sample) in temp.iter().enumerate() {
            if let Some(slot) = out.get_mut(start + index) {
                *slot += *sample;
            }
        }
    }
    out
}

pub fn render_layer(layer: &Layer, rate: u32) -> Vec<f32> {
    let rate = rate.clamp(8_000, 192_000);
    if !layer.enabled
        || !layer.duration.is_finite()
        || layer.duration <= 0.0
        || layer.duration > 30.0
    {
        return Vec::new();
    }
    match layer.wave {
        Wave::Noise => render_noise(layer, rate),
        Wave::Arp => render_arp(layer, rate),
        Wave::Fm => render_fm(layer, rate),
        Wave::Pluck => render_pluck(layer, rate),
        Wave::Sample => render_sample(layer, rate),
        Wave::Kick => render_kick(layer, rate),
        Wave::Snare => render_snare(layer, rate),
        Wave::Hat => render_hat(layer, rate),
        Wave::Clap => render_clap(layer, rate),
        _ => render_tone(layer, rate),
    }
}

/// DSP v3 voice path: band-limited oscillators (2× oversampled + PolyBLEP),
/// rate-normalized noise filtering and the rebuilt drum voices. DSP v2 keeps
/// using [`render_layer`] so the Godot golden never moves.
pub fn render_layer_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let rate = rate.clamp(8_000, 96_000);
    if !layer.enabled
        || !layer.duration.is_finite()
        || layer.duration <= 0.0
        || layer.duration > 30.0
    {
        return Vec::new();
    }
    match layer.wave {
        wave if wave.is_tone() => decimate(&render_tone_at(layer, rate * 2, true)),
        Wave::Fm => decimate(&render_fm(layer, rate * 2)),
        Wave::Arp => decimate(&render_arp(layer, rate * 2)),
        Wave::Pluck => render_pluck_hq(layer, rate),
        Wave::Noise => render_noise_hq(layer, rate),
        Wave::Sample => render_sample_hq(layer, rate),
        Wave::Kick => render_kick_hq(layer, rate),
        Wave::Snare => render_snare_hq(layer, rate),
        Wave::Hat => render_hat_hq(layer, rate),
        Wave::Clap => render_clap_hq(layer, rate),
        _ => Vec::new(),
    }
}

/// Halfband taps for 2× → 1× decimation. Non-zero taps sit at odd offsets
/// from the centre (offset 1 … 15), so 16 multiplies per output frame.
const HALFBAND: [f32; 8] = [
    0.313_737_5,
    -0.093_090_5,
    0.043_989,
    -0.021_591_9,
    0.009_804_1,
    -0.003_772_5,
    0.001_064_5,
    -0.000_125_9,
];
const HALFBAND_CENTER: f32 = 0.499_971_5;

fn decimate(input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len() / 2);
    for m in 0..input.len() / 2 {
        let center = m * 2;
        let mut acc = input[center] * HALFBAND_CENTER;
        for (k, tap) in HALFBAND.iter().enumerate() {
            let offset = k * 2 + 1;
            let left = if center >= offset {
                input[center - offset]
            } else {
                0.0
            };
            let right = if center + offset < input.len() {
                input[center + offset]
            } else {
                0.0
            };
            acc += (left + right) * tap;
        }
        out.push(acc);
    }
    out
}

/// PolyBLEP residual for a unit step at `t` (0..1) with phase increment `dt`.
fn poly_blep(t: f32, dt: f32) -> f32 {
    let dt = dt.max(1e-6);
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// Band-limited pulse: one rising edge at 0 and one falling edge at `duty`.
fn pulse_blep(t: f32, duty: f32, dt: f32) -> f32 {
    let duty = duty.clamp(0.05, 0.95);
    let naive = if t < duty { 1.0 } else { -1.0 };
    naive + poly_blep(t, dt) - poly_blep((t - duty).rem_euclid(1.0), dt)
}

fn hq_wave(wave: Wave, phase: f32, dt: f32, duty: f32, harsh: f32, triangle: &mut f32) -> f32 {
    let p = (phase / TAU).rem_euclid(1.0);
    match wave {
        // Triangle as the integral of a band-limited square: no slope
        // discontinuities, so the polyBLEP edges are all we need.
        Wave::Triangle => {
            *triangle += 4.0 * dt * pulse_blep(p, 0.5, dt);
            triangle.clamp(-1.2, 1.2)
        }
        Wave::Saw => 2.0 * p - 1.0 - poly_blep(p, dt),
        Wave::Pulse => pulse_blep(p, duty, dt),
        Wave::Square => pulse_blep(p, 0.5, dt),
        _ => {
            let sample = phase.sin();
            if harsh > 0.0 {
                lerp(sample, sample.signum(), harsh)
            } else {
                sample
            }
        }
    }
}

pub fn render_music(music: &Music, rate: u32) -> Vec<f32> {
    let rate = rate.clamp(8_000, 192_000);
    // Match the Godot `.tres` behaviour: no notes/chords means no stream.
    if music.bass_notes.is_empty() || music.chords.is_empty() {
        return Vec::new();
    }
    let beat = 60.0 / music.bpm.max(20.0);
    let bar = beat * 4.0;
    let total = ((rate as f32 * bar * music.bars as f32).round() as usize)
        .min((rate as f32 * 300.0) as usize);
    let mut buf = vec![0.0f32; total];
    let mut rng = Rng::new(music.seed as u64);

    for b in 0..music.bars as usize {
        let t0 = b as f32 * bar;
        let root = music
            .bass_notes
            .get(b % music.bass_notes.len().max(1))
            .copied()
            .unwrap_or(110.0);
        let chord = music
            .chords
            .get(b % music.chords.len().max(1))
            .cloned()
            .unwrap_or_default();

        if music.use_patterns {
            for step in 0..16usize {
                let tone_index = music.arp_pattern.get(step).copied().unwrap_or(-1);
                if tone_index >= 0 && !chord.is_empty() {
                    let tone = chord[tone_index as usize % chord.len()] * 2.0;
                    let swing = if step % 2 == 1 {
                        music.swing * beat * 0.125
                    } else {
                        0.0
                    };
                    note(
                        &mut buf,
                        rate,
                        t0 + step as f32 * beat * 0.25 + swing,
                        beat * 0.20,
                        tone,
                        music.arp_gain,
                        music.arp_harsh,
                        music.arp_decay,
                        music.arp_wave,
                    );
                }
                let bass_index = music.bass_pattern.get(step).copied().unwrap_or(-1);
                if bass_index >= 0 {
                    let freq = root * if bass_index == 1 { 2.0 } else { 1.0 };
                    note(
                        &mut buf,
                        rate,
                        t0 + step as f32 * beat * 0.25,
                        beat * 0.46,
                        freq,
                        music.bass_gain,
                        music.bass_harsh,
                        music.bass_decay,
                        music.bass_wave,
                    );
                }
            }
            for step in 0..16usize {
                if music.kick_steps & (1 << step) != 0 {
                    kick(
                        &mut buf,
                        rate,
                        t0 + step as f32 * beat * 0.25,
                        music.kick_gain,
                    );
                }
                if music.hat_steps & (1 << step) != 0 {
                    hat(
                        &mut buf,
                        rate,
                        &mut rng,
                        t0 + step as f32 * beat * 0.25,
                        music.hat_gain,
                    );
                }
            }
        } else {
            for i in 0..8usize {
                let freq = root * if i % 4 == 3 { 2.0 } else { 1.0 };
                note(
                    &mut buf,
                    rate,
                    t0 + i as f32 * beat * 0.5,
                    beat * 0.46,
                    freq,
                    music.bass_gain,
                    music.bass_harsh,
                    music.bass_decay,
                    music.bass_wave,
                );
            }
            if !chord.is_empty() {
                for i in 0..16usize {
                    let freq = chord[i % chord.len()] * 2.0;
                    let swing = if i % 2 == 1 {
                        music.swing * beat * 0.125
                    } else {
                        0.0
                    };
                    note(
                        &mut buf,
                        rate,
                        t0 + i as f32 * beat * 0.25 + swing,
                        beat * 0.20,
                        freq,
                        music.arp_gain,
                        music.arp_harsh,
                        music.arp_decay,
                        music.arp_wave,
                    );
                }
            }
            kick(&mut buf, rate, t0, music.kick_gain);
            kick(&mut buf, rate, t0 + beat * 2.0, music.kick_gain);
            for i in 0..4usize {
                hat(
                    &mut buf,
                    rate,
                    &mut rng,
                    t0 + beat * (i as f32 + 0.5),
                    music.hat_gain,
                );
            }
        }
    }

    if music.normalize {
        let peak = peak(&buf);
        if peak > 0.85 {
            let scale = 0.85 / peak;
            for sample in &mut buf {
                *sample *= scale;
            }
        }
    }
    buf
}

fn apply_cutoff(samples: &mut [f32], clip: &crate::model::Clip, resonance: f32, rate: u32) {
    let mut low = 0.0f32;
    let mut band = 0.0f32;
    let q = 1.0 / resonance.max(0.5);
    for (index, sample) in samples.iter_mut().enumerate() {
        let t = index as f32 / rate as f32;
        let cutoff = clip.cutoff_at(t);
        let f = (2.0
            * (std::f32::consts::PI * cutoff.clamp(20.0, rate as f32 * 0.45) / rate as f32).sin())
        .clamp(0.0, 0.9);
        low += f * band;
        let high = *sample - low - q * band;
        band += f * high;
        *sample = low;
    }
}

fn render_tone(layer: &Layer, rate: u32) -> Vec<f32> {
    render_tone_at(layer, rate, false)
}

fn render_tone_at(layer: &Layer, rate: u32, hq: bool) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut phase = 0.0f32;
    let mut triangle = -1.0f32;
    let (mut low, mut band) = (0.0f32, 0.0f32);
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let k = i as f32 / count.max(1) as f32;
        let mut freq = lerp(layer.freq_start, layer.freq_end, k);
        if layer.vibrato_rate > 0.0 && layer.vibrato_depth > 0.0 {
            freq *= 2f32.powf((TAU * layer.vibrato_rate * t).sin() * layer.vibrato_depth / 12.0);
        }
        let increment = freq / rate as f32;
        phase += TAU * increment;
        let raw = if layer.wave == Wave::Curve {
            cycle_value(&layer.curve, phase / TAU)
        } else if hq {
            hq_wave(
                layer.wave,
                phase,
                increment,
                layer.duty,
                layer.harsh,
                &mut triangle,
            )
        } else {
            wave_sample(layer.wave, phase, layer.duty, layer.harsh)
        };
        let (nl, nb, filtered) = filter(layer, raw, low, band, k, rate);
        low = nl;
        band = nb;
        let env = if layer.wave == Wave::Curve && layer.amp_curve.len() >= 2 {
            amp_curve_value(&layer.amp_curve, k)
        } else {
            match layer.tone_env {
                ToneEnv::Exp => {
                    (-layer.exp_decay * k).exp() * (t / layer.attack.max(0.0001)).min(1.0)
                }
                ToneEnv::Adsr => adsr(
                    t,
                    layer.duration,
                    layer.attack,
                    layer.decay,
                    layer.sustain,
                    layer.release,
                ),
            }
        };
        *slot = filtered * env * layer.gain;
    }
    out
}

fn render_noise(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut rng = Rng::new(layer.seed as u64);
    let mut low = 0.0f32;
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let t = i as f32 / rate as f32;
        let noise = rng.next_f32();
        low = lerp(low, noise, lerp(layer.lowpass_from, layer.lowpass_to, k));
        let sweep = (TAU * lerp(layer.freq_start, layer.freq_end, k) * t).sin();
        let env = if layer.env_shape == EnvShape::Sine {
            (std::f32::consts::PI * k).sin()
        } else {
            (-layer.decay * k).exp()
        };
        *slot = (low * layer.noise_gain + sweep * layer.sweep_gain) * env * layer.gain;
    }
    out
}

fn render_arp(layer: &Layer, rate: u32) -> Vec<f32> {
    if layer.arp_freqs.is_empty() {
        return Vec::new();
    }
    let count = (rate as f32 * layer.step * layer.arp_freqs.len() as f32) as usize;
    let mut out = vec![0.0f32; count];
    let fade = layer.attack.max(0.0001);
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let index = ((t / layer.step) as usize).min(layer.arp_freqs.len() - 1);
        let local = t - index as f32 * layer.step;
        let freq = layer.arp_freqs[index];
        let mut s = (TAU * freq * t).sin();
        if layer.harsh > 0.0 {
            s = lerp(s, s.signum(), layer.harsh);
        }
        let env = (-layer.decay * local / layer.step).exp() * (local / fade).min(1.0);
        *slot = s * env * layer.gain;
    }
    out
}

fn render_fm(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let (mut carrier, mut modulator) = (0.0f32, 0.0f32);
    let (mut low, mut band) = (0.0f32, 0.0f32);
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let k = i as f32 / count.max(1) as f32;
        let mut freq = lerp(layer.freq_start, layer.freq_end, k);
        if layer.vibrato_rate > 0.0 && layer.vibrato_depth > 0.0 {
            freq *= 2f32.powf((TAU * layer.vibrato_rate * t).sin() * layer.vibrato_depth / 12.0);
        }
        modulator += TAU * freq * layer.fm_ratio / rate as f32;
        carrier += TAU * freq / rate as f32;
        let index = layer.fm_index * (-layer.fm_decay * k).exp();
        let raw = (carrier + modulator.sin() * index).sin();
        let (nl, nb, filtered) = filter(layer, raw, low, band, k, rate);
        low = nl;
        band = nb;
        *slot = filtered
            * adsr(
                t,
                layer.duration,
                layer.attack,
                layer.decay,
                layer.sustain,
                layer.release,
            )
            * layer.gain;
    }
    out
}

fn render_pluck(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let n = ((rate as f32 / layer.freq_start.max(20.0)) as usize).max(2);
    let mut line = vec![0.0f32; n];
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    for slot in &mut line {
        let noise = rng.next_f32();
        previous = lerp(previous, noise, layer.pluck_brightness.clamp(0.02, 1.0));
        *slot = previous;
    }
    let mut pos = 0usize;
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let current = line[pos];
        let next = line[(pos + 1) % n];
        line[pos] = (current + next) * 0.5 * layer.pluck_damping;
        pos = (pos + 1) % n;
        *slot = current
            * adsr(
                t,
                layer.duration,
                layer.attack,
                layer.decay,
                layer.sustain,
                layer.release,
            )
            * layer.gain;
    }
    out
}

/// Mirrors Godot `SfxLayer._build_sample`: linear interpolation, optional
/// loop, fixed source ratio plus `sample_pitch`, ADSR and layer gain.
fn render_sample(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let Some(data) = layer.sample_data.as_ref() else {
        return out;
    };
    if data.samples.is_empty() || data.rate == 0 {
        return out;
    }
    let frames = data.samples.len();
    let ratio = (data.rate as f32 / rate as f32) * 2f32.powf(layer.sample_pitch / 12.0);
    let mut pos = 0.0f32;
    for (i, slot) in out.iter_mut().enumerate() {
        if !layer.sample_loop && pos >= (frames as f32 - 1.0) {
            break;
        }
        let index = pos as usize;
        if index >= frames {
            break;
        }
        let frac = pos - index as f32;
        let a = data.samples[index];
        let b = if index + 1 < frames {
            data.samples[index + 1]
        } else {
            a
        };
        let t = i as f32 / rate as f32;
        *slot = (a + (b - a) * frac)
            * adsr(
                t,
                layer.duration,
                layer.attack,
                layer.decay,
                layer.sustain,
                layer.release,
            )
            * layer.gain;
        pos += ratio;
        if layer.sample_loop {
            pos %= frames as f32;
        }
    }
    out
}

fn render_kick(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut phase = 0.0f32;
    let start = 135.0 * layer.drum_tone;
    let end = 48.0 * layer.drum_tone;
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        phase += TAU * lerp(start, end, k) / rate as f32;
        *slot = phase.sin() * (-7.0 * k).exp() * layer.gain;
    }
    out
}

fn render_snare(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let t = i as f32 / rate as f32;
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        let body = (TAU * 185.0 * layer.drum_tone * t).sin() * 0.45;
        *slot = (high * 0.8 * layer.drum_snap + body) * (-11.0 * k).exp() * layer.gain;
    }
    out
}

fn render_hat(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        *slot = high * (-38.0 * k).exp() * layer.gain * (0.4 + layer.drum_snap * 0.6);
    }
    out
}

fn render_clap(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        let mut env = 0.0f32;
        for burst in 0..3 {
            let start = burst as f32 * 0.11;
            if k >= start {
                env = env.max((-30.0 * (k - start)).exp());
            }
        }
        *slot = high * env * layer.gain * (0.5 + layer.drum_snap * 0.5);
    }
    out
}

#[allow(clippy::too_many_arguments)] // deliberate: a note is a flat DSP record
fn note(
    buf: &mut [f32],
    rate: u32,
    t0: f32,
    dur: f32,
    freq: f32,
    amp: f32,
    harsh: f32,
    decay: f32,
    wave: MusicWave,
) {
    let first = (t0 * rate as f32) as usize;
    let count = (dur * rate as f32) as usize;
    let mut phase = 0.0f32;
    for i in 0..count {
        let index = first + i;
        if index >= buf.len() {
            break;
        }
        let t = i as f32 / rate as f32;
        let k = i as f32 / count.max(1) as f32;
        let env = (-decay * k).exp() * (t * 1200.0).min(1.0);
        phase += TAU * freq / rate as f32;
        let s = match wave {
            MusicWave::Sine => {
                let s = phase.sin();
                if harsh > 0.0 {
                    lerp(s, s.signum(), harsh)
                } else {
                    s
                }
            }
            MusicWave::Triangle => 1.0 - 4.0 * ((phase / TAU).rem_euclid(1.0) - 0.5).abs(),
            MusicWave::Saw => 2.0 * (phase / TAU).rem_euclid(1.0) - 1.0,
            MusicWave::Pulse => {
                if (phase / TAU).rem_euclid(1.0) < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            MusicWave::Square => {
                if (phase / TAU).rem_euclid(1.0) < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        buf[index] += s * env * amp;
    }
}

fn kick(buf: &mut [f32], rate: u32, t0: f32, gain: f32) {
    let first = (t0 * rate as f32) as usize;
    let count = (0.16 * rate as f32) as usize;
    let mut phase = 0.0f32;
    for i in 0..count {
        let index = first + i;
        if index >= buf.len() {
            break;
        }
        let k = i as f32 / count.max(1) as f32;
        phase += TAU * lerp(135.0, 48.0, k) / rate as f32;
        buf[index] += phase.sin() * (-7.0 * k).exp() * gain;
    }
}

fn hat(buf: &mut [f32], rate: u32, rng: &mut Rng, t0: f32, gain: f32) {
    let first = (t0 * rate as f32) as usize;
    let count = (0.05 * rate as f32) as usize;
    let mut previous = 0.0f32;
    for i in 0..count {
        let index = first + i;
        if index >= buf.len() {
            break;
        }
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        let env = (-38.0 * i as f32 / count.max(1) as f32).exp();
        buf[index] += high * env * gain;
    }
}

/// v3 Karplus-Strong: the loop filter cutoff is derived from Hz (relative to
/// the fundamental) and the loop gain from a 22.05 kHz reference, so the
/// pluck has the same pitch, brightness and decay at preview and bake rates.
fn render_pluck_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let render_rate = rate * 2;
    let count = (render_rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let freq = layer.freq_start.max(20.0);
    let n = ((render_rate as f32 / freq) as usize).max(2);
    let mut line = vec![0.0f32; n];
    let brightness = layer.pluck_brightness.clamp(0.0, 1.0);
    let excite_hz = (500.0 + brightness * 15_000.0).min(render_rate as f32 * 0.45);
    let excite = 1.0 - (-TAU * excite_hz / render_rate as f32).exp();
    // One-pole noise variance is a/(2-a); normalise it so the pluck level
    // does not shrink at higher render rates.
    let excitation_scale = ((2.0 - excite) / excite).sqrt();
    let mut rng = Rng::new(layer.seed as u64);
    let mut noise = 0.0f32;
    for slot in &mut line {
        noise = lerp(noise, rng.next_f32(), excite);
        *slot = noise * excitation_scale;
    }
    let loop_hz = (freq * (2.0 + brightness * 18.0)).clamp(200.0, render_rate as f32 * 0.45);
    let filter = 1.0 - (-TAU * loop_hz / render_rate as f32).exp();
    let gain = layer
        .pluck_damping
        .clamp(0.9, 0.9999)
        .powf(22_050.0 / render_rate as f32);
    let mut low = 0.0f32;
    let mut pos = 0usize;
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / render_rate as f32;
        let current = line[pos];
        low += filter * (current - low);
        line[pos] = low * gain;
        pos = (pos + 1) % n;
        *slot = current
            * adsr(
                t,
                layer.duration,
                layer.attack,
                layer.decay,
                layer.sustain,
                layer.release,
            )
            * layer.gain;
    }
    decimate(&out)
}

/// v3 noise: the UI low-pass value is mapped to Hz so preview (48k) and bake
/// (patch rate) share the same timbre. v2 keeps the raw coefficient for parity.
fn render_noise_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut rng = Rng::new(layer.seed as u64);
    let mut low = 0.0f32;
    // Interpret the UI value as the v2 one-pole coefficient measured at
    // 22.05 kHz, then convert to an equivalent cutoff in Hz. That keeps the
    // demo presets' timbre while making 22k bake and 48k preview agree.
    let cutoff = |value: f32| -(1.0 - value.clamp(0.01, 0.98)).ln() * 22_050.0 / TAU;
    let coefficient = |hz: f32| 1.0 - (-TAU * hz / rate as f32).exp();
    let from = coefficient(cutoff(layer.lowpass_from));
    let to = coefficient(cutoff(layer.lowpass_to));
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let t = i as f32 / rate as f32;
        let noise = rng.next_f32();
        low = lerp(low, noise, lerp(from, to, k));
        let sweep = (TAU * lerp(layer.freq_start, layer.freq_end, k) * t).sin();
        let env = if layer.env_shape == EnvShape::Sine {
            (std::f32::consts::PI * k).sin()
        } else {
            (-layer.decay * k).exp()
        };
        *slot = (low * layer.noise_gain + sweep * layer.sweep_gain) * env * layer.gain;
    }
    out
}

fn sample_value(samples: &[f32], reverse: bool, index: usize) -> f32 {
    let last = samples.len().saturating_sub(1);
    let index = index.min(last);
    if reverse {
        samples[last - index]
    } else {
        samples[index]
    }
}

fn sample_lerp(samples: &[f32], reverse: bool, pos: f32) -> f32 {
    let index = pos as usize;
    let frac = pos - index as f32;
    let a = sample_value(samples, reverse, index);
    let b = sample_value(samples, reverse, index + 1);
    a + (b - a) * frac
}

/// v3 sample playback: start offset, reverse, pitch, and a 10 ms crossfade
/// loop so sustained samples do not click at the wrap point.
fn render_sample_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let Some(data) = layer.sample_data.as_ref() else {
        return out;
    };
    let frames = data.samples.len();
    if frames < 2 || data.rate == 0 {
        return out;
    }
    let loop_start =
        ((layer.sample_start.clamp(0.0, 0.95) * (frames - 1) as f32) as usize).min(frames - 2);
    let loop_len = frames - loop_start;
    let xfade = ((0.010 * data.rate as f32) as usize)
        .min(loop_len / 4)
        .max(1);
    let ratio = (data.rate as f32 / rate as f32) * 2f32.powf(layer.sample_pitch / 12.0);
    let mut pos = loop_start as f32;
    for (i, slot) in out.iter_mut().enumerate() {
        if !layer.sample_loop && pos >= (frames - 1) as f32 {
            break;
        }
        let mut value = sample_lerp(&data.samples, layer.sample_reverse, pos);
        if layer.sample_loop && pos >= (frames - xfade) as f32 {
            let start = (frames - xfade) as f32;
            let u = ((pos - start) / xfade as f32).clamp(0.0, 1.0);
            let head = sample_lerp(
                &data.samples,
                layer.sample_reverse,
                loop_start as f32 + (pos - start),
            );
            value = value * (1.0 - u) + head * u;
        }
        let t = i as f32 / rate as f32;
        *slot = value
            * adsr(
                t,
                layer.duration,
                layer.attack,
                layer.decay,
                layer.sustain,
                layer.release,
            )
            * layer.gain;
        pos += ratio;
        if layer.sample_loop {
            let period = (loop_len - xfade).max(1) as f32;
            while pos >= frames as f32 {
                pos -= period;
            }
        }
    }
    out
}

/// v3 kick: noise click + pitch-swept body + sub, soft-saturated.
fn render_kick_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let tone = layer.drum_tone.clamp(0.25, 4.0);
    let snap = layer.drum_snap.clamp(0.0, 2.0);
    let click_len = (0.0035 * rate as f32).max(1.0);
    let mut phase = 0.0f32;
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    let mut click_low = 0.0f32;
    // Keep the click's bandwidth fixed in Hz so preview and bake rates agree.
    let click_filter = 1.0 - (-TAU * 5_000.0 / rate as f32).exp();
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let freq = (42.0 + 150.0 * (-9.0 * k).exp()) * tone;
        phase += TAU * freq / rate as f32;
        let body = phase.sin() * (-7.0 * k).exp();
        let sub = (phase * 0.5).sin() * (-4.5 * k).exp() * 0.45;
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        click_low += click_filter * (high - click_low);
        let click = if (i as f32) < click_len {
            (1.0 - i as f32 / click_len) * click_low * (0.2 + snap * 0.5)
        } else {
            0.0
        };
        *slot = (body + sub + click).mul_add(1.35, 0.0).tanh() * layer.gain;
    }
    out
}

/// v3 snare: two body tones + band-passed noise + snap transient.
fn render_snare_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let tone = layer.drum_tone.clamp(0.25, 4.0);
    let snap = layer.drum_snap.clamp(0.0, 2.0);
    let cutoff = (1650.0 * tone).clamp(300.0, 10_000.0);
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    let (mut low, mut band) = (0.0f32, 0.0f32);
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let t = i as f32 / rate as f32;
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        let (new_low, new_band, _) = svf_step(high, low, band, cutoff, 0.9, rate);
        low = new_low;
        band = new_band;
        let body = (TAU * 185.0 * tone * t).sin() * 0.55 + (TAU * 330.0 * tone * t).sin() * 0.28;
        let body = body * (-16.0 * k).exp();
        let noise_env = (-10.0 * k).exp() * (0.55 + snap * 0.35);
        let click = if (i as f32) < 0.0015 * rate as f32 {
            high * (1.0 - i as f32 / (0.0015 * rate as f32)) * snap * 0.5
        } else {
            0.0
        };
        *slot = (body + band * noise_env + click) * layer.gain;
    }
    out
}

/// v3 hat: six band-limited square "metal" partials, high-passed.
fn render_hat_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    const RATIOS: [f32; 6] = [2.0, 3.0, 4.16, 5.43, 6.79, 8.21];
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let tone = layer.drum_tone.clamp(0.25, 4.0);
    let snap = layer.drum_snap.clamp(0.0, 2.0);
    let base = 480.0 * tone;
    let decay = 38.0 - snap * 12.0;
    let mut phases = [0.0f32; 6];
    let (mut low, mut band) = (0.0f32, 0.0f32);
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let mut metal = 0.0f32;
        for (index, ratio) in RATIOS.iter().enumerate() {
            let increment = base * ratio / rate as f32;
            phases[index] = (phases[index] + TAU * increment).rem_euclid(TAU);
            let p = phases[index] / TAU;
            metal += pulse_blep(p, 0.5, increment);
        }
        metal /= RATIOS.len() as f32;
        let (new_low, new_band, high) = svf_step(metal, low, band, 6_500.0, 0.8, rate);
        low = new_low;
        band = new_band;
        *slot = high * (-decay * k).exp() * layer.gain * (0.6 + snap * 0.2);
    }
    out
}

/// v3 clap: four short band-passed bursts plus a short room tail.
fn render_clap_hq(layer: &Layer, rate: u32) -> Vec<f32> {
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let tone = layer.drum_tone.clamp(0.25, 4.0);
    let snap = layer.drum_snap.clamp(0.0, 2.0);
    let cutoff = (1_200.0 * tone).clamp(300.0, 8_000.0);
    let mut rng = Rng::new(layer.seed as u64);
    let mut previous = 0.0f32;
    let (mut low, mut band) = (0.0f32, 0.0f32);
    for (i, slot) in out.iter_mut().enumerate() {
        let k = i as f32 / count.max(1) as f32;
        let t = i as f32 / rate as f32;
        let noise = rng.next_f32();
        let high = noise - previous;
        previous = noise;
        let (new_low, new_band, _) = svf_step(high, low, band, cutoff, 0.7, rate);
        low = new_low;
        band = new_band;
        let mut env = 0.0f32;
        for burst in 0..4 {
            let start = burst as f32 * 0.009;
            if t >= start {
                let value = (-(t - start) * 110.0).exp() * (1.0 - burst as f32 * 0.12);
                env = env.max(value);
            }
        }
        let tail = (-12.0 * k).exp() * 0.35;
        *slot = band * (env + tail) * layer.gain * (0.6 + snap * 0.3);
    }
    out
}

fn wave_sample(wave: Wave, phase: f32, duty: f32, harsh: f32) -> f32 {
    let p = (phase / TAU).rem_euclid(1.0);
    match wave {
        Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        Wave::Saw => 2.0 * p - 1.0,
        Wave::Pulse => {
            if p < duty.clamp(0.05, 0.95) {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        _ => {
            let s = phase.sin();
            if harsh > 0.0 {
                lerp(s, s.signum(), harsh)
            } else {
                s
            }
        }
    }
}

/// Linear read from an evenly-spaced, wrapping single-cycle table.
pub(crate) fn cycle_value(curve: &[f32], phase: f32) -> f32 {
    if curve.len() < 2 {
        return (TAU * phase).sin();
    }
    let n = curve.len();
    let pos = phase.rem_euclid(1.0) * n as f32;
    let index = pos.floor() as usize % n;
    let frac = pos - pos.floor();
    let a = curve[index];
    let b = curve[(index + 1) % n];
    a + (b - a) * frac
}

/// Linear read from an evenly-spaced amplitude table over 0..1.
pub(crate) fn amp_curve_value(curve: &[f32], t: f32) -> f32 {
    if curve.len() < 2 {
        return 1.0;
    }
    let pos = t.clamp(0.0, 1.0) * (curve.len() - 1) as f32;
    let index = (pos.floor() as usize).min(curve.len() - 2);
    let frac = pos - index as f32;
    let a = curve[index];
    let b = curve[index + 1];
    a + (b - a) * frac
}

pub(crate) fn adsr(
    t: f32,
    duration: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    if attack > 0.0 && t < attack {
        return t / attack;
    }
    let sustain_start = attack + decay.max(0.0);
    if t < sustain_start {
        let k = (t - attack) / decay.max(0.0001);
        return lerp(1.0, sustain, k);
    }
    let release_start = (duration - release).max(sustain_start);
    if t < release_start {
        return sustain;
    }
    if release <= 0.0 {
        return sustain;
    }
    sustain * (1.0 - (t - release_start) / release).max(0.0)
}

fn filter(
    layer: &Layer,
    input: f32,
    mut low: f32,
    mut band: f32,
    k: f32,
    rate: u32,
) -> (f32, f32, f32) {
    if layer.filter == FilterMode::None {
        return (low, band, input);
    }
    let cutoff = lerp(layer.cutoff_start, layer.cutoff_end, k);
    let (new_low, new_band, high) = svf_step(input, low, band, cutoff, layer.resonance, rate);
    low = new_low;
    band = new_band;
    let out = match layer.filter {
        FilterMode::Lowpass => low,
        FilterMode::Highpass => high,
        FilterMode::Bandpass => band,
        FilterMode::None => input,
    };
    (low, band, out)
}

/// Fade the first/last milliseconds of a buffer. v2 never calls this (parity
/// with the Godot plugin); the v3 chain uses it to remove clip-edge clicks.
pub fn fade_edges(buffer: &mut [f32], rate: u32, fade_in_ms: f32, fade_out_ms: f32) {
    let len = buffer.len();
    if len < 4 {
        return;
    }
    let max = (len / 4).max(1);
    let fade_in = (((fade_in_ms * 0.001 * rate as f32) as usize).max(1)).min(max);
    let fade_out = (((fade_out_ms * 0.001 * rate as f32) as usize).max(1)).min(max);
    for (index, sample) in buffer[..fade_in].iter_mut().enumerate() {
        *sample *= index as f32 / fade_in as f32;
    }
    for (index, sample) in buffer[len - fade_out..].iter_mut().rev().enumerate() {
        *sample *= index as f32 / fade_out as f32;
    }
}

fn svf_step(
    input: f32,
    mut low: f32,
    mut band: f32,
    cutoff: f32,
    resonance: f32,
    rate: u32,
) -> (f32, f32, f32) {
    let f = (2.0
        * (std::f32::consts::PI * cutoff.clamp(20.0, rate as f32 * 0.45) / rate as f32).sin())
    .clamp(0.0, 0.9);
    let q = 1.0 / resonance.max(0.5);
    low += f * band;
    let high = input - low - q * band;
    band += f * high;
    (low, band, high)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()))
}

/// Min/max buckets for fast waveform painting. Rendering a multi-second cache
/// every frame is wasteful, so the UI reads this small envelope instead.
#[derive(Clone, Debug, Default)]
pub struct Envelope {
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

impl Envelope {
    pub fn is_empty(&self) -> bool {
        self.min.is_empty()
    }
}

pub fn envelope(samples: &[f32], bins: usize) -> Envelope {
    let bins = bins.max(1);
    if samples.is_empty() {
        return Envelope::default();
    }
    let mut envelope = Envelope {
        min: vec![0.0; bins],
        max: vec![0.0; bins],
    };
    for (bin, (lo, hi)) in envelope
        .min
        .iter_mut()
        .zip(envelope.max.iter_mut())
        .enumerate()
    {
        let first = bin * samples.len() / bins;
        let last = (((bin + 1) * samples.len()) / bins)
            .max(first + 1)
            .min(samples.len());
        let mut min = 0.0f32;
        let mut max = 0.0f32;
        for value in &samples[first..last] {
            if value.is_finite() {
                min = min.min(*value);
                max = max.max(*value);
            }
        }
        *lo = min;
        *hi = max;
    }
    envelope
}

pub fn db(amplitude: f32) -> f32 {
    if amplitude <= 1e-6 {
        -120.0
    } else {
        20.0 * amplitude.log10()
    }
}

/// 16-bit mono PCM, used by parity fixtures, music bake and sample tests.
pub fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let mut out = wav_header(1, rate, 16, samples.len());
    out.reserve(samples.len() * 2);
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// 16-bit stereo PCM (interleaved by frame). Baked Godot assets stay 16-bit
/// because every Godot 4 import path accepts it.
pub fn encode_wav_stereo16(left: &[f32], right: &[f32], rate: u32) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let mut out = wav_header(2, rate, 16, frames);
    out.reserve(frames * 4);
    for frame in 0..frames {
        for sample in [left[frame], right[frame]] {
            let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// 24-bit stereo PCM with TPDF dither, for the manual "Export WAV" path.
/// The dither is deterministic so repeated exports are byte-identical.
pub fn encode_wav24_stereo(left: &[f32], right: &[f32], rate: u32) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let mut out = wav_header(2, rate, 24, frames);
    out.reserve(frames * 6);
    let mut rng = Rng::new(0x5F37_59DF);
    for frame in 0..frames {
        for sample in [left[frame], right[frame]] {
            // Triangular PDF over ±1 LSB of the 24-bit grid.
            let dither = (rng.next_f32() + rng.next_f32()) * 0.5;
            let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0 + dither)
                .round()
                .clamp(-8_388_608.0, 8_388_607.0) as i32;
            out.extend_from_slice(&value.to_le_bytes()[0..3]);
        }
    }
    out
}

fn wav_header(channels: u16, rate: u32, bits: u16, frames: usize) -> Vec<u8> {
    let block = channels as usize * bits as usize / 8;
    let data_len = (frames * block) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block as u32).to_le_bytes());
    out.extend_from_slice(&(block as u16).to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out
}

pub fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) -> std::io::Result<()> {
    crate::model::write_atomic(path, &encode_wav(samples, rate))
}

/// Write a stereo WAV; `bits` is 16 or 24.
pub fn write_wav_stereo(
    path: &std::path::Path,
    left: &[f32],
    right: &[f32],
    rate: u32,
    bits: u16,
) -> std::io::Result<()> {
    let bytes = if bits == 24 {
        encode_wav24_stereo(left, right, rate)
    } else {
        encode_wav_stereo16(left, right, rate)
    };
    crate::model::write_atomic(path, &bytes)
}

/// Small deterministic RNG (xorshift64*), matching "same seed -> same noise".
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(2685821657736338717).max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(2685821657736338717)
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Music, Patch, ToneEnv, Wave};

    #[test]
    fn dsp_v1_keeps_the_old_clip_envelope() {
        use crate::model::{Clip, Patch};
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.3;
        layer.sustain = 0.6;
        layer.release = 0.1;
        layer.gain = 1.0;
        layer.clips = vec![Clip {
            start: 0.0,
            duration: 0.1,
            ..Clip::default()
        }];
        let mut patch = Patch {
            dsp_version: 1,
            layers: vec![layer],
            normalize: false,
            ..Patch::default()
        };
        let old = render_patch(&patch, 22050);
        patch.dsp_version = 2;
        let new = render_patch(&patch, 22050);
        let frames = old.len().min(new.len());
        assert!(frames > 0);
        // v1 still has sustain near the end of the clip; v2 has released.
        assert!(old[..frames]
            .iter()
            .zip(new[..frames].iter())
            .any(|(a, b)| (a - b).abs() > 1e-4));
    }

    #[test]
    fn sample_layers_loop_and_apply_pitch() {
        let mut layer = Layer::new(Wave::Sample);
        layer.duration = 4.5 / 48_000.0;
        layer.gain = 1.0;
        layer.attack = 0.0;
        layer.decay = 0.0;
        layer.sustain = 1.0;
        layer.release = 0.0;
        layer.sample_loop = true;
        layer.sample_data = Some(std::sync::Arc::new(crate::wav::SampleData {
            rate: 48_000,
            samples: vec![0.0, 1.0, 0.0, -1.0],
        }));
        let out = render_layer(&layer, 48_000);
        assert_eq!(out.len(), 4);
        assert!((out[0] - 0.0).abs() < 0.01);
        assert!((out[1] - 1.0).abs() < 0.01);
        assert!((out[2] - 0.0).abs() < 0.01);
        assert!((out[3] + 1.0).abs() < 0.01);

        layer.sample_pitch = 12.0;
        layer.sample_data = Some(std::sync::Arc::new(crate::wav::SampleData {
            rate: 48_000,
            samples: vec![0.0, 1.0, 2.0, 3.0],
        }));
        let faster = render_layer(&layer, 48_000);
        assert!((faster[0] - 0.0).abs() < 0.01);
        assert!((faster[1] - 2.0).abs() < 0.01);
    }

    fn goertzel(samples: &[f32], rate: u32, freq: f32) -> f32 {
        let w = TAU * freq / rate as f32;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for sample in samples {
            let s0 = sample + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / samples.len() as f32 * 2.0
    }

    fn sustained(layer: &mut Layer) {
        layer.duration = 0.3;
        layer.attack = 0.001;
        layer.decay = 0.0;
        layer.sustain = 1.0;
        layer.release = 0.02;
        layer.gain = 1.0;
    }

    #[test]
    fn hq_oscillators_reject_alias_energy() {
        let rate = 48_000;
        for wave in [Wave::Saw, Wave::Square, Wave::Pulse] {
            let mut layer = Layer::new(wave);
            sustained(&mut layer);
            layer.freq_start = 5_000.0;
            layer.freq_end = 5_000.0;
            layer.duty = 0.3;
            let legacy = render_layer(&layer, rate);
            let hq = render_layer_hq(&layer, rate);
            // 5 kHz harmonics land at 5/10/15/20 kHz; the 7th (35 kHz) folds
            // to 13 kHz at 48 kHz, so a clean oscillator has no energy there.
            let alias_legacy = goertzel(&legacy, rate, 13_000.0);
            let alias_hq = goertzel(&hq, rate, 13_000.0);
            let fundamental = goertzel(&hq, rate, 5_000.0);
            assert!(
                alias_hq < alias_legacy * 0.35 + 1e-4,
                "{wave:?}: alias {alias_hq} vs legacy {alias_legacy}"
            );
            assert!(fundamental > 0.2, "{wave:?}: fundamental {fundamental}");
        }
    }

    #[test]
    fn decimator_preserves_dc_and_low_frequencies() {
        let dc = decimate(&[1.0f32; 64]);
        // The FIR zero-pads at the very edges; the interior must be unity.
        assert!(dc[16..dc.len() - 16]
            .iter()
            .all(|sample| (sample - 1.0).abs() < 1e-4));

        let rate = 96_000;
        let tone: Vec<f32> = (0..8_000)
            .map(|i| (TAU * 1_000.0 * i as f32 / rate as f32).sin())
            .collect();
        let down = decimate(&tone);
        let peak = peak(&down);
        assert!((0.95..=1.05).contains(&peak), "peak {peak}");
        assert_eq!(down.len(), tone.len() / 2);
    }

    #[test]
    fn hq_drums_are_finite_and_remodelled() {
        let rate = 24_000;
        for wave in [Wave::Kick, Wave::Snare, Wave::Hat, Wave::Clap] {
            let mut layer = Layer::new(wave);
            layer.duration = 0.25;
            layer.gain = 0.8;
            let legacy = render_layer(&layer, rate);
            let hq = render_layer_hq(&layer, rate);
            assert!(!hq.is_empty(), "{wave:?} empty");
            assert!(hq.iter().all(|sample| sample.is_finite()));
            assert!(peak(&hq) > 0.01, "{wave:?} silent");
            assert!(
                legacy
                    .iter()
                    .zip(hq.iter())
                    .any(|(a, b)| (a - b).abs() > 1e-3),
                "{wave:?} should use the v3 voice"
            );
        }
        // A kick must be much darker than a hat (fewer zero crossings).
        let crossings = |layer: &Layer| {
            let samples = render_layer_hq(layer, rate);
            samples
                .windows(2)
                .filter(|pair| (pair[0] >= 0.0) != (pair[1] >= 0.0))
                .count()
        };
        let mut kick = Layer::new(Wave::Kick);
        kick.duration = 0.2;
        let mut hat = Layer::new(Wave::Hat);
        hat.duration = 0.2;
        assert!(crossings(&kick) * 4 < crossings(&hat));
    }

    #[test]
    fn sample_start_reverse_and_loop_crossfade() {
        let rate = 48_000;
        let mut layer = Layer::new(Wave::Sample);
        layer.duration = 0.02;
        layer.attack = 0.0;
        layer.decay = 0.0;
        layer.sustain = 1.0;
        layer.release = 0.0;
        layer.gain = 1.0;
        layer.sample_data = Some(std::sync::Arc::new(crate::wav::SampleData {
            rate,
            samples: vec![0.0, 1.0, 0.0, -1.0],
        }));
        layer.sample_start = 0.5;
        let started = render_sample_hq(&layer, rate);
        assert!((started[0] - 1.0).abs() < 1e-3, "start {}", started[0]);

        layer.sample_start = 0.0;
        layer.sample_reverse = true;
        let reversed = render_sample_hq(&layer, rate);
        assert!((reversed[0] + 1.0).abs() < 1e-3, "reverse {}", reversed[0]);

        // A loop that does not contain whole cycles would click without the
        // 10 ms crossfade.
        let frames = 480usize;
        let samples: Vec<f32> = (0..frames)
            .map(|i| (TAU * 110.5 * i as f32 / rate as f32).sin())
            .collect();
        layer.sample_reverse = false;
        layer.sample_loop = true;
        layer.duration = 0.05;
        layer.sample_data = Some(std::sync::Arc::new(crate::wav::SampleData {
            rate,
            samples,
        }));
        let looped = render_sample_hq(&layer, rate);
        let max_step = looped
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_step < 0.2, "loop step {max_step}");
    }

    #[test]
    fn curve_wave_uses_the_drawn_cycle_and_amp_curve() {
        let mut layer = Layer::new(Wave::Curve);
        layer.duration = 0.02;
        layer.gain = 1.0;
        layer.attack = 0.0;
        layer.decay = 0.0;
        layer.sustain = 1.0;
        layer.release = 0.0;
        layer.curve = vec![1.0, -1.0, 1.0, -1.0];
        let loud = render_layer_hq(&layer, 48_000);
        assert!(!loud.is_empty());
        assert!(peak(&loud) > 0.1);

        layer.amp_curve = vec![0.0; 32];
        let silent = render_layer_hq(&layer, 48_000);
        assert!(peak(&silent) < 0.001, "amp curve should silence the voice");
    }

    #[test]
    fn hq_voices_are_rate_consistent() {
        fn rms(samples: &[f32]) -> f32 {
            (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
        }
        fn crossings_per_second(samples: &[f32], rate: u32) -> f32 {
            let count = samples
                .windows(2)
                .filter(|pair| (pair[0] >= 0.0) != (pair[1] >= 0.0))
                .count();
            count as f32 / samples.len().max(1) as f32 * rate as f32
        }
        for wave in [Wave::Sine, Wave::Saw, Wave::Pluck, Wave::Noise, Wave::Kick] {
            let mut layer = Layer::new(wave);
            layer.duration = 0.2;
            layer.gain = 0.8;
            let a = render_layer_hq(&layer, 22_050);
            let b = render_layer_hq(&layer, 44_100);
            let dr = (rms(&b) / rms(&a).max(1e-6) - 1.0).abs();
            let za = crossings_per_second(&a, 22_050);
            let zb = crossings_per_second(&b, 44_100);
            let dz = (zb / za.max(1.0) - 1.0).abs();
            assert!(
                dr < 0.25,
                "{wave:?} rms drift {dr}: {} vs {}",
                rms(&a),
                rms(&b)
            );
            assert!(dz < 0.25, "{wave:?} brightness drift {dz}: {za} vs {zb}");
        }
    }

    #[test]
    fn every_wave_renders_finite_samples() {
        for wave in Wave::ALL {
            let mut layer = Layer::new(wave);
            layer.duration = 0.08;
            if wave == Wave::Sample {
                layer.sample_loop = true;
                layer.sample_data = Some(std::sync::Arc::new(crate::wav::SampleData {
                    rate: 22050,
                    samples: vec![0.0, 0.5, -0.5, 1.0],
                }));
            }
            let samples = render_layer(&layer, 22050);
            assert!(!samples.is_empty(), "{} empty", wave.label());
            assert!(
                samples.iter().all(|s| s.is_finite()),
                "{} has non-finite samples",
                wave.label()
            );
            assert!(peak(&samples) > 0.0, "{} silent", wave.label());
        }
    }

    #[test]
    fn layers_mix_to_the_longest_delay() {
        let mut patch = Patch {
            layers: vec![Layer::new(Wave::Sine), Layer::new(Wave::Noise)],
            ..Patch::default()
        };
        patch.layers[0].duration = 0.1;
        patch.layers[1].delay = 0.05;
        patch.layers[1].duration = 0.2;
        let samples = render_patch(&patch, 8000);
        assert!((samples.len() as f32 / 8000.0 - 0.25).abs() < 0.01);
    }

    #[test]
    fn clips_place_source_multiple_times() {
        use crate::model::Clip;
        let mut patch = Patch::default();
        let mut layer = Layer::new(Wave::Square);
        layer.duration = 0.05;
        layer.clips = vec![
            Clip {
                start: 0.0,
                duration: 0.05,
                gain: 1.0,
                ..Clip::default()
            },
            Clip {
                start: 0.1,
                duration: 0.05,
                gain: 0.5,
                ..Clip::default()
            },
        ];
        patch.layers = vec![layer];
        let samples = render_patch(&patch, 8000);
        assert!((samples.len() as f32 / 8000.0 - 0.15).abs() < 0.01);
        assert!(peak(&samples) > 0.0);
    }

    #[test]
    fn clip_envelope_is_linear_and_applied() {
        use crate::model::{AutomationPoint, Clip};
        let mut clip = Clip {
            start: 0.0,
            duration: 0.1,
            gain: 1.0,
            ..Clip::default()
        };
        clip.gain_points = vec![
            AutomationPoint {
                time: 0.0,
                value: 0.0,
            },
            AutomationPoint {
                time: 0.1,
                value: 1.0,
            },
        ];
        assert!((clip.gain_at(0.05) - 0.5).abs() < 0.01);
        clip.gain_points.clear();
        assert_eq!(clip.gain_at(0.05), 1.0);
    }

    #[test]
    fn cutoff_automation_renders() {
        use crate::model::{AutomationPoint, Clip};
        let mut layer = Layer::new(Wave::Saw);
        layer.duration = 0.1;
        layer.clips = vec![Clip {
            start: 0.0,
            duration: 0.1,
            ..Clip::default()
        }];
        layer.clips[0].cutoff_points = vec![
            AutomationPoint {
                time: 0.0,
                value: 200.0,
            },
            AutomationPoint {
                time: 0.1,
                value: 12000.0,
            },
        ];
        let patch = Patch {
            layers: vec![layer],
            ..Patch::default()
        };
        assert!(!render_patch(&patch, 8000).is_empty());
    }

    #[test]
    fn music_without_notes_is_empty_like_the_godot_plugin() {
        let music = Music {
            bass_notes: Vec::new(),
            ..Music::default()
        };
        assert!(render_music(&music, 8000).is_empty());
    }

    #[test]
    fn music_has_expected_length() {
        let music = Music {
            bars: 1,
            bpm: 120.0,
            rate: 8000,
            ..Music::default()
        };
        let samples = render_music(&music, 8000);
        assert!((samples.len() as f32 / 8000.0 - 2.0).abs() < 0.02);
        assert!(peak(&samples) > 0.0);
    }

    #[test]
    fn wav24_header_is_stereo_pcm() {
        fn sample24(bytes: &[u8]) -> i32 {
            let value = i32::from(bytes[0]) | i32::from(bytes[1]) << 8 | i32::from(bytes[2]) << 16;
            if value & 0x0080_0000 != 0 {
                value | !0x00FF_FFFF
            } else {
                value
            }
        }
        let wav = encode_wav24_stereo(&[0.0, 0.5], &[0.0, -0.5], 48_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48_000);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 24);
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 6);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 12);
        assert_eq!(wav.len(), 44 + 12);
        // Second frame: left +0.5, right -0.5.
        let left = sample24(&wav[50..53]);
        let right = sample24(&wav[53..56]);
        assert!((left - 4_194_303).abs() <= 2, "left {left}");
        assert!((right + 4_194_303).abs() <= 2, "right {right}");
        assert_eq!(encode_wav24_stereo(&[0.0, 0.5], &[0.0, -0.5], 48_000), wav);
    }

    #[test]
    fn wav_is_a_mono_16bit_riff() {
        let wav = encode_wav(&[0.0, 0.5, -0.5], 22050);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 22050);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
    }

    #[test]
    fn exp_tone_envelope_matches_godot_formula() {
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.2;
        layer.attack = 0.001;
        layer.exp_decay = 6.0;
        layer.tone_env = ToneEnv::Exp;
        let samples = render_layer(&layer, 8000);
        assert!(samples[0].abs() < 0.05);
        assert!(peak(&samples) > 0.0);
        assert!(samples[samples.len() - 1].abs() < samples[samples.len() / 2].abs());
    }

    #[test]
    fn envelope_buckets_track_min_and_max() {
        let buckets = envelope(&[0.0, 1.0, -1.0, 0.5], 2);
        assert_eq!(buckets.min, vec![0.0, -1.0]);
        assert_eq!(buckets.max, vec![1.0, 0.5]);
        assert!(envelope(&[], 8).is_empty());
    }
}
