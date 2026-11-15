//! Built-in preset families modelled after jsfxr's library API.
//!
//! jsfxr exposes a tiny "generate by name" surface:
//!
//! ```text
//! const sound = sfxr.generate("pickupCoin");
//! sfxr.play(sound);
//! ```
//!
//! SFX LAB mirrors the same vocabulary and maps every family onto its own
//! layer/DSP model. The parameter choices are hand-tuned versions of the
//! sfxr families rather than a literal port (the layer model is richer), but
//! the names, categories and the `random` family behave the same way.
//!
//! Generation is deterministic for a given `(name, seed)` pair so presets can
//! be written to `<project>/presets/*.json`, tested and rendered by the CLI
//! without `Math.random()`-style surprises. `seed` is the equivalent of
//! jsfxr's "generate another one" button.

use crate::model::{Category, EnvShape, FilterMode, Layer, Patch, Wave, DSP_VERSION};
use crate::synth::Rng;

/// The public, jsfxr-compatible preset names. `random` picks one of the
/// other ten families and keeps the generated `Patch` reproducible.
pub const PRESET_NAMES: [&str; 11] = [
    "pickupCoin",
    "laserShoot",
    "explosion",
    "powerUp",
    "hitHurt",
    "jump",
    "blipSelect",
    "synth",
    "tone",
    "click",
    "random",
];

/// Canonical snake_case patch names used for files and the library rows.
pub fn patch_name(canonical: &str) -> &'static str {
    match canonical {
        "pickupCoin" => "pickup_coin",
        "laserShoot" => "laser_shoot",
        "explosion" => "explosion_burst",
        "powerUp" => "power_up",
        "hitHurt" => "hit_hurt",
        "jump" => "jump",
        "blipSelect" => "blip_select",
        "synth" => "synth_lead",
        "tone" => "pure_tone",
        "click" => "ui_click",
        "random" => "random_sfx",
        _ => "preset",
    }
}

/// Case/punctuation-insensitive key: `pickupCoin`, `pickup_coin` and
/// `PICKUP-COIN` all resolve to the same family.
pub fn normalize(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn canonical(name: &str) -> Option<&'static str> {
    let key = normalize(name);
    let family = match key.as_str() {
        "pickupcoin" | "pickup" | "coin" => "pickupCoin",
        "lasershoot" | "laser" | "shoot" => "laserShoot",
        "explosion" | "explosionburst" | "explode" | "boom" => "explosion",
        "powerup" | "power" => "powerUp",
        "hithurt" | "hurt" | "hit" => "hitHurt",
        "jump" => "jump",
        "blipselect" | "blip" | "select" => "blipSelect",
        "synth" | "synthlead" | "lead" => "synth",
        "tone" | "puretone" => "tone",
        "click" | "uiclick" | "ui" => "click",
        "random" | "randomsfx" | "rand" => "random",
        _ => return None,
    };
    Some(family)
}

pub fn is_builtin(name: &str) -> bool {
    canonical(name).is_some()
}

/// Generate one preset. Unknown names return `None`; the same `(name, seed)`
/// always produces the same `Patch`.
pub fn generate(name: &str, seed: u32) -> Option<Patch> {
    let family = canonical(name)?;
    let salt = fnv1a(family.as_bytes());
    let mut rng = Rng::new((seed as u64).wrapping_mul(0x9E37_79B9) ^ salt);
    let mut patch = generate_family(family, &mut rng);
    patch.name = patch_name(family).to_string();
    Some(patch)
}

fn generate_family(family: &'static str, rng: &mut Rng) -> Patch {
    match family {
        "pickupCoin" => pickup_coin(rng),
        "laserShoot" => laser_shoot(rng),
        "explosion" => explosion(rng),
        "powerUp" => power_up(rng),
        "hitHurt" => hit_hurt(rng),
        "jump" => jump(rng),
        "blipSelect" => blip_select(rng),
        "synth" => synth_lead(rng),
        "tone" => pure_tone(rng),
        "click" => ui_click(rng),
        "random" => random_patch(rng),
        _ => pure_tone(rng),
    }
}

/// A bank of every family in [`PRESET_NAMES`] order.
pub fn bank(seed: u32) -> Vec<Patch> {
    PRESET_NAMES
        .iter()
        .filter_map(|name| generate(name, seed))
        .collect()
}

fn pickup_coin(rng: &mut Rng) -> Patch {
    let mut coin = Layer::new(Wave::Arp);
    coin.arp_freqs = vec![987.77 + j(rng, 12.0), 1318.51 + j(rng, 16.0)];
    coin.step = 0.052 + j(rng, 0.006);
    coin.duration = coin.step * coin.arp_freqs.len() as f32 + 0.005;
    coin.gain = 0.30;
    coin.decay = 3.2 + j(rng, 0.5);
    coin.harsh = 0.55;
    coin.pan = -0.04;

    let mut sparkle = Layer::new(Wave::Sine);
    sparkle.duration = 0.15;
    sparkle.gain = 0.09;
    sparkle.freq_start = 2637.0 + j(rng, 40.0);
    sparkle.freq_end = 3520.0;
    sparkle.attack = 0.001;
    sparkle.decay = 0.10;
    sparkle.release = 0.04;
    sparkle.pan = 0.10;

    finish(patch(vec![coin, sparkle]), Category::Ui, 0.08, 0.30)
}

fn laser_shoot(rng: &mut Rng) -> Patch {
    let mut beam = Layer::new(Wave::Saw);
    beam.duration = 0.22;
    beam.gain = 0.28;
    beam.freq_start = 1600.0 + j(rng, 180.0);
    beam.freq_end = 180.0 + j(rng, 24.0);
    beam.attack = 0.001;
    beam.decay = 0.14;
    beam.sustain = 0.0;
    beam.release = 0.04;
    beam.filter = FilterMode::Lowpass;
    beam.cutoff_start = 6000.0;
    beam.cutoff_end = 900.0 + j(rng, 120.0);
    beam.resonance = 1.1;
    beam.pan = -0.10;

    let mut tail = Layer::new(Wave::Pulse);
    tail.duty = 0.30;
    tail.delay = 0.02;
    tail.duration = 0.16;
    tail.gain = 0.10;
    tail.freq_start = 900.0;
    tail.freq_end = 140.0;
    tail.attack = 0.001;
    tail.decay = 0.10;
    tail.release = 0.02;
    tail.pan = 0.12;

    finish(patch(vec![beam, tail]), Category::Laser, 0.12, 0.35)
}

fn explosion(rng: &mut Rng) -> Patch {
    let mut boom = Layer::new(Wave::Kick);
    boom.duration = 0.65;
    boom.gain = 0.40;
    boom.drum_tone = 0.80 + j(rng, 0.06);

    let mut blast = Layer::new(Wave::Noise);
    blast.duration = 0.75;
    blast.gain = 0.45;
    blast.freq_start = 1200.0 + j(rng, 200.0);
    blast.freq_end = 70.0;
    blast.lowpass_from = 0.55;
    blast.lowpass_to = 0.08;
    blast.noise_gain = 0.92;
    blast.sweep_gain = 0.22;
    blast.decay = 4.0 + j(rng, 0.6);
    blast.seed = 9001;

    let mut debris = Layer::new(Wave::Noise);
    debris.delay = 0.12;
    debris.duration = 0.55;
    debris.gain = 0.12;
    debris.freq_start = 2400.0;
    debris.freq_end = 420.0;
    debris.lowpass_from = 0.40;
    debris.lowpass_to = 0.05;
    debris.noise_gain = 0.95;
    debris.sweep_gain = 0.0;
    debris.env_shape = EnvShape::Exp;
    debris.decay = 7.0;
    debris.seed = 9002;
    debris.pan = 0.35;

    let mut rumble = Layer::new(Wave::Sine);
    rumble.duration = 0.82;
    rumble.gain = 0.18;
    rumble.freq_start = 65.0;
    rumble.freq_end = 30.0;
    rumble.attack = 0.004;
    rumble.decay = 0.50;
    rumble.release = 0.20;

    finish(
        patch(vec![boom, blast, debris, rumble]),
        Category::Impact,
        0.38,
        0.80,
    )
}

fn power_up(rng: &mut Rng) -> Patch {
    let mut rise = Layer::new(Wave::Arp);
    rise.arp_freqs = vec![261.63, 329.63, 392.0, 523.25, 659.25 + j(rng, 8.0)];
    rise.step = 0.070 + j(rng, 0.008);
    rise.duration = rise.step * rise.arp_freqs.len() as f32 + 0.01;
    rise.gain = 0.22;
    rise.decay = 2.0;
    rise.harsh = 0.35;
    rise.pan = -0.05;

    let mut octave = Layer::new(Wave::Pulse);
    octave.delay = 0.02;
    octave.duration = 0.36;
    octave.gain = 0.07;
    octave.duty = 0.30;
    octave.freq_start = 523.25;
    octave.freq_end = 1318.51;
    octave.attack = 0.001;
    octave.decay = 0.18;
    octave.release = 0.12;
    octave.pan = 0.15;

    let mut shimmer = Layer::new(Wave::Sine);
    shimmer.delay = rise.duration - 0.06;
    shimmer.duration = 0.28;
    shimmer.gain = 0.10;
    shimmer.freq_start = 1568.0;
    shimmer.freq_end = 2093.0;
    shimmer.attack = 0.002;
    shimmer.decay = 0.20;
    shimmer.release = 0.08;

    finish(
        patch(vec![rise, octave, shimmer]),
        Category::Magic,
        0.18,
        0.45,
    )
}

fn hit_hurt(rng: &mut Rng) -> Patch {
    let mut crunch = Layer::new(Wave::Noise);
    crunch.duration = 0.18;
    crunch.gain = 0.32;
    crunch.freq_start = 2200.0;
    crunch.freq_end = 350.0 + j(rng, 40.0);
    crunch.lowpass_from = 0.70;
    crunch.lowpass_to = 0.12;
    crunch.noise_gain = 0.95;
    crunch.sweep_gain = 0.08;
    crunch.decay = 8.0;
    crunch.seed = 9101;

    let mut body = Layer::new(Wave::Square);
    body.duration = 0.14;
    body.gain = 0.18;
    body.freq_start = 320.0;
    body.freq_end = 90.0;
    body.decay = 0.10;
    body.release = 0.03;
    body.pan = 0.10;

    finish(patch(vec![crunch, body]), Category::Combat, 0.10, 0.35)
}

fn jump(rng: &mut Rng) -> Patch {
    let mut leap = Layer::new(Wave::Pulse);
    leap.duty = 0.35;
    leap.duration = 0.20;
    leap.gain = 0.26;
    leap.freq_start = 190.0 + j(rng, 20.0);
    leap.freq_end = 620.0 + j(rng, 45.0);
    leap.attack = 0.002;
    leap.decay = 0.13;
    leap.release = 0.05;

    let mut sub = Layer::new(Wave::Sine);
    sub.duration = 0.20;
    sub.gain = 0.10;
    sub.freq_start = 95.0;
    sub.freq_end = 310.0;
    sub.attack = 0.002;
    sub.decay = 0.13;
    sub.release = 0.05;

    finish(patch(vec![leap, sub]), Category::Movement, 0.08, 0.30)
}

fn blip_select(rng: &mut Rng) -> Patch {
    let mut blip = Layer::new(Wave::Square);
    blip.duration = 0.075;
    blip.gain = 0.17;
    blip.freq_start = 880.0 + j(rng, 20.0);
    blip.freq_end = 1320.0 + j(rng, 30.0);
    blip.attack = 0.001;
    blip.decay = 0.05;
    blip.release = 0.015;

    let mut overtone = Layer::new(Wave::Sine);
    overtone.duration = 0.05;
    overtone.gain = 0.07;
    overtone.freq_start = 1760.0;
    overtone.freq_end = 2640.0;
    overtone.attack = 0.0005;
    overtone.decay = 0.035;
    overtone.release = 0.01;

    finish(patch(vec![blip, overtone]), Category::Ui, 0.06, 0.25)
}

fn synth_lead(rng: &mut Rng) -> Patch {
    let start = 330.0 + j(rng, 24.0);
    let mut lead = Layer::new(Wave::Saw);
    lead.duration = 0.55;
    lead.gain = 0.18;
    lead.freq_start = start;
    lead.freq_end = start * 1.02;
    lead.vibrato_rate = 5.2;
    lead.vibrato_depth = 0.20;
    lead.attack = 0.010;
    lead.decay = 0.20;
    lead.sustain = 0.45;
    lead.release = 0.25;
    lead.filter = FilterMode::Lowpass;
    lead.cutoff_start = 2400.0;
    lead.cutoff_end = 1200.0;
    lead.resonance = 1.2;
    lead.pan = -0.12;

    let mut body = Layer::new(Wave::Pulse);
    body.duty = 0.25;
    body.duration = 0.50;
    body.gain = 0.07;
    body.freq_start = start * 2.0;
    body.freq_end = start * 2.0;
    body.vibrato_rate = 5.2;
    body.vibrato_depth = 0.12;
    body.attack = 0.012;
    body.decay = 0.20;
    body.sustain = 0.40;
    body.release = 0.22;
    body.pan = 0.12;

    let mut sub = Layer::new(Wave::Sine);
    sub.duration = 0.45;
    sub.gain = 0.12;
    sub.freq_start = start * 0.5;
    sub.freq_end = start * 0.5;
    sub.attack = 0.008;
    sub.decay = 0.18;
    sub.sustain = 0.50;
    sub.release = 0.22;

    finish(patch(vec![lead, body, sub]), Category::Magic, 0.22, 0.50)
}

fn pure_tone(rng: &mut Rng) -> Patch {
    let mut tone = Layer::new(Wave::Sine);
    tone.duration = 0.60;
    tone.gain = 0.26;
    tone.freq_start = 440.0 + j(rng, 6.0);
    tone.freq_end = tone.freq_start;
    tone.attack = 0.005;
    tone.decay = 0.25;
    tone.sustain = 0.50;
    tone.release = 0.25;

    let mut octave = Layer::new(Wave::Sine);
    octave.duration = 0.60;
    octave.gain = 0.05;
    octave.freq_start = tone.freq_start * 2.0;
    octave.freq_end = octave.freq_start;
    octave.attack = 0.006;
    octave.decay = 0.25;
    octave.sustain = 0.45;
    octave.release = 0.25;

    finish(patch(vec![tone, octave]), Category::Ui, 0.18, 0.40)
}

fn ui_click(rng: &mut Rng) -> Patch {
    let mut click = Layer::new(Wave::Triangle);
    click.duration = 0.028;
    click.gain = 0.16;
    click.freq_start = 1600.0 + j(rng, 80.0);
    click.freq_end = 1100.0;
    click.attack = 0.0005;
    click.decay = 0.020;
    click.release = 0.005;

    let mut tick = Layer::new(Wave::Noise);
    tick.duration = 0.012;
    tick.gain = 0.07;
    tick.freq_start = 3000.0;
    tick.freq_end = 1200.0;
    tick.lowpass_from = 0.90;
    tick.lowpass_to = 0.40;
    tick.noise_gain = 0.9;
    tick.sweep_gain = 0.0;
    tick.env_shape = EnvShape::Exp;
    tick.decay = 20.0;
    tick.seed = 9201;

    finish(patch(vec![click, tick]), Category::Ui, 0.02, 0.20)
}

fn random_patch(rng: &mut Rng) -> Patch {
    // `random` excludes itself and keeps the chosen family's name so the
    // library row still says what was generated.
    let index = (rng.next_u64() % (PRESET_NAMES.len() as u64 - 1)) as usize;
    let family = PRESET_NAMES[index];
    let mut patch = generate_family(family, rng);
    patch.name = patch_name("random").to_string();
    patch.favorite = false;
    patch
}

fn patch(layers: Vec<Layer>) -> Patch {
    Patch {
        layers,
        ..Patch::default()
    }
}

fn finish(mut patch: Patch, category: Category, space: f32, room: f32) -> Patch {
    patch.category = category;
    patch.dsp_version = DSP_VERSION;
    patch.normalize = true;
    patch.space = space;
    patch.room = room;
    patch
}

/// Symmetric jitter around zero.
fn j(rng: &mut Rng, span: f32) -> f32 {
    rng.next_f32() * span
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx;
    use crate::synth;

    #[test]
    fn every_jsfxr_family_is_available_by_any_spelling() {
        for name in PRESET_NAMES {
            assert!(is_builtin(name), "{name}");
            let snake = patch_name(name);
            assert!(is_builtin(snake), "{snake}");
            let upper = name.to_ascii_uppercase();
            assert!(is_builtin(&upper), "{upper}");
            assert!(generate(name, 7).is_some(), "{name}");
        }
        assert!(!is_builtin("not-a-preset"));
        assert!(generate("not-a-preset", 0).is_none());
    }

    #[test]
    fn generation_is_deterministic_per_seed() {
        for name in PRESET_NAMES {
            let a = generate(name, 42).expect("preset");
            let b = generate(name, 42).expect("preset");
            assert_eq!(a, b, "{name} must be reproducible");
            assert_eq!(a.layers.len(), b.layers.len());
        }
        assert_ne!(
            generate("laserShoot", 1).unwrap(),
            generate("laserShoot", 2).unwrap()
        );
    }

    #[test]
    fn bank_has_unique_names_and_renders_audible_audio() {
        let bank = bank(11);
        assert_eq!(bank.len(), PRESET_NAMES.len());
        let mut names = std::collections::HashSet::new();
        for patch in &bank {
            assert!(names.insert(patch.name.clone()), "{}", patch.name);
            assert_eq!(patch.dsp_version, DSP_VERSION);
            let stereo = fx::render_patch(patch, synth::PREVIEW_RATE);
            let frames = stereo.frames();
            assert!(frames > 0, "{} rendered no frames", patch.name);
            let peak = synth::peak(&stereo.mono());
            assert!(peak > 0.01, "{} is silent", patch.name);
            assert!(peak.is_finite() && peak <= 1.001);
            assert!(stereo.interleave().iter().all(|sample| sample.is_finite()));
        }
    }

    #[test]
    fn random_is_a_real_family_with_a_stable_name() {
        let patch = generate("random", 5).expect("random");
        assert_eq!(patch.name, "random_sfx");
        assert!(!patch.layers.is_empty());
        // Several seeds should not all collapse to one family arrangement.
        let variants: std::collections::HashSet<String> = (0..12)
            .map(|seed| {
                let p = generate("random", seed).unwrap();
                format!("{}{}", p.layers.len(), p.layers[0].wave.label())
            })
            .collect();
        assert!(variants.len() > 1, "{variants:?}");
    }
}
