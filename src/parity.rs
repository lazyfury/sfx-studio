//! Rust ↔ Godot golden harness for deterministic waveforms.
//!
//! `--parity-fixtures DIR` writes one `.tres` per fixture plus the Rust render.
//! `scripts/parity.sh` then runs headless Godot over the same fixtures and
//! calls `--parity-check DIR`, which compares peak/RMS within tolerance.
//!
//! Noise/drum/pluck waves are excluded while D1 (RNG mismatch) is unresolved.

use std::path::Path;

use crate::model::{Clip, Layer, Patch, Wave};
use crate::{export, synth, wav};

pub const PEAK_TOL_DB: f32 = 0.2;
pub const RMS_TOL_DB: f32 = 0.2;
pub const MAX_ABS_TOL: f32 = 0.01;

pub struct Fixture {
    pub name: &'static str,
    pub patch: Patch,
}

fn layer(wave: Wave, duration: f32, gain: f32) -> Layer {
    let mut layer = Layer::new(wave);
    layer.duration = duration;
    layer.gain = gain;
    layer.attack = 0.005;
    layer.decay = 0.05;
    layer.sustain = 0.6;
    layer.release = 0.05;
    layer
}

/// Deterministic fixtures that both implementations must agree on.
pub fn fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();

    let mut sine = layer(Wave::Sine, 0.15, 0.8);
    sine.freq_start = 440.0;
    sine.freq_end = 880.0;
    sine.attack = 0.01;
    sine.decay = 0.06;
    sine.sustain = 0.5;
    sine.release = 0.04;
    out.push(Fixture {
        name: "sine_adsr",
        patch: Patch {
            name: "sine_adsr".into(),
            layers: vec![sine],
            ..Patch::default()
        },
    });

    for (name, wave) in [
        ("triangle", Wave::Triangle),
        ("saw", Wave::Saw),
        ("pulse", Wave::Pulse),
        ("square", Wave::Square),
    ] {
        let mut l = layer(wave, 0.12, 0.7);
        l.freq_start = 220.0;
        l.freq_end = 330.0;
        l.duty = 0.3;
        l.harsh = 0.2;
        out.push(Fixture {
            name,
            patch: Patch {
                name: name.into(),
                layers: vec![l],
                ..Patch::default()
            },
        });
    }

    let mut fm = layer(Wave::Fm, 0.18, 0.7);
    fm.freq_start = 220.0;
    fm.freq_end = 220.0;
    fm.fm_ratio = 2.0;
    fm.fm_index = 2.0;
    fm.fm_decay = 4.0;
    out.push(Fixture {
        name: "fm",
        patch: Patch {
            name: "fm".into(),
            layers: vec![fm],
            ..Patch::default()
        },
    });

    let mut arp = layer(Wave::Arp, 0.12, 0.7);
    arp.arp_freqs = vec![523.25, 659.25, 783.99];
    arp.step = 0.03;
    arp.duration = 0.12;
    arp.harsh = 0.1;
    out.push(Fixture {
        name: "arp",
        patch: Patch {
            name: "arp".into(),
            layers: vec![arp],
            ..Patch::default()
        },
    });

    let mut clips = layer(Wave::Sine, 0.3, 0.7);
    clips.freq_start = 440.0;
    clips.freq_end = 440.0;
    clips.clips = vec![
        Clip {
            start: 0.0,
            duration: 0.1,
            gain: 1.0,
            pitch: 0.0,
            ..Clip::default()
        },
        Clip {
            start: 0.15,
            duration: 0.1,
            gain: 0.5,
            pitch: 7.0,
            ..Clip::default()
        },
    ];
    out.push(Fixture {
        name: "clips",
        patch: Patch {
            name: "clips".into(),
            layers: vec![clips],
            ..Patch::default()
        },
    });

    let mut low = layer(Wave::Sine, 0.15, 0.9);
    low.freq_start = 110.0;
    low.freq_end = 110.0;
    let mut high = layer(Wave::Triangle, 0.15, 0.9);
    high.freq_start = 330.0;
    high.freq_end = 330.0;
    out.push(Fixture {
        name: "normalize",
        patch: Patch {
            name: "normalize".into(),
            layers: vec![low, high],
            normalize: true,
            ..Patch::default()
        },
    });

    out
}

pub fn write_fixtures(dir: &Path) -> Result<Vec<String>, String> {
    let fixtures_dir = dir.join("fixtures");
    let rust_dir = dir.join("rust");
    std::fs::create_dir_all(&fixtures_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&rust_dir).map_err(|error| error.to_string())?;
    let mut written = Vec::new();
    for fixture in fixtures() {
        let tres = fixtures_dir.join(format!("{}.tres", fixture.name));
        std::fs::write(&tres, export::patch_tres(&fixture.patch))
            .map_err(|error| format!("{}: {error}", tres.display()))?;
        let samples = synth::render_patch(&fixture.patch, fixture.patch.rate);
        let wave = rust_dir.join(format!("{}.wav", fixture.name));
        synth::write_wav(&wave, &samples, fixture.patch.rate)
            .map_err(|error| format!("{}: {error}", wave.display()))?;
        written.push(fixture.name.to_string());
    }
    Ok(written)
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

fn db(value: f32) -> f32 {
    20.0 * value.max(1e-9).log10()
}

pub struct Report {
    pub name: String,
    pub peak_delta_db: f32,
    pub rms_delta_db: f32,
    pub max_abs_delta: f32,
    pub frames: usize,
}

pub fn compare_buffers(name: &str, rust: &[f32], godot: &[f32]) -> Result<Report, String> {
    if rust.len().abs_diff(godot.len()) > 2 {
        return Err(format!(
            "{name}: length differs (rust {} vs godot {})",
            rust.len(),
            godot.len()
        ));
    }
    let frames = rust.len().min(godot.len());
    if frames == 0 {
        return Err(format!("{name}: empty render"));
    }
    let rust = &rust[..frames];
    let godot = &godot[..frames];
    let rust_peak = rust.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
    let godot_peak = godot.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
    let peak_delta = (db(rust_peak) - db(godot_peak)).abs();
    let rms_delta = (db(rms(rust)) - db(rms(godot))).abs();
    let max_abs_delta = rust
        .iter()
        .zip(godot.iter())
        .fold(0.0f32, |delta, (a, b)| delta.max((a - b).abs()));
    if peak_delta > PEAK_TOL_DB || rms_delta > RMS_TOL_DB || max_abs_delta > MAX_ABS_TOL {
        return Err(format!(
            "{name}: peak Δ{peak_delta:.3} dB / RMS Δ{rms_delta:.3} dB / max |Δ| {max_abs_delta:.5} exceeds tolerance ({PEAK_TOL_DB}/{RMS_TOL_DB}/{MAX_ABS_TOL})"
        ));
    }
    Ok(Report {
        name: name.into(),
        peak_delta_db: peak_delta,
        rms_delta_db: rms_delta,
        max_abs_delta,
        frames,
    })
}

pub fn compare(dir: &Path) -> Result<Vec<Report>, String> {
    let mut reports = Vec::new();
    let mut errors = Vec::new();
    for fixture in fixtures() {
        let rust_path = dir.join("rust").join(format!("{}.wav", fixture.name));
        let godot_path = dir.join("godot").join(format!("{}.wav", fixture.name));
        let rust = match wav::load(&rust_path) {
            Ok(data) => data.samples,
            Err(error) => {
                errors.push(format!("{}: rust wav {error}", fixture.name));
                continue;
            }
        };
        let godot = match wav::load(&godot_path) {
            Ok(data) => data.samples,
            Err(error) => {
                errors.push(format!("{}: godot wav {error}", fixture.name));
                continue;
            }
        };
        match compare_buffers(fixture.name, &rust, &godot) {
            Ok(report) => reports.push(report),
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(reports)
    } else {
        Err(errors.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_render_deterministically() {
        for fixture in fixtures() {
            let a = synth::render_patch(&fixture.patch, fixture.patch.rate);
            let b = synth::render_patch(&fixture.patch, fixture.patch.rate);
            assert_eq!(a, b, "{} is not deterministic", fixture.name);
            assert!(!a.is_empty(), "{} is empty", fixture.name);
            assert!(a.iter().all(|sample| sample.is_finite()));
            assert!(a.iter().any(|sample| sample.abs() > 0.0));
        }
    }

    #[test]
    fn comparison_tolerates_identical_buffers_and_rejects_drift() {
        let samples = vec![0.0f32, 0.5, -0.5, 0.25];
        let report = compare_buffers("same", &samples, &samples).expect("identical");
        assert!(report.peak_delta_db < 0.001);
        let mut quiet = samples.clone();
        for sample in &mut quiet {
            *sample *= 0.5;
        }
        assert!(compare_buffers("quiet", &samples, &quiet).is_err());
    }

    #[test]
    fn fixture_roundtrip_through_wav_compare() {
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-parity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_fixtures(&root).expect("fixtures");
        std::fs::create_dir_all(root.join("godot")).unwrap();
        for file in std::fs::read_dir(root.join("rust")).unwrap() {
            let file = file.unwrap();
            std::fs::copy(file.path(), root.join("godot").join(file.file_name())).unwrap();
        }
        let reports = compare(&root).expect("compare");
        assert_eq!(reports.len(), fixtures().len());
        std::fs::remove_dir_all(&root).ok();
    }
}
