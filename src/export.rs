//! Export the project for Godot:
//! - baked mono WAVs + `audio/sfx_manifest.json`
//! - `.tres` resources for the `sfx_studio` addon

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use crate::fx;
use crate::model::{self, Layer, Music, Patch, Project};
use crate::synth;

/// Deterministic, collision-free file stems for the whole project. Two patches
/// called "coin" (or two Chinese names sanitising to "sound") must not
/// overwrite each other's WAV / `.tres`.
pub fn unique_names(project: &Project) -> Vec<String> {
    let mut used = HashSet::new();
    let mut names = Vec::with_capacity(project.patches.len());
    for patch in &project.patches {
        let base = patch.safe_name();
        let mut name = base.clone();
        let mut suffix = 2;
        while !used.insert(name.clone()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        names.push(name);
    }
    names
}

/// Render one patch (project sound or project-local preset) for the manual
/// WAV export path: 48 kHz, 24-bit stereo, deterministic TPDF dither.
pub fn write_patch_wav(patch: &Patch, path: &Path) -> Result<f32, String> {
    let rate = synth::PREVIEW_RATE;
    let stereo = fx::render_patch(patch, rate);
    synth::write_wav_stereo(path, &stereo.left, &stereo.right, rate, 24)
        .map_err(|error| error.to_string())?;
    Ok(stereo.frames() as f32 / rate as f32)
}

pub fn bake(project: &Project, root: &Path) -> Result<String, String> {
    let baked = root.join("audio").join("baked");
    std::fs::create_dir_all(&baked).map_err(|e| e.to_string())?;
    let names = unique_names(project);

    let mut entries = Vec::new();
    for (patch, name) in project.patches.iter().zip(names.iter()) {
        let rate = patch.bake_rate();
        let stereo = fx::render_patch(patch, rate);
        write_baked_wav(&baked.join(format!("{name}.wav")), &stereo, rate)?;
        entries.push(serde_json::json!({
            "name": name,
            "patch": patch.name,
            "category": patch.category.group(),
            "favorite": patch.favorite,
            "dsp_version": patch.dsp_version,
            "space": patch.space,
            "room": patch.room,
            "wav": format!("res://audio/baked/{name}.wav"),
            "rate": rate,
            "channels": 2,
            "bits": 16,
            "duration": stereo.frames() as f32 / rate as f32,
            "loop": false
        }));
    }

    let music_samples = synth::render_music(&project.music, project.music.rate);
    let music_wav = baked.join("music.wav");
    synth::write_wav(&music_wav, &music_samples, project.music.rate).map_err(|e| e.to_string())?;
    model::write_atomic(
        &music_wav.with_extension("wav.import"),
        b"[params]\n\ncompress/mode=0\n",
    )
    .map_err(|e| e.to_string())?;

    let manifest = serde_json::json!({
        "version": 2,
        "schema_version": model::PROJECT_VERSION,
        "project": project.name,
        "sounds": entries,
        "music": {
            "wav": "res://audio/baked/music.wav",
            "rate": project.music.rate,
            "duration": music_samples.len() as f32 / project.music.rate as f32,
            "loop": true,
            "db": project.music.volume_db
        }
    });
    write_json(&root.join("audio").join("sfx_manifest.json"), &manifest)?;
    Ok(format!(
        "baked {} patch(es) + music → audio/baked/",
        project.patches.len()
    ))
}

pub fn export_tres(project: &Project, root: &Path) -> Result<String, String> {
    let dir = root.join("audio").join("sfx");
    let baked_dir = root.join("audio").join("baked");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&baked_dir).map_err(|e| e.to_string())?;
    let names = unique_names(project);
    let mut baked_count = 0;
    for (patch, name) in project.patches.iter().zip(names.iter()) {
        let needs_bake = patch.needs_baked_tres();
        let text = if needs_bake {
            // Automation / HD-only routing cannot be flattened to SfxLayer.
            // Bake the exact result and point `baked` at it; the layer
            // parameters stay in the file.
            let rate = patch.bake_rate();
            let stereo = fx::render_patch(patch, rate);
            write_baked_wav(&baked_dir.join(format!("{name}.wav")), &stereo, rate)?;
            baked_count += 1;
            patch_tres_baked(patch, &format!("res://audio/baked/{name}.wav"))
        } else {
            patch_tres(patch)
        };
        let path = dir.join(format!("{name}.tres"));
        model::write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string())?;
    }
    model::write_atomic(
        &root.join("audio").join("music.tres"),
        music_tres(&project.music).as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(format!(
        "exported {} .tres patch(es) + music.tres ({baked_count} baked: v3/automation/sample)",
        project.patches.len()
    ))
}

fn write_baked_wav(path: &Path, stereo: &fx::Stereo, rate: u32) -> Result<(), String> {
    // 16-bit stereo is accepted by every Godot 4 WAV import path.
    synth::write_wav_stereo(path, &stereo.left, &stereo.right, rate, 16)
        .map_err(|e| e.to_string())?;
    // Force PCM import. Godot 4.7's default WAV import is QOA (lossy); a minimal
    // sidecar with `compress/mode=0` makes the editor fill the rest and keep PCM.
    let sidecar = path.with_extension("wav.import");
    model::write_atomic(&sidecar, b"[params]\n\ncompress/mode=0\n").map_err(|e| e.to_string())
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    model::write_atomic(path, text.as_bytes()).map_err(|e| e.to_string())
}

pub fn patch_tres(patch: &Patch) -> String {
    patch_tres_inner(patch, None)
}

/// A patch with automation exports the exact rendered WAV as `baked`.
pub fn patch_tres_baked(patch: &Patch, wav_path: &str) -> String {
    patch_tres_inner(patch, Some(wav_path))
}

fn patch_tres_inner(patch: &Patch, baked: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("[gd_resource type=\"Resource\" script_class=\"SfxPatch\" format=3]\n\n");
    if let Some(path) = baked {
        let _ = writeln!(
            out,
            "[ext_resource type=\"AudioStreamWAV\" path=\"{path}\" id=\"1_audio\"]"
        );
    }
    out.push_str("[ext_resource type=\"Script\" path=\"res://addons/sfx_studio/core/sfx_layer.gd\" id=\"1_layer\"]\n");
    out.push_str("[ext_resource type=\"Script\" path=\"res://addons/sfx_studio/core/sfx_patch.gd\" id=\"2_patch\"]\n\n");

    let mut ids = Vec::new();
    for (index, layer) in patch.layers.iter().enumerate() {
        if layer.clips.is_empty() {
            let id = format!("layer_{index}");
            ids.push(id.clone());
            let _ = writeln!(out, "[sub_resource type=\"Resource\" id=\"{id}\"]");
            out.push_str("script = ExtResource(\"1_layer\")\n");
            write_layer(&mut out, layer);
            out.push('\n');
        } else {
            for (clip_index, clip) in layer.clips.iter().enumerate() {
                let id = format!("layer_{index}_{clip_index}");
                ids.push(id.clone());
                let mut copy = layer.with_pitch(clip.pitch);
                copy.delay = clip.start;
                copy.duration = clip.duration.min(layer.duration);
                copy.gain = layer.gain * clip.gain;
                copy.clips.clear();
                let _ = writeln!(out, "[sub_resource type=\"Resource\" id=\"{id}\"]");
                out.push_str("script = ExtResource(\"1_layer\")\n");
                write_layer(&mut out, &copy);
                out.push('\n');
            }
        }
    }
    out.push_str("[resource]\n");
    out.push_str("script = ExtResource(\"2_patch\")\n");
    if baked.is_some() {
        out.push_str("baked = ExtResource(\"1_audio\")\n");
    }
    let _ = writeln!(out, "rate = {}", patch.bake_rate());
    let _ = writeln!(out, "normalize_layers = {}", bool_lit(patch.normalize));
    let _ = writeln!(out, "resource_name = \"{}\"", quote(&patch.name));
    let _ = writeln!(out, "group = \"{}\"", quote(patch.category.group()));
    out.push_str("layers = Array[ExtResource(\"1_layer\")]([");
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "SubResource(\"{id}\")");
    }
    out.push_str("])\n");
    out
}

fn write_layer(out: &mut String, layer: &Layer) {
    // `Curve` has no native Godot enum value and is baked on export; its
    // documented alias must still round-trip to the same integer.
    debug_assert!(layer
        .wave
        .godot_alias()
        .is_none_or(|alias| alias.godot_id() == layer.wave.godot_id()));
    let _ = writeln!(out, "wave = {}", layer.wave.godot_id());
    let _ = writeln!(out, "enabled = {}", bool_lit(layer.enabled));
    let _ = writeln!(out, "tone_env = {}", layer.tone_env.godot_id());
    let _ = writeln!(out, "exp_decay = {}", f(layer.exp_decay));
    let _ = writeln!(out, "delay = {}", f(layer.delay));
    let _ = writeln!(out, "duration = {}", f(layer.duration));
    let _ = writeln!(out, "gain = {}", f(layer.gain));
    let _ = writeln!(out, "freq_start = {}", f(layer.freq_start));
    let _ = writeln!(out, "freq_end = {}", f(layer.freq_end));
    let _ = writeln!(out, "harsh = {}", f(layer.harsh));
    let _ = writeln!(out, "duty = {}", f(layer.duty));
    let _ = writeln!(out, "vibrato_rate = {}", f(layer.vibrato_rate));
    let _ = writeln!(out, "vibrato_depth = {}", f(layer.vibrato_depth));
    let _ = writeln!(out, "attack = {}", f(layer.attack));
    let _ = writeln!(out, "decay = {}", f(layer.decay));
    let _ = writeln!(out, "sustain = {}", f(layer.sustain));
    let _ = writeln!(out, "release = {}", f(layer.release));
    let _ = writeln!(out, "filter_mode = {}", layer.filter as u32);
    let _ = writeln!(out, "cutoff_start = {}", f(layer.cutoff_start));
    let _ = writeln!(out, "cutoff_end = {}", f(layer.cutoff_end));
    let _ = writeln!(out, "resonance = {}", f(layer.resonance));
    let _ = writeln!(out, "arp_freqs = {}", float_array(&layer.arp_freqs));
    let _ = writeln!(out, "step = {}", f(layer.step));
    let _ = writeln!(out, "lowpass_from = {}", f(layer.lowpass_from));
    let _ = writeln!(out, "lowpass_to = {}", f(layer.lowpass_to));
    let _ = writeln!(out, "noise_gain = {}", f(layer.noise_gain));
    let _ = writeln!(out, "sweep_gain = {}", f(layer.sweep_gain));
    let _ = writeln!(out, "env_shape = {}", layer.env_shape as u32);
    let _ = writeln!(out, "seed_value = {}", layer.seed);
    let _ = writeln!(out, "fm_ratio = {}", f(layer.fm_ratio));
    let _ = writeln!(out, "fm_index = {}", f(layer.fm_index));
    let _ = writeln!(out, "fm_decay = {}", f(layer.fm_decay));
    let _ = writeln!(out, "pluck_damping = {}", f(layer.pluck_damping));
    let _ = writeln!(out, "pluck_brightness = {}", f(layer.pluck_brightness));
    let _ = writeln!(out, "drum_tone = {}", f(layer.drum_tone));
    let _ = writeln!(out, "drum_snap = {}", f(layer.drum_snap));
}

pub fn music_tres(music: &Music) -> String {
    let mut out = String::new();
    out.push_str("[gd_resource type=\"Resource\" script_class=\"SfxMusic\" format=3]\n\n");
    out.push_str("[ext_resource type=\"Script\" path=\"res://addons/sfx_studio/core/sfx_music.gd\" id=\"1_music\"]\n\n");
    out.push_str("[resource]\n");
    out.push_str("script = ExtResource(\"1_music\")\n");
    let _ = writeln!(out, "bpm = {}", f(music.bpm));
    let _ = writeln!(out, "bars = {}", music.bars);
    let _ = writeln!(out, "rate = {}", music.rate);
    let _ = writeln!(out, "volume_db = {}", f(music.volume_db));
    let _ = writeln!(out, "seed_value = {}", music.seed);
    let _ = writeln!(out, "bass_notes = {}", float_array(&music.bass_notes));
    out.push_str("chords = Array[PackedFloat64Array]([");
    for (index, chord) in music.chords.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&float_array(chord));
    }
    out.push_str("])\n");
    let _ = writeln!(out, "bass_wave = {}", music.bass_wave as u32);
    let _ = writeln!(out, "arp_wave = {}", music.arp_wave as u32);
    let _ = writeln!(out, "swing = {}", f(music.swing));
    let _ = writeln!(out, "use_patterns = {}", bool_lit(music.use_patterns));
    let _ = writeln!(out, "arp_pattern = {}", int_array(&music.arp_pattern));
    let _ = writeln!(out, "bass_pattern = {}", int_array(&music.bass_pattern));
    let _ = writeln!(out, "kick_steps = {}", music.kick_steps);
    let _ = writeln!(out, "hat_steps = {}", music.hat_steps);
    let _ = writeln!(out, "bass_gain = {}", f(music.bass_gain));
    let _ = writeln!(out, "bass_harsh = {}", f(music.bass_harsh));
    let _ = writeln!(out, "bass_decay = {}", f(music.bass_decay));
    let _ = writeln!(out, "arp_gain = {}", f(music.arp_gain));
    let _ = writeln!(out, "arp_harsh = {}", f(music.arp_harsh));
    let _ = writeln!(out, "arp_decay = {}", f(music.arp_decay));
    let _ = writeln!(out, "kick_gain = {}", f(music.kick_gain));
    let _ = writeln!(out, "hat_gain = {}", f(music.hat_gain));
    let _ = writeln!(out, "normalize = {}", bool_lit(music.normalize));
    out
}

fn f(value: f32) -> String {
    let mut text = format!("{value:.5}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

fn bool_lit(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn float_array(values: &[f32]) -> String {
    let mut out = String::from("PackedFloat64Array(");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&f(*value));
    }
    out.push(')');
    out
}

fn int_array(values: &[i32]) -> String {
    let mut out = String::from("PackedInt32Array(");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{value}");
    }
    out.push(')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tres;

    #[test]
    fn patch_tres_roundtrips_through_the_reader() {
        let patch = crate::model::Patch::new_layer("laser", crate::model::Wave::Pulse);
        let text = patch_tres(&patch);
        assert!(text.contains("script_class=\"SfxPatch\""));
        assert!(text.contains("wave = 3"));
        let back = tres::parse_patch(&text).expect("parse");
        assert_eq!(back.layers.len(), 1);
        assert_eq!(back.layers[0].wave, crate::model::Wave::Pulse);
    }

    #[test]
    fn clips_are_flattened_to_layers_for_godot() {
        use crate::model::{Clip, Layer, Patch, Wave};
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.1;
        layer.clips = vec![
            Clip {
                start: 0.0,
                duration: 0.1,
                gain: 1.0,
                ..Clip::default()
            },
            Clip {
                start: 0.2,
                duration: 0.1,
                gain: 0.5,
                ..Clip::default()
            },
        ];
        let patch = Patch {
            name: "clipped".into(),
            layers: vec![layer],
            ..Patch::default()
        };
        let text = patch_tres(&patch);
        let back = tres::parse_patch(&text).expect("parse");
        assert_eq!(back.layers.len(), 2);
        assert!((back.layers[1].delay - 0.2).abs() < 0.001);
        assert!((back.layers[1].gain - 0.15).abs() < 0.001);
    }

    #[test]
    fn clip_pitch_is_applied_when_flattened() {
        use crate::model::{Clip, Layer, Patch, Wave};
        let mut layer = Layer::new(Wave::Sine);
        layer.freq_start = 440.0;
        layer.freq_end = 440.0;
        layer.duration = 0.1;
        layer.clips = vec![Clip {
            start: 0.0,
            duration: 0.1,
            pitch: 12.0,
            ..Clip::default()
        }];
        let patch = Patch {
            name: "pitch".into(),
            layers: vec![layer],
            ..Patch::default()
        };
        let back = tres::parse_patch(&patch_tres(&patch)).expect("parse");
        assert!((back.layers[0].freq_start - 880.0).abs() < 0.01);
    }

    #[test]
    fn sample_patches_bake_for_tres_export() {
        use crate::model::{Patch, Wave};
        let root = temp_project_root("sample-export");
        let mut project = crate::model::Project::demo("sample-export");
        let mut patch = Patch::new_layer("sample", Wave::Sample);
        let layer = &mut patch.layers[0];
        layer.duration = 4.0 / 48_000.0;
        layer.gain = 1.0;
        layer.attack = 0.0;
        layer.decay = 0.0;
        layer.sustain = 1.0;
        layer.release = 0.0;
        layer.sample_loop = true;
        layer.sample_path = "audio/sfx/samples/x.wav".into();
        layer.sample_data = Some(std::sync::Arc::new(crate::wav::SampleData {
            rate: 48_000,
            samples: vec![0.0, 0.5, -0.5, 1.0],
        }));
        project.patches.push(patch);

        export_tres(&project, &root).expect("tres");
        assert!(root.join("audio/baked/sample.wav").exists());
        let text = std::fs::read_to_string(root.join("audio/sfx/sample.tres")).unwrap();
        assert!(text.contains("baked = ExtResource"), "{text}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn v3_patch_bakes_for_tres_export() {
        use crate::model::{Layer, Patch, Wave};
        let root = temp_project_root("v3-export");
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.05;
        layer.pan = 0.4;
        let mut patch = Patch {
            name: "hd".into(),
            layers: vec![layer],
            ..Patch::default()
        };
        patch.space = 0.3;
        let mut project = crate::model::Project::demo("v3-export");
        project.patches = vec![patch];
        export_tres(&project, &root).expect("tres");
        let text = std::fs::read_to_string(root.join("audio/sfx/hd.tres")).unwrap();
        assert!(text.contains("baked = ExtResource"), "{text}");
        assert!(root.join("audio/baked/hd.wav").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn automated_patch_uses_baked_wav_reference() {
        use crate::model::{AutomationPoint, Clip, Layer, Patch, Wave};
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.1;
        layer.clips = vec![Clip {
            start: 0.0,
            duration: 0.1,
            gain: 1.0,
            ..Clip::default()
        }];
        layer.clips[0].gain_points = vec![
            AutomationPoint {
                time: 0.0,
                value: 0.0,
            },
            AutomationPoint {
                time: 0.1,
                value: 1.0,
            },
        ];
        let patch = Patch {
            name: "auto".into(),
            layers: vec![layer],
            ..Patch::default()
        };
        assert!(patch.has_automation());
        let text = patch_tres_baked(&patch, "res://audio/baked/auto.wav");
        assert!(text.contains("baked = ExtResource(\"1_audio\")"));
        assert!(text.contains("type=\"AudioStreamWAV\""));
        assert!(text.contains("path=\"res://audio/baked/auto.wav\""));
    }

    #[test]
    fn curve_patches_bake_on_tres_export() {
        use crate::model::{Layer, Patch, Wave};
        let root = temp_project_root("curve-export");
        let mut layer = Layer::new(Wave::Curve);
        layer.duration = 0.05;
        layer.curve = vec![0.0, 1.0, -1.0, 0.0];
        let patch = Patch {
            name: "curve".into(),
            layers: vec![layer],
            ..Patch::default()
        };
        assert!(patch.needs_baked_tres());
        let mut project = crate::model::Project::demo("curve");
        project.patches = vec![patch];
        export_tres(&project, &root).expect("tres");
        assert!(root.join("audio/baked/curve.wav").exists());
        let text = std::fs::read_to_string(root.join("audio/sfx/curve.tres")).unwrap();
        assert!(text.contains("baked = ExtResource"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unique_names_never_overwrite() {
        use crate::model::{Patch, Wave};
        let mut project = crate::model::Project::demo("names");
        project.patches = vec![
            Patch::new_layer("Coin", Wave::Sine),
            Patch::new_layer("coin", Wave::Sine),
            Patch::new_layer("金币", Wave::Sine),
            Patch::new_layer("金币 2", Wave::Sine),
        ];
        let names = unique_names(&project);
        assert_eq!(names, vec!["coin", "coin_2", "sound", "sound_2"]);
    }

    #[test]
    fn patch_metadata_and_tone_env_roundtrip() {
        use crate::model::{Category, Layer, Patch, ToneEnv, Wave};
        let mut layer = Layer::new(Wave::Sine);
        layer.tone_env = ToneEnv::Exp;
        layer.exp_decay = 7.5;
        let patch = Patch {
            name: "Laser Beam".into(),
            category: Category::Magic,
            normalize: false,
            layers: vec![layer],
            ..Patch::default()
        };
        let text = patch_tres(&patch);
        assert!(text.contains("resource_name = \"Laser Beam\""));
        assert!(text.contains("group = \"Magic\""));
        assert!(text.contains("normalize_layers = false"));
        let back = tres::parse_patch(&text).expect("parse");
        assert_eq!(back.name, "Laser Beam");
        assert_eq!(back.category, Category::Magic);
        assert!(!back.normalize);
        // `.tres` can only express the live v2 graph.
        assert_eq!(back.dsp_version, 2);
        assert_eq!(back.layers[0].tone_env, ToneEnv::Exp);
        assert!((back.layers[0].exp_decay - 7.5).abs() < 0.001);
    }

    fn temp_project_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn bake_and_export_write_unique_assets_and_roundtrip_metadata() {
        use crate::model::{Category, Patch, Wave};
        let root = temp_project_root("export");
        let mut project = crate::model::Project::demo("export");
        project.patches.push(Patch::new_layer("coin", Wave::Sine));
        project
            .patches
            .push(Patch::new_layer("Laser Beam", Wave::Square));
        project.patches.last_mut().unwrap().category = Category::Magic;
        project.patches.last_mut().unwrap().favorite = true;

        bake(&project, &root).expect("bake");
        assert!(root.join("audio/baked/music.wav").exists());
        assert!(root.join("audio/baked/music.wav.import").exists());
        assert!(root.join("audio/baked/laser_beam.wav").exists());
        assert!(root.join("audio/sfx_manifest.json").exists());
        let baked = std::fs::read(root.join("audio/baked/laser_beam.wav")).unwrap();
        assert_eq!(&baked[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes(baked[22..24].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(baked[34..36].try_into().unwrap()), 16);
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("audio/sfx_manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["schema_version"], crate::model::PROJECT_VERSION);
        let coin = manifest["sounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sound| sound["name"] == "coin")
            .expect("coin entry");
        assert_eq!(coin["channels"], 2);
        assert_eq!(coin["bits"], 16);
        assert!(coin["dsp_version"].as_u64().unwrap() >= 3);
        let names: Vec<_> = manifest["sounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sound| sound["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"coin"));
        assert!(names.contains(&"laser_beam"));

        export_tres(&project, &root).expect("tres");
        assert!(
            root.join("audio/sfx/music.tres").exists() || root.join("audio/music.tres").exists()
        );
        let laser_tres =
            std::fs::read_to_string(root.join("audio/sfx/laser_beam.tres")).expect("laser tres");
        assert!(
            laser_tres.contains("rate = 48000"),
            "v3 baked .tres must advertise the 48 kHz bake rate: {laser_tres}"
        );
        let imported = crate::tres::import_project(&root).unwrap().unwrap();
        let magic = imported
            .patches
            .iter()
            .find(|patch| patch.name == "Laser Beam")
            .expect("exported Laser Beam");
        assert_eq!(magic.category, Category::Magic);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bake_manifest_duration_matches_the_written_wav() {
        use crate::model::Project;
        let root = temp_project_root("manifest-duration");
        let mut project = Project::demo("manifest-duration");
        let mut patch = crate::builtin::generate("pickupCoin", 3).expect("builtin preset");
        patch.name = "coin".into();
        // Legacy field is still 22050 on generated patches; v3 must bake at 48k.
        assert_eq!(patch.rate, 22_050);
        project.patches = vec![patch];

        bake(&project, &root).expect("bake");
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("audio/sfx_manifest.json")).unwrap(),
        )
        .unwrap();
        let entry = &manifest["sounds"][0];
        let duration = entry["duration"].as_f64().expect("duration");
        assert_eq!(entry["rate"].as_u64(), Some(48_000));

        let wav = std::fs::read(root.join("audio/baked/coin.wav")).expect("baked wav");
        let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
        let frames = (wav.len() - 44) / 4;
        let actual = frames as f64 / rate as f64;
        assert_eq!(rate, 48_000);
        assert!(
            (duration - actual).abs() < 0.001,
            "manifest duration {duration} must match WAV duration {actual}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn preset_wav_export_is_48k_24bit_stereo() {
        let root = temp_project_root("preset-wav");
        let patch = crate::builtin::generate("pickupCoin", 3).expect("builtin preset");
        let path = root.join("coin.wav");
        let seconds = write_patch_wav(&patch, &path).expect("write preset wav");
        assert!(seconds > 0.05, "{seconds}");

        let bytes = std::fs::read(&path).expect("read wav");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 24);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn music_tres_contains_patterns() {
        let text = music_tres(&crate::model::Music::default());
        assert!(text.contains("bass_notes = PackedFloat64Array"));
        assert!(text.contains("arp_pattern = PackedInt32Array"));
        assert!(tres::parse_music(&text).is_some());
    }
}
