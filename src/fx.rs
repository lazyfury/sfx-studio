//! DSP v3: stereo mix, de-clicking, early reflections, reverb and master.
//!
//! DSP v2 (`synth::render_patch`) is the dry mono path that the Godot plugin
//! mirrors. v3 keeps the same oscillator formulas but adds a real output
//! chain:
//! - per-layer constant-power pan;
//! - 2 ms/4 ms fades so no layer or clip starts/ends on a step;
//! - six early-reflection taps and a small feedback-delay reverb (papered by
//!   two allpasses) that also produces the extra tail seconds;
//! - DC block, 20 Hz high-pass, soft limiter and peak normalization.
//!
//! Everything is deterministic (no RNG in the chain), which keeps exports and
//! golden files reproducible.

use crate::model::Patch;
use crate::synth;

const TAU: f32 = std::f32::consts::TAU;

/// Extra seconds rendered behind the source when `space` is on.
pub const MAX_TAIL: f32 = 1.20;

/// Stereo (non-interleaved) render result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stereo {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Stereo {
    pub fn from_mono(samples: Vec<f32>) -> Self {
        Self {
            right: samples.clone(),
            left: samples,
        }
    }

    pub fn frames(&self) -> usize {
        self.left.len().min(self.right.len())
    }

    /// Interleaved `L, R, L, R, …` buffer, ready for the audio callback.
    pub fn interleave(&self) -> Vec<f32> {
        let frames = self.frames();
        let mut out = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            out.push(self.left[frame]);
            out.push(self.right[frame]);
        }
        out
    }

    /// Downmix used for waveform envelopes / peaks.
    pub fn mono(&self) -> Vec<f32> {
        let frames = self.frames();
        (0..frames)
            .map(|frame| (self.left[frame] + self.right[frame]) * 0.5)
            .collect()
    }
}

/// Extra seconds the reverb needs after the source ends.
pub fn tail_seconds(patch: &Patch) -> f32 {
    if patch.dsp_version < 3 || patch.space <= 0.001 {
        0.0
    } else {
        // A quiet send needs less tail; short UI sounds should not ship a
        // half-second of near-silence.
        let space = patch.space.clamp(0.0, 1.0);
        let scale = 0.35 + 0.65 * space;
        (0.12 + patch.room.clamp(0.0, 1.0) * (MAX_TAIL - 0.12)) * scale
    }
}

/// Duration the renderer (and therefore the timeline / export) uses.
pub fn render_duration(patch: &Patch) -> f32 {
    patch.end().max(0.0) + tail_seconds(patch)
}

/// Version-dispatched render. v2 projects stay dry mono (dual-mono stereo) so
/// an existing project never changes sound on upgrade.
pub fn render_patch(patch: &Patch, rate: u32) -> Stereo {
    let rate = rate.clamp(8_000, 192_000);
    if patch.dsp_version < 3 {
        return Stereo::from_mono(synth::render_patch(patch, rate));
    }
    if !patch.layers.iter().any(|layer| layer.enabled) {
        return Stereo::default();
    }
    let frames = ((render_duration(patch) * rate as f32).round() as usize)
        .clamp(1, (rate as f32 * 40.0) as usize);
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];

    for layer in &patch.layers {
        if !layer.enabled {
            continue;
        }
        let mut track = synth::render_layer_track(layer, rate, synth::TrackOptions::HD);
        declick(&mut track, rate);
        let (gain_l, gain_r) = pan_gains(layer.pan);
        for (index, sample) in track.iter().enumerate().take(frames) {
            left[index] += sample * gain_l;
            right[index] += sample * gain_r;
        }
    }

    for channel in [&mut left, &mut right] {
        dc_block(channel);
        highpass(channel, 20.0, rate);
    }

    if patch.space > 0.001 {
        let (wet_l, wet_r) = reverb(&left, &right, rate, patch.space, patch.room);
        for index in 0..frames {
            left[index] += wet_l[index];
            right[index] += wet_r[index];
        }
    }

    for channel in [&mut left, &mut right] {
        for sample in channel.iter_mut() {
            *sample = soft_clip(*sample);
        }
    }

    if patch.normalize {
        // v3 normalizes toward the target in both directions so every patch
        // lands at a consistent preview/export level.
        normalize(&mut left, &mut right, 0.95);
    }

    declick(&mut left, rate);
    declick(&mut right, rate);
    Stereo { left, right }
}

fn pan_gains(pan: f32) -> (f32, f32) {
    // Constant-power pan; centre sends -3 dB to each side.
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Fade the first 2 ms and last 4 ms so a layer cannot end on a step.
fn declick(buffer: &mut [f32], rate: u32) {
    synth::fade_edges(buffer, rate, 2.0, 4.0);
}

fn dc_block(buffer: &mut [f32]) {
    let mut previous_in = 0.0f32;
    let mut previous_out = 0.0f32;
    for sample in buffer.iter_mut() {
        let out = *sample - previous_in + 0.995 * previous_out;
        previous_in = *sample;
        previous_out = out;
        *sample = out;
    }
}

fn highpass(buffer: &mut [f32], cutoff: f32, rate: u32) {
    let a = (-TAU * cutoff.clamp(1.0, rate as f32 * 0.4) / rate as f32).exp();
    let mut previous_in = 0.0f32;
    let mut previous_out = 0.0f32;
    for sample in buffer.iter_mut() {
        let out = a * (previous_out + *sample - previous_in);
        previous_in = *sample;
        previous_out = out;
        *sample = out;
    }
}

fn soft_clip(sample: f32) -> f32 {
    let threshold = 0.9f32;
    let magnitude = sample.abs();
    if magnitude <= threshold {
        sample
    } else {
        let sign = if sample < 0.0 { -1.0 } else { 1.0 };
        let over = (magnitude - threshold) / (1.0 - threshold);
        sign * (threshold + (1.0 - threshold) * over.tanh())
    }
}

fn normalize(left: &mut [f32], right: &mut [f32], target: f32) {
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |acc, sample| acc.max(sample.abs()));
    if peak <= 1e-6 {
        return;
    }
    let scale = target / peak;
    for sample in left.iter_mut().chain(right.iter_mut()) {
        *sample *= scale;
    }
}

/// Early reflections are cheap and do most of the "this happens in a room"
/// work; the comb/allpass tail adds depth and the tail seconds.
fn reverb(
    input_l: &[f32],
    input_r: &[f32],
    rate: u32,
    mix: f32,
    room: f32,
) -> (Vec<f32>, Vec<f32>) {
    let frames = input_l.len().min(input_r.len());
    let room = room.clamp(0.0, 1.0);
    let mut wet_l = vec![0.0f32; frames];
    let mut wet_r = vec![0.0f32; frames];

    // Early reflections: alternate sides, alternate polarity, decay with time.
    const TAPS: [(f32, f32, usize); 6] = [
        (0.0071, 0.62, 0),
        (0.0113, 0.55, 1),
        (0.0167, 0.48, 0),
        (0.0219, 0.42, 1),
        (0.0293, 0.36, 0),
        (0.0389, 0.30, 1),
    ];
    let er_gain = 0.30 * mix;
    for (time, gain, side) in TAPS {
        let delay = ((time * (0.55 + room * 1.6)) * rate as f32) as usize;
        if delay == 0 || delay >= frames {
            continue;
        }
        for index in delay..frames {
            let value = if side == 0 {
                input_l[index - delay] * gain * er_gain
            } else {
                -input_r[index - delay] * gain * er_gain
            };
            if side == 0 {
                wet_l[index] += value;
            } else {
                wet_r[index] += value;
            }
        }
    }

    let feedback = 0.55 + room * 0.33;
    // Damping is a one-pole coefficient; derive it from Hz so preview (48k)
    // and bake (22k) share the same tail colour.
    let damping_hz = 9_000.0 - room * 6_000.0;
    let damping = (-TAU * damping_hz / rate as f32).exp();
    let comb_scale = 0.55 + room * 1.30;
    let send = 0.55 * mix;
    const COMBS: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];

    for (channel, input) in [(0usize, input_l), (1usize, input_r)] {
        let mut tail = vec![0.0f32; frames];
        // Slightly different lengths per side decorrelate the tail.
        let spread = 1.0 + channel as f32 * 0.017;
        for (index, base) in COMBS.iter().enumerate() {
            let seconds = base * comb_scale * spread * (1.0 + index as f32 * 0.011);
            let delay = ((seconds * rate as f32) as usize).max(1);
            let mut line = vec![0.0f32; delay];
            let mut cursor = 0usize;
            let mut low = 0.0f32;
            for frame in 0..frames {
                let delayed = line[cursor];
                low = delayed * (1.0 - damping) + low * damping;
                line[cursor] = input[frame] * send + low * feedback;
                cursor = (cursor + 1) % delay;
                tail[frame] += delayed;
            }
        }
        for sample in tail.iter_mut() {
            *sample *= 0.25;
        }
        allpass(&mut tail, ((0.0050 * rate as f32) as usize).max(1), 0.62);
        allpass(&mut tail, ((0.0017 * rate as f32) as usize).max(1), 0.54);
        for sample in tail.iter_mut() {
            *sample *= 0.75 * mix;
        }
        if channel == 0 {
            for frame in 0..frames {
                wet_l[frame] += tail[frame];
            }
        } else {
            for frame in 0..frames {
                wet_r[frame] += tail[frame];
            }
        }
    }

    (wet_l, wet_r)
}

fn allpass(buffer: &mut [f32], delay: usize, gain: f32) {
    if delay == 0 || buffer.len() <= delay {
        return;
    }
    let mut line = vec![0.0f32; delay];
    let mut cursor = 0usize;
    for sample in buffer.iter_mut() {
        let delayed = line[cursor];
        let input = *sample;
        let out = -gain * input + delayed;
        line[cursor] = input + gain * out;
        cursor = (cursor + 1) % delay;
        *sample = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Patch, Wave};

    fn tone_patch() -> Patch {
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.2;
        layer.gain = 0.5;
        layer.freq_start = 220.0;
        layer.freq_end = 220.0;
        layer.decay = 0.12;
        layer.release = 0.05;
        Patch {
            name: "tone".into(),
            layers: vec![layer],
            ..Patch::default()
        }
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn legacy_patches_stay_mono_and_unprocessed() {
        let mut patch = tone_patch();
        patch.dsp_version = 2;
        patch.space = 1.0;
        patch.layers[0].pan = -1.0;
        let stereo = render_patch(&patch, 8_000);
        assert_eq!(stereo.left, stereo.right);
        // No tail frames are added for the legacy path.
        assert_eq!(stereo.frames(), synth::render_patch(&patch, 8_000).len());
    }

    #[test]
    fn pan_hard_left_is_silent_on_the_right() {
        let mut patch = tone_patch();
        patch.layers[0].pan = -1.0;
        let stereo = render_patch(&patch, 8_000);
        assert!(rms(&stereo.left) > 0.01);
        assert!(stereo.right.iter().all(|sample| sample.abs() < 1e-4));
    }

    #[test]
    fn space_adds_energy_after_the_source() {
        let mut dry = tone_patch();
        dry.space = 0.0;
        let mut wet = tone_patch();
        wet.space = 1.0;
        wet.room = 0.9;
        let dry = render_patch(&dry, 8_000);
        let wet = render_patch(&wet, 8_000);
        assert!(wet.frames() > dry.frames());
        let source_end = (tone_patch().end() * 8_000.0) as usize;
        let tail = &wet.mono()[source_end..];
        assert!(rms(tail) > 0.002, "tail rms {}", rms(tail));
        assert!(dry.frames() <= source_end + 2);
    }

    #[test]
    fn master_output_never_clips() {
        let mut layer = Layer::new(Wave::Noise);
        layer.duration = 0.3;
        layer.gain = 1.0;
        layer.noise_gain = 1.0;
        layer.sweep_gain = 1.0;
        let mut patch = Patch {
            layers: vec![layer.clone(), layer.clone(), layer.clone(), layer],
            space: 1.0,
            room: 1.0,
            ..Patch::default()
        };
        patch.normalize = true;
        let stereo = render_patch(&patch, 8_000);
        let peak = stereo
            .left
            .iter()
            .chain(stereo.right.iter())
            .fold(0.0f32, |acc, sample| acc.max(sample.abs()));
        assert!(peak <= 1.0, "peak {peak}");
        assert!(peak > 0.90, "normalize should reach the target: {peak}");
    }

    #[test]
    fn edges_are_declicked() {
        let mut patch = tone_patch();
        patch.layers[0].attack = 0.0;
        patch.layers[0].decay = 0.0;
        patch.layers[0].sustain = 1.0;
        patch.layers[0].release = 0.0;
        patch.layers[0].tone_env = crate::model::ToneEnv::Exp;
        patch.layers[0].exp_decay = 0.5;
        let stereo = render_patch(&patch, 8_000);
        let last = stereo.frames() - 1;
        assert!(stereo.left[0].abs() < 1e-3);
        assert!(stereo.left[last].abs() < 1e-3);
    }

    #[test]
    fn clip_edges_are_faded_before_mixing() {
        use crate::model::{Clip, ToneEnv};
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.3;
        layer.freq_start = 220.0;
        layer.freq_end = 220.0;
        layer.attack = 0.0;
        layer.tone_env = ToneEnv::Exp;
        layer.exp_decay = 0.5;
        layer.clips = vec![Clip {
            start: 0.0,
            duration: 0.1,
            ..Clip::default()
        }];
        let mut patch = Patch {
            layers: vec![layer],
            ..Patch::default()
        };
        patch.normalize = false;
        let stereo = render_patch(&patch, 8_000);
        let last = (0.1 * 8_000.0) as usize - 1;
        assert!(
            stereo.left[last].abs() < 0.01,
            "clip end {} was not faded",
            stereo.left[last]
        );
    }

    #[test]
    fn stereo_render_is_deterministic() {
        let mut patch = tone_patch();
        patch.space = 0.5;
        patch.layers[0].pan = 0.3;
        let first = render_patch(&patch, 8_000);
        let second = render_patch(&patch, 8_000);
        assert_eq!(first, second);
    }

    #[test]
    fn demo_presets_render_as_stereo_and_use_the_space_chain() {
        let project = crate::model::Project::demo("fx");
        let mut widest = 0.0f32;
        for patch in &project.patches {
            let stereo = render_patch(patch, 22_050);
            assert!(stereo.frames() > 0, "{} silent", patch.name);
            let peak = stereo
                .left
                .iter()
                .chain(stereo.right.iter())
                .fold(0.0f32, |acc, sample| acc.max(sample.abs()));
            assert!(peak <= 1.0 && peak > 0.5, "{} peak {peak}", patch.name);
            assert!(stereo.frames() as f32 / 22_050.0 >= patch.end() - 0.01);
            let diff = stereo
                .left
                .iter()
                .zip(stereo.right.iter())
                .map(|(l, r)| (l - r).abs())
                .fold(0.0f32, f32::max);
            widest = widest.max(diff);
        }
        assert!(widest > 0.05, "expected audible stereo width, got {widest}");
    }

    #[test]
    fn render_duration_includes_the_reverb_tail() {
        let mut patch = tone_patch();
        assert!((render_duration(&patch) - patch.end()).abs() < 1e-6);
        patch.space = 1.0;
        patch.room = 1.0;
        assert!(render_duration(&patch) > patch.end() + 0.5);
    }
}
