//! Demo tracks and the one-shot demo pack.
//!
//! Two kinds of "demo" live here:
//!
//! * **Demo tracks** are three ready-made `Music` arrangements (arcade, boss,
//!   menu) with a handful of `builtin` sound effects placed on the timeline.
//!   They can be loaded from the MUSIC inspector or rendered to WAV.
//! * **The demo pack** (`--demo DIR`) writes a self-contained project with the
//!   whole builtin preset bank, baked assets, one WAV per demo track and a
//!   `reel.wav` that plays every preset in jsfxr order.
//!
//! Rendering is deterministic, so the same seed always produces the same
//! files. The tracks are intentionally short (4–8 bars) so they are cheap to
//! audition inside the editor.

use std::path::Path;

use crate::builtin;
use crate::export;
use crate::fx::{self, Stereo};
use crate::model::{Music, MusicWave, Project};
use crate::presets::PresetBank;
use crate::synth;

/// One sound effect placed over a music bed.
#[derive(Clone, Debug)]
pub struct Cue {
    pub preset: &'static str,
    /// Seconds from the start of the track.
    pub at: f32,
    pub gain: f32,
}

/// A named demo arrangement.
#[derive(Clone, Debug)]
pub struct DemoTrack {
    /// File-system friendly id (`neon_drive`).
    pub key: &'static str,
    /// Inspector label (`NEON DRIVE`).
    pub title: &'static str,
    pub music: Music,
    pub cues: Vec<Cue>,
}

/// The three tracks shipped with the demo pack / MUSIC inspector.
pub fn tracks() -> Vec<DemoTrack> {
    vec![neon_drive(), boss_theme(), star_menu()]
}

/// Resolve a key or title (case/punctuation-insensitive).
pub fn find(name: &str) -> Option<usize> {
    let wanted = builtin::normalize(name);
    tracks().iter().position(|track| {
        builtin::normalize(track.key) == wanted || builtin::normalize(track.title) == wanted
    })
}

fn neon_drive() -> DemoTrack {
    let music = Music {
        bpm: 138.0,
        bars: 4,
        rate: synth::PREVIEW_RATE,
        volume_db: -14.0,
        seed: 13_801,
        bass_notes: vec![110.0, 87.31, 130.81, 98.0],
        chords: vec![
            vec![220.0, 261.63, 329.63],
            vec![174.61, 220.0, 261.63],
            vec![261.63, 329.63, 392.0],
            vec![196.0, 246.94, 293.66],
        ],
        bass_wave: MusicWave::Saw,
        arp_wave: MusicWave::Pulse,
        swing: 0.04,
        use_patterns: true,
        arp_pattern: vec![0, 1, 2, 1, 0, 2, 1, 2, 0, 1, 2, 1, 0, 2, 1, 2],
        bass_pattern: vec![0, -1, 1, -1, 0, -1, 1, 1, 0, -1, 1, -1, 0, -1, 1, -1],
        kick_steps: 0x0101,
        hat_steps: 0x4444,
        bass_gain: 0.28,
        bass_harsh: 1.0,
        bass_decay: 2.0,
        arp_gain: 0.075,
        arp_harsh: 0.45,
        arp_decay: 7.0,
        kick_gain: 0.48,
        hat_gain: 0.05,
        normalize: true,
    };
    let beat = 60.0 / music.bpm;
    let bar = beat * 4.0;
    // Keep the cues inside the 4-bar bed; explosion is the last accent.
    DemoTrack {
        key: "neon_drive",
        title: "NEON DRIVE",
        music,
        cues: vec![
            Cue {
                preset: "pickupCoin",
                at: bar + beat * 2.0,
                gain: 0.55,
            },
            Cue {
                preset: "laserShoot",
                at: bar * 2.0 + beat,
                gain: 0.50,
            },
            Cue {
                preset: "blipSelect",
                at: bar * 3.0 + beat * 3.0,
                gain: 0.45,
            },
        ],
    }
}

fn boss_theme() -> DemoTrack {
    let music = Music {
        bpm: 168.0,
        bars: 8,
        rate: synth::PREVIEW_RATE,
        volume_db: -13.0,
        seed: 16_801,
        bass_notes: vec![73.42, 87.31, 98.0, 65.41],
        chords: vec![
            vec![146.83, 174.61, 220.0],
            vec![174.61, 220.0, 261.63],
            vec![196.0, 246.94, 293.66],
            vec![130.81, 164.81, 196.0],
        ],
        bass_wave: MusicWave::Square,
        arp_wave: MusicWave::Saw,
        swing: 0.08,
        use_patterns: true,
        arp_pattern: vec![0, 2, 1, 2, 0, 1, 2, 1, 0, 2, 1, 2, 0, 1, 2, 0],
        bass_pattern: vec![0, -1, 0, 1, 0, -1, 1, -1, 0, -1, 0, 1, 1, -1, 0, -1],
        kick_steps: 0x1111,
        hat_steps: 0x8888,
        bass_gain: 0.30,
        bass_harsh: 0.85,
        bass_decay: 2.6,
        arp_gain: 0.070,
        arp_harsh: 0.30,
        arp_decay: 8.0,
        kick_gain: 0.42,
        hat_gain: 0.06,
        normalize: true,
    };
    let beat = 60.0 / music.bpm;
    let bar = beat * 4.0;
    DemoTrack {
        key: "boss_theme",
        title: "BOSS THEME",
        music,
        cues: vec![
            Cue {
                preset: "hitHurt",
                at: bar * 2.0,
                gain: 0.50,
            },
            Cue {
                preset: "laserShoot",
                at: bar * 4.0 + beat * 2.0,
                gain: 0.48,
            },
            Cue {
                preset: "powerUp",
                at: bar * 6.0,
                gain: 0.42,
            },
            Cue {
                preset: "explosion",
                at: bar * 7.0 + beat * 2.0,
                gain: 0.38,
            },
        ],
    }
}

fn star_menu() -> DemoTrack {
    let music = Music {
        bpm: 92.0,
        bars: 4,
        rate: synth::PREVIEW_RATE,
        volume_db: -16.0,
        seed: 9_201,
        bass_notes: vec![130.81, 98.0, 110.0, 87.31],
        chords: vec![
            vec![261.63, 329.63, 392.0, 493.88],
            vec![196.0, 246.94, 293.66, 392.0],
            vec![220.0, 261.63, 329.63, 392.0],
            vec![174.61, 220.0, 261.63, 349.23],
        ],
        bass_wave: MusicWave::Triangle,
        arp_wave: MusicWave::Sine,
        swing: 0.0,
        use_patterns: true,
        arp_pattern: vec![0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3],
        bass_pattern: vec![0, -1, -1, -1, 1, -1, -1, -1, 0, -1, -1, -1, 1, -1, -1, -1],
        kick_steps: 0x0011,
        hat_steps: 0x1010,
        bass_gain: 0.22,
        bass_harsh: 0.20,
        bass_decay: 1.6,
        arp_gain: 0.055,
        arp_harsh: 0.0,
        arp_decay: 5.0,
        kick_gain: 0.30,
        hat_gain: 0.035,
        normalize: true,
    };
    let beat = 60.0 / music.bpm;
    let bar = beat * 4.0;
    DemoTrack {
        key: "star_menu",
        title: "STAR MENU",
        music,
        cues: vec![
            Cue {
                preset: "blipSelect",
                at: bar + beat * 2.0,
                gain: 0.55,
            },
            Cue {
                preset: "click",
                at: bar * 2.0 + beat,
                gain: 0.50,
            },
            Cue {
                preset: "powerUp",
                at: bar * 3.0 + beat * 2.0,
                gain: 0.40,
            },
        ],
    }
}

/// Render a track: the music bed plus its sound-effect cues, normalised to
/// −0.9 dBFS-ish so a demo WAV never clips.
pub fn render_track(track: &DemoTrack, seed: u32, rate: u32) -> Stereo {
    let rate = rate.clamp(8_000, 96_000);
    let music = synth::render_music(&track.music, rate);
    let mut out = Stereo::from_mono(music);
    for cue in &track.cues {
        let salt = (cue.at * 1_000.0) as u32;
        let Some(patch) = builtin::generate(cue.preset, seed.wrapping_add(salt)) else {
            continue;
        };
        let effect = fx::render_patch(&patch, rate);
        mix_at(
            &mut out,
            &effect,
            (cue.at * rate as f32).round() as usize,
            cue.gain,
        );
    }
    normalize(&mut out, 0.90);
    out
}

/// Render every preset in jsfxr order with short gaps: a quick audition reel.
pub fn render_reel(seed: u32, rate: u32) -> Stereo {
    let rate = rate.clamp(8_000, 96_000);
    let mut out = Stereo::default();
    for (index, name) in builtin::PRESET_NAMES.iter().enumerate() {
        let patch_seed = seed.wrapping_add((index as u32).wrapping_mul(7_919));
        let Some(patch) = builtin::generate(name, patch_seed) else {
            continue;
        };
        let effect = fx::render_patch(&patch, rate);
        let gap = if index == 0 { 0.05 } else { 0.18 };
        let gap_frames = (gap * rate as f32) as usize;
        out.left.resize(out.left.len() + gap_frames, 0.0);
        out.right.resize(out.right.len() + gap_frames, 0.0);
        let offset = out.left.len();
        mix_at(&mut out, &effect, offset, 0.95);
    }
    normalize(&mut out, 0.90);
    out
}

/// Write a self-contained demo pack: project + presets + baked sounds +
/// three demo tracks + the preset reel. Refuses to replace an existing
/// project so a second run cannot destroy edits.
pub fn write_pack(root: &Path, seed: u32) -> Result<String, String> {
    if root.join(crate::model::PROJECT_FILE).exists() {
        return Err(format!(
            "{} already contains {}",
            root.display(),
            crate::model::PROJECT_FILE
        ));
    }
    let preset_dir = root.join(crate::presets::PRESETS_DIR);
    if preset_dir.exists()
        && std::fs::read_dir(&preset_dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    {
        return Err(format!(
            "{} already contains files in {}/",
            root.display(),
            crate::presets::PRESETS_DIR
        ));
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("demo");
    let tracks = tracks();
    let mut project = Project {
        name: format!("{name} demo"),
        patches: builtin::bank(seed),
        music: tracks[0].music.clone(),
        ..Project::default()
    };
    project.validate();
    crate::model::save_project(root, &project).map_err(|error| error.to_string())?;
    PresetBank::write_presets(root, &project.patches).map_err(|error| error.to_string())?;
    export::bake(&project, root)?;

    let demo_dir = root.join("audio").join("demo");
    std::fs::create_dir_all(&demo_dir).map_err(|error| error.to_string())?;
    let rate = synth::PREVIEW_RATE;
    let mut track_index = Vec::new();
    for track in &tracks {
        let stereo = render_track(track, seed, rate);
        let file = format!("{}.wav", track.key);
        synth::write_wav_stereo(&demo_dir.join(&file), &stereo.left, &stereo.right, rate, 24)
            .map_err(|error| error.to_string())?;
        track_index.push(serde_json::json!({
            "key": track.key,
            "title": track.title,
            "wav": format!("res://audio/demo/{file}"),
            "bpm": track.music.bpm,
            "bars": track.music.bars,
            "duration": stereo.frames() as f32 / rate as f32,
        }));
    }
    let reel = render_reel(seed, rate);
    synth::write_wav_stereo(
        &demo_dir.join("reel.wav"),
        &reel.left,
        &reel.right,
        rate,
        24,
    )
    .map_err(|error| error.to_string())?;

    let index = serde_json::json!({
        "seed": seed,
        "rate": rate,
        "bits": 24,
        "channels": 2,
        "presets": builtin::PRESET_NAMES
            .iter()
            .map(|name| builtin::patch_name(name))
            .collect::<Vec<_>>(),
        "tracks": track_index,
        "reel": {
            "wav": "res://audio/demo/reel.wav",
            "duration": reel.frames() as f32 / rate as f32,
        }
    });
    let text = serde_json::to_string_pretty(&index).map_err(|error| error.to_string())?;
    crate::model::write_atomic(&demo_dir.join("index.json"), text.as_bytes())
        .map_err(|error| error.to_string())?;

    Ok(format!(
        "demo pack → {} · {} presets · {} tracks + reel · 48 kHz / 24-bit stereo",
        root.display(),
        project.patches.len(),
        tracks.len()
    ))
}

fn mix_at(out: &mut Stereo, effect: &Stereo, offset: usize, gain: f32) {
    let frames = effect.frames();
    let end = offset + frames;
    if out.left.len() < end {
        out.left.resize(end, 0.0);
        out.right.resize(end, 0.0);
    }
    for index in 0..frames {
        out.left[offset + index] += effect.left[index] * gain;
        out.right[offset + index] += effect.right[index] * gain;
    }
}

fn normalize(stereo: &mut Stereo, target: f32) {
    let peak = synth::peak(&stereo.left).max(synth::peak(&stereo.right));
    if peak > 0.0001 {
        let scale = (target / peak).min(8.0);
        for sample in &mut stereo.left {
            *sample *= scale;
        }
        for sample in &mut stereo.right {
            *sample *= scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("sfx-studio-{label}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_three_tracks_have_unique_keys_and_a_beat() {
        let tracks = tracks();
        assert_eq!(tracks.len(), 3);
        let keys: std::collections::HashSet<_> = tracks.iter().map(|track| track.key).collect();
        assert_eq!(keys.len(), tracks.len());
        for track in &tracks {
            assert!(track.music.bpm >= 40.0);
            assert!(track.music.bars >= 1);
            assert!(!track.music.bass_notes.is_empty());
            assert!(!track.music.chords.is_empty());
            assert!(track.music.length_seconds() > 0.0);
        }
        assert_eq!(find("neon_drive"), Some(0));
        assert_eq!(find("Neon Drive"), Some(0));
        assert_eq!(find("STAR-MENU"), Some(2));
        assert_eq!(find("nope"), None);
    }

    #[test]
    fn rendered_tracks_are_finite_audible_and_cover_the_cues() {
        let rate = 24_000;
        for track in tracks() {
            let stereo = render_track(&track, 3, rate);
            let frames = stereo.frames();
            let music_frames = (track.music.length_seconds() * rate as f32) as usize;
            assert!(frames >= music_frames, "{} lost music", track.key);
            let peak = synth::peak(&stereo.left).max(synth::peak(&stereo.right));
            assert!(peak > 0.05, "{} is silent", track.key);
            assert!(peak <= 1.0, "{} clips: {peak}", track.key);
            assert!(stereo.interleave().iter().all(|sample| sample.is_finite()));
        }
    }

    #[test]
    fn reel_visits_every_preset_in_order() {
        let rate = 24_000;
        let reel = render_reel(9, rate);
        let frames = reel.frames();
        let first = fx::render_patch(&builtin::generate("pickupCoin", 9).expect("preset"), rate);
        assert!(frames > first.frames());
        assert!(synth::peak(&reel.mono()) > 0.05);
        assert!(reel.interleave().iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn demo_pack_writes_a_loadable_project_presets_tracks_and_reel() {
        let root = temp_dir("demo-pack");
        let message = write_pack(&root, 21).expect("demo pack");
        assert!(message.contains("demo pack"), "{message}");
        assert!(root.join(model::PROJECT_FILE).exists());
        let project = model::load_project(&root).expect("load").expect("project");
        assert_eq!(project.patches.len(), builtin::PRESET_NAMES.len());
        assert_eq!(
            std::fs::read_dir(root.join("presets"))
                .expect("presets")
                .count(),
            builtin::PRESET_NAMES.len()
        );
        for track in tracks() {
            assert!(
                root.join("audio")
                    .join("demo")
                    .join(format!("{}.wav", track.key))
                    .exists(),
                "{}",
                track.key
            );
        }
        assert!(root.join("audio/demo/reel.wav").exists());
        assert!(root.join("audio/demo/index.json").exists());
        // The generated project must be openable by the regular loader.
        let (loaded, warnings) = crate::project_io::load_folder(&root).expect("open demo");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(loaded.music.bpm, tracks()[0].music.bpm);
        // Refuses to overwrite an existing project or a preset library.
        assert!(write_pack(&root, 21).is_err());
        let other = temp_dir("demo-pack-presets");
        std::fs::create_dir_all(other.join("presets")).expect("presets");
        std::fs::write(other.join("presets").join("x.json"), b"{}").expect("file");
        assert!(write_pack(&other, 21).is_err());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }
}
