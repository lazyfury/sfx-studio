//! Unified left-panel library: project sounds, audio files, presets and music.
//!
//! The library is a *view* over four existing data sources. It deliberately
//! does not add a second project document:
//!
//! * `Sound`  — editable patches in `sfx.project.json`
//! * `Audio`  — WAV assets under `<project>/audio/` (generated `baked/` and
//!   `demo/` folders are skipped)
//! * `Preset` — `<project>/presets/*.json`
//! * `Music`  — `<project>/music/**/*.json` arrangement files
//!
//! Row identity is small and `Copy` so it can travel through `ui::Hit` and
//! `Drag` without borrowing the document.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibraryKind {
    Sound,
    Audio,
    Preset,
    Music,
}

impl LibraryKind {
    pub const ALL: [LibraryKind; 4] = [
        LibraryKind::Sound,
        LibraryKind::Audio,
        LibraryKind::Preset,
        LibraryKind::Music,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sound => "SOUND",
            Self::Audio => "AUDIO",
            Self::Preset => "PRESET",
            Self::Music => "MUSIC",
        }
    }

    /// Short tag painted on a library row.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Sound => "SND",
            Self::Audio => "WAV",
            Self::Preset => "PRE",
            Self::Music => "MUS",
        }
    }

    pub fn is_sound_or_preset(self) -> bool {
        matches!(self, Self::Sound | Self::Preset)
    }

    /// Stable order used by the unified list and folder keys.
    pub fn order(self) -> u8 {
        match self {
            Self::Sound => 0,
            Self::Audio => 1,
            Self::Preset => 2,
            Self::Music => 3,
        }
    }
}

/// Session-stable identity for a scanned asset. IDs are reused for the same
/// relative path across rescans, so an external file addition/rename cannot
/// silently retarget the selection to a different row.
pub type AssetId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibraryKey {
    Sound(u32),
    Audio(AssetId),
    Preset(AssetId),
    Music(AssetId),
}

impl LibraryKey {
    pub fn kind(self) -> LibraryKind {
        match self {
            Self::Sound(_) => LibraryKind::Sound,
            Self::Audio(_) => LibraryKind::Audio,
            Self::Preset(_) => LibraryKind::Preset,
            Self::Music(_) => LibraryKind::Music,
        }
    }
}

/// One library entry. `name`/`detail` are owned because the paint pass consumes
/// the list after layout; the underlying data stays in the document/banks.
#[derive(Clone, Debug)]
pub struct LibraryEntry {
    pub key: LibraryKey,
    pub kind: LibraryKind,
    pub name: String,
    pub detail: String,
    pub favorite: bool,
    /// Optional folder relative to the kind's root (e.g. `impact` for presets,
    /// `sfx/samples` for audio). `None` = top level.
    pub folder: Option<String>,
}

impl LibraryEntry {
    pub fn folder_key(&self) -> Option<String> {
        self.folder
            .as_ref()
            .map(|folder| folder_key(self.kind, folder))
    }
}

pub fn folder_key(kind: LibraryKind, folder: &str) -> String {
    format!("{}:{folder}", kind.tag())
}

/// One row in the unified library list: a folder header or a resource entry.
#[derive(Clone, Debug)]
pub enum LibraryListRow {
    Folder {
        key: String,
        label: String,
        count: usize,
    },
    Entry(LibraryEntry),
}

#[derive(Clone, Debug)]
pub struct AudioAsset {
    /// Stable identity within this session; see [`AssetId`].
    pub id: AssetId,
    /// Path relative to the project root; used by `Layer.sample_path`.
    pub path: String,
    /// File name shown in the library.
    pub name: String,
    pub bytes: u64,
}

impl AudioAsset {
    pub fn absolute(&self, root: &Path) -> PathBuf {
        let path = Path::new(&self.path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AudioBank {
    assets: Vec<AudioAsset>,
    path_ids: HashMap<String, AssetId>,
}

impl AudioBank {
    pub fn assets(&self) -> &[AudioAsset] {
        &self.assets
    }

    pub fn get(&self, index: usize) -> Option<&AudioAsset> {
        self.assets.get(index)
    }

    pub fn id(&self, index: usize) -> Option<AssetId> {
        self.assets.get(index).map(|asset| asset.id)
    }

    pub fn get_id(&self, id: AssetId) -> Option<&AudioAsset> {
        self.assets.iter().find(|asset| asset.id == id)
    }

    pub fn index_of(&self, id: AssetId) -> Option<usize> {
        self.assets.iter().position(|asset| asset.id == id)
    }

    /// Rename a WAV in place, keeping its stable ID. Only the file stem
    /// changes; the relative folder path is preserved.
    pub fn rename(&mut self, root: &Path, index: usize, new_name: &str) -> std::io::Result<()> {
        let Some(asset) = self.assets.get(index) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "audio not found",
            ));
        };
        let raw = new_name.trim();
        let raw = raw
            .strip_suffix(".wav")
            .or_else(|| raw.strip_suffix(".WAV"))
            .unwrap_or(raw);
        let stem = crate::model::safe_file_stem(raw, "audio");
        let old_relative = asset.path.clone();
        let old = root.join(&old_relative);
        let dir = old
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad path"))?;
        let target = dir.join(format!("{stem}.wav"));
        if target != old && target.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("audio '{}' already exists", target.display()),
            ));
        }
        std::fs::rename(&old, &target)?;
        let new_relative = target
            .strip_prefix(root)
            .unwrap_or(&target)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(asset) = self.assets.get_mut(index) {
            asset.path = new_relative.clone();
            asset.name = target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("audio.wav")
                .to_string();
        }
        if let Some(asset) = self.assets.get(index) {
            self.path_ids.remove(&old_relative);
            self.path_ids.insert(new_relative, asset.id);
        }
        Ok(())
    }

    /// Move a WAV into `<project>/audio/.trash/` and remove it from the
    /// catalogue. The generated trash folder is skipped by the scanner.
    pub fn delete(&mut self, root: &Path, index: usize) -> std::io::Result<()> {
        let Some(asset) = self.assets.get(index) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "audio not found",
            ));
        };
        let relative = asset.path.clone();
        let source = root.join(&relative);
        let target = root.join("audio").join(".trash").join(
            Path::new(&relative)
                .strip_prefix("audio")
                .unwrap_or(Path::new(&relative)),
        );
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let file_name = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio.wav");
        std::fs::rename(
            &source,
            target.with_file_name(format!("{file_name}.{stamp}.bak")),
        )?;
        self.path_ids.remove(&relative);
        self.assets.remove(index);
        Ok(())
    }

    /// Restore the newest WAV moved into `<project>/audio/.trash/`. Returns
    /// the restored file name. The caller reloads the bank to reassign an ID.
    pub fn restore_latest(&mut self, root: &Path) -> std::io::Result<Option<String>> {
        let base = root.join("audio");
        let trash = base.join(".trash");
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
            if !original.to_ascii_lowercase().ends_with(".wav") {
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
        let target = unique_restore_target(&base.join(&original));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&source, &target)?;
        Ok(target
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string))
    }

    /// Scan `<root>/audio/**/*.wav`, skipping hidden files and the generated
    /// `baked/` / `demo/` output folders. Missing folders are not an error.
    /// IDs are reused for unchanged relative paths.
    pub fn load_project(&mut self, root: &Path) -> Vec<String> {
        let dir = root.join("audio");
        let mut found = Vec::new();
        if dir.is_dir() {
            collect_wavs(&dir, root, &mut found);
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        let mut next_id = self
            .path_ids
            .values()
            .copied()
            .max()
            .map_or(0, |id| id.saturating_add(1));
        let mut path_ids = HashMap::with_capacity(found.len());
        for asset in &mut found {
            let id = self.path_ids.get(&asset.path).copied().unwrap_or_else(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            asset.id = id;
            path_ids.insert(asset.path.clone(), id);
        }
        self.assets = found;
        self.path_ids = path_ids;
        Vec::new()
    }
}

/// Recursively collect `.bak` files under a `.trash` directory. Hidden
/// subdirectories are included here (unlike the asset scanner) because that is
/// exactly where the trash stores nested backups.
pub(crate) fn collect_backup_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_backup_files(&path, out);
        } else if file_type.is_file()
            && path.extension().is_some_and(|extension| extension == "bak")
        {
            out.push(path);
        }
    }
}

pub(crate) fn unique_restore_target(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("restored");
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    for index in 2.. {
        let name = if extension.is_empty() {
            format!("{stem}_{index}")
        } else {
            format!("{stem}_{index}.{extension}")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

fn collect_wavs(dir: &Path, root: &Path, out: &mut Vec<AudioAsset>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if matches!(name, "baked" | "demo" | "trash") {
                continue;
            }
            collect_wavs(&path, root, out);
            continue;
        }
        if !file_type.is_file() || !name.to_ascii_lowercase().ends_with(".wav") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        out.push(AudioAsset {
            id: 0,
            path: relative.to_string_lossy().replace('\\', "/"),
            name: name.to_string(),
            bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_bank_scans_wavs_but_skips_generated_folders() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-audio-bank-{}-{stamp}",
            std::process::id()
        ));
        let samples = root.join("audio/sfx/samples");
        let baked = root.join("audio/baked");
        std::fs::create_dir_all(&samples).unwrap();
        std::fs::create_dir_all(&baked).unwrap();
        std::fs::write(samples.join("click.wav"), b"RIFF").unwrap();
        std::fs::write(samples.join("note.txt"), b"nope").unwrap();
        std::fs::write(baked.join("laser.wav"), b"RIFF").unwrap();

        let mut bank = AudioBank::default();
        let warnings = bank.load_project(&root);
        assert!(warnings.is_empty());
        assert_eq!(bank.assets().len(), 1);
        assert_eq!(bank.assets()[0].name, "click.wav");
        assert_eq!(bank.assets()[0].path, "audio/sfx/samples/click.wav");
        assert_eq!(bank.assets()[0].absolute(&root), samples.join("click.wav"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn audio_ids_are_stable_across_rescans() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-audio-ids-{}-{stamp}",
            std::process::id()
        ));
        let dir = root.join("audio/sfx");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.wav"), b"RIFF").unwrap();

        let mut bank = AudioBank::default();
        bank.load_project(&root);
        let id_b = bank.assets()[0].id;
        let path_b = bank.assets()[0].path.clone();

        // A new file sorting before the old one must not retarget the ID.
        std::fs::write(dir.join("a.wav"), b"RIFF").unwrap();
        bank.load_project(&root);
        assert_eq!(bank.get_id(id_b).expect("b survives").path, path_b);
        let id_a = bank
            .assets()
            .iter()
            .find(|asset| asset.name == "a.wav")
            .expect("a")
            .id;
        assert_ne!(id_a, id_b);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn audio_rename_and_delete_keep_ids_and_use_trash() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-audio-lifecycle-{}-{stamp}",
            std::process::id()
        ));
        let dir = root.join("audio/sfx");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.wav"), b"RIFF").unwrap();

        let mut bank = AudioBank::default();
        bank.load_project(&root);
        let id = bank.assets()[0].id;
        bank.rename(&root, 0, "金币 01").expect("rename");
        assert_eq!(bank.id(0), Some(id));
        assert_eq!(bank.assets()[0].name, "金币 01.wav");
        assert_eq!(
            bank.get_id(id).expect("same id").path,
            "audio/sfx/金币 01.wav"
        );
        assert!(!dir.join("a.wav").exists());
        assert!(dir.join("金币 01.wav").exists());

        bank.delete(&root, 0).expect("delete");
        assert_eq!(bank.assets().len(), 0);
        assert!(!dir.join("金币 01.wav").exists());
        assert!(std::fs::read_dir(root.join("audio/.trash/sfx"))
            .expect("trash")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("金币 01.wav.")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn library_keys_keep_their_kind() {
        assert_eq!(LibraryKey::Sound(1).kind(), LibraryKind::Sound);
        assert_eq!(LibraryKey::Audio(2).kind(), LibraryKind::Audio);
        assert_eq!(LibraryKey::Preset(3).kind(), LibraryKind::Preset);
        assert_eq!(LibraryKey::Music(0).kind(), LibraryKind::Music);
    }
}
