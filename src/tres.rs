//! Small `.tres` reader for the resources written by the Godot plugin.
//!
//! It is intentionally narrow: it understands the flat resource files this
//! project produces (`SfxPatch` with layer sub-resources, `SfxMusic`, and the
//! legacy single-kind patch fields). Anything else is skipped.

use std::collections::HashMap;
use std::path::Path;

use crate::model::{
    Category, EnvShape, FilterMode, Layer, Music, MusicWave, Patch, Project, ToneEnv, Wave,
    PROJECT_VERSION,
};

/// Import a Godot project folder. `Ok(None)` means the folder has no SFX
/// resources at all; a malformed project JSON is an error, never a silent demo.
pub fn import_project(root: &Path) -> Result<Option<Project>, String> {
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();
    let mut project = Project {
        version: PROJECT_VERSION,
        next_id: 1,
        name: name.clone(),
        patches: Vec::new(),
        music: Music::default(),
    };

    let sfx_dir = root.join("audio").join("sfx");
    if let Ok(entries) = std::fs::read_dir(&sfx_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("tres") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some(mut patch) = parse_patch(&text) {
                    if patch.name.is_empty() {
                        patch.name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("sound")
                            .to_string();
                    }
                    project.patches.push(patch);
                }
            }
        }
    }
    project.patches.sort_by(|a, b| a.name.cmp(&b.name));

    let music_path = root.join("audio").join("music.tres");
    let mut found_music = false;
    if let Ok(text) = std::fs::read_to_string(music_path) {
        if let Some(music) = parse_music(&text) {
            project.music = music;
            found_music = true;
        }
    }

    if project.patches.is_empty() && !found_music {
        return Ok(None);
    }
    project.migrate();
    Ok(Some(project))
}

#[derive(Default)]
struct Sections {
    ext: HashMap<String, String>,
    subs: HashMap<String, HashMap<String, String>>,
    resource: HashMap<String, String>,
}

pub fn parse_patch(text: &str) -> Option<Patch> {
    let sections = parse_sections(text);
    let mut patch = Patch::default();
    patch.layers.clear();

    if let Some(ids) = sections.resource.get("layers") {
        for id in sub_resource_ids(ids) {
            if let Some(fields) = sections.subs.get(&id) {
                if let Some(layer) = layer_from_fields(fields, &sections.ext) {
                    patch.layers.push(layer);
                }
            }
        }
    }
    if patch.layers.is_empty() {
        if let Some(layer) = legacy_layer(&sections.resource) {
            patch.layers.push(layer);
        }
    }
    if patch.layers.is_empty() {
        return None;
    }

    patch.rate = int(&sections.resource, "rate")
        .unwrap_or(22050)
        .clamp(8000, 48000) as u32;
    patch.normalize = sections
        .resource
        .get("normalize_layers")
        .map(|value| bool_value(value))
        .unwrap_or(true);
    if let Some(name) = sections.resource.get("resource_name") {
        patch.name = unquote(name);
    }
    if let Some(group) = sections.resource.get("group") {
        if let Some(category) = Category::from_group(&unquote(group)) {
            patch.category = category;
        }
    }
    Some(patch)
}

pub fn parse_music(text: &str) -> Option<Music> {
    let sections = parse_sections(text);
    let fields = &sections.resource;
    if fields.is_empty() {
        return None;
    }
    let mut music = Music::default();
    if let Some(v) = float(fields, "bpm") {
        music.bpm = v.max(20.0);
    }
    if let Some(v) = int(fields, "bars") {
        music.bars = (v as u32).clamp(1, 64);
    }
    if let Some(v) = int(fields, "rate") {
        music.rate = (v as u32).clamp(8000, 48000);
    }
    if let Some(v) = float(fields, "volume_db") {
        music.volume_db = v;
    }
    if let Some(v) = int(fields, "seed_value") {
        music.seed = v as u32;
    }
    if let Some(v) = fields.get("bass_notes") {
        music.bass_notes = float_array(v);
    }
    if let Some(v) = fields.get("chords") {
        music.chords = float_nested(v);
    }
    if let Some(v) = int(fields, "bass_wave") {
        music.bass_wave = music_wave(v as u32);
    }
    if let Some(v) = int(fields, "arp_wave") {
        music.arp_wave = music_wave(v as u32);
    }
    if let Some(v) = float(fields, "swing") {
        music.swing = v;
    }
    if let Some(v) = fields.get("use_patterns") {
        music.use_patterns = bool_value(v);
    }
    if let Some(v) = fields.get("arp_pattern") {
        music.arp_pattern = int_array(v);
    }
    if let Some(v) = fields.get("bass_pattern") {
        music.bass_pattern = int_array(v);
    }
    if let Some(v) = int(fields, "kick_steps") {
        music.kick_steps = v as u32;
    }
    if let Some(v) = int(fields, "hat_steps") {
        music.hat_steps = v as u32;
    }
    for (key, slot) in [
        ("bass_gain", &mut music.bass_gain),
        ("bass_harsh", &mut music.bass_harsh),
        ("bass_decay", &mut music.bass_decay),
        ("arp_gain", &mut music.arp_gain),
        ("arp_harsh", &mut music.arp_harsh),
        ("arp_decay", &mut music.arp_decay),
        ("kick_gain", &mut music.kick_gain),
        ("hat_gain", &mut music.hat_gain),
    ] {
        if let Some(value) = float(fields, key) {
            *slot = value;
        }
    }
    if let Some(value) = fields.get("normalize") {
        music.normalize = bool_value(value);
    }
    Some(music)
}

fn parse_sections(text: &str) -> Sections {
    let mut sections = Sections::default();
    let mut current: Option<(String, String, HashMap<String, String>)> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if let Some((kind, id, values)) = current.take() {
                match kind.as_str() {
                    "ext_resource" => {
                        if let Some(path) = values.get("path") {
                            sections.ext.insert(id, path.clone());
                        }
                    }
                    "sub_resource" => {
                        sections.subs.insert(id, values);
                    }
                    "resource" => {
                        sections.resource = values;
                    }
                    _ => {}
                }
            }
            let header = line.trim_matches(['[', ']']);
            let kind = header.split_whitespace().next().unwrap_or("").to_string();
            let id = extract_attr(line, "id").unwrap_or_default();
            if kind == "ext_resource" {
                if let Some(path) = extract_attr(line, "path") {
                    sections.ext.insert(id.clone(), path);
                }
            }
            current = Some((kind, id, HashMap::new()));
        } else if let Some((_, _, values)) = current.as_mut() {
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    if let Some((kind, id, values)) = current.take() {
        match kind.as_str() {
            "ext_resource" => {
                if let Some(path) = values.get("path") {
                    sections.ext.insert(id, path.clone());
                }
            }
            "sub_resource" => {
                sections.subs.insert(id, values);
            }
            "resource" => {
                sections.resource = values;
            }
            _ => {}
        }
    }
    sections
}

fn extract_attr(line: &str, key: &str) -> Option<String> {
    let needle = format!(" {key}=\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn sub_resource_ids(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(index) = rest.find("SubResource(\"") {
        let after = &rest[index + "SubResource(\"".len()..];
        if let Some(end) = after.find('"') {
            out.push(after[..end].to_string());
            rest = &after[end..];
        } else {
            break;
        }
    }
    out
}

fn layer_from_fields(
    fields: &HashMap<String, String>,
    ext: &HashMap<String, String>,
) -> Option<Layer> {
    let script_id = fields
        .get("script")?
        .trim_start_matches("ExtResource(\"")
        .trim_end_matches("\")")
        .to_string();
    let script_path = ext.get(&script_id)?;
    if !script_path.ends_with("sfx_layer.gd") {
        return None;
    }
    let mut layer = Layer::default();
    if let Some(v) = int(fields, "wave") {
        layer.wave = Wave::from_godot_id(v as u32).unwrap_or(Wave::Sine);
    }
    layer.enabled = fields.get("enabled").map(|v| bool_value(v)).unwrap_or(true);
    set_f32(fields, "delay", &mut layer.delay);
    set_f32(fields, "duration", &mut layer.duration);
    set_f32(fields, "gain", &mut layer.gain);
    set_f32(fields, "freq_start", &mut layer.freq_start);
    set_f32(fields, "freq_end", &mut layer.freq_end);
    set_f32(fields, "harsh", &mut layer.harsh);
    set_f32(fields, "duty", &mut layer.duty);
    set_f32(fields, "vibrato_rate", &mut layer.vibrato_rate);
    set_f32(fields, "vibrato_depth", &mut layer.vibrato_depth);
    set_f32(fields, "attack", &mut layer.attack);
    set_f32(fields, "decay", &mut layer.decay);
    set_f32(fields, "sustain", &mut layer.sustain);
    set_f32(fields, "release", &mut layer.release);
    set_f32(fields, "exp_decay", &mut layer.exp_decay);
    set_f32(fields, "cutoff_start", &mut layer.cutoff_start);
    set_f32(fields, "cutoff_end", &mut layer.cutoff_end);
    set_f32(fields, "resonance", &mut layer.resonance);
    set_f32(fields, "step", &mut layer.step);
    set_f32(fields, "lowpass_from", &mut layer.lowpass_from);
    set_f32(fields, "lowpass_to", &mut layer.lowpass_to);
    set_f32(fields, "noise_gain", &mut layer.noise_gain);
    set_f32(fields, "sweep_gain", &mut layer.sweep_gain);
    set_f32(fields, "fm_ratio", &mut layer.fm_ratio);
    set_f32(fields, "fm_index", &mut layer.fm_index);
    set_f32(fields, "fm_decay", &mut layer.fm_decay);
    set_f32(fields, "pluck_damping", &mut layer.pluck_damping);
    set_f32(fields, "pluck_brightness", &mut layer.pluck_brightness);
    set_f32(fields, "drum_tone", &mut layer.drum_tone);
    set_f32(fields, "drum_snap", &mut layer.drum_snap);
    if let Some(v) = fields.get("arp_freqs") {
        layer.arp_freqs = float_array(v);
    }
    if let Some(v) = int(fields, "filter_mode") {
        layer.filter = match v {
            1 => FilterMode::Lowpass,
            2 => FilterMode::Highpass,
            3 => FilterMode::Bandpass,
            _ => FilterMode::None,
        };
    }
    if let Some(v) = int(fields, "env_shape") {
        layer.env_shape = if v == 1 {
            EnvShape::Sine
        } else {
            EnvShape::Exp
        };
    }
    if let Some(v) = int(fields, "tone_env") {
        layer.tone_env = ToneEnv::from_godot_id(v as u32);
    }
    if let Some(v) = int(fields, "seed_value") {
        layer.seed = v as u32;
    }
    Some(layer)
}

fn legacy_layer(fields: &HashMap<String, String>) -> Option<Layer> {
    let kind = int(fields, "kind").unwrap_or(0);
    let mut layer = Layer::default();
    let duration = float(fields, "duration").unwrap_or(0.15);
    let gain = float(fields, "gain").unwrap_or(0.3);
    match kind {
        0 => {
            layer.wave = Wave::Sine;
            layer.freq_start = float(fields, "freq_start").unwrap_or(440.0);
            layer.freq_end = float(fields, "freq_end").unwrap_or(440.0);
            layer.duration = duration;
            layer.gain = gain;
            layer.harsh = float(fields, "harsh").unwrap_or(0.0);
            layer.attack = 0.001;
            layer.decay = (duration * 0.4).max(0.005);
            layer.release = (duration * 0.2).min(0.05);
        }
        1 => {
            layer.wave = Wave::Arp;
            layer.arp_freqs = fields
                .get("arp_freqs")
                .map(|v| float_array(v))
                .unwrap_or_else(|| vec![523.25, 659.25, 783.99]);
            layer.step = float(fields, "step").unwrap_or(0.06);
            layer.duration = layer.step * layer.arp_freqs.len() as f32;
            layer.gain = gain;
            layer.harsh = float(fields, "harsh").unwrap_or(0.0);
            layer.decay = float(fields, "decay").unwrap_or(5.0);
            let attack = float(fields, "attack").unwrap_or(700.0);
            layer.attack = 1.0 / attack.max(1.0);
        }
        2 => {
            layer.wave = Wave::Noise;
            layer.duration = duration;
            layer.gain = gain;
            layer.freq_start = float(fields, "freq_start").unwrap_or(950.0);
            layer.freq_end = float(fields, "freq_end").unwrap_or(160.0);
            layer.lowpass_from = float(fields, "lowpass_from").unwrap_or(0.35);
            layer.lowpass_to = float(fields, "lowpass_to").unwrap_or(0.35);
            layer.noise_gain = float(fields, "noise_gain").unwrap_or(0.85);
            layer.sweep_gain = float(fields, "sweep_gain").unwrap_or(0.45);
            layer.decay = float(fields, "decay").unwrap_or(6.5);
            if let Some(v) = int(fields, "env_shape") {
                layer.env_shape = if v == 1 {
                    EnvShape::Sine
                } else {
                    EnvShape::Exp
                };
            }
            if let Some(v) = int(fields, "seed_value") {
                layer.seed = v as u32;
            }
        }
        _ => return None,
    }
    Some(layer)
}

fn music_wave(value: u32) -> MusicWave {
    match value {
        1 => MusicWave::Triangle,
        2 => MusicWave::Saw,
        3 => MusicWave::Pulse,
        4 => MusicWave::Square,
        _ => MusicWave::Sine,
    }
}

fn set_f32(fields: &HashMap<String, String>, key: &str, slot: &mut f32) {
    if let Some(value) = float(fields, key) {
        *slot = value;
    }
}

fn float(fields: &HashMap<String, String>, key: &str) -> Option<f32> {
    fields.get(key)?.parse::<f32>().ok()
}

fn int(fields: &HashMap<String, String>, key: &str) -> Option<i64> {
    fields.get(key)?.parse::<i64>().ok()
}

fn bool_value(value: &str) -> bool {
    matches!(value.trim(), "true" | "True" | "1")
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value);
    inner.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn float_array(value: &str) -> Vec<f32> {
    let inner = value
        .split_once('(')
        .map(|(_, rest)| rest.trim_end_matches(')'))
        .unwrap_or("");
    inner
        .split(',')
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .collect()
}

fn int_array(value: &str) -> Vec<i32> {
    let inner = value
        .split_once('(')
        .map(|(_, rest)| rest.trim_end_matches(')'))
        .unwrap_or("");
    inner
        .split(',')
        .filter_map(|part| part.trim().parse::<i32>().ok())
        .collect()
}

fn float_nested(value: &str) -> Vec<Vec<f32>> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(index) = rest.find("PackedFloat64Array(") {
        let after = &rest[index + "PackedFloat64Array(".len()..];
        if let Some(end) = after.find(')') {
            out.push(float_array(&format!("({})", &after[..end])));
            rest = &after[end..];
        } else {
            break;
        }
    }
    out
}

#[allow(dead_code)]
fn _path(_: &Path) {}
