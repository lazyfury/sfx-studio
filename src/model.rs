//! Pure project model: patches, layers, music, JSON persistence.
//!
//! The standalone app owns this format. It can also import/export the Godot
//! plugin's `.tres` resources (see `tres.rs`).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const PROJECT_FILE: &str = "sfx.project.json";

/// Current on-disk project schema. Bump this whenever a migration is needed.
pub const PROJECT_VERSION: u32 = 4;

/// Current DSP formula generation. Reserved for versioned render dispatch.
pub const DSP_VERSION: u32 = 2;

fn default_project_version() -> u32 {
    1
}

fn default_dsp_version() -> u32 {
    DSP_VERSION
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    Saw,
    Pulse,
    Square,
    Noise,
    Arp,
    Fm,
    Pluck,
    Sample,
    Kick,
    Snare,
    Hat,
    Clap,
}

impl Wave {
    pub const ALL: [Wave; 14] = [
        Wave::Sine,
        Wave::Triangle,
        Wave::Saw,
        Wave::Pulse,
        Wave::Square,
        Wave::Noise,
        Wave::Arp,
        Wave::Fm,
        Wave::Pluck,
        Wave::Sample,
        Wave::Kick,
        Wave::Snare,
        Wave::Hat,
        Wave::Clap,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Wave::Sine => "SINE",
            Wave::Triangle => "TRI",
            Wave::Saw => "SAW",
            Wave::Pulse => "PULSE",
            Wave::Square => "SQR",
            Wave::Noise => "NOISE",
            Wave::Arp => "ARP",
            Wave::Fm => "FM",
            Wave::Pluck => "PLUCK",
            Wave::Sample => "SAMPLE",
            Wave::Kick => "KICK",
            Wave::Snare => "SNARE",
            Wave::Hat => "HAT",
            Wave::Clap => "CLAP",
        }
    }

    pub fn is_tone(self) -> bool {
        matches!(
            self,
            Wave::Sine | Wave::Triangle | Wave::Saw | Wave::Pulse | Wave::Square
        )
    }

    pub fn is_drum(self) -> bool {
        matches!(self, Wave::Kick | Wave::Snare | Wave::Hat | Wave::Clap)
    }

    /// Godot `SfxLayer.Wave` integer (their enum order).
    pub fn godot_id(self) -> u32 {
        match self {
            Wave::Sine => 0,
            Wave::Triangle => 1,
            Wave::Saw => 2,
            Wave::Pulse => 3,
            Wave::Square => 4,
            Wave::Noise => 5,
            Wave::Arp => 6,
            Wave::Fm => 7,
            Wave::Pluck => 8,
            Wave::Sample => 9,
            Wave::Kick => 10,
            Wave::Snare => 11,
            Wave::Hat => 12,
            Wave::Clap => 13,
        }
    }

    pub fn from_godot_id(id: u32) -> Option<Self> {
        Some(match id {
            0 => Wave::Sine,
            1 => Wave::Triangle,
            2 => Wave::Saw,
            3 => Wave::Pulse,
            4 => Wave::Square,
            5 => Wave::Noise,
            6 => Wave::Arp,
            7 => Wave::Fm,
            8 => Wave::Pluck,
            // Godot SAMPLE layers embed an AudioStreamWAV; the Rust client
            // imports them as Pluck (see docs/PARITY.md D4).
            9 => Wave::Pluck,
            10 => Wave::Kick,
            11 => Wave::Snare,
            12 => Wave::Hat,
            13 => Wave::Clap,
            _ => return None,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Impact,
    Laser,
    Ui,
    Movement,
    Magic,
    Environment,
    Combat,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Impact,
        Category::Laser,
        Category::Ui,
        Category::Movement,
        Category::Magic,
        Category::Environment,
        Category::Combat,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Impact => "Impact",
            Category::Laser => "Laser",
            Category::Ui => "UI",
            Category::Movement => "Movement",
            Category::Magic => "Magic",
            Category::Environment => "Environment",
            Category::Combat => "Combat",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Category::Impact => "IMP",
            Category::Laser => "LASER",
            Category::Ui => "UI",
            Category::Movement => "MOVE",
            Category::Magic => "MAGIC",
            Category::Environment => "ENV",
            Category::Combat => "COMBAT",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// Stable, human-readable group name shared with the Godot `SfxPatch.group`.
    pub fn group(self) -> &'static str {
        self.label()
    }

    /// Parse a `.tres` group back into an app category (case-insensitive).
    pub fn from_group(group: &str) -> Option<Self> {
        let group = group.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|category| category.label().eq_ignore_ascii_case(group))
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToneEnv {
    #[default]
    Adsr,
    Exp,
}

impl ToneEnv {
    pub fn label(self) -> &'static str {
        match self {
            Self::Adsr => "ADSR",
            Self::Exp => "EXP",
        }
    }

    pub fn godot_id(self) -> u32 {
        match self {
            Self::Adsr => 0,
            Self::Exp => 1,
        }
    }

    pub fn from_godot_id(id: u32) -> Self {
        if id == 1 {
            Self::Exp
        } else {
            Self::Adsr
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    #[default]
    None,
    Lowpass,
    Highpass,
    Bandpass,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnvShape {
    #[default]
    Exp,
    Sine,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum MusicWave {
    #[default]
    Sine,
    Triangle,
    Saw,
    Pulse,
    Square,
}

impl MusicWave {
    pub const ALL: [MusicWave; 5] = [
        MusicWave::Sine,
        MusicWave::Triangle,
        MusicWave::Saw,
        MusicWave::Pulse,
        MusicWave::Square,
    ];
    pub fn label(self) -> &'static str {
        match self {
            MusicWave::Sine => "SINE",
            MusicWave::Triangle => "TRI",
            MusicWave::Saw => "SAW",
            MusicWave::Pulse => "PULSE",
            MusicWave::Square => "SQR",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct AutomationPoint {
    /// Seconds relative to the clip start.
    pub time: f32,
    /// Gain multiplier (1.0 = unchanged).
    pub value: f32,
}

impl Default for AutomationPoint {
    fn default() -> Self {
        Self {
            time: 0.0,
            value: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Clip {
    /// Stable identity within the project; assigned on load/edit.
    #[serde(default)]
    pub id: u32,
    pub start: f32,
    pub duration: f32,
    /// Multiplier on the layer gain (1.0 = unchanged).
    pub gain: f32,
    /// Semitone transposition applied to this clip only.
    pub pitch: f32,
    /// Optional gain envelope, relative to the clip start. Empty = flat gain.
    pub gain_points: Vec<AutomationPoint>,
    /// Optional low-pass cutoff envelope (Hz), relative to the clip start.
    pub cutoff_points: Vec<AutomationPoint>,
}

impl Default for Clip {
    fn default() -> Self {
        Self {
            id: 0,
            start: 0.0,
            duration: 0.15,
            gain: 1.0,
            pitch: 0.0,
            gain_points: Vec::new(),
            cutoff_points: Vec::new(),
        }
    }
}

impl Clip {
    pub fn end(&self) -> f32 {
        self.start + self.duration
    }

    pub fn has_automation(&self) -> bool {
        !self.gain_points.is_empty() || !self.cutoff_points.is_empty()
    }

    pub fn has_cutoff_automation(&self) -> bool {
        !self.cutoff_points.is_empty()
    }

    fn sanitize(&mut self) {
        self.start = finite(self.start, 0.0).clamp(0.0, 8.0);
        self.duration = finite(self.duration, 0.15).clamp(0.005, 3.0);
        self.gain = finite(self.gain, 1.0).clamp(0.0, 2.0);
        self.pitch = finite(self.pitch, 0.0).clamp(-24.0, 24.0);
        sanitize_points(&mut self.gain_points, 0.0, 2.0, self.duration);
        sanitize_points(&mut self.cutoff_points, 20.0, 18000.0, self.duration);
    }

    /// Linear gain envelope at `t` seconds inside the clip.
    pub fn gain_at(&self, t: f32) -> f32 {
        if self.gain_points.is_empty() {
            return 1.0;
        }
        let t = t.clamp(0.0, self.duration.max(0.0));
        let first = self.gain_points[0];
        if t <= first.time {
            return first.value;
        }
        for pair in self.gain_points.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if t <= b.time {
                let span = (b.time - a.time).max(0.0001);
                let k = ((t - a.time) / span).clamp(0.0, 1.0);
                return a.value + (b.value - a.value) * k;
            }
        }
        self.gain_points
            .last()
            .map(|point| point.value)
            .unwrap_or(1.0)
    }

    pub fn add_point(&mut self, kind: AutomationKind, time: f32, value: f32) -> usize {
        let (min, max, _) = kind.range();
        let point = AutomationPoint {
            time: time.clamp(0.0, self.duration),
            value: value.clamp(min, max),
        };
        let points = kind.points_mut(self);
        let index = points
            .iter()
            .position(|p| p.time > point.time)
            .unwrap_or(points.len());
        points.insert(index, point);
        index
    }

    pub fn cutoff_at(&self, t: f32) -> f32 {
        if self.cutoff_points.is_empty() {
            return 18000.0;
        }
        let t = t.clamp(0.0, self.duration.max(0.0));
        let first = self.cutoff_points[0];
        if t <= first.time {
            return first.value;
        }
        for pair in self.cutoff_points.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if t <= b.time {
                let span = (b.time - a.time).max(0.0001);
                let k = ((t - a.time) / span).clamp(0.0, 1.0);
                return a.value + (b.value - a.value) * k;
            }
        }
        self.cutoff_points
            .last()
            .map(|point| point.value)
            .unwrap_or(18000.0)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Layer {
    /// Stable identity within the project; assigned on load/edit.
    #[serde(default)]
    pub id: u32,
    pub wave: Wave,
    pub enabled: bool,
    pub delay: f32,
    pub duration: f32,
    pub gain: f32,
    pub freq_start: f32,
    pub freq_end: f32,
    pub harsh: f32,
    pub duty: f32,
    pub vibrato_rate: f32,
    pub vibrato_depth: f32,
    /// Tone envelope shape. `Adsr` is the legacy/Godot default.
    pub tone_env: ToneEnv,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// Decay speed for `ToneEnv::Exp` (Godot `exp_decay`).
    pub exp_decay: f32,
    pub filter: FilterMode,
    pub cutoff_start: f32,
    pub cutoff_end: f32,
    pub resonance: f32,
    pub arp_freqs: Vec<f32>,
    pub step: f32,
    pub lowpass_from: f32,
    pub lowpass_to: f32,
    pub noise_gain: f32,
    pub sweep_gain: f32,
    pub env_shape: EnvShape,
    pub seed: u32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_decay: f32,
    pub pluck_damping: f32,
    pub pluck_brightness: f32,
    pub drum_tone: f32,
    pub drum_snap: f32,
    /// Empty = one implicit clip using `delay` / `duration` / layer gain.
    pub clips: Vec<Clip>,
    /// WAV path for `Wave::Sample` (relative to the project root when copied).
    #[serde(default)]
    pub sample_path: String,
    #[serde(default)]
    pub sample_pitch: f32,
    #[serde(default)]
    pub sample_loop: bool,
    /// Decoded samples; loaded from `sample_path` on open, never serialized.
    #[serde(skip)]
    pub sample_data: Option<crate::wav::SampleData>,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            id: 0,
            wave: Wave::Sine,
            sample_path: String::new(),
            sample_pitch: 0.0,
            sample_loop: false,
            sample_data: None,
            enabled: true,
            delay: 0.0,
            duration: 0.15,
            gain: 0.3,
            freq_start: 440.0,
            freq_end: 440.0,
            harsh: 0.0,
            duty: 0.5,
            vibrato_rate: 0.0,
            vibrato_depth: 0.0,
            tone_env: ToneEnv::Adsr,
            attack: 0.002,
            decay: 0.06,
            sustain: 0.0,
            release: 0.03,
            exp_decay: 4.0,
            filter: FilterMode::None,
            cutoff_start: 12000.0,
            cutoff_end: 12000.0,
            resonance: 0.707,
            arp_freqs: vec![523.25, 659.25, 783.99],
            step: 0.06,
            lowpass_from: 0.35,
            lowpass_to: 0.35,
            noise_gain: 0.85,
            sweep_gain: 0.45,
            env_shape: EnvShape::Exp,
            seed: 20240607,
            fm_ratio: 2.0,
            fm_index: 2.0,
            fm_decay: 4.0,
            pluck_damping: 0.996,
            pluck_brightness: 0.5,
            drum_tone: 1.0,
            drum_snap: 1.0,
            clips: Vec::new(),
        }
    }
}

impl Layer {
    pub fn new(wave: Wave) -> Self {
        Self {
            id: 0,
            sample_path: String::new(),
            sample_pitch: 0.0,
            sample_loop: false,
            sample_data: None,
            wave,
            ..Default::default()
        }
    }

    pub fn end(&self) -> f32 {
        if self.clips.is_empty() {
            self.delay + self.duration
        } else {
            self.clips.iter().map(Clip::end).fold(0.0, f32::max)
        }
    }

    pub fn implicit_clip(&self) -> Clip {
        Clip {
            id: 0,
            start: self.delay,
            duration: self.duration,
            gain: 1.0,
            pitch: 0.0,
            gain_points: Vec::new(),
            cutoff_points: Vec::new(),
        }
    }

    /// Materialize the implicit block before editing clips.
    pub fn ensure_clips(&mut self) {
        if self.clips.is_empty() {
            self.clips.push(self.implicit_clip());
        }
    }

    /// A copy transposed by semitones. Drums use `drum_tone` as pitch.
    pub fn with_pitch(&self, semitones: f32) -> Self {
        let mut copy = self.clone();
        if semitones.abs() < 0.001 {
            return copy;
        }
        let factor = 2f32.powf(semitones / 12.0);
        copy.freq_start = (copy.freq_start * factor).max(20.0);
        copy.freq_end = (copy.freq_end * factor).max(20.0);
        for freq in &mut copy.arp_freqs {
            *freq = (*freq * factor).max(20.0);
        }
        copy.drum_tone = (copy.drum_tone * factor).clamp(0.25, 4.0);
        copy
    }

    /// Clamp untrusted values loaded from JSON / `.tres` into the ranges the
    /// editor and DSP can handle. Never panics on NaN or absurd values.
    fn sanitize(&mut self) {
        self.delay = finite(self.delay, 0.0).clamp(0.0, 8.0);
        self.duration = finite(self.duration, 0.15).clamp(0.005, 3.0);
        self.gain = finite(self.gain, 0.3).clamp(0.0, 1.0);
        self.freq_start = finite(self.freq_start, 440.0).clamp(20.0, 12000.0);
        self.freq_end = finite(self.freq_end, 440.0).clamp(20.0, 12000.0);
        self.harsh = finite(self.harsh, 0.0).clamp(0.0, 1.0);
        self.duty = finite(self.duty, 0.5).clamp(0.05, 0.95);
        self.vibrato_rate = finite(self.vibrato_rate, 0.0).clamp(0.0, 20.0);
        self.vibrato_depth = finite(self.vibrato_depth, 0.0).clamp(0.0, 12.0);
        self.attack = finite(self.attack, 0.002).clamp(0.0, 2.0);
        self.decay = finite(self.decay, 0.06).clamp(0.0, 3.0);
        self.sustain = finite(self.sustain, 0.0).clamp(0.0, 1.0);
        self.release = finite(self.release, 0.03).clamp(0.0, 3.0);
        self.exp_decay = finite(self.exp_decay, 4.0).clamp(0.5, 30.0);
        self.cutoff_start = finite(self.cutoff_start, 12000.0).clamp(20.0, 18000.0);
        self.cutoff_end = finite(self.cutoff_end, 12000.0).clamp(20.0, 18000.0);
        self.resonance = finite(self.resonance, 0.707).clamp(0.5, 5.0);
        self.step = finite(self.step, 0.06).clamp(0.01, 0.5);
        self.lowpass_from = finite(self.lowpass_from, 0.35).clamp(0.01, 1.0);
        self.lowpass_to = finite(self.lowpass_to, 0.35).clamp(0.01, 1.0);
        self.noise_gain = finite(self.noise_gain, 0.85).clamp(0.0, 1.0);
        self.sweep_gain = finite(self.sweep_gain, 0.45).clamp(0.0, 1.0);
        self.fm_ratio = finite(self.fm_ratio, 2.0).clamp(0.25, 12.0);
        self.fm_index = finite(self.fm_index, 2.0).clamp(0.0, 20.0);
        self.fm_decay = finite(self.fm_decay, 4.0).clamp(0.5, 30.0);
        self.pluck_damping = finite(self.pluck_damping, 0.996).clamp(0.9, 0.9999);
        self.pluck_brightness = finite(self.pluck_brightness, 0.5).clamp(0.0, 1.0);
        self.drum_tone = finite(self.drum_tone, 1.0).clamp(0.25, 4.0);
        self.drum_snap = finite(self.drum_snap, 1.0).clamp(0.0, 2.0);
        self.seed = self.seed.min(99_999_999);

        self.arp_freqs
            .retain(|freq| freq.is_finite() && *freq > 0.0);
        for freq in &mut self.arp_freqs {
            *freq = freq.clamp(20.0, 12000.0);
        }
        self.arp_freqs.truncate(16);
        if self.wave == Wave::Arp && self.arp_freqs.is_empty() {
            self.arp_freqs = vec![523.25, 659.25, 783.99];
        }

        self.clips.truncate(64);
        for clip in &mut self.clips {
            clip.sanitize();
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Patch {
    /// Stable identity within the project; assigned on load/edit.
    #[serde(default)]
    pub id: u32,
    pub name: String,
    pub category: Category,
    pub favorite: bool,
    pub rate: u32,
    pub normalize: bool,
    /// DSP formula generation reserved for future versioned rendering.
    #[serde(default = "default_dsp_version")]
    pub dsp_version: u32,
    pub layers: Vec<Layer>,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            category: Category::Impact,
            favorite: false,
            rate: 22050,
            normalize: true,
            dsp_version: DSP_VERSION,
            layers: vec![Layer::default()],
        }
    }
}

impl Patch {
    pub fn new_layer(name: impl Into<String>, wave: Wave) -> Self {
        Self {
            name: name.into(),
            layers: vec![Layer::new(wave)],
            ..Self::default()
        }
    }

    pub fn end(&self) -> f32 {
        self.layers.iter().map(Layer::end).fold(0.0, f32::max)
    }

    pub fn safe_name(&self) -> String {
        sanitize(&self.name)
    }

    pub fn has_automation(&self) -> bool {
        self.layers
            .iter()
            .flat_map(|layer| layer.clips.iter())
            .any(|clip| clip.has_automation())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Music {
    pub bpm: f32,
    pub bars: u32,
    pub rate: u32,
    pub volume_db: f32,
    pub seed: u32,
    pub bass_notes: Vec<f32>,
    pub chords: Vec<Vec<f32>>,
    pub bass_wave: MusicWave,
    pub arp_wave: MusicWave,
    pub swing: f32,
    pub use_patterns: bool,
    pub arp_pattern: Vec<i32>,
    pub bass_pattern: Vec<i32>,
    pub kick_steps: u32,
    pub hat_steps: u32,
    pub bass_gain: f32,
    pub bass_harsh: f32,
    pub bass_decay: f32,
    pub arp_gain: f32,
    pub arp_harsh: f32,
    pub arp_decay: f32,
    pub kick_gain: f32,
    pub hat_gain: f32,
    pub normalize: bool,
}

impl Default for Music {
    fn default() -> Self {
        Self {
            bpm: 132.0,
            bars: 4,
            rate: 11025,
            volume_db: -17.0,
            seed: 20240607,
            bass_notes: vec![110.0, 87.31, 130.81, 98.0],
            chords: vec![
                vec![220.0, 261.63, 329.63],
                vec![174.61, 220.0, 261.63],
                vec![261.63, 329.63, 392.0],
                vec![196.0, 246.94, 293.66],
            ],
            bass_wave: MusicWave::Sine,
            arp_wave: MusicWave::Sine,
            swing: 0.0,
            use_patterns: false,
            arp_pattern: (0..16).map(|i| i % 3).collect(),
            bass_pattern: vec![0, -1, 0, -1, 0, -1, 1, -1, 0, -1, 0, -1, 0, -1, 1, -1],
            kick_steps: 0x0101,
            hat_steps: 0x4444,
            bass_gain: 0.26,
            bass_harsh: 1.0,
            bass_decay: 2.2,
            arp_gain: 0.085,
            arp_harsh: 0.25,
            arp_decay: 9.0,
            kick_gain: 0.45,
            hat_gain: 0.055,
            normalize: true,
        }
    }
}

impl Music {
    pub fn length_seconds(&self) -> f32 {
        60.0 / self.bpm * 4.0 * self.bars as f32
    }

    pub fn transpose(&mut self, semitones: i32) {
        let factor = 2f32.powf(semitones as f32 / 12.0);
        for note in &mut self.bass_notes {
            *note *= factor;
        }
        for chord in &mut self.chords {
            for note in chord {
                *note *= factor;
            }
        }
    }

    pub fn reset_patterns(&mut self) {
        let tones = self
            .chords
            .first()
            .map(|c| c.len().clamp(1, 4))
            .unwrap_or(3);
        self.arp_pattern = (0..16).map(|i| (i % tones) as i32).collect();
        self.bass_pattern = vec![0, -1, 0, -1, 0, -1, 1, -1, 0, -1, 0, -1, 0, -1, 1, -1];
        self.kick_steps = 0x0101;
        self.hat_steps = 0x4444;
        self.use_patterns = true;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    /// On-disk schema version. Files written before versioning default to 1.
    #[serde(default = "default_project_version")]
    pub version: u32,
    /// Monotonic ID source for patches/layers/clips.
    #[serde(default)]
    pub next_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub music: Music,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: PROJECT_VERSION,
            next_id: 1,
            name: String::new(),
            patches: Vec::new(),
            music: Music::default(),
        }
    }
}

fn assign_id(id: &mut u32, seen: &mut Vec<u32>, next: &mut u32) {
    if *id == 0 || seen.contains(id) {
        while seen.contains(next) {
            *next += 1;
        }
        *id = *next;
        *next += 1;
    }
    seen.push(*id);
}

impl Project {
    pub fn demo(name: impl Into<String>) -> Self {
        let mut project = Self {
            version: PROJECT_VERSION,
            next_id: 1,
            name: name.into(),
            patches: Vec::new(),
            music: Music::default(),
        };

        // The five layered presets that ship with SFX LAB.
        let mut creak = Layer::new(Wave::Noise);
        creak.duration = 0.92;
        creak.gain = 0.45;
        creak.freq_start = 260.0;
        creak.freq_end = 480.0;
        creak.lowpass_from = 0.05;
        creak.lowpass_to = 0.20;
        creak.noise_gain = 0.85;
        creak.sweep_gain = 0.10;
        creak.env_shape = EnvShape::Sine;
        creak.seed = 7101;
        let mut string = Layer::new(Wave::Saw);
        string.delay = 0.02;
        string.duration = 0.86;
        string.gain = 0.18;
        string.freq_start = 85.0;
        string.freq_end = 165.0;
        string.filter = FilterMode::Lowpass;
        string.cutoff_start = 260.0;
        string.cutoff_end = 1600.0;
        string.resonance = 1.1;
        string.attack = 0.14;
        string.decay = 0.5;
        string.sustain = 0.25;
        string.release = 0.18;
        let mut friction = Layer::new(Wave::Noise);
        friction.delay = 0.24;
        friction.duration = 0.60;
        friction.gain = 0.16;
        friction.freq_start = 1200.0;
        friction.freq_end = 2200.0;
        friction.lowpass_from = 0.30;
        friction.lowpass_to = 0.55;
        friction.noise_gain = 0.9;
        friction.sweep_gain = 0.0;
        friction.env_shape = EnvShape::Sine;
        friction.seed = 7102;
        project.patches.push(patch_with_layers(
            "bow_draw",
            Category::Combat,
            vec![creak, string, friction],
        ));

        let mut snap = Layer::new(Wave::Saw);
        snap.duration = 0.09;
        snap.gain = 0.34;
        snap.freq_start = 760.0;
        snap.freq_end = 120.0;
        snap.filter = FilterMode::Lowpass;
        snap.cutoff_start = 5000.0;
        snap.cutoff_end = 700.0;
        snap.attack = 0.0005;
        snap.decay = 0.06;
        snap.release = 0.02;
        let mut twang = Layer::new(Wave::Pulse);
        twang.duty = 0.35;
        twang.delay = 0.005;
        twang.duration = 0.07;
        twang.gain = 0.12;
        twang.freq_start = 1200.0;
        twang.freq_end = 260.0;
        twang.attack = 0.0005;
        twang.decay = 0.05;
        twang.release = 0.015;
        let mut release_thump = Layer::new(Wave::Kick);
        release_thump.duration = 0.12;
        release_thump.gain = 0.18;
        release_thump.drum_tone = 1.3;
        let mut whoosh = Layer::new(Wave::Noise);
        whoosh.delay = 0.015;
        whoosh.duration = 0.30;
        whoosh.gain = 0.26;
        whoosh.freq_start = 700.0;
        whoosh.freq_end = 140.0;
        whoosh.lowpass_from = 0.45;
        whoosh.lowpass_to = 0.07;
        whoosh.noise_gain = 0.8;
        whoosh.sweep_gain = 0.25;
        whoosh.env_shape = EnvShape::Sine;
        whoosh.seed = 7201;
        let mut air = Layer::new(Wave::Noise);
        air.delay = 0.03;
        air.duration = 0.20;
        air.gain = 0.10;
        air.freq_start = 1600.0;
        air.freq_end = 500.0;
        air.lowpass_from = 0.55;
        air.lowpass_to = 0.15;
        air.noise_gain = 0.9;
        air.sweep_gain = 0.05;
        air.env_shape = EnvShape::Exp;
        air.decay = 7.0;
        air.seed = 7202;
        project.patches.push(patch_with_layers(
            "arrow_shot",
            Category::Combat,
            vec![snap, twang, release_thump, whoosh, air],
        ));

        let mut thud = Layer::new(Wave::Kick);
        thud.duration = 0.28;
        thud.gain = 0.55;
        let mut crack = Layer::new(Wave::Noise);
        crack.duration = 0.16;
        crack.gain = 0.38;
        crack.freq_start = 2600.0;
        crack.freq_end = 500.0;
        crack.lowpass_from = 0.60;
        crack.lowpass_to = 0.15;
        crack.noise_gain = 0.92;
        crack.sweep_gain = 0.08;
        crack.env_shape = EnvShape::Exp;
        crack.decay = 11.0;
        crack.seed = 7301;
        let mut clang = Layer::new(Wave::Fm);
        clang.duration = 0.12;
        clang.gain = 0.13;
        clang.freq_start = 320.0;
        clang.freq_end = 190.0;
        clang.fm_ratio = 3.7;
        clang.fm_index = 2.4;
        clang.fm_decay = 5.0;
        clang.attack = 0.0005;
        clang.decay = 0.08;
        clang.release = 0.03;
        let mut sub = Layer::new(Wave::Sine);
        sub.duration = 0.32;
        sub.gain = 0.20;
        sub.freq_start = 90.0;
        sub.freq_end = 45.0;
        sub.attack = 0.002;
        sub.decay = 0.25;
        sub.release = 0.10;
        let mut hit = patch_with_layers(
            "hit_impact",
            Category::Impact,
            vec![thud, crack, clang, sub],
        );
        hit.favorite = true;
        project.patches.push(hit);

        let mut boom = Layer::new(Wave::Kick);
        boom.duration = 0.95;
        boom.gain = 0.55;
        boom.drum_tone = 0.85;
        let mut blast = Layer::new(Wave::Noise);
        blast.duration = 0.90;
        blast.gain = 0.55;
        blast.freq_start = 900.0;
        blast.freq_end = 70.0;
        blast.lowpass_from = 0.55;
        blast.lowpass_to = 0.10;
        blast.noise_gain = 0.90;
        blast.sweep_gain = 0.30;
        blast.env_shape = EnvShape::Exp;
        blast.decay = 4.5;
        blast.seed = 7401;
        let mut crackle = Layer::new(Wave::Noise);
        crackle.delay = 0.08;
        crackle.duration = 0.70;
        crackle.gain = 0.20;
        crackle.freq_start = 3200.0;
        crackle.freq_end = 900.0;
        crackle.lowpass_from = 0.55;
        crackle.lowpass_to = 0.12;
        crackle.noise_gain = 0.95;
        crackle.sweep_gain = 0.0;
        crackle.env_shape = EnvShape::Exp;
        crackle.decay = 8.0;
        crackle.seed = 7402;
        let mut debris = Layer::new(Wave::Noise);
        debris.delay = 0.25;
        debris.duration = 0.80;
        debris.gain = 0.12;
        debris.freq_start = 1200.0;
        debris.freq_end = 200.0;
        debris.lowpass_from = 0.18;
        debris.lowpass_to = 0.04;
        debris.noise_gain = 0.9;
        debris.sweep_gain = 0.0;
        debris.env_shape = EnvShape::Sine;
        debris.seed = 7403;
        let mut rumble = Layer::new(Wave::Sine);
        rumble.duration = 0.70;
        rumble.gain = 0.28;
        rumble.freq_start = 70.0;
        rumble.freq_end = 28.0;
        rumble.attack = 0.003;
        rumble.decay = 0.40;
        rumble.release = 0.15;
        let mut explosion = patch_with_layers(
            "explosion",
            Category::Impact,
            vec![boom, blast, crackle, debris, rumble],
        );
        explosion.favorite = true;
        project.patches.push(explosion);

        let mut low_wind = Layer::new(Wave::Noise);
        low_wind.duration = 2.20;
        low_wind.gain = 0.45;
        low_wind.freq_start = 110.0;
        low_wind.freq_end = 70.0;
        low_wind.lowpass_from = 0.07;
        low_wind.lowpass_to = 0.03;
        low_wind.noise_gain = 0.9;
        low_wind.sweep_gain = 0.04;
        low_wind.env_shape = EnvShape::Sine;
        low_wind.seed = 7501;
        let mut mid_wind = Layer::new(Wave::Noise);
        mid_wind.delay = 0.15;
        mid_wind.duration = 1.90;
        mid_wind.gain = 0.32;
        mid_wind.freq_start = 500.0;
        mid_wind.freq_end = 220.0;
        mid_wind.lowpass_from = 0.25;
        mid_wind.lowpass_to = 0.08;
        mid_wind.noise_gain = 0.9;
        mid_wind.sweep_gain = 0.03;
        mid_wind.env_shape = EnvShape::Sine;
        mid_wind.seed = 7502;
        let mut hiss = Layer::new(Wave::Noise);
        hiss.delay = 0.35;
        hiss.duration = 1.60;
        hiss.gain = 0.16;
        hiss.freq_start = 1800.0;
        hiss.freq_end = 900.0;
        hiss.lowpass_from = 0.55;
        hiss.lowpass_to = 0.25;
        hiss.noise_gain = 0.9;
        hiss.sweep_gain = 0.0;
        hiss.env_shape = EnvShape::Sine;
        hiss.seed = 7503;
        let mut howl = Layer::new(Wave::Triangle);
        howl.delay = 0.20;
        howl.duration = 2.0;
        howl.gain = 0.040;
        howl.freq_start = 320.0;
        howl.freq_end = 420.0;
        howl.vibrato_rate = 0.18;
        howl.vibrato_depth = 1.2;
        howl.attack = 0.4;
        howl.decay = 1.0;
        howl.sustain = 0.4;
        howl.release = 0.4;
        howl.filter = FilterMode::Lowpass;
        howl.cutoff_start = 900.0;
        howl.cutoff_end = 1400.0;
        let mut wind = patch_with_layers(
            "wind",
            Category::Environment,
            vec![low_wind, mid_wind, hiss, howl],
        );
        wind.favorite = true;
        project.patches.push(wind);

        project
            .patches
            .push(make_patch("blip_up", Wave::Sine, Category::Ui, |l| {
                l.freq_start = 620.0;
                l.freq_end = 980.0;
                l.duration = 0.085;
                l.gain = 0.38;
                l.harsh = 0.25;
                l.decay = 0.04;
                l.release = 0.01;
            }));
        let mut laser = make_patch("laser", Wave::Pulse, Category::Laser, |l| {
            l.duty = 0.25;
            l.freq_start = 900.0;
            l.freq_end = 180.0;
            l.duration = 0.18;
            l.gain = 0.32;
            l.attack = 0.001;
            l.decay = 0.12;
            l.release = 0.02;
        });
        laser.favorite = true;
        project.patches.push(laser);
        project
            .patches
            .push(make_patch("coin", Wave::Square, Category::Ui, |l| {
                l.freq_start = 988.0;
                l.freq_end = 1319.0;
                l.duration = 0.11;
                l.gain = 0.24;
                l.decay = 0.05;
            }));
        project
            .patches
            .push(make_patch("boom", Wave::Noise, Category::Impact, |l| {
                l.duration = 0.5;
                l.gain = 0.6;
                l.freq_start = 800.0;
                l.freq_end = 90.0;
                l.lowpass_from = 0.5;
                l.lowpass_to = 0.15;
                l.noise_gain = 0.9;
                l.sweep_gain = 0.35;
                l.decay = 5.0;
            }));
        project
            .patches
            .push(make_patch("click", Wave::Triangle, Category::Ui, |l| {
                l.freq_start = 1200.0;
                l.freq_end = 700.0;
                l.duration = 0.03;
                l.gain = 0.16;
                l.attack = 0.0005;
                l.decay = 0.02;
                l.release = 0.005;
            }));
        project
            .patches
            .push(make_patch("pluck", Wave::Pluck, Category::Magic, |l| {
                l.freq_start = 330.0;
                l.duration = 0.5;
                l.gain = 0.45;
                l.pluck_damping = 0.996;
                l.pluck_brightness = 0.6;
                l.decay = 0.3;
                l.release = 0.1;
            }));
        project.ensure_ids();
        project
    }

    /// Assign stable IDs to any patch/layer/clip that lacks one (or repeats
    /// an existing ID). Safe to call after every structural edit.
    pub fn ensure_ids(&mut self) {
        let mut seen: Vec<u32> = Vec::new();
        let mut next = self.next_id.max(1);
        for patch in &mut self.patches {
            assign_id(&mut patch.id, &mut seen, &mut next);
            for layer in &mut patch.layers {
                assign_id(&mut layer.id, &mut seen, &mut next);
                for clip in &mut layer.clips {
                    assign_id(&mut clip.id, &mut seen, &mut next);
                }
            }
        }
        self.next_id = next.max(self.next_id);
    }

    /// Decode every `sample_path` into `sample_data`. Paths are resolved
    /// relative to `root` (the project folder); absolute paths still work.
    pub fn load_samples(&mut self, root: &Path) -> Vec<String> {
        let mut warnings = Vec::new();
        for patch in &mut self.patches {
            for layer in &mut patch.layers {
                layer.sample_data = None;
                if layer.sample_path.is_empty() {
                    continue;
                }
                let path = Path::new(&layer.sample_path);
                let resolved = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    root.join(path)
                };
                match crate::wav::load(&resolved) {
                    Ok(data) => layer.sample_data = Some(data),
                    Err(error) => warnings.push(format!(
                        "{}: sample {} unreadable ({error})",
                        patch.name, layer.sample_path
                    )),
                }
            }
        }
        warnings
    }

    pub fn next_patch_name(&self) -> String {
        let mut index = self.patches.len() + 1;
        loop {
            let name = format!("patch_{index:02}");
            if !self.patches.iter().any(|p| p.name == name) {
                return name;
            }
            index += 1;
        }
    }
}

fn make_patch(name: &str, wave: Wave, category: Category, edit: impl FnOnce(&mut Layer)) -> Patch {
    let mut layer = Layer::new(wave);
    edit(&mut layer);
    Patch {
        name: name.into(),
        category,
        layers: vec![layer],
        ..Patch::default()
    }
}

fn patch_with_layers(name: &str, category: Category, layers: Vec<Layer>) -> Patch {
    Patch {
        name: name.into(),
        category,
        layers,
        ..Patch::default()
    }
}

pub fn sanitize(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').chars().take(48).collect::<String>();
    let trimmed = trimmed.trim_matches('_').to_string();
    // A name made only of non-ASCII characters and digits would otherwise
    // collapse to "" or "2", causing every such patch to share a file name.
    if trimmed.is_empty() || !trimmed.chars().any(|ch| ch.is_ascii_alphabetic()) {
        "sound".into()
    } else {
        trimmed
    }
}

pub fn load_project(root: &Path) -> Result<Option<Project>, String> {
    let path = root.join(PROJECT_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let mut project: Project = serde_json::from_str(&text)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            project.migrate();
            Ok(Some(project))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

pub fn save_project(root: &Path, project: &Project) -> std::io::Result<()> {
    let path = root.join(PROJECT_FILE);
    let text = serde_json::to_string_pretty(project)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    if path.exists() {
        let _ = std::fs::copy(&path, path.with_extension("json.bak"));
    }
    write_atomic(&path, text.as_bytes())
}

/// Write through a sibling temp file + rename so a crash cannot truncate a
/// project or an exported asset.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn sanitize_points(points: &mut Vec<AutomationPoint>, low: f32, high: f32, max_time: f32) {
    points.retain(|point| point.time.is_finite() && point.value.is_finite());
    for point in points.iter_mut() {
        point.time = point.time.clamp(0.0, max_time.max(0.0));
        point.value = point.value.clamp(low, high);
    }
    points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points.dedup_by(|a, b| (a.time - b.time).abs() < 0.0005);
    points.truncate(512);
}

impl Music {
    fn sanitize(&mut self) {
        self.bpm = finite(self.bpm, 132.0).clamp(40.0, 240.0);
        self.bars = self.bars.clamp(1, 32);
        self.rate = self.rate.clamp(8000, 48000);
        self.volume_db = finite(self.volume_db, -17.0).clamp(-60.0, 6.0);
        self.swing = finite(self.swing, 0.0).clamp(0.0, 0.5);
        self.seed = self.seed.min(99_999_999);
        for value in [
            &mut self.bass_gain,
            &mut self.bass_harsh,
            &mut self.arp_gain,
            &mut self.arp_harsh,
            &mut self.kick_gain,
            &mut self.hat_gain,
        ] {
            *value = finite(*value, 0.0).clamp(0.0, 1.0);
        }
        self.bass_decay = finite(self.bass_decay, 2.2).clamp(0.5, 30.0);
        self.arp_decay = finite(self.arp_decay, 9.0).clamp(0.5, 30.0);

        self.bass_notes
            .retain(|note| note.is_finite() && *note > 0.0);
        for note in &mut self.bass_notes {
            *note = note.clamp(20.0, 12000.0);
        }
        self.bass_notes.truncate(64);
        self.chords.truncate(64);
        for chord in &mut self.chords {
            chord.retain(|note| note.is_finite() && *note > 0.0);
            for note in chord.iter_mut() {
                *note = note.clamp(20.0, 12000.0);
            }
            chord.truncate(8);
        }
        self.chords.retain(|chord| !chord.is_empty());

        for pattern in [&mut self.arp_pattern, &mut self.bass_pattern] {
            pattern.truncate(16);
            pattern.resize(16, -1);
        }
        self.kick_steps &= 0xFFFF;
        self.hat_steps &= 0xFFFF;
    }
}

impl Project {
    /// Bring an older file up to [`PROJECT_VERSION`].
    pub fn migrate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.version < PROJECT_VERSION {
            warnings.push(format!(
                "project schema {} → {PROJECT_VERSION}",
                self.version
            ));
            self.version = PROJECT_VERSION;
        }
        for patch in &mut self.patches {
            if patch.dsp_version == 0 {
                patch.dsp_version = DSP_VERSION;
            }
        }
        self.ensure_ids();
        warnings
    }

    /// Clamp untrusted data and guarantee unique patch names. Returns a short
    /// list of structural repairs the UI can surface.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        self.ensure_ids();
        if self.name.trim().is_empty() {
            self.name = "untitled".into();
        }
        if self.music.bass_notes.is_empty() || self.music.chords.is_empty() {
            warnings.push("music has no notes/chords".into());
        }
        self.music.sanitize();

        if self.patches.len() > 512 {
            warnings.push("library truncated to 512 patches".into());
            self.patches.truncate(512);
        }

        let mut seen = std::collections::HashSet::new();
        for (index, patch) in self.patches.iter_mut().enumerate() {
            patch.name = patch
                .name
                .chars()
                .map(|ch| if ch.is_control() { ' ' } else { ch })
                .collect::<String>()
                .trim()
                .to_string();
            if patch.name.is_empty() {
                patch.name = format!("patch_{:02}", index + 1);
            }
            if patch.name.chars().count() > 64 {
                patch.name = patch.name.chars().take(64).collect();
                warnings.push(format!("patch {} name truncated to 64 chars", index + 1));
            }
            let base = patch.name.clone();
            let mut candidate = base.clone();
            let mut suffix = 2;
            while !seen.insert(candidate.to_lowercase()) {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            if candidate != base {
                warnings.push(format!("renamed duplicate patch '{base}' → '{candidate}'"));
                patch.name = candidate;
            }
            patch.rate = patch.rate.clamp(8000, 48000);
            if patch.dsp_version == 0 {
                patch.dsp_version = DSP_VERSION;
            }
            if patch.layers.is_empty() {
                patch.layers.push(Layer::default());
            }
            if patch.layers.len() > 32 {
                warnings.push(format!("{}: layers truncated to 32", patch.name));
                patch.layers.truncate(32);
            }
            for layer in &mut patch.layers {
                layer.sanitize();
            }
        }
        warnings
    }
}

/// How a parameter maps to a 0..1 slider track.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    Linear,
    Log,
}

/// Single source of truth for a parameter's label, unit, range and mapping.
/// The UI slider, mouse-to-value conversion and formatting all use this.
#[derive(Clone, Copy, Debug)]
pub struct FieldSpec {
    pub label: &'static str,
    pub unit: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub scale: Scale,
}

impl FieldSpec {
    pub const fn new(
        label: &'static str,
        unit: &'static str,
        min: f32,
        max: f32,
        step: f32,
        scale: Scale,
    ) -> Self {
        Self {
            label,
            unit,
            min,
            max,
            step,
            scale,
        }
    }

    pub fn norm(&self, value: f32) -> f32 {
        match self.scale {
            Scale::Linear => {
                ((value - self.min) / (self.max - self.min).max(0.0001)).clamp(0.0, 1.0)
            }
            Scale::Log if self.min > 0.0 && self.max > self.min => {
                let low = self.min.ln();
                let high = self.max.ln();
                ((value.clamp(self.min, self.max).max(1e-6).ln() - low) / (high - low))
                    .clamp(0.0, 1.0)
            }
            Scale::Log => self.norm_linear(value),
        }
    }

    fn norm_linear(&self, value: f32) -> f32 {
        ((value - self.min) / (self.max - self.min).max(0.0001)).clamp(0.0, 1.0)
    }

    pub fn denorm(&self, norm: f32) -> f32 {
        let norm = norm.clamp(0.0, 1.0);
        match self.scale {
            Scale::Linear => self.min + norm * (self.max - self.min),
            Scale::Log if self.min > 0.0 && self.max > self.min => {
                (self.min.ln() + norm * (self.max.ln() - self.min.ln())).exp()
            }
            Scale::Log => self.min + norm * (self.max - self.min),
        }
    }

    pub fn snap(&self, value: f32) -> f32 {
        if self.step > 0.0 {
            (value / self.step).round() * self.step
        } else {
            value
        }
    }

    pub fn format(&self, value: f32) -> String {
        let decimals = if self.step >= 1.0 {
            0
        } else if self.step >= 0.01 {
            2
        } else {
            3
        };
        if self.unit.is_empty() {
            format!("{value:.decimals$}")
        } else {
            format!("{value:.decimals$}{}", self.unit)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerField {
    Delay,
    Duration,
    Gain,
    FreqStart,
    FreqEnd,
    Harsh,
    Duty,
    VibratoRate,
    VibratoDepth,
    Attack,
    Decay,
    Sustain,
    Release,
    ExpDecay,
    CutoffStart,
    CutoffEnd,
    Resonance,
    Step,
    LowpassFrom,
    LowpassTo,
    NoiseGain,
    SweepGain,
    FmRatio,
    FmIndex,
    FmDecay,
    PluckDamping,
    PluckBrightness,
    SamplePitch,
    DrumTone,
    DrumSnap,
    Seed,
    Arp0,
    Arp1,
    Arp2,
    Arp3,
}

impl LayerField {
    pub fn spec(self) -> FieldSpec {
        use Scale::{Linear, Log};
        match self {
            Self::Delay => FieldSpec::new("DELAY", "s", 0.0, 2.0, 0.001, Linear),
            Self::Duration => FieldSpec::new("LENGTH", "s", 0.005, 3.0, 0.001, Linear),
            Self::Gain => FieldSpec::new("GAIN", "", 0.0, 1.0, 0.001, Linear),
            Self::FreqStart => FieldSpec::new("FREQ A", "Hz", 20.0, 12000.0, 1.0, Log),
            Self::FreqEnd => FieldSpec::new("FREQ B", "Hz", 20.0, 12000.0, 1.0, Log),
            Self::Harsh => FieldSpec::new("HARSH", "", 0.0, 1.0, 0.001, Linear),
            Self::Duty => FieldSpec::new("DUTY", "", 0.05, 0.95, 0.01, Linear),
            Self::VibratoRate => FieldSpec::new("VIB RATE", "Hz", 0.0, 20.0, 0.1, Linear),
            Self::VibratoDepth => FieldSpec::new("VIB DEPTH", "", 0.0, 12.0, 0.01, Linear),
            Self::Attack => FieldSpec::new("ATTACK", "s", 0.0, 2.0, 0.001, Linear),
            Self::Decay => FieldSpec::new("DECAY", "s", 0.0, 3.0, 0.001, Linear),
            Self::Sustain => FieldSpec::new("SUSTAIN", "", 0.0, 1.0, 0.001, Linear),
            Self::Release => FieldSpec::new("RELEASE", "s", 0.0, 3.0, 0.001, Linear),
            Self::ExpDecay => FieldSpec::new("EXP DECAY", "", 0.5, 30.0, 0.1, Linear),
            Self::CutoffStart => FieldSpec::new("CUTOFF A", "Hz", 20.0, 18000.0, 1.0, Log),
            Self::CutoffEnd => FieldSpec::new("CUTOFF B", "Hz", 20.0, 18000.0, 1.0, Log),
            Self::Resonance => FieldSpec::new("RESONANCE", "", 0.5, 5.0, 0.01, Linear),
            Self::Step => FieldSpec::new("STEP", "s", 0.01, 0.5, 0.001, Linear),
            Self::LowpassFrom => FieldSpec::new("LP A", "", 0.01, 1.0, 0.001, Linear),
            Self::LowpassTo => FieldSpec::new("LP B", "", 0.01, 1.0, 0.001, Linear),
            Self::NoiseGain => FieldSpec::new("NOISE", "", 0.0, 1.0, 0.001, Linear),
            Self::SweepGain => FieldSpec::new("SWEEP", "", 0.0, 1.0, 0.001, Linear),
            Self::FmRatio => FieldSpec::new("FM RATIO", "", 0.25, 12.0, 0.01, Linear),
            Self::FmIndex => FieldSpec::new("FM INDEX", "", 0.0, 20.0, 0.1, Linear),
            Self::FmDecay => FieldSpec::new("FM DECAY", "", 0.5, 30.0, 0.1, Linear),
            Self::PluckDamping => FieldSpec::new("DAMPING", "", 0.9, 0.9999, 0.0001, Linear),
            Self::PluckBrightness => FieldSpec::new("BRIGHT", "", 0.0, 1.0, 0.001, Linear),
            Self::SamplePitch => FieldSpec::new("PITCH", "st", -24.0, 24.0, 0.1, Linear),
            Self::DrumTone => FieldSpec::new("TONE", "", 0.25, 2.0, 0.01, Linear),
            Self::DrumSnap => FieldSpec::new("SNAP", "", 0.0, 2.0, 0.01, Linear),
            Self::Seed => FieldSpec::new("SEED", "", 0.0, 99_999_999.0, 1.0, Linear),
            Self::Arp0 => FieldSpec::new("ARP 1", "Hz", 20.0, 5000.0, 1.0, Log),
            Self::Arp1 => FieldSpec::new("ARP 2", "Hz", 20.0, 5000.0, 1.0, Log),
            Self::Arp2 => FieldSpec::new("ARP 3", "Hz", 20.0, 5000.0, 1.0, Log),
            Self::Arp3 => FieldSpec::new("ARP 4", "Hz", 20.0, 5000.0, 1.0, Log),
        }
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn range(self) -> (f32, f32, f32) {
        let spec = self.spec();
        (spec.min, spec.max, spec.step)
    }

    pub fn norm(self, value: f32) -> f32 {
        self.spec().norm(value)
    }

    pub fn denorm(self, norm: f32) -> f32 {
        self.spec().denorm(norm)
    }

    pub fn get(self, layer: &Layer) -> f32 {
        match self {
            Self::Delay => layer.delay,
            Self::Duration => layer.duration,
            Self::Gain => layer.gain,
            Self::FreqStart => layer.freq_start,
            Self::FreqEnd => layer.freq_end,
            Self::Harsh => layer.harsh,
            Self::Duty => layer.duty,
            Self::VibratoRate => layer.vibrato_rate,
            Self::VibratoDepth => layer.vibrato_depth,
            Self::Attack => layer.attack,
            Self::Decay => layer.decay,
            Self::Sustain => layer.sustain,
            Self::Release => layer.release,
            Self::ExpDecay => layer.exp_decay,
            Self::CutoffStart => layer.cutoff_start,
            Self::CutoffEnd => layer.cutoff_end,
            Self::Resonance => layer.resonance,
            Self::Step => layer.step,
            Self::LowpassFrom => layer.lowpass_from,
            Self::LowpassTo => layer.lowpass_to,
            Self::NoiseGain => layer.noise_gain,
            Self::SweepGain => layer.sweep_gain,
            Self::FmRatio => layer.fm_ratio,
            Self::FmIndex => layer.fm_index,
            Self::FmDecay => layer.fm_decay,
            Self::PluckDamping => layer.pluck_damping,
            Self::PluckBrightness => layer.pluck_brightness,
            Self::SamplePitch => layer.sample_pitch,
            Self::DrumTone => layer.drum_tone,
            Self::DrumSnap => layer.drum_snap,
            Self::Seed => layer.seed as f32,
            Self::Arp0 => layer.arp_freqs.first().copied().unwrap_or(523.25),
            Self::Arp1 => layer.arp_freqs.get(1).copied().unwrap_or(659.25),
            Self::Arp2 => layer.arp_freqs.get(2).copied().unwrap_or(783.99),
            Self::Arp3 => layer.arp_freqs.get(3).copied().unwrap_or(1046.5),
        }
    }

    pub fn set(self, layer: &mut Layer, value: f32) {
        match self {
            Self::Delay => layer.delay = value.max(0.0),
            Self::Duration => layer.duration = value.max(0.005),
            Self::Gain => layer.gain = value.clamp(0.0, 1.0),
            Self::FreqStart => layer.freq_start = value.max(20.0),
            Self::FreqEnd => layer.freq_end = value.max(20.0),
            Self::Harsh => layer.harsh = value.clamp(0.0, 1.0),
            Self::Duty => layer.duty = value.clamp(0.05, 0.95),
            Self::VibratoRate => layer.vibrato_rate = value.max(0.0),
            Self::VibratoDepth => layer.vibrato_depth = value.max(0.0),
            Self::Attack => layer.attack = value.max(0.0),
            Self::Decay => layer.decay = value.max(0.0),
            Self::Sustain => layer.sustain = value.clamp(0.0, 1.0),
            Self::Release => layer.release = value.max(0.0),
            Self::ExpDecay => layer.exp_decay = value.clamp(0.5, 30.0),
            Self::CutoffStart => layer.cutoff_start = value.max(20.0),
            Self::CutoffEnd => layer.cutoff_end = value.max(20.0),
            Self::Resonance => layer.resonance = value.clamp(0.5, 5.0),
            Self::Step => {
                layer.step = value.clamp(0.01, 0.5);
                layer.duration = layer.step * layer.arp_freqs.len().max(1) as f32;
            }
            Self::LowpassFrom => layer.lowpass_from = value.clamp(0.01, 1.0),
            Self::LowpassTo => layer.lowpass_to = value.clamp(0.01, 1.0),
            Self::NoiseGain => layer.noise_gain = value.clamp(0.0, 1.0),
            Self::SweepGain => layer.sweep_gain = value.clamp(0.0, 1.0),
            Self::FmRatio => layer.fm_ratio = value.max(0.25),
            Self::FmIndex => layer.fm_index = value.max(0.0),
            Self::FmDecay => layer.fm_decay = value.max(0.5),
            Self::PluckDamping => layer.pluck_damping = value.clamp(0.9, 0.9999),
            Self::PluckBrightness => layer.pluck_brightness = value.clamp(0.0, 1.0),
            Self::SamplePitch => layer.sample_pitch = value.clamp(-24.0, 24.0),
            Self::DrumTone => layer.drum_tone = value.max(0.25),
            Self::DrumSnap => layer.drum_snap = value.max(0.0),
            Self::Seed => layer.seed = value.max(0.0) as u32,
            Self::Arp0 | Self::Arp1 | Self::Arp2 | Self::Arp3 => {
                let index = match self {
                    Self::Arp0 => 0,
                    Self::Arp1 => 1,
                    Self::Arp2 => 2,
                    _ => 3,
                };
                while layer.arp_freqs.len() <= index {
                    layer.arp_freqs.push(523.25);
                }
                layer.arp_freqs[index] = value.max(20.0);
            }
        }
    }

    pub fn format(self, value: f32) -> String {
        self.spec().format(value)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MusicField {
    Bpm,
    Bars,
    Rate,
    Volume,
    Swing,
    BassGain,
    BassHarsh,
    BassDecay,
    ArpGain,
    ArpHarsh,
    ArpDecay,
    KickGain,
    HatGain,
    Seed,
}

impl MusicField {
    pub fn spec(self) -> FieldSpec {
        use Scale::{Linear, Log};
        match self {
            Self::Bpm => FieldSpec::new("BPM", "", 40.0, 240.0, 1.0, Linear),
            Self::Bars => FieldSpec::new("BARS", "", 1.0, 16.0, 1.0, Linear),
            Self::Rate => FieldSpec::new("BAKE RATE", "Hz", 8000.0, 48000.0, 100.0, Log),
            Self::Volume => FieldSpec::new("VOLUME", "", -40.0, 0.0, 0.5, Linear),
            Self::Swing => FieldSpec::new("SWING", "", 0.0, 0.5, 0.01, Linear),
            Self::BassGain => FieldSpec::new("BASS GAIN", "", 0.0, 1.0, 0.005, Linear),
            Self::BassHarsh => FieldSpec::new("BASS SQR", "", 0.0, 1.0, 0.01, Linear),
            Self::BassDecay => FieldSpec::new("BASS DECAY", "", 0.5, 30.0, 0.1, Linear),
            Self::ArpGain => FieldSpec::new("ARP GAIN", "", 0.0, 1.0, 0.005, Linear),
            Self::ArpHarsh => FieldSpec::new("ARP SQR", "", 0.0, 1.0, 0.01, Linear),
            Self::ArpDecay => FieldSpec::new("ARP DECAY", "", 0.5, 30.0, 0.1, Linear),
            Self::KickGain => FieldSpec::new("KICK", "", 0.0, 1.0, 0.005, Linear),
            Self::HatGain => FieldSpec::new("HAT", "", 0.0, 1.0, 0.005, Linear),
            Self::Seed => FieldSpec::new("SEED", "", 0.0, 99_999_999.0, 1.0, Linear),
        }
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn range(self) -> (f32, f32, f32) {
        let spec = self.spec();
        (spec.min, spec.max, spec.step)
    }

    pub fn norm(self, value: f32) -> f32 {
        self.spec().norm(value)
    }

    pub fn denorm(self, norm: f32) -> f32 {
        self.spec().denorm(norm)
    }

    pub fn get(self, music: &Music) -> f32 {
        match self {
            Self::Bpm => music.bpm,
            Self::Bars => music.bars as f32,
            Self::Rate => music.rate as f32,
            Self::Volume => music.volume_db,
            Self::Swing => music.swing,
            Self::BassGain => music.bass_gain,
            Self::BassHarsh => music.bass_harsh,
            Self::BassDecay => music.bass_decay,
            Self::ArpGain => music.arp_gain,
            Self::ArpHarsh => music.arp_harsh,
            Self::ArpDecay => music.arp_decay,
            Self::KickGain => music.kick_gain,
            Self::HatGain => music.hat_gain,
            Self::Seed => music.seed as f32,
        }
    }

    pub fn set(self, music: &mut Music, value: f32) {
        match self {
            Self::Bpm => music.bpm = value.clamp(40.0, 240.0),
            Self::Bars => music.bars = (value.round() as u32).clamp(1, 16),
            Self::Rate => music.rate = (value.round() as u32).clamp(8000, 48000),
            Self::Volume => music.volume_db = value.clamp(-40.0, 0.0),
            Self::Swing => music.swing = value.clamp(0.0, 0.5),
            Self::BassGain => music.bass_gain = value.clamp(0.0, 1.0),
            Self::BassHarsh => music.bass_harsh = value.clamp(0.0, 1.0),
            Self::BassDecay => music.bass_decay = value.clamp(0.5, 30.0),
            Self::ArpGain => music.arp_gain = value.clamp(0.0, 1.0),
            Self::ArpHarsh => music.arp_harsh = value.clamp(0.0, 1.0),
            Self::ArpDecay => music.arp_decay = value.clamp(0.5, 30.0),
            Self::KickGain => music.kick_gain = value.clamp(0.0, 1.0),
            Self::HatGain => music.hat_gain = value.clamp(0.0, 1.0),
            Self::Seed => music.seed = value.max(0.0) as u32,
        }
    }

    pub fn format(self, value: f32) -> String {
        match self {
            Self::Bpm => format!("{value:.0} BPM"),
            Self::Bars => format!("{value:.0} bars"),
            Self::Rate => format!("{value:.0} Hz"),
            Self::Volume => format!("{value:.1} dB"),
            Self::Swing => format!("{value:.2}"),
            Self::BassGain | Self::ArpGain | Self::KickGain | Self::HatGain => {
                format!("{value:.2}")
            }
            Self::BassHarsh | Self::ArpHarsh => format!("{value:.2}"),
            Self::BassDecay | Self::ArpDecay => format!("{value:.1}"),
            Self::Seed => format!("{}", value as u32),
        }
    }
}

/// One inspector row inside a layer section: a slider backed by a
/// [`LayerField`], or a mutually exclusive option group.
#[derive(Clone, Copy, Debug)]
pub enum LayerRowSpec {
    Field(LayerField),
    ToneEnvOptions,
    EnvShapeOptions,
    FilterOptions,
    SampleFile,
    SampleLoop,
}

/// A row plus the predicate that decides whether it applies to a layer.
#[derive(Clone, Copy, Debug)]
pub struct LayerRow {
    pub spec: LayerRowSpec,
    pub visible: fn(&Layer) -> bool,
}

/// A titled block of inspector rows for one wave family.
#[derive(Clone, Copy, Debug)]
pub struct LayerSectionSpec {
    pub title: &'static str,
    pub rows: &'static [LayerRow],
}

const fn row(spec: LayerRowSpec, visible: fn(&Layer) -> bool) -> LayerRow {
    LayerRow { spec, visible }
}

fn always(_: &Layer) -> bool {
    true
}

fn is_sine(layer: &Layer) -> bool {
    layer.wave == Wave::Sine
}

fn is_pulse(layer: &Layer) -> bool {
    layer.wave == Wave::Pulse
}

fn is_exp_env(layer: &Layer) -> bool {
    layer.tone_env == ToneEnv::Exp
}

fn reads_seed(layer: &Layer) -> bool {
    layer.wave != Wave::Kick
}

static TONE_PITCH: &[LayerRow] = &[
    row(LayerRowSpec::Field(LayerField::FreqStart), always),
    row(LayerRowSpec::Field(LayerField::FreqEnd), always),
    row(LayerRowSpec::Field(LayerField::Harsh), is_sine),
    row(LayerRowSpec::Field(LayerField::Duty), is_pulse),
    row(LayerRowSpec::Field(LayerField::VibratoRate), always),
    row(LayerRowSpec::Field(LayerField::VibratoDepth), always),
];

static TONE_AMP: &[LayerRow] = &[
    row(LayerRowSpec::ToneEnvOptions, always),
    row(LayerRowSpec::Field(LayerField::Attack), always),
    row(LayerRowSpec::Field(LayerField::Decay), always),
    row(LayerRowSpec::Field(LayerField::Sustain), always),
    row(LayerRowSpec::Field(LayerField::Release), always),
    row(LayerRowSpec::Field(LayerField::ExpDecay), is_exp_env),
];

static TONE_FILTER: &[LayerRow] = &[
    row(LayerRowSpec::FilterOptions, always),
    row(LayerRowSpec::Field(LayerField::CutoffStart), always),
    row(LayerRowSpec::Field(LayerField::CutoffEnd), always),
    row(LayerRowSpec::Field(LayerField::Resonance), always),
];

static TONE_SECTIONS: &[LayerSectionSpec] = &[
    LayerSectionSpec {
        title: "PITCH",
        rows: TONE_PITCH,
    },
    LayerSectionSpec {
        title: "AMP ENV",
        rows: TONE_AMP,
    },
    LayerSectionSpec {
        title: "FILTER",
        rows: TONE_FILTER,
    },
];

static NOISE_SECTIONS: &[LayerSectionSpec] = &[LayerSectionSpec {
    title: "NOISE",
    rows: &[
        row(LayerRowSpec::Field(LayerField::FreqStart), always),
        row(LayerRowSpec::Field(LayerField::FreqEnd), always),
        row(LayerRowSpec::Field(LayerField::LowpassFrom), always),
        row(LayerRowSpec::Field(LayerField::LowpassTo), always),
        row(LayerRowSpec::Field(LayerField::NoiseGain), always),
        row(LayerRowSpec::Field(LayerField::SweepGain), always),
        row(LayerRowSpec::Field(LayerField::Decay), always),
        row(LayerRowSpec::EnvShapeOptions, always),
        row(LayerRowSpec::Field(LayerField::Seed), always),
    ],
}];

static ARP_SECTIONS: &[LayerSectionSpec] = &[LayerSectionSpec {
    title: "ARP",
    rows: &[
        row(LayerRowSpec::Field(LayerField::Arp0), always),
        row(LayerRowSpec::Field(LayerField::Arp1), always),
        row(LayerRowSpec::Field(LayerField::Arp2), always),
        row(LayerRowSpec::Field(LayerField::Arp3), always),
        row(LayerRowSpec::Field(LayerField::Step), always),
        row(LayerRowSpec::Field(LayerField::Harsh), always),
        row(LayerRowSpec::Field(LayerField::Decay), always),
        row(LayerRowSpec::Field(LayerField::Attack), always),
    ],
}];

static FM_SECTIONS: &[LayerSectionSpec] = &[LayerSectionSpec {
    title: "FM",
    rows: &[
        row(LayerRowSpec::Field(LayerField::FreqStart), always),
        row(LayerRowSpec::Field(LayerField::FreqEnd), always),
        row(LayerRowSpec::Field(LayerField::FmRatio), always),
        row(LayerRowSpec::Field(LayerField::FmIndex), always),
        row(LayerRowSpec::Field(LayerField::FmDecay), always),
        row(LayerRowSpec::Field(LayerField::Attack), always),
        row(LayerRowSpec::Field(LayerField::Decay), always),
        row(LayerRowSpec::Field(LayerField::Sustain), always),
        row(LayerRowSpec::Field(LayerField::Release), always),
        row(LayerRowSpec::FilterOptions, always),
        row(LayerRowSpec::Field(LayerField::CutoffStart), always),
        row(LayerRowSpec::Field(LayerField::CutoffEnd), always),
        row(LayerRowSpec::Field(LayerField::Resonance), always),
    ],
}];

static PLUCK_SECTIONS: &[LayerSectionSpec] = &[LayerSectionSpec {
    title: "PLUCK",
    rows: &[
        row(LayerRowSpec::Field(LayerField::FreqStart), always),
        row(LayerRowSpec::Field(LayerField::PluckDamping), always),
        row(LayerRowSpec::Field(LayerField::PluckBrightness), always),
        row(LayerRowSpec::Field(LayerField::Attack), always),
        row(LayerRowSpec::Field(LayerField::Decay), always),
        row(LayerRowSpec::Field(LayerField::Sustain), always),
        row(LayerRowSpec::Field(LayerField::Release), always),
    ],
}];

static SAMPLE_SECTIONS: &[LayerSectionSpec] = &[LayerSectionSpec {
    title: "SAMPLE",
    rows: &[
        row(LayerRowSpec::SampleFile, always),
        row(LayerRowSpec::Field(LayerField::SamplePitch), always),
        row(LayerRowSpec::SampleLoop, always),
    ],
}];

static DRUM_SECTIONS: &[LayerSectionSpec] = &[LayerSectionSpec {
    title: "DRUM",
    rows: &[
        row(LayerRowSpec::Field(LayerField::DrumTone), always),
        row(LayerRowSpec::Field(LayerField::DrumSnap), always),
        row(LayerRowSpec::Field(LayerField::Seed), reads_seed),
    ],
}];

/// Inspector layout for a wave family. This is the single source of truth for
/// which fields a layer exposes; `ui::inspector_items` only renders it.
pub fn layer_sections(wave: Wave) -> &'static [LayerSectionSpec] {
    if wave.is_tone() {
        TONE_SECTIONS
    } else {
        match wave {
            Wave::Noise => NOISE_SECTIONS,
            Wave::Arp => ARP_SECTIONS,
            Wave::Fm => FM_SECTIONS,
            Wave::Pluck => PLUCK_SECTIONS,
            Wave::Sample => SAMPLE_SECTIONS,
            _ if wave.is_drum() => DRUM_SECTIONS,
            _ => TONE_SECTIONS,
        }
    }
}

/// Fields shown for a selected clip, in inspector order.
pub const CLIP_FIELDS: [ClipField; 4] = [
    ClipField::Start,
    ClipField::Duration,
    ClipField::Gain,
    ClipField::Pitch,
];

/// One row of the music inspector: a slider, a wave picker, or a static note.
#[derive(Clone, Copy, Debug)]
pub enum MusicRowSpec {
    Field(MusicField),
    BassWaveOptions,
    ArpWaveOptions,
    ChordPresets,
    TransposeOptions,
    PatternToggle,
    ResetOptions,
    PreviewNote,
    ChordNote,
}

/// A titled block of music inspector rows.
#[derive(Clone, Copy, Debug)]
pub struct MusicSectionSpec {
    pub title: &'static str,
    pub rows: &'static [MusicRowSpec],
}

static MUSIC_SECTIONS: &[MusicSectionSpec] = &[
    MusicSectionSpec {
        title: "MUSIC",
        rows: &[
            MusicRowSpec::Field(MusicField::Bpm),
            MusicRowSpec::Field(MusicField::Bars),
            MusicRowSpec::Field(MusicField::Rate),
            MusicRowSpec::Field(MusicField::Volume),
            MusicRowSpec::Field(MusicField::Swing),
            MusicRowSpec::PreviewNote,
        ],
    },
    MusicSectionSpec {
        title: "INSTRUMENTS",
        rows: &[MusicRowSpec::BassWaveOptions, MusicRowSpec::ArpWaveOptions],
    },
    MusicSectionSpec {
        title: "MIX",
        rows: &[
            MusicRowSpec::Field(MusicField::BassGain),
            MusicRowSpec::Field(MusicField::BassHarsh),
            MusicRowSpec::Field(MusicField::BassDecay),
            MusicRowSpec::Field(MusicField::ArpGain),
            MusicRowSpec::Field(MusicField::ArpHarsh),
            MusicRowSpec::Field(MusicField::ArpDecay),
            MusicRowSpec::Field(MusicField::KickGain),
            MusicRowSpec::Field(MusicField::HatGain),
            MusicRowSpec::Field(MusicField::Seed),
        ],
    },
    MusicSectionSpec {
        title: "HARMONY",
        rows: &[MusicRowSpec::ChordPresets, MusicRowSpec::TransposeOptions],
    },
    MusicSectionSpec {
        title: "PATTERN",
        rows: &[
            MusicRowSpec::PatternToggle,
            MusicRowSpec::ResetOptions,
            MusicRowSpec::ChordNote,
        ],
    },
];

/// Inspector layout for the music tab.
pub fn music_sections() -> &'static [MusicSectionSpec] {
    MUSIC_SECTIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_and_clip_registries_keep_their_order() {
        let titles: Vec<_> = music_sections()
            .iter()
            .map(|section| section.title)
            .collect();
        assert_eq!(
            titles,
            vec!["MUSIC", "INSTRUMENTS", "MIX", "HARMONY", "PATTERN"]
        );
        let music_fields: Vec<_> = music_sections()
            .iter()
            .flat_map(|section| section.rows.iter())
            .filter_map(|row| match row {
                MusicRowSpec::Field(field) => Some(*field),
                _ => None,
            })
            .collect();
        assert_eq!(music_fields.first(), Some(&MusicField::Bpm));
        assert!(music_fields.contains(&MusicField::Seed));
        assert_eq!(
            CLIP_FIELDS,
            [
                ClipField::Start,
                ClipField::Duration,
                ClipField::Gain,
                ClipField::Pitch
            ]
        );
    }

    #[test]
    fn layer_sections_expose_wave_specific_fields() {
        fn visible(layer: &Layer) -> Vec<LayerField> {
            let mut fields = Vec::new();
            for section in layer_sections(layer.wave) {
                for row in section.rows {
                    if (row.visible)(layer) {
                        if let LayerRowSpec::Field(field) = row.spec {
                            fields.push(field);
                        }
                    }
                }
            }
            fields
        }

        let sine = Layer::new(Wave::Sine);
        assert!(visible(&sine).contains(&LayerField::Harsh));
        assert!(!visible(&sine).contains(&LayerField::Duty));
        assert_eq!(
            layer_sections(Wave::Sine)
                .iter()
                .map(|section| section.title)
                .collect::<Vec<_>>(),
            vec!["PITCH", "AMP ENV", "FILTER"]
        );

        let pulse = Layer::new(Wave::Pulse);
        assert!(visible(&pulse).contains(&LayerField::Duty));
        assert!(!visible(&pulse).contains(&LayerField::Harsh));

        let mut exp = Layer::new(Wave::Sine);
        exp.tone_env = ToneEnv::Exp;
        assert!(visible(&exp).contains(&LayerField::ExpDecay));
        assert!(!visible(&sine).contains(&LayerField::ExpDecay));

        let noise = Layer::new(Wave::Noise);
        let noise_fields = visible(&noise);
        assert!(noise_fields.contains(&LayerField::SweepGain));
        assert!(noise_fields.contains(&LayerField::Seed));

        let kick = Layer::new(Wave::Kick);
        assert!(!visible(&kick).contains(&LayerField::Seed));
        let snare = Layer::new(Wave::Snare);
        assert!(visible(&snare).contains(&LayerField::Seed));

        assert!(visible(&Layer::new(Wave::Arp)).contains(&LayerField::Arp3));
        let fm = visible(&Layer::new(Wave::Fm));
        assert!(fm.contains(&LayerField::FmIndex) && fm.contains(&LayerField::Resonance));
        assert!(visible(&Layer::new(Wave::Pluck)).contains(&LayerField::PluckDamping));
    }

    #[test]
    fn project_json_roundtrip() {
        let mut project = Project::demo("roundtrip");
        project.patches[0].layers[0].freq_start = 123.4;
        project.music.use_patterns = true;
        let text = serde_json::to_string_pretty(&project).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(back.name, "roundtrip");
        assert_eq!(back.patches.len(), project.patches.len());
        assert!((back.patches[0].layers[0].freq_start - 123.4).abs() < 0.001);
        assert!(back.music.use_patterns);
        assert_eq!(back.next_id, project.next_id);
        assert_eq!(
            back.patches
                .iter()
                .map(|patch| patch.id)
                .collect::<Vec<_>>(),
            project
                .patches
                .iter()
                .map(|patch| patch.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pitch_transposes_layer_frequencies() {
        let mut layer = Layer::new(Wave::Sine);
        layer.freq_start = 440.0;
        layer.freq_end = 880.0;
        let up = layer.with_pitch(12.0);
        assert!((up.freq_start - 880.0).abs() < 0.01);
        assert!((up.freq_end - 1760.0).abs() < 0.01);
    }

    #[test]
    fn cutoff_envelope_interpolates() {
        let clip = Clip {
            cutoff_points: vec![
                AutomationPoint {
                    time: 0.0,
                    value: 200.0,
                },
                AutomationPoint {
                    time: 0.1,
                    value: 10000.0,
                },
            ],
            ..Clip::default()
        };
        assert!((clip.cutoff_at(0.05) - 5100.0).abs() < 1.0);
    }

    #[test]
    fn sample_fields_roundtrip_and_load() {
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-sample-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("audio/sfx/samples")).unwrap();
        let wav_path = root.join("audio/sfx/samples/click.wav");
        crate::synth::write_wav(&wav_path, &[0.0, 0.5, -0.5], 22_050).unwrap();

        let mut project = Project::demo("samples");
        let layer = &mut project.patches[0].layers[0];
        layer.wave = Wave::Sample;
        layer.sample_path = "audio/sfx/samples/click.wav".into();
        layer.sample_pitch = 7.0;
        layer.sample_loop = true;

        let text = serde_json::to_string_pretty(&project).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        let layer = &back.patches[0].layers[0];
        assert_eq!(layer.sample_path, "audio/sfx/samples/click.wav");
        assert!((layer.sample_pitch - 7.0).abs() < 0.001);
        assert!(layer.sample_loop);
        assert!(layer.sample_data.is_none());

        let warnings = project.load_samples(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        let data = project.patches[0].layers[0]
            .sample_data
            .as_ref()
            .expect("sample decoded");
        assert_eq!(data.rate, 22_050);
        assert_eq!(data.samples.len(), 3);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn demo_has_layered_presets_and_categories() {
        let project = Project::demo("demo");
        assert!(project.patches.len() >= 8);
        assert_eq!(project.patches[0].name, "bow_draw");
        assert_eq!(project.patches[0].category, Category::Combat);
        assert!(project.patches[0].layers.len() >= 3);
        let wind = project
            .patches
            .iter()
            .find(|patch| patch.name == "wind")
            .expect("wind preset");
        assert_eq!(wind.category, Category::Environment);
        assert!(wind.favorite);
        assert_eq!(Category::Combat.next(), Category::Impact);
    }

    #[test]
    fn favorites_and_categories_survive_json() {
        let mut project = Project::demo("filters");
        project.patches[0].favorite = true;
        let text = serde_json::to_string(&project).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert!(back.patches[0].favorite);
        assert_eq!(back.patches[0].category, Category::Combat);
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize("Laser Beam!"), "laser_beam");
        assert_eq!(sanitize("  "), "sound");
        assert_eq!(sanitize("金币 2"), "sound");
    }

    #[test]
    fn validate_repairs_untrusted_values_and_duplicate_names() {
        let mut project = Project::demo("validate");
        project.patches[0].rate = 0;
        project.patches[0].layers[0].duration = f32::NAN;
        project.patches[0].layers[0].freq_start = -50.0;
        project.patches[0].layers[0].clips.push(Clip {
            start: f32::INFINITY,
            duration: 99.0,
            gain: 99.0,
            ..Clip::default()
        });
        let duplicate = project.patches[0].clone();
        project.patches.push(duplicate);
        let warnings = project.validate();
        assert!(!warnings.is_empty());
        assert!(project.patches[0].rate >= 8000);
        assert!(project.patches[0].layers[0].duration.is_finite());
        assert!(project.patches[0].layers[0].freq_start >= 20.0);
        assert!((project.patches[0].layers[0].clips[0].gain - 2.0).abs() < 0.001);
        assert_ne!(project.patches[0].name, project.patches[1].name);
    }

    #[test]
    fn migrate_stamps_the_current_schema() {
        let mut project = Project::demo("old");
        project.version = 1;
        project.patches[0].dsp_version = 0;
        let warnings = project.migrate();
        assert!(!warnings.is_empty());
        assert_eq!(project.version, PROJECT_VERSION);
        assert_eq!(project.patches[0].dsp_version, DSP_VERSION);
    }

    #[test]
    fn migrate_assigns_unique_stable_ids() {
        let mut project = Project::demo("ids");
        project.version = 2;
        project.next_id = 1;
        for patch in &mut project.patches {
            patch.id = 0;
            for layer in &mut patch.layers {
                layer.id = 0;
                layer.ensure_clips();
                for clip in &mut layer.clips {
                    clip.id = 3;
                }
            }
        }
        project.migrate();
        assert_eq!(project.version, PROJECT_VERSION);
        let mut ids = Vec::new();
        for patch in &project.patches {
            assert!(patch.id > 0);
            ids.push(patch.id);
            for layer in &patch.layers {
                assert!(layer.id > 0);
                ids.push(layer.id);
                for clip in &layer.clips {
                    assert!(clip.id > 0);
                    ids.push(clip.id);
                }
            }
        }
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "ids must be unique");
        assert!(project.next_id > *ids.iter().max().unwrap());
    }

    #[test]
    fn demo_already_has_stable_ids() {
        let project = Project::demo("ids");
        assert!(project.next_id > 1);
        for patch in &project.patches {
            assert!(patch.id > 0);
            for layer in &patch.layers {
                assert!(layer.id > 0);
            }
        }
    }

    #[test]
    fn save_is_atomic_and_roundtrips() {
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-save-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::demo("atomic");
        save_project(&root, &project).unwrap();
        assert!(!root.join("sfx.project.json.tmp").exists());
        let loaded = load_project(&root).unwrap().unwrap();
        assert_eq!(loaded.name, "atomic");
        assert_eq!(loaded.patches.len(), project.patches.len());
        // A corrupt file must be an error, not a silent demo fallback.
        std::fs::write(root.join(PROJECT_FILE), "{ not json").unwrap();
        assert!(load_project(&root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn log_fields_map_to_a_geometric_midpoint() {
        for field in [
            LayerField::FreqStart,
            LayerField::CutoffStart,
            LayerField::Arp0,
        ] {
            let spec = field.spec();
            let mid = field.denorm(0.5);
            let expected = (spec.min * spec.max).sqrt();
            assert!(
                (mid - expected).abs() / expected < 0.01,
                "{:?} midpoint {mid} != {expected}",
                field
            );
            let roundtrip = field.norm(field.denorm(0.37));
            assert!((roundtrip - 0.37).abs() < 0.001);
        }
        // Linear fields still behave linearly.
        assert!((LayerField::Gain.denorm(0.5) - 0.5).abs() < 0.001);
        assert!((LayerField::Gain.norm(0.5) - 0.5).abs() < 0.001);
    }

    #[test]
    fn tone_env_survives_json() {
        let mut project = Project::demo("tone");
        project.patches[0].layers[0].tone_env = ToneEnv::Exp;
        project.patches[0].layers[0].exp_decay = 7.5;
        let text = serde_json::to_string(&project).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(back.patches[0].layers[0].tone_env, ToneEnv::Exp);
        assert!((back.patches[0].layers[0].exp_decay - 7.5).abs() < 0.001);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AutomationKind {
    #[default]
    Gain,
    Cutoff,
}

impl AutomationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gain => "GAIN",
            Self::Cutoff => "CUTOFF",
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Gain => (0.0, 2.0, 0.01),
            Self::Cutoff => (20.0, 18000.0, 10.0),
        }
    }

    pub fn points(self, clip: &Clip) -> &[AutomationPoint] {
        match self {
            Self::Gain => &clip.gain_points,
            Self::Cutoff => &clip.cutoff_points,
        }
    }

    pub fn points_mut(self, clip: &mut Clip) -> &mut Vec<AutomationPoint> {
        match self {
            Self::Gain => &mut clip.gain_points,
            Self::Cutoff => &mut clip.cutoff_points,
        }
    }

    /// Map a value into 0..1 for drawing; cutoff is logarithmic.
    pub fn norm(self, value: f32) -> f32 {
        match self {
            Self::Gain => (value / 2.0).clamp(0.0, 1.0),
            Self::Cutoff => {
                let low = 20f32.ln();
                let high = 18000f32.ln();
                ((value.clamp(20.0, 18000.0).ln() - low) / (high - low)).clamp(0.0, 1.0)
            }
        }
    }

    pub fn denorm(self, norm: f32) -> f32 {
        match self {
            Self::Gain => (norm.clamp(0.0, 1.0) * 2.0).clamp(0.0, 2.0),
            Self::Cutoff => {
                let low = 20f32.ln();
                let high = 18000f32.ln();
                (low + norm.clamp(0.0, 1.0) * (high - low)).exp()
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipField {
    Start,
    Duration,
    Gain,
    Pitch,
}

impl ClipField {
    pub fn spec(self) -> FieldSpec {
        use Scale::Linear;
        match self {
            Self::Start => FieldSpec::new("START", "s", 0.0, 8.0, 0.001, Linear),
            Self::Duration => FieldSpec::new("LENGTH", "s", 0.005, 3.0, 0.001, Linear),
            Self::Gain => FieldSpec::new("GAIN", "", 0.0, 2.0, 0.005, Linear),
            Self::Pitch => FieldSpec::new("PITCH", "st", -24.0, 24.0, 0.1, Linear),
        }
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn range(self) -> (f32, f32, f32) {
        let spec = self.spec();
        (spec.min, spec.max, spec.step)
    }

    pub fn norm(self, value: f32) -> f32 {
        self.spec().norm(value)
    }

    pub fn denorm(self, norm: f32) -> f32 {
        self.spec().denorm(norm)
    }

    pub fn get(self, clip: &Clip) -> f32 {
        match self {
            Self::Start => clip.start,
            Self::Duration => clip.duration,
            Self::Gain => clip.gain,
            Self::Pitch => clip.pitch,
        }
    }

    pub fn set(self, clip: &mut Clip, value: f32) {
        match self {
            Self::Start => clip.start = value.max(0.0),
            Self::Duration => clip.duration = value.max(0.005),
            Self::Gain => clip.gain = value.max(0.0),
            Self::Pitch => clip.pitch = value.clamp(-24.0, 24.0),
        }
    }

    pub fn format(self, value: f32) -> String {
        match self {
            Self::Start | Self::Duration => format!("{value:.3}s"),
            Self::Gain => format!("{value:.2}"),
            Self::Pitch => format!("{value:+.1}st"),
        }
    }
}
