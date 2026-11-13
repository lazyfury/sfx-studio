//! Pure project model: patches, layers, music, JSON persistence.
//!
//! The standalone app owns this format. It can also import/export the Godot
//! plugin's `.tres` resources (see `tres.rs`).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const PROJECT_FILE: &str = "sfx.project.json";

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
    Kick,
    Snare,
    Hat,
    Clap,
}

impl Wave {
    pub const ALL: [Wave; 13] = [
        Wave::Sine,
        Wave::Triangle,
        Wave::Saw,
        Wave::Pulse,
        Wave::Square,
        Wave::Noise,
        Wave::Arp,
        Wave::Fm,
        Wave::Pluck,
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
            8 | 9 => Wave::Pluck,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Layer {
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
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
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
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            wave: Wave::Sine,
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
            attack: 0.002,
            decay: 0.06,
            sustain: 0.0,
            release: 0.03,
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Patch {
    pub name: String,
    pub category: Category,
    pub favorite: bool,
    pub rate: u32,
    pub normalize: bool,
    pub layers: Vec<Layer>,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            name: String::new(),
            category: Category::Impact,
            favorite: false,
            rate: 22050,
            normalize: true,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Project {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub music: Music,
}

impl Project {
    pub fn demo(name: impl Into<String>) -> Self {
        let mut project = Self {
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
        project
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
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "sound".into()
    } else {
        trimmed
    }
}

pub fn load_project(root: &Path) -> Option<Project> {
    let path = root.join(PROJECT_FILE);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_project(root: &Path, project: &Project) -> std::io::Result<()> {
    let path = root.join(PROJECT_FILE);
    let text = serde_json::to_string_pretty(project).unwrap_or_default();
    std::fs::write(path, text)
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
    DrumTone,
    DrumSnap,
    Seed,
    Arp0,
    Arp1,
    Arp2,
    Arp3,
}

impl LayerField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Delay => "DELAY",
            Self::Duration => "LENGTH",
            Self::Gain => "GAIN",
            Self::FreqStart => "FREQ A",
            Self::FreqEnd => "FREQ B",
            Self::Harsh => "SQUARE",
            Self::Duty => "DUTY",
            Self::VibratoRate => "VIB RATE",
            Self::VibratoDepth => "VIB DEPTH",
            Self::Attack => "ATTACK",
            Self::Decay => "DECAY",
            Self::Sustain => "SUSTAIN",
            Self::Release => "RELEASE",
            Self::CutoffStart => "CUTOFF A",
            Self::CutoffEnd => "CUTOFF B",
            Self::Resonance => "RESONANCE",
            Self::Step => "STEP",
            Self::LowpassFrom => "LP A",
            Self::LowpassTo => "LP B",
            Self::NoiseGain => "NOISE",
            Self::SweepGain => "SWEEP",
            Self::FmRatio => "FM RATIO",
            Self::FmIndex => "FM INDEX",
            Self::FmDecay => "FM DECAY",
            Self::PluckDamping => "DAMPING",
            Self::PluckBrightness => "BRIGHT",
            Self::DrumTone => "TONE",
            Self::DrumSnap => "SNAP",
            Self::Seed => "SEED",
            Self::Arp0 => "ARP 1",
            Self::Arp1 => "ARP 2",
            Self::Arp2 => "ARP 3",
            Self::Arp3 => "ARP 4",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Delay
            | Self::Duration
            | Self::Attack
            | Self::Decay
            | Self::Release
            | Self::Step => "s",
            Self::FreqStart
            | Self::FreqEnd
            | Self::CutoffStart
            | Self::CutoffEnd
            | Self::Arp0
            | Self::Arp1
            | Self::Arp2
            | Self::Arp3 => "Hz",
            Self::VibratoRate => "Hz",
            Self::VibratoDepth
            | Self::Resonance
            | Self::FmRatio
            | Self::FmIndex
            | Self::FmDecay
            | Self::Sustain
            | Self::LowpassFrom
            | Self::LowpassTo
            | Self::NoiseGain
            | Self::SweepGain
            | Self::Duty
            | Self::Harsh
            | Self::Gain
            | Self::PluckBrightness
            | Self::DrumSnap => "",
            Self::DrumTone | Self::PluckDamping => "",
            Self::Seed => "",
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Delay => (0.0, 2.0, 0.001),
            Self::Duration => (0.005, 3.0, 0.001),
            Self::Gain
            | Self::Harsh
            | Self::Sustain
            | Self::LowpassFrom
            | Self::LowpassTo
            | Self::NoiseGain
            | Self::SweepGain
            | Self::PluckBrightness => (0.0, 1.0, 0.001),
            Self::Duty => (0.05, 0.95, 0.01),
            Self::FreqStart | Self::FreqEnd | Self::CutoffStart | Self::CutoffEnd => {
                (20.0, 12000.0, 1.0)
            }
            Self::VibratoRate => (0.0, 20.0, 0.1),
            Self::VibratoDepth => (0.0, 12.0, 0.01),
            Self::Attack => (0.0, 2.0, 0.001),
            Self::Decay => (0.0, 3.0, 0.001),
            Self::Release => (0.0, 3.0, 0.001),
            Self::Resonance => (0.5, 5.0, 0.01),
            Self::Step => (0.01, 0.5, 0.001),
            Self::FmRatio => (0.25, 12.0, 0.01),
            Self::FmIndex => (0.0, 20.0, 0.1),
            Self::FmDecay => (0.5, 30.0, 0.1),
            Self::PluckDamping => (0.9, 0.9999, 0.0001),
            Self::DrumTone => (0.25, 2.0, 0.01),
            Self::DrumSnap => (0.0, 2.0, 0.01),
            Self::Seed => (0.0, 99_999_999.0, 1.0),
            Self::Arp0 | Self::Arp1 | Self::Arp2 | Self::Arp3 => (20.0, 5000.0, 1.0),
        }
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
        if self == Self::Seed {
            return format!("{}", value as u32);
        }
        let (_, _, step) = self.range();
        let decimals = if step >= 1.0 {
            0
        } else if step >= 0.01 {
            2
        } else {
            3
        };
        let unit = self.unit();
        if unit.is_empty() {
            format!("{value:.decimals$}")
        } else {
            format!("{value:.decimals$}{unit}")
        }
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Bpm => "BPM",
            Self::Bars => "BARS",
            Self::Rate => "RATE",
            Self::Volume => "VOLUME",
            Self::Swing => "SWING",
            Self::BassGain => "BASS GAIN",
            Self::BassHarsh => "BASS SQR",
            Self::BassDecay => "BASS DECAY",
            Self::ArpGain => "ARP GAIN",
            Self::ArpHarsh => "ARP SQR",
            Self::ArpDecay => "ARP DECAY",
            Self::KickGain => "KICK",
            Self::HatGain => "HAT",
            Self::Seed => "SEED",
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Bpm => (40.0, 240.0, 1.0),
            Self::Bars => (1.0, 16.0, 1.0),
            Self::Rate => (8000.0, 48000.0, 100.0),
            Self::Volume => (-40.0, 0.0, 0.5),
            Self::Swing => (0.0, 0.5, 0.01),
            Self::BassGain | Self::ArpGain | Self::KickGain | Self::HatGain => (0.0, 1.0, 0.005),
            Self::BassHarsh | Self::ArpHarsh => (0.0, 1.0, 0.01),
            Self::BassDecay | Self::ArpDecay => (0.5, 30.0, 0.1),
            Self::Seed => (0.0, 99_999_999.0, 1.0),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut clip = Clip::default();
        clip.cutoff_points = vec![
            AutomationPoint {
                time: 0.0,
                value: 200.0,
            },
            AutomationPoint {
                time: 0.1,
                value: 10000.0,
            },
        ];
        assert!((clip.cutoff_at(0.05) - 5100.0).abs() < 1.0);
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

    pub fn points(self, clip: &Clip) -> &Vec<AutomationPoint> {
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "START",
            Self::Duration => "LENGTH",
            Self::Gain => "GAIN",
            Self::Pitch => "PITCH",
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Start => (0.0, 8.0, 0.001),
            Self::Duration => (0.005, 3.0, 0.001),
            Self::Gain => (0.0, 2.0, 0.005),
            Self::Pitch => (-24.0, 24.0, 0.1),
        }
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
