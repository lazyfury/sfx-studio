//! Project folder loading and UI-state replacement.
//!
//! Kept separate from `app.rs` so the CLI and the interactive Open action share
//! exactly the same validation / error handling.

use std::path::{Path, PathBuf};

use crate::app::{LibraryMode, State, Tab};
use crate::model::{self, Project};
use crate::presets::PresetBank;
use crate::tres;

pub(crate) fn open_folder(state: &mut State, folder: PathBuf) -> Result<Vec<String>, String> {
    let (project, mut warnings) = load_folder(&folder)?;
    state.doc.project = project;
    state.doc.root = Some(folder.clone());
    warnings.extend(state.presets.load_project(&folder));
    state.view.selected_patch = None;
    state.view.selected_layer = None;
    state.view.selected_clip = None;
    state.view.selected_env_point = None;
    state.view.inspector_scroll = 0.0;
    state.view.patch_scroll = 0;
    state.view.preset_scroll = 0;
    state.view.layer_scroll = 0;
    state.view.library_mode = LibraryMode::Project;
    state.view.selected_preset = None;
    state.view.category_filter = None;
    state.view.favorites_only = false;
    state.view.search.clear();
    state.view.search_focus = false;
    state.view.menu_open = false;
    state.view.tab = Tab::Patches;
    state.playback.playhead = 0.0;
    state.playback.seeked = false;
    state.doc.dirty = false;
    state.doc.undo.clear();
    state.doc.redo.clear();
    state.doc.pending_edit = None;
    state.doc.version = state.doc.version.wrapping_add(1);
    state.doc.music_version = state.doc.music_version.wrapping_add(1);
    state.view.view_version = state.view.view_version.wrapping_add(1);
    state.playback.cache_version = 0;
    state.playback.music_cache_version = 0;
    state.playback.defer_render = true;
    state.ensure_visible();
    Ok(warnings)
}

/// Display names stay exactly as typed (including Chinese); only export file
/// stems go through `model::sanitize`.
pub(crate) fn display_name(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "patch".into()
    } else {
        trimmed.chars().take(48).collect()
    }
}

pub(crate) fn print_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

/// Create a new project in `folder`: `sfx.project.json`, `audio/sfx/` samples
/// and an internal `presets/` library seeded from the demo sounds.
/// Refuses to overwrite an existing project file or a non-empty `presets/`.
pub(crate) fn create_project(folder: &Path) -> Result<(Project, Vec<String>), String> {
    if folder.join(model::PROJECT_FILE).exists() {
        return Err(format!(
            "{} already contains {}",
            folder.display(),
            model::PROJECT_FILE
        ));
    }
    let preset_dir = folder.join(crate::presets::PRESETS_DIR);
    if preset_dir.exists()
        && std::fs::read_dir(&preset_dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    {
        return Err(format!(
            "{} already contains files in {}/",
            folder.display(),
            crate::presets::PRESETS_DIR
        ));
    }
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let mut project = Project::starter(name);
    let warnings = project.validate();
    model::save_project(folder, &project).map_err(|error| error.to_string())?;
    let presets = Project::demo("preset bank").patches;
    if let Err(error) = PresetBank::write_presets(folder, &presets) {
        return Err(format!("presets: {error}"));
    }
    Ok((project, warnings))
}

/// Load `sfx.project.json` when present, otherwise import `audio/sfx/*.tres`.
/// Returns a clear error instead of falling back to the demo content.
pub(crate) fn load_folder(folder: &Path) -> Result<(Project, Vec<String>), String> {
    let mut project = if let Some(project) = model::load_project(folder)? {
        project
    } else {
        match tres::import_project(folder)? {
            Some(project) => project,
            None => {
                return Err(format!(
                    "no {} or audio/sfx/*.tres in {}",
                    model::PROJECT_FILE,
                    folder.display()
                ))
            }
        }
    };
    project.migrate();
    let mut warnings = project.validate();
    warnings.extend(project.load_samples(folder));
    Ok((project, warnings))
}
