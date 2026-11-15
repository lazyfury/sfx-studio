//! Project-local music files.
//!
//! Each file is one `Music` JSON object under `<project>/music/**/*.json`.
//! The project document still owns the *current* `Project.music`; the bank is
//! a read-only catalogue that can load a track into it (undoable).

use std::io;
use std::path::{Path, PathBuf};

use crate::model::Music;

pub const MUSIC_DIR: &str = "music";

#[derive(Clone, Debug)]
pub struct MusicAsset {
    /// File stem shown in the library.
    pub name: String,
    /// Folder relative to `music/`, if nested.
    pub folder: Option<String>,
    pub music: Music,
}

#[derive(Clone, Debug, Default)]
pub struct MusicBank {
    assets: Vec<MusicAsset>,
}

impl MusicBank {
    pub fn assets(&self) -> &[MusicAsset] {
        &self.assets
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn get(&self, index: usize) -> Option<&MusicAsset> {
        self.assets.get(index)
    }

    /// Scan `<root>/music/**/*.json`. Missing folders are not an error; bad
    /// JSON is reported as a warning and skipped.
    pub fn load_project(&mut self, root: &Path) -> Vec<String> {
        self.assets.clear();
        let dir = root.join(MUSIC_DIR);
        let mut paths = Vec::new();
        collect_json_files(&dir, &mut paths);
        paths.sort();
        let mut warnings = Vec::new();
        for path in paths {
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    warnings.push(format!("music {}: {error}", path.display()));
                    continue;
                }
            };
            let mut music = match serde_json::from_str::<Music>(&text) {
                Ok(music) => music,
                Err(error) => {
                    warnings.push(format!("music {}: {error}", path.display()));
                    continue;
                }
            };
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("music.json");
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("music")
                .to_string();
            let folder = path
                .parent()
                .and_then(|parent| parent.strip_prefix(&dir).ok())
                .map(|parent| parent.to_string_lossy().replace('\\', "/"))
                .filter(|folder| !folder.is_empty());
            for warning in music.validate() {
                warnings.push(format!("music {file_name}: {warning}"));
            }
            self.assets.push(MusicAsset {
                name,
                folder,
                music,
            });
        }
        warnings
    }

    /// Write one JSON file per track into `<root>/music/`. Existing files are
    /// never overwritten so a project created beside user data stays safe.
    pub fn write_tracks(root: &Path, tracks: &[(&str, &Music)]) -> io::Result<usize> {
        let dir = root.join(MUSIC_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut written = 0;
        for (name, music) in tracks {
            let stem = crate::model::sanitize(name);
            let mut target = dir.join(format!("{stem}.json"));
            let mut suffix = 2;
            while target.exists() {
                target = dir.join(format!("{stem}_{suffix}.json"));
                suffix += 1;
            }
            let text = serde_json::to_string_pretty(music)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            crate::model::write_atomic(&target, text.as_bytes())?;
            written += 1;
        }
        Ok(written)
    }
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
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
            Ok(file_type) if file_type.is_dir() => collect_json_files(&path, out),
            Ok(file_type)
                if file_type.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "json") =>
            {
                out.push(path);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("sfx-studio-{label}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn writes_and_loads_music_files() {
        let root = temp_dir("music-bank");
        let music = Music::default();
        let written = MusicBank::write_tracks(&root, &[("neon_drive", &music)]).expect("write");
        assert_eq!(written, 1);
        assert!(root.join("music/neon_drive.json").exists());
        let mut bank = MusicBank::default();
        let warnings = bank.load_project(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(bank.len(), 1);
        assert_eq!(bank.assets()[0].name, "neon_drive");
        assert_eq!(bank.assets()[0].folder, None);
        assert!((bank.assets()[0].music.bpm - music.bpm).abs() < 0.001);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nested_music_files_report_their_folder() {
        let root = temp_dir("music-bank-nested");
        let dir = root.join("music/levels");
        std::fs::create_dir_all(&dir).expect("music dir");
        std::fs::write(
            dir.join("boss.json"),
            serde_json::to_string(&Music::default()).expect("json"),
        )
        .expect("write");
        let mut bank = MusicBank::default();
        bank.load_project(&root);
        assert_eq!(bank.len(), 1);
        assert_eq!(bank.assets()[0].folder.as_deref(), Some("levels"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_music_is_reported_and_skipped() {
        let root = temp_dir("music-bank-bad");
        let dir = root.join("music");
        std::fs::create_dir_all(&dir).expect("music dir");
        std::fs::write(dir.join("bad.json"), b"not json").expect("write");
        let mut bank = MusicBank::default();
        let warnings = bank.load_project(&root);
        assert!(bank.assets().is_empty());
        assert!(warnings.iter().any(|warning| warning.contains("bad.json")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
