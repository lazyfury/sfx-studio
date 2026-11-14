//! Deterministic synthesis. Mirrors the Godot `sfx_synth.gd` / `sfx_layer.gd`
//! formulas so a project sounds the same in the app and in-game.

use crate::model::{EnvShape, FilterMode, Layer, Music, MusicWave, Patch, ToneEnv, Wave};

const TAU: f32 = std::f32::consts::TAU;

/// Preview/export render rate. Audio output resamples from this rate.
pub const PREVIEW_RATE: u32 = 48_000;

/// Bucket count for the cached waveform envelope.
pub const ENVELOPE_BINS: usize = 2048;

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
    let mut buf = vec![0.0f32; total];
    let clip_adsr = patch.dsp_version >= 2;
    for layer in &patch.layers {
        if !layer.enabled {
            continue;
        }
        if layer.clips.is_empty() {
            let source = render_layer(layer, rate);
            let offset = (layer.delay * rate as f32) as usize;
            for (index, sample) in source.iter().enumerate() {
                if let Some(slot) = buf.get_mut(offset + index) {
                    *slot += *sample;
                }
            }
        } else {
            for clip in &layer.clips {
                if clip.gain <= 0.0 {
                    continue;
                }
                let mut source_layer = if clip.pitch.abs() > 0.001 {
                    layer.with_pitch(clip.pitch)
                } else {
                    layer.clone()
                };
                // Godot flattens clips to layers with the clip duration, so
                // dsp v2 follows the clip; v1 keeps the old layer-length ADSR.
                source_layer.duration = if clip_adsr {
                    clip.duration.min(layer.duration)
                } else {
                    layer.duration
                };
                let source = render_layer(&source_layer, rate);
                let start = (clip.start * rate as f32) as usize;
                let count = ((clip.duration * rate as f32) as usize).min(source.len());
                let mut temp = vec![0.0f32; count];
                for index in 0..count {
                    let env = clip.gain_at(index as f32 / rate as f32);
                    temp[index] = source[index] * clip.gain * env;
                }
                if clip.has_cutoff_automation() {
                    apply_cutoff(&mut temp, clip, layer.resonance, rate);
                }
                for (index, sample) in temp.iter().enumerate() {
                    if let Some(slot) = buf.get_mut(start + index) {
                        *slot += *sample;
                    }
                }
            }
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
    let count = (rate as f32 * layer.duration) as usize;
    let mut out = vec![0.0f32; count];
    let mut phase = 0.0f32;
    let (mut low, mut band) = (0.0f32, 0.0f32);
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let k = i as f32 / count.max(1) as f32;
        let mut freq = lerp(layer.freq_start, layer.freq_end, k);
        if layer.vibrato_rate > 0.0 && layer.vibrato_depth > 0.0 {
            freq *= 2f32.powf((TAU * layer.vibrato_rate * t).sin() * layer.vibrato_depth / 12.0);
        }
        phase += TAU * freq / rate as f32;
        let raw = wave_sample(layer.wave, phase, layer.duty, layer.harsh);
        let (nl, nb, filtered) = filter(layer, raw, low, band, k, rate);
        low = nl;
        band = nb;
        let env = match layer.tone_env {
            ToneEnv::Exp => (-layer.exp_decay * k).exp() * (t / layer.attack.max(0.0001)).min(1.0),
            ToneEnv::Adsr => adsr(
                t,
                layer.duration,
                layer.attack,
                layer.decay,
                layer.sustain,
                layer.release,
            ),
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

fn adsr(t: f32, duration: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
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
    let f = (2.0
        * (std::f32::consts::PI * cutoff.clamp(20.0, rate as f32 * 0.45) / rate as f32).sin())
    .clamp(0.0, 0.9);
    let q = 1.0 / layer.resonance.max(0.5);
    low += f * band;
    let high = input - low - q * band;
    band += f * high;
    let out = match layer.filter {
        FilterMode::Lowpass => low,
        FilterMode::Highpass => high,
        FilterMode::Bandpass => band,
        FilterMode::None => input,
    };
    (low, band, out)
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

pub fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) -> std::io::Result<()> {
    crate::model::write_atomic(path, &encode_wav(samples, rate))
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
        layer.sample_data = Some(crate::wav::SampleData {
            rate: 48_000,
            samples: vec![0.0, 1.0, 0.0, -1.0],
        });
        let out = render_layer(&layer, 48_000);
        assert_eq!(out.len(), 4);
        assert!((out[0] - 0.0).abs() < 0.01);
        assert!((out[1] - 1.0).abs() < 0.01);
        assert!((out[2] - 0.0).abs() < 0.01);
        assert!((out[3] + 1.0).abs() < 0.01);

        layer.sample_pitch = 12.0;
        layer.sample_data = Some(crate::wav::SampleData {
            rate: 48_000,
            samples: vec![0.0, 1.0, 2.0, 3.0],
        });
        let faster = render_layer(&layer, 48_000);
        assert!((faster[0] - 0.0).abs() < 0.01);
        assert!((faster[1] - 2.0).abs() < 0.01);
    }

    #[test]
    fn every_wave_renders_finite_samples() {
        for wave in Wave::ALL {
            let mut layer = Layer::new(wave);
            layer.duration = 0.08;
            if wave == Wave::Sample {
                layer.sample_loop = true;
                layer.sample_data = Some(crate::wav::SampleData {
                    rate: 22050,
                    samples: vec![0.0, 0.5, -0.5, 1.0],
                });
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
