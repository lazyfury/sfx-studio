//! Project-local music files.
//!
//! Each file is one `Music` JSON object under `<project>/music/**/*.json`.
//! The project document still owns the *current* `Project.music`; the bank is
//! a read-only catalogue that can load a track into it (undoable).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::library::{collect_backup_files, unique_restore_target, AssetId};
use crate::model::Music;

pub const MUSIC_DIR: &str = "music";

#[derive(Clone, Debug)]
pub struct MusicAsset {
    /// Stable identity within this session.
    pub id: AssetId,
    /// Absolute source path; used by rename/delete.
    pub path: PathBuf,
    /// File stem shown in the library.
    pub name: String,
    /// Folder relative to `music/`, if nested.
    pub folder: Option<String>,
    pub music: Music,
}

#[derive(Clone, Debug, Default)]
pub struct MusicBank {
    assets: Vec<MusicAsset>,
    path_ids: HashMap<PathBuf, AssetId>,
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

    pub fn id(&self, index: usize) -> Option<AssetId> {
        self.assets.get(index).map(|asset| asset.id)
    }

    pub fn index_of(&self, id: AssetId) -> Option<usize> {
        self.assets.iter().position(|asset| asset.id == id)
    }

    /// Scan `<root>/music/**/*.json`. Missing folders are not an error; bad
    /// JSON is reported as a warning and skipped. IDs are reused by path.
    pub fn load_project(&mut self, root: &Path) -> Vec<String> {
        self.assets.clear();
        let mut path_ids = HashMap::new();
        let mut next_id = self
            .path_ids
            .values()
            .copied()
            .max()
            .map_or(0, |id| id.saturating_add(1));
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
            let id = self.path_ids.get(&path).copied().unwrap_or_else(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            path_ids.insert(path.clone(), id);
            self.assets.push(MusicAsset {
                id,
                path,
                name,
                folder,
                music,
            });
        }
        self.path_ids = path_ids;
        warnings
    }

    /// Rename a music file in place, keeping its stable ID.
    pub fn rename(&mut self, index: usize, new_name: &str) -> io::Result<()> {
        let Some(asset) = self.assets.get(index) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "music not found"));
        };
        let raw = new_name.trim();
        let raw = raw
            .strip_suffix(".json")
            .or_else(|| raw.strip_suffix(".JSON"))
            .unwrap_or(raw);
        let stem = crate::model::safe_file_stem(raw, "music");
        let old = asset.path.clone();
        let dir = old
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid music path"))?;
        let target = dir.join(format!("{stem}.json"));
        if target != old && target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("music '{}' already exists", target.display()),
            ));
        }
        std::fs::rename(&old, &target)?;
        if let Some(asset) = self.assets.get_mut(index) {
            asset.path = target.clone();
            asset.name = stem;
        }
        if let Some(asset) = self.assets.get(index) {
            self.path_ids.remove(&old);
            self.path_ids.insert(target, asset.id);
        }
        Ok(())
    }

    /// Move a music file into `<project>/music/.trash/` and remove it from the
    /// catalogue. The file can be restored manually; no silent deletion.
    pub fn delete(&mut self, root: &Path, index: usize) -> io::Result<()> {
        let Some(asset) = self.assets.get(index) else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "music not found"));
        };
        let source = asset.path.clone();
        let relative = source
            .strip_prefix(root.join(MUSIC_DIR))
            .unwrap_or_else(|_| {
                source
                    .file_name()
                    .map(Path::new)
                    .unwrap_or_else(|| Path::new("music.json"))
            });
        let target = root.join(MUSIC_DIR).join(".trash").join(relative);
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
            .unwrap_or("music.json");
        std::fs::rename(
            &source,
            target.with_file_name(format!("{file_name}.{stamp}.bak")),
        )?;
        self.path_ids.remove(&source);
        self.assets.remove(index);
        Ok(())
    }

    /// Restore the newest JSON moved into `<project>/music/.trash/`.
    pub fn restore_latest(&mut self, root: &Path) -> io::Result<Option<String>> {
        let base = root.join(MUSIC_DIR);
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
            if !original.to_ascii_lowercase().ends_with(".json") {
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
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string))
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
    fn music_rename_and_delete_keep_ids_and_use_trash() {
        let root = temp_dir("music-lifecycle");
        let dir = root.join("music");
        std::fs::create_dir_all(&dir).expect("music dir");
        std::fs::write(
            dir.join("a.json"),
            serde_json::to_string(&Music::default()).expect("json"),
        )
        .expect("write");
        let mut bank = MusicBank::default();
        bank.load_project(&root);
        let id = bank.assets()[0].id;
        bank.rename(0, "b").expect("rename");
        assert_eq!(bank.id(0), Some(id));
        assert_eq!(bank.assets()[0].name, "b");
        assert!(!dir.join("a.json").exists());
        assert!(dir.join("b.json").exists());

        bank.delete(&root, 0).expect("delete");
        assert_eq!(bank.len(), 0);
        assert!(!dir.join("b.json").exists());
        assert!(std::fs::read_dir(root.join("music/.trash"))
            .expect("trash")
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("b.json.")));

        let restored = bank.restore_latest(&root).expect("restore").expect("name");
        assert_eq!(restored, "b");
        assert!(dir.join("b.json").exists());
        bank.load_project(&root);
        assert_eq!(bank.len(), 1);
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

    #[test]
    fn music_ids_are_stable_across_rescans() {
        let root = temp_dir("music-ids");
        let dir = root.join(MUSIC_DIR);
        std::fs::create_dir_all(&dir).expect("music dir");
        std::fs::write(
            dir.join("b.json"),
            serde_json::to_string(&Music::default()).expect("json"),
        )
        .expect("write");
        let mut bank = MusicBank::default();
        bank.load_project(&root);
        let id_b = bank.assets()[0].id;

        std::fs::write(
            dir.join("a.json"),
            serde_json::to_string(&Music::default()).expect("json"),
        )
        .expect("write");
        bank.load_project(&root);
        let index_b = bank
            .assets()
            .iter()
            .position(|asset| asset.name == "b")
            .expect("b loaded");
        assert_eq!(bank.id(index_b), Some(id_b));
        assert_ne!(bank.id(0), Some(id_b));
        let _ = std::fs::remove_dir_all(&root);
    }
}
