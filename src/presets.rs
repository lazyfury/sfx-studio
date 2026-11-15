//! Project-local preset library.
//!
//! Presets are ordinary `Patch` JSON files shipped inside the project folder:
//! `<project>/presets/*.json`. The bank is loaded when a project folder is
//! opened, so switching projects switches the internal preset library too.
//! Favourites live beside the presets in `<project>/presets/.favorites.json`.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::{self, Patch, Project};

pub const PRESETS_DIR: &str = "presets";
const FAVORITES_FILE: &str = ".favorites.json";

#[derive(Clone, Debug, Default)]
pub struct PresetBank {
    pub presets: Vec<Patch>,
    /// Source JSON path for each preset. `None` for session-only banks.
    paths: Vec<Option<PathBuf>>,
    /// Folder relative to `<project>/presets/`, e.g. `impact`.
    folders: Vec<Option<String>>,
    root: Option<PathBuf>,
    favorites: HashSet<String>,
}

impl PresetBank {
    /// Session-only bank used by demos and tests; it has no on-disk root.
    pub fn from_patches(presets: Vec<Patch>) -> Self {
        let paths = vec![None; presets.len()];
        let folders = vec![None; presets.len()];
        Self {
            presets,
            paths,
            folders,
            ..Default::default()
        }
    }

    /// Load `<root>/presets/*.json` plus `.favorites.json`. Missing directories
    /// are not an error: a project simply may not ship an internal library.
    pub fn load_project(&mut self, root: &Path) -> Vec<String> {
        self.root = Some(root.to_path_buf());
        self.presets.clear();
        self.paths.clear();
        self.folders.clear();
        self.favorites.clear();

        let dir = root.join(PRESETS_DIR);
        let mut warnings = Vec::new();
        let mut paths = Vec::new();
        collect_preset_files(&dir, &mut paths);
        paths.sort();
        let mut presets = Vec::new();
        let mut sources = Vec::new();
        let mut folders = Vec::new();
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<Patch>(&text) {
                    Ok(patch) => {
                        presets.push(patch);
                        folders.push(preset_folder(&path, &dir));
                        sources.push(Some(path));
                    }
                    Err(error) => {
                        warnings.push(format!("preset {}: {error}", path.display()));
                    }
                },
                Err(error) => warnings.push(format!("preset {}: {error}", path.display())),
            }
        }
        if !presets.is_empty() {
            // Reuse the project validator so malformed presets are clamped and
            // duplicate names are made unique before they reach the UI/DSP.
            let mut project = Project {
                patches: presets,
                ..Project::default()
            };
            warnings.extend(project.validate());
            self.presets = project.patches;
            self.paths = sources.into_iter().take(self.presets.len()).collect();
            self.folders = folders.into_iter().take(self.presets.len()).collect();
        }
        if let Ok(favorites) = read_favorites(&dir.join(FAVORITES_FILE)) {
            self.favorites = favorites;
        }
        warnings
    }

    /// Write one JSON file per preset. Used by NEW PROJECT / `--new` so a new
    /// project starts with its own internal library.
    pub fn write_presets(root: &Path, patches: &[Patch]) -> io::Result<usize> {
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut used = HashSet::new();
        let mut written = 0;
        for (index, patch) in patches.iter().enumerate() {
            let folder = patch.category.folder();
            let target_dir = dir.join(folder);
            std::fs::create_dir_all(&target_dir)?;
            let base = preset_file_stem(&patch.name, index);
            let mut stem = base.clone();
            let mut suffix = 2;
            while !used.insert(format!("{folder}/{}", stem.to_lowercase())) {
                stem = format!("{base}_{suffix}");
                suffix += 1;
            }
            let text = serde_json::to_string_pretty(patch)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            model::write_atomic(&target_dir.join(format!("{stem}.json")), text.as_bytes())?;
            written += 1;
        }
        Ok(written)
    }

    /// Add only presets that are not already present by patch name *or* by
    /// target file stem. Existing files are never rewritten, which makes the
    /// CLI's `--install-presets` safe on a project the user has edited.
    pub fn install_missing(root: &Path, patches: &[Patch]) -> io::Result<usize> {
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut names = HashSet::new();
        let mut files = HashSet::new();
        let mut paths = Vec::new();
        collect_preset_files(&dir, &mut paths);
        for path in paths {
            if let Ok(relative) = path.strip_prefix(&dir) {
                let stem = relative
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(stem.to_lowercase());
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(patch) = serde_json::from_str::<Patch>(&text) {
                    names.insert(patch.name.to_lowercase());
                }
            }
        }
        let mut written = 0;
        for (index, patch) in patches.iter().enumerate() {
            if names.contains(&patch.name.to_lowercase()) {
                continue;
            }
            let folder = patch.category.folder();
            let target_dir = dir.join(folder);
            std::fs::create_dir_all(&target_dir)?;
            let base = preset_file_stem(&patch.name, index);
            let mut stem = base.clone();
            let mut suffix = 2;
            loop {
                let relative = format!("{folder}/{}", stem.to_lowercase());
                if !files.contains(&relative) && !target_dir.join(format!("{stem}.json")).exists() {
                    break;
                }
                stem = format!("{base}_{suffix}");
                suffix += 1;
            }
            let text = serde_json::to_string_pretty(patch)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            model::write_atomic(&target_dir.join(format!("{stem}.json")), text.as_bytes())?;
            files.insert(format!("{folder}/{}", stem.to_lowercase()));
            names.insert(patch.name.to_lowercase());
            written += 1;
        }
        Ok(written)
    }

    pub fn len(&self) -> usize {
        self.presets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Patch> {
        self.presets.get(index)
    }

    pub fn path(&self, index: usize) -> Option<&Path> {
        self.paths.get(index).and_then(Option::as_deref)
    }

    pub fn folder(&self, index: usize) -> Option<&str> {
        self.folders.get(index).and_then(Option::as_deref)
    }

    /// Decode sample layers for one preset on demand. Preset libraries can be
    /// large, so project load only parses JSON; audition/export/drag call this.
    pub fn hydrate(&mut self, index: usize) -> Vec<String> {
        let Some(root) = self.root.clone() else {
            return Vec::new();
        };
        let Some(patch) = self.presets.get_mut(index) else {
            return Vec::new();
        };
        let needs_load = patch
            .layers
            .iter()
            .any(|layer| !layer.sample_path.is_empty() && layer.sample_data.is_none());
        if !needs_load {
            return Vec::new();
        }
        patch.load_samples(&root)
    }

    /// Restore the newest JSON moved into `presets/.trash/`. Nested preset
    /// folders are preserved. Returns the restored patch name when a file was
    /// moved back.
    pub fn restore_latest(&mut self) -> io::Result<Option<String>> {
        let Some(root) = self.root.clone() else {
            return Ok(None);
        };
        let dir = root.join(PRESETS_DIR);
        let trash = dir.join(".trash");
        let mut backups = Vec::new();
        collect_backup_files(&trash, &mut backups);
        let mut newest: Option<(u128, PathBuf, PathBuf)> = None;
        for path in backups {
            let Ok(relative) = path.strip_prefix(&trash) else {
                continue;
            };
            let Some(file_name) = relative.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(rest) = file_name.strip_suffix(".bak") else {
                continue;
            };
            let Some((original, stamp)) = rest.rsplit_once('.') else {
                continue;
            };
            if !original.ends_with(".json") {
                continue;
            }
            let Ok(stamp) = stamp.parse::<u128>() else {
                continue;
            };
            let original = relative
                .parent()
                .map(|parent| parent.join(original))
                .unwrap_or_else(|| PathBuf::from(original));
            if newest.as_ref().is_none_or(|(best, _, _)| stamp > *best) {
                newest = Some((stamp, path, original));
            }
        }
        let Some((_, source, original)) = newest else {
            return Ok(None);
        };
        let mut target = dir.join(&original);
        let parent = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        std::fs::create_dir_all(&parent)?;
        let stem = target
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("preset")
            .to_string();
        let mut suffix = 2;
        while target.exists() {
            target = parent.join(format!("{stem}_{suffix}.json"));
            suffix += 1;
        }
        std::fs::rename(&source, &target)?;
        let name = std::fs::read_to_string(&target)
            .ok()
            .and_then(|text| serde_json::from_str::<Patch>(&text).ok())
            .map(|patch| patch.name)
            .unwrap_or(stem);
        Ok(Some(name))
    }

    /// Rename the preset file, patch name and favourite key together. Existing
    /// file names are never overwritten.
    pub fn rename(&mut self, index: usize, new_name: &str) -> io::Result<()> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty name"));
        }
        let Some(patch) = self.presets.get(index) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "preset not found"));
        };
        if let Some(conflict) =
            self.presets.iter().enumerate().find(|(other, preset)| {
                *other != index && preset.name.eq_ignore_ascii_case(new_name)
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("preset '{}' already exists", conflict.1.name),
            ));
        }
        let old_name = patch.name.clone();
        let Some(source) = self.paths.get(index).cloned().flatten() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "preset has no project file",
            ));
        };
        let mut renamed = patch.clone();
        renamed.name = new_name.to_string();
        let text = serde_json::to_string_pretty(&renamed)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let dir = source
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid preset path"))?;
        let base = preset_file_stem(new_name, index);
        let mut target = dir.join(format!("{base}.json"));
        let mut suffix = 2;
        while target.exists() && target != source {
            target = dir.join(format!("{base}_{suffix}.json"));
            suffix += 1;
        }
        model::write_atomic(&target, text.as_bytes())?;
        if target != source {
            std::fs::remove_file(&source)?;
        }
        if let Some(preset) = self.presets.get_mut(index) {
            preset.name = new_name.to_string();
        }
        if index < self.paths.len() {
            self.paths[index] = Some(target);
        } else {
            self.paths.resize(index + 1, None);
            self.paths[index] = Some(target);
        }
        if self.favorites.remove(&old_name) {
            self.favorites.insert(new_name.to_string());
            self.save_favorites();
        }
        Ok(())
    }

    /// Remove a preset from the library. The JSON file is moved into
    /// `presets/.trash/` (preserving its folder) so an accidental delete can
    /// still be recovered.
    pub fn delete(&mut self, index: usize) -> io::Result<()> {
        let Some(patch) = self.presets.get(index) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "preset not found"));
        };
        let name = patch.name.clone();
        let Some(source) = self.paths.get(index).cloned().flatten() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "preset has no project file",
            ));
        };
        let dir = self
            .root
            .clone()
            .map(|root| root.join(PRESETS_DIR))
            .or_else(|| source.parent().map(Path::to_path_buf))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid preset path"))?;
        let relative = source.strip_prefix(&dir).unwrap_or_else(|_| {
            source
                .file_name()
                .map(Path::new)
                .unwrap_or_else(|| Path::new("preset.json"))
        });
        let trash = dir.join(".trash");
        let target = trash.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let file_name = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("preset.json");
        std::fs::rename(
            &source,
            target.with_file_name(format!("{file_name}.{stamp}.bak")),
        )?;
        self.presets.remove(index);
        if index < self.paths.len() {
            self.paths.remove(index);
        }
        if index < self.folders.len() {
            self.folders.remove(index);
        }
        if self.favorites.remove(&name) {
            self.save_favorites();
        }
        Ok(())
    }

    pub fn is_favorite(&self, patch: &Patch) -> bool {
        self.favorites.contains(&patch.name)
    }

    /// Toggle a preset favourite. Returns `true` when it was added.
    pub fn toggle_favorite(&mut self, patch: &Patch) -> bool {
        if self.favorites.remove(&patch.name) {
            false
        } else {
            self.favorites.insert(patch.name.clone());
            true
        }
    }

    pub fn save_favorites(&self) {
        let Some(root) = &self.root else {
            return;
        };
        let _ = write_favorites(
            &root.join(PRESETS_DIR).join(FAVORITES_FILE),
            &self.favorites,
        );
    }
}

fn collect_preset_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => collect_preset_files(&path, out),
            Ok(file_type)
                if file_type.is_file() && path.extension().is_some_and(|ext| ext == "json") =>
            {
                out.push(path);
            }
            _ => {}
        }
    }
}

fn preset_folder(path: &Path, presets_dir: &Path) -> Option<String> {
    let parent = path.parent()?.strip_prefix(presets_dir).ok()?;
    let folder = parent.to_string_lossy().replace('\\', "/");
    (!folder.is_empty()).then_some(folder)
}

fn collect_backup_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => collect_backup_files(&path, out),
            Ok(file_type)
                if file_type.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".bak")) =>
            {
                out.push(path);
            }
            _ => {}
        }
    }
}

fn preset_file_stem(name: &str, index: usize) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '-' | '_' | ' ') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        format!("preset_{:02}", index + 1)
    } else {
        trimmed
    }
}

#[derive(Serialize, Deserialize, Default)]
struct FavoritesFile {
    #[serde(default)]
    favorites: Vec<String>,
}

fn read_favorites(path: &Path) -> io::Result<HashSet<String>> {
    let text = std::fs::read_to_string(path)?;
    let file: FavoritesFile = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(file.favorites.into_iter().collect())
}

fn write_favorites(path: &Path, favorites: &HashSet<String>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut names: Vec<&String> = favorites.iter().collect();
    names.sort();
    let file = FavoritesFile {
        favorites: names.into_iter().cloned().collect(),
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    model::write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Wave;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("sfx-studio-{label}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn loads_presets_and_favorites_from_a_project_folder() {
        let root = temp_dir("preset-load");
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir).expect("preset dir");
        let one = Patch::new_layer("one", Wave::Sine);
        let two = Patch::new_layer("two", Wave::Noise);
        for patch in [&one, &two] {
            std::fs::write(
                dir.join(format!("{}.json", patch.name)),
                serde_json::to_string(&patch).expect("patch json"),
            )
            .expect("write preset");
        }
        std::fs::write(dir.join(FAVORITES_FILE), r#"{"favorites":["two"]}"#)
            .expect("write favorites");

        let mut bank = PresetBank::default();
        let warnings = bank.load_project(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(bank.len(), 2);
        assert!(bank.is_favorite(&two));
        assert!(!bank.is_favorite(&one));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_missing_never_overwrites_existing_names_or_files() {
        let root = temp_dir("preset-install");
        let dir = root
            .join(PRESETS_DIR)
            .join(crate::model::Category::Impact.folder());
        std::fs::create_dir_all(&dir).expect("preset dir");
        let existing = Patch::new_layer("tone", Wave::Sine);
        std::fs::write(
            dir.join("custom.json"),
            serde_json::to_string(&existing).expect("json"),
        )
        .expect("write custom");
        // A different preset whose name is free, but whose file stem collides
        // inside the same category folder.
        let collides = Patch::new_layer("custom", Wave::Noise);
        let fresh = Patch::new_layer("brand_new", Wave::Saw);
        let written = PresetBank::install_missing(&root, &[existing.clone(), collides, fresh])
            .expect("install");
        assert_eq!(written, 2);
        assert!(dir.join("custom_2.json").exists());
        assert!(dir.join("brand_new.json").exists());
        let text = std::fs::read_to_string(dir.join("custom.json")).expect("read");
        let kept: Patch = serde_json::from_str(&text).expect("parse");
        assert_eq!(kept.layers[0].wave, Wave::Sine);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loads_preset_sample_layers_relative_to_the_project() {
        let root = temp_dir("preset-sample");
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir).expect("preset dir");
        let wav_path = root.join("audio/sfx/samples/click.wav");
        std::fs::create_dir_all(wav_path.parent().unwrap()).unwrap();
        crate::synth::write_wav(&wav_path, &[0.0, 0.5, -0.5], 22_050).expect("write wav");

        let mut patch = Patch::new_layer("sampled", Wave::Sample);
        patch.layers[0].sample_path = "audio/sfx/samples/click.wav".into();
        patch.layers[0].duration = 0.01;
        std::fs::write(
            dir.join("sampled.json"),
            serde_json::to_string(&patch).expect("json"),
        )
        .expect("write preset");

        let mut bank = PresetBank::default();
        let warnings = bank.load_project(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(bank.presets[0].layers[0].sample_data.is_none());
        let warnings = bank.hydrate(0);
        assert!(warnings.is_empty(), "{warnings:?}");
        let data = bank.presets[0].layers[0]
            .sample_data
            .as_ref()
            .expect("preset sample resolved");
        assert_eq!(data.rate, 22_050);
        assert_eq!(data.samples.len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rename_moves_the_file_and_favourite_key() {
        let root = temp_dir("preset-rename");
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir).expect("preset dir");
        let one = Patch::new_layer("one", Wave::Sine);
        let two = Patch::new_layer("two", Wave::Noise);
        for patch in [&one, &two] {
            std::fs::write(
                dir.join(format!("{}.json", patch.name)),
                serde_json::to_string(&patch).expect("json"),
            )
            .expect("write preset");
        }
        std::fs::write(dir.join(FAVORITES_FILE), r#"{"favorites":["one"]}"#)
            .expect("write favorites");

        let mut bank = PresetBank::default();
        bank.load_project(&root);
        bank.rename(0, "renamed").expect("rename");
        assert_eq!(bank.presets[0].name, "renamed");
        assert!(dir.join("renamed.json").exists());
        assert!(!dir.join("one.json").exists());
        assert!(!bank.is_favorite(&one));
        assert!(bank.is_favorite(&bank.presets[0].clone()));

        let mut reloaded = PresetBank::default();
        reloaded.load_project(&root);
        assert!(reloaded
            .presets
            .iter()
            .any(|preset| preset.name == "renamed"));
        assert!(reloaded
            .presets
            .iter()
            .any(|preset| preset.name == "renamed" && reloaded.is_favorite(preset)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_moves_the_preset_file_into_the_trash() {
        let root = temp_dir("preset-delete");
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir).expect("preset dir");
        let one = Patch::new_layer("one", Wave::Sine);
        std::fs::write(
            dir.join("one.json"),
            serde_json::to_string(&one).expect("json"),
        )
        .expect("write preset");
        let mut bank = PresetBank::default();
        bank.load_project(&root);
        assert_eq!(bank.len(), 1);
        bank.delete(0).expect("delete");
        assert!(bank.presets.is_empty());
        assert!(!dir.join("one.json").exists());
        let trashed = std::fs::read_dir(dir.join(".trash"))
            .expect("trash dir")
            .flatten()
            .count();
        assert_eq!(trashed, 1);
        let mut reloaded = PresetBank::default();
        reloaded.load_project(&root);
        assert!(reloaded.presets.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_latest_brings_back_a_deleted_preset() {
        let root = temp_dir("preset-restore");
        let dir = root.join(PRESETS_DIR);
        std::fs::create_dir_all(&dir).expect("preset dir");
        let one = Patch::new_layer("one", Wave::Sine);
        std::fs::write(
            dir.join("one.json"),
            serde_json::to_string(&one).expect("json"),
        )
        .expect("write preset");
        let mut bank = PresetBank::default();
        bank.load_project(&root);
        bank.delete(0).expect("delete");
        assert!(bank.presets.is_empty());

        let restored = bank.restore_latest().expect("restore").expect("name");
        assert_eq!(restored, "one");
        assert!(dir.join("one.json").exists());
        let mut reloaded = PresetBank::default();
        reloaded.load_project(&root);
        assert_eq!(reloaded.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_presets_creates_unique_files_and_roundtrips() {
        let root = temp_dir("preset-write");
        let a = Patch::new_layer("same", Wave::Sine);
        let b = Patch::new_layer("same", Wave::Noise);
        let written = PresetBank::write_presets(&root, &[a, b]).expect("write presets");
        assert_eq!(written, 2);
        let mut bank = PresetBank::default();
        let warnings = bank.load_project(&root);
        assert!(warnings.iter().any(|warning| warning.contains("same_2")));
        assert_eq!(bank.len(), 2);
        assert_eq!(bank.presets[0].name, "same");
        assert_eq!(bank.presets[1].name, "same_2");
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn loads_nested_preset_folders_and_reports_them() {
        let root = temp_dir("preset-folders");
        let impact = root.join(PRESETS_DIR).join("impact");
        let laser = root.join(PRESETS_DIR).join("laser");
        std::fs::create_dir_all(&impact).expect("impact");
        std::fs::create_dir_all(&laser).expect("laser");
        let a = Patch::new_layer("a", Wave::Sine);
        let b = Patch::new_layer("b", Wave::Square);
        std::fs::write(
            impact.join("a.json"),
            serde_json::to_string(&a).expect("json"),
        )
        .expect("write");
        std::fs::write(
            laser.join("b.json"),
            serde_json::to_string(&b).expect("json"),
        )
        .expect("write");
        let mut bank = PresetBank::default();
        let warnings = bank.load_project(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(bank.len(), 2);
        assert_eq!(bank.folder(0), Some("impact"));
        assert_eq!(bank.folder(1), Some("laser"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_presets_uses_category_folders() {
        let root = temp_dir("preset-folders-write");
        let mut impact = Patch::new_layer("boom", Wave::Noise);
        impact.category = crate::model::Category::Impact;
        let mut laser = Patch::new_layer("zap", Wave::Saw);
        laser.category = crate::model::Category::Laser;
        let written = PresetBank::write_presets(&root, &[impact, laser]).expect("write");
        assert_eq!(written, 2);
        assert!(root.join(PRESETS_DIR).join("impact/boom.json").exists());
        assert!(root.join(PRESETS_DIR).join("laser/zap.json").exists());
        let mut bank = PresetBank::default();
        bank.load_project(&root);
        assert_eq!(bank.len(), 2);
        assert_eq!(bank.folder(0), Some("impact"));
        assert_eq!(bank.folder(1), Some("laser"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_and_restore_preserve_nested_folders() {
        let root = temp_dir("preset-folders-restore");
        let impact = root.join(PRESETS_DIR).join("impact");
        std::fs::create_dir_all(&impact).expect("impact");
        let patch = Patch::new_layer("boom", Wave::Noise);
        std::fs::write(
            impact.join("boom.json"),
            serde_json::to_string(&patch).expect("json"),
        )
        .expect("write");
        let mut bank = PresetBank::default();
        bank.load_project(&root);
        assert_eq!(bank.len(), 1);
        bank.delete(0).expect("delete");
        assert!(!impact.join("boom.json").exists());
        assert!(
            std::fs::read_dir(root.join(PRESETS_DIR).join(".trash/impact"))
                .expect("trash folder")
                .flatten()
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("boom.json."))
        );

        let restored = bank.restore_latest().expect("restore").expect("name");
        assert_eq!(restored, "boom");
        assert!(impact.join("boom.json").exists());
        let mut reloaded = PresetBank::default();
        reloaded.load_project(&root);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.folder(0), Some("impact"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
