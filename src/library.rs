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

use std::path::{Path, PathBuf};

use crate::model::Category;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibraryKey {
    Sound(u32),
    /// Index into `State::audio_bank`.
    Audio(usize),
    /// Index into `State::presets`.
    Preset(usize),
    /// Index into `State::music_bank`.
    Music(usize),
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
    pub category: Option<Category>,
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
    root: Option<PathBuf>,
}

impl AudioBank {
    pub fn assets(&self) -> &[AudioAsset] {
        &self.assets
    }

    pub fn get(&self, index: usize) -> Option<&AudioAsset> {
        self.assets.get(index)
    }

    /// Scan `<root>/audio/**/*.wav`, skipping hidden files and the generated
    /// `baked/` / `demo/` output folders. Missing folders are not an error.
    pub fn load_project(&mut self, root: &Path) -> Vec<String> {
        self.root = Some(root.to_path_buf());
        self.assets.clear();
        let dir = root.join("audio");
        let mut found = Vec::new();
        if dir.is_dir() {
            collect_wavs(&dir, root, &mut found);
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        self.assets = found;
        Vec::new()
    }
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
    fn library_keys_keep_their_kind() {
        assert_eq!(LibraryKey::Sound(1).kind(), LibraryKind::Sound);
        assert_eq!(LibraryKey::Audio(2).kind(), LibraryKind::Audio);
        assert_eq!(LibraryKey::Preset(3).kind(), LibraryKind::Preset);
        assert_eq!(LibraryKey::Music(0).kind(), LibraryKind::Music);
    }
}
