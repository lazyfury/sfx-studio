//! Project folder loading and UI-state replacement.
//!
//! Kept separate from `app.rs` so the CLI and the interactive Open action share
//! exactly the same validation / error handling.

use std::path::{Path, PathBuf};

use crate::app::{State, Tab};
use crate::model::{self, Project};
use crate::tres;

pub(crate) fn open_folder(state: &mut State, folder: PathBuf) -> Result<Vec<String>, String> {
    let (project, warnings) = load_folder(&folder)?;
    state.doc.project = project;
    state.doc.root = Some(folder);
    state.view.selected_patch = 0;
    state.view.selected_layer = 0;
    state.view.selected_clip = None;
    state.view.selected_env_point = None;
    state.view.inspector_scroll = 0.0;
    state.view.patch_scroll = 0;
    state.view.layer_scroll = 0;
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
    let warnings = project.validate();
    Ok((project, warnings))
}
