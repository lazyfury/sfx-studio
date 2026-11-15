//! App state, input handling, audio bridging and the per-frame entry points.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use igui_app::{AppLogic, EventContext, FrameContext, InitContext};
use igui_core::{Cursor, EventResult, InputEvent, Key, Modifiers, PointerButton, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_ui::{ApproxTextMeasurer, TextMeasurer};

use crate::audio::AudioEngine;
use crate::fx;
use crate::library::{AudioBank, LibraryEntry, LibraryKey, LibraryKind, LibraryListRow};
use crate::model::{
    AutomationKind, Category, Clip, ClipField, Layer, LayerField, MusicField, Patch, PatchField,
    Project, Wave,
};
use crate::music_bank::MusicBank;
use crate::presets::PresetBank;
use crate::project_io::{display_name, open_folder};
use crate::synth::{self, Envelope};
use crate::{export, model, theme, ui};

mod audio_bridge;
mod commands;
mod input;
mod render;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Patches,
    Music,
}

/// Source shown in the left library. The project document and timeline stay
/// in place while this switches, so both are valid drop targets.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LibraryMode {
    #[default]
    Project,
    Presets,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ModalAction {
    NewPatch,
    RenamePatch,
    RenamePreset,
    ConfirmDeletePreset,
    /// Discard the current dirty project and open this folder.
    ConfirmOpen(PathBuf),
    /// Discard the current dirty project and create a new one here.
    ConfirmNewProject(PathBuf),
}

pub struct Modal {
    pub title: String,
    pub text: String,
    pub action: ModalAction,
}

impl Modal {
    pub fn is_input(&self) -> bool {
        matches!(
            self.action,
            ModalAction::NewPatch | ModalAction::RenamePatch | ModalAction::RenamePreset
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimelineDrag {
    Move,
    Resize,
    ResizeLeft,
}

pub enum Drag {
    None,
    Layer {
        field: LayerField,
        last_x: f32,
    },
    PatchField {
        field: PatchField,
        last_x: f32,
    },
    Music {
        field: MusicField,
        last_x: f32,
    },
    ClipField {
        layer: usize,
        clip: usize,
        field: ClipField,
        last_x: f32,
    },
    Clip {
        layer: usize,
        clip: usize,
        mode: TimelineDrag,
        start_x: f32,
        start: f32,
        duration: f32,
    },
    AutomationPoint {
        layer: usize,
        clip: usize,
        index: usize,
    },
    Seek {
        waveform: bool,
    },
    Curve {
        kind: ui::CurveKind,
    },
    InspectorScroll {
        start_y: f32,
        start_scroll: f32,
    },
    LibraryScroll {
        mode: LibraryMode,
        start_y: f32,
        start_scroll: usize,
        travel: f32,
        max_start: usize,
    },
    /// A project-local preset row is being dragged towards the timeline or
    /// the project library. `moved` only becomes true after the 4 px threshold.
    LibraryItem {
        key: LibraryKey,
        start: Vec2,
        moved: bool,
    },
}

pub struct DocState {
    pub project: Project,
    pub root: Option<PathBuf>,
    pub dirty: bool,
    /// Patch/document revision; invalidates the patch render cache.
    pub version: u64,
    /// Music revision; invalidates the music render cache independently.
    pub music_version: u64,
    pub undo: Vec<(String, Project)>,
    pub redo: Vec<(String, Project)>,
    /// Project clone captured at pointer-down; committed on pointer-up only if
    /// something actually changed.
    pub pending_edit: Option<Project>,
}

pub struct ViewState {
    pub tab: Tab,
    /// Stable IDs of the current selection; resolve to indices through the
    /// `State` helpers so reordering/deleting rows cannot silently retarget.
    pub selected_patch: Option<u32>,
    pub selected_layer: Option<u32>,
    pub selected_clip: Option<u32>,
    pub selected_env_point: Option<usize>,
    pub automation_lane: AutomationKind,
    pub library_mode: LibraryMode,
    pub selected_preset: Option<usize>,
    /// Unified row selection across sound/audio/preset/music.
    pub selected_library: Option<LibraryKey>,
    pub kind_filter: Option<LibraryKind>,
    pub kind_menu_open: bool,
    /// Folder keys (kind + relative path) collapsed by the user.
    pub folded_folders: HashSet<String>,
    pub patch_scroll: usize,
    pub preset_scroll: usize,
    pub library_scroll: usize,
    pub layer_scroll: usize,
    /// Pure view revision (selection/scroll/playhead); never renders audio.
    pub view_version: u64,
    pub modifiers: Modifiers,
    pub drag: Drag,
    pub modal: Option<Modal>,
    pub hover: Option<ui::Action>,
    pub hover_hit: Option<ui::Hit>,
    pub viewport: Size,
    pub cursor: Vec2,
    pub category_filter: Option<Category>,
    pub category_menu_open: bool,
    pub favorites_only: bool,
    pub search: String,
    pub search_focus: bool,
    pub inspector_scroll: f32,
    /// Timeline span (seconds) frozen while a clip gesture is in flight, so
    /// the scale cannot drift under the cursor as the clip grows.
    pub timeline_pin: Option<f32>,
    /// Zoom factor over the fitted timeline (1.0 = fit) and its left edge in
    /// seconds. Both are clamped by `Layout` every frame.
    pub timeline_zoom: f32,
    pub timeline_offset: f32,
    /// Top-left system menu (all file/export actions live here).
    pub menu_open: bool,
}

pub struct PlaybackState {
    pub cache_version: u64,
    pub music_cache_version: u64,
    pub cache_rate: u32,
    pub music_cache_rate: u32,
    /// Interleaved preview frames: `L, R, L, R, …` for patches, mono for music.
    pub cache: Vec<f32>,
    pub cache_channels: usize,
    pub cache_peak: f32,
    pub cache_envelope: Envelope,
    pub music_cache: Vec<f32>,
    pub music_cache_peak: f32,
    pub music_envelope: Envelope,
    pub loop_play: bool,
    pub playhead: f32,
    pub seeked: bool,
    pub was_playing: bool,
    /// True while the audio callback is playing an audition from the project
    /// preset library. The project playhead must not follow that audition.
    pub preset_playing: bool,
    pub audio_ok: bool,
    pub audio_rate: u32,
    /// Time of the last actual preview render; throttles drag-time renders.
    pub last_render: Instant,
    /// Defer the next preview render by one frame so the shell/tab paints
    /// before a potentially slow render blocks the UI thread.
    pub defer_render: bool,
}

pub struct UiState {
    pub status: String,
    pub toast: Option<(String, Instant)>,
}

pub struct State {
    pub doc: DocState,
    pub view: ViewState,
    pub playback: PlaybackState,
    pub ui: UiState,
    /// Read-only project-local preset bank plus persisted favourites.
    pub presets: PresetBank,
    /// WAV assets discovered under `<project>/audio/`.
    pub audio_bank: AudioBank,
    /// Music arrangement files discovered under `<project>/music/`.
    pub music_bank: MusicBank,
}

impl State {
    /// No project selected: the UI shows an empty shell until OPEN / NEW
    /// PROJECT picks a directory.
    pub fn empty() -> Self {
        let mut state = Self {
            doc: DocState {
                project: Project::default(),
                root: None,
                dirty: false,
                version: 1,
                music_version: 1,
                undo: Vec::new(),
                redo: Vec::new(),
                pending_edit: None,
            },
            view: ViewState {
                tab: Tab::Patches,
                selected_patch: None,
                selected_layer: None,
                selected_clip: None,
                selected_env_point: None,
                automation_lane: AutomationKind::Gain,
                library_mode: LibraryMode::Project,
                selected_preset: None,
                selected_library: None,
                kind_filter: None,
                kind_menu_open: false,
                folded_folders: HashSet::new(),
                patch_scroll: 0,
                preset_scroll: 0,
                library_scroll: 0,
                layer_scroll: 0,
                view_version: 1,
                modifiers: Modifiers::NONE,
                drag: Drag::None,
                modal: None,
                hover: None,
                hover_hit: None,
                viewport: Size::new(theme::VIEW_W, theme::VIEW_H),
                cursor: Vec2::ZERO,
                category_filter: None,
                category_menu_open: false,
                favorites_only: false,
                search: String::new(),
                search_focus: false,
                inspector_scroll: 0.0,
                timeline_pin: None,
                timeline_zoom: 1.0,
                timeline_offset: 0.0,
                menu_open: false,
            },
            playback: PlaybackState {
                cache_version: 0,
                music_cache_version: 0,
                cache_rate: 22050,
                music_cache_rate: 11025,
                cache: Vec::new(),
                cache_channels: 2,
                cache_peak: 0.0,
                cache_envelope: Envelope::default(),
                music_cache: Vec::new(),
                music_cache_peak: 0.0,
                music_envelope: Envelope::default(),
                loop_play: false,
                playhead: 0.0,
                seeked: false,
                was_playing: false,
                preset_playing: false,
                audio_ok: false,
                audio_rate: 22050,
                last_render: Instant::now(),
                defer_render: true,
            },
            ui: UiState {
                status: "OPEN a project folder (⌘O) · MENU → NEW PROJECT".into(),
                toast: None,
            },
            presets: PresetBank::default(),
            audio_bank: AudioBank::default(),
            music_bank: MusicBank::default(),
        };
        state.clamp_selection();
        state
    }

    /// Populated demo used by tests and headless selfcheck. The interactive
    /// app starts from [`State::empty`] and must open a real project folder.
    pub fn demo() -> Self {
        let mut state = Self::empty();
        state.doc.project = Project::demo("untitled");
        state.presets = PresetBank::from_patches(Project::demo("demo presets").patches);
        state.ui.status = "OPEN a project folder · demo loaded".into();
        state.clamp_selection();
        state
    }

    pub fn patch_index(&self) -> Option<usize> {
        let id = self.view.selected_patch?;
        self.doc
            .project
            .patches
            .iter()
            .position(|patch| patch.id == id)
    }

    pub fn patch(&self) -> Option<&Patch> {
        self.patch_index()
            .and_then(|index| self.doc.project.patches.get(index))
    }

    /// Seconds represented by the timeline, including the reverb tail so the
    /// playhead and waveform cover the whole audible render. Pinned during
    /// clip drags.
    pub fn timeline_span(&self) -> f32 {
        self.view.timeline_pin.unwrap_or_else(|| {
            self.patch()
                .map(|patch| fx::render_duration(patch).max(0.05))
                .unwrap_or(0.2)
        })
    }

    pub fn patch_mut(&mut self) -> Option<&mut Patch> {
        let index = self.patch_index()?;
        self.doc.project.patches.get_mut(index)
    }

    pub fn layer_index(&self) -> Option<usize> {
        let id = self.view.selected_layer?;
        self.patch()
            .and_then(|patch| patch.layers.iter().position(|layer| layer.id == id))
    }

    pub fn layer(&self) -> Option<&Layer> {
        let index = self.layer_index()?;
        self.patch().and_then(|patch| patch.layers.get(index))
    }

    pub fn layer_mut(&mut self) -> Option<&mut Layer> {
        let index = self.layer_index()?;
        self.patch_mut()
            .and_then(|patch| patch.layers.get_mut(index))
    }

    pub fn clip_index(&self) -> Option<usize> {
        let id = self.view.selected_clip?;
        self.layer()
            .and_then(|layer| layer.clips.iter().position(|clip| clip.id == id))
    }

    pub fn clip(&self) -> Option<&Clip> {
        let index = self.clip_index()?;
        self.layer().and_then(|layer| layer.clips.get(index))
    }

    pub fn clip_mut(&mut self) -> Option<&mut Clip> {
        let index = self.clip_index()?;
        self.layer_mut()
            .and_then(|layer| layer.clips.get_mut(index))
    }

    fn matches_filters(&self, patch: &Patch, favorite: bool) -> bool {
        let category_ok = self
            .view
            .category_filter
            .map(|category| patch.category == category)
            .unwrap_or(true);
        let favorite_ok = !self.view.favorites_only || favorite;
        let search = self.view.search.trim().to_lowercase();
        let search_ok = search.is_empty() || patch.name.to_lowercase().contains(&search);
        category_ok && favorite_ok && search_ok
    }

    pub fn patch_visible(&self, index: usize) -> bool {
        if self
            .view
            .kind_filter
            .is_some_and(|kind| kind != LibraryKind::Sound)
        {
            return false;
        }
        let Some(patch) = self.doc.project.patches.get(index) else {
            return false;
        };
        self.matches_filters(patch, patch.favorite)
    }

    pub fn visible_patches(&self) -> Vec<usize> {
        self.doc
            .project
            .patches
            .iter()
            .enumerate()
            .filter(|(index, _)| self.patch_visible(*index))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn preset_visible(&self, index: usize) -> bool {
        if self
            .view
            .kind_filter
            .is_some_and(|kind| kind != LibraryKind::Preset)
        {
            return false;
        }
        let Some(patch) = self.presets.get(index) else {
            return false;
        };
        self.matches_filters(patch, self.presets.is_favorite(patch))
    }

    pub fn visible_presets(&self) -> Vec<usize> {
        (0..self.presets.len())
            .filter(|index| self.preset_visible(*index))
            .collect()
    }

    /// Every entry from all four sources, before the library filters run.
    /// Layout/paint consume `visible_library_rows`; this full list is useful
    /// for tests and for the clear/reset path.
    pub fn all_library_entries(&self) -> Vec<LibraryEntry> {
        let mut entries = Vec::new();
        for patch in &self.doc.project.patches {
            let wave = patch
                .layers
                .first()
                .map(|layer| layer.wave.label())
                .unwrap_or("—");
            entries.push(LibraryEntry {
                key: LibraryKey::Sound(patch.id),
                kind: LibraryKind::Sound,
                name: patch.name.clone(),
                detail: wave.to_string(),
                category: Some(patch.category),
                favorite: patch.favorite,
                folder: None,
            });
        }
        for (index, asset) in self.audio_bank.assets().iter().enumerate() {
            entries.push(LibraryEntry {
                key: LibraryKey::Audio(index),
                kind: LibraryKind::Audio,
                name: asset.name.clone(),
                detail: format_size(asset.bytes),
                category: None,
                favorite: false,
                folder: asset
                    .path
                    .strip_prefix("audio/")
                    .and_then(|path| path.rsplit_once('/'))
                    .map(|(folder, _)| folder.to_string())
                    .filter(|folder| !folder.is_empty()),
            });
        }
        for (index, preset) in self.presets.presets.iter().enumerate() {
            let wave = preset
                .layers
                .first()
                .map(|layer| layer.wave.label())
                .unwrap_or("—");
            entries.push(LibraryEntry {
                key: LibraryKey::Preset(index),
                kind: LibraryKind::Preset,
                name: preset.name.clone(),
                detail: wave.to_string(),
                category: Some(preset.category),
                favorite: self.presets.is_favorite(preset),
                folder: self.presets.folder(index).map(str::to_string),
            });
        }
        for (index, asset) in self.music_bank.assets().iter().enumerate() {
            entries.push(LibraryEntry {
                key: LibraryKey::Music(index),
                kind: LibraryKind::Music,
                name: asset.name.clone(),
                detail: format!("{:.0} BPM · {} BARS", asset.music.bpm, asset.music.bars),
                category: None,
                favorite: false,
                folder: asset.folder.clone(),
            });
        }
        // Stable sort only by kind: source order inside each kind is already
        // meaningful (project order, path order) and keeps folders contiguous.
        entries.sort_by_key(|entry| entry.kind.order());
        entries
    }

    fn library_entry_matches(&self, entry: &LibraryEntry) -> bool {
        if self.view.kind_filter.is_some_and(|kind| entry.kind != kind) {
            return false;
        }
        if let Some(category) = self.view.category_filter {
            if entry.category != Some(category) {
                return false;
            }
        }
        if self.view.favorites_only && !entry.favorite {
            return false;
        }
        let search = self.view.search.trim().to_lowercase();
        search.is_empty() || entry.name.to_lowercase().contains(&search)
    }

    pub fn visible_library_entries(&self) -> Vec<LibraryEntry> {
        self.all_library_entries()
            .into_iter()
            .filter(|entry| self.library_entry_matches(entry))
            .collect()
    }

    pub fn library_entry(&self, key: LibraryKey) -> Option<LibraryEntry> {
        self.all_library_entries()
            .into_iter()
            .find(|entry| entry.key == key)
    }

    /// Unified list including collapsible folder headers. Layout, paint, hit,
    /// scrolling and keyboard navigation all consume this one order.
    pub fn visible_library_rows(&self) -> Vec<LibraryListRow> {
        let entries = self.visible_library_entries();
        let mut folder_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for entry in &entries {
            if let Some(key) = entry.folder_key() {
                *folder_counts.entry(key).or_default() += 1;
            }
        }
        let mut rows = Vec::new();
        let mut last_folder: Option<String> = None;
        for entry in entries {
            let folder = entry.folder_key();
            if folder != last_folder {
                if let Some(key) = &folder {
                    let label = entry.folder.clone().unwrap_or_else(|| "/".into());
                    rows.push(LibraryListRow::Folder {
                        key: key.clone(),
                        label,
                        count: folder_counts.get(key).copied().unwrap_or(0),
                    });
                }
                last_folder = folder.clone();
            }
            if folder.is_some_and(|key| self.view.folded_folders.contains(&key)) {
                continue;
            }
            rows.push(LibraryListRow::Entry(entry));
        }
        rows
    }

    pub fn toggle_library_folder(&mut self, key: &str) {
        if !self.view.folded_folders.remove(key) {
            self.view.folded_folders.insert(key.to_string());
        }
        // Do not auto-unfold the selected row here: folding the folder that
        // contains the selection must actually collapse it.
        let capacity = ui::library_capacity(self.view.viewport);
        let len = self.visible_library_rows().len();
        self.view.library_scroll = self.view.library_scroll.min(len.saturating_sub(capacity));
        self.bump_view();
    }

    pub fn selected_library_entry(&self) -> Option<LibraryEntry> {
        let key = self.view.selected_library?;
        self.library_entry(key)
    }

    pub fn selected_library_kind(&self) -> Option<LibraryKind> {
        self.view.selected_library.map(LibraryKey::kind)
    }

    pub fn selected_audio(&self) -> Option<&crate::library::AudioAsset> {
        match self.view.selected_library {
            Some(LibraryKey::Audio(index)) => self.audio_bank.get(index),
            _ => None,
        }
    }

    pub fn selected_music(&self) -> Option<usize> {
        match self.view.selected_library {
            Some(LibraryKey::Music(index)) => Some(index),
            _ => None,
        }
    }

    pub fn library_scroll(&self) -> usize {
        self.view.library_scroll
    }

    pub fn set_library_scroll(&mut self, value: usize) {
        self.view.library_scroll = value;
    }

    pub fn set_library_mode(&mut self, mode: LibraryMode) {
        self.view.library_mode = mode;
        self.view.category_menu_open = false;
        self.view.kind_menu_open = false;
        self.view.menu_open = false;
        if mode == LibraryMode::Presets
            && self
                .view
                .selected_preset
                .is_some_and(|index| !self.preset_visible(index))
        {
            self.view.selected_preset = None;
        }
        match mode {
            LibraryMode::Project => {
                self.view.selected_library = self.patch().map(|patch| LibraryKey::Sound(patch.id));
            }
            LibraryMode::Presets => {
                self.view.selected_library = self.view.selected_preset.map(LibraryKey::Preset);
            }
        }
        self.ui.status = match mode {
            LibraryMode::Project => "PROJECT library · edits save with the project".into(),
            LibraryMode::Presets => {
                "PRESETS library · bundled with this project · click to audition · drag to timeline"
                    .into()
            }
        };
        self.ensure_visible();
        self.bump_view();
    }

    pub fn preset(&self, index: usize) -> Option<&Patch> {
        self.presets.get(index)
    }

    /// The patch that the WAV export command should write. The active library
    /// decides the target: PRESETS exports the selected preset without adding
    /// it to the project, PROJECT exports the current sound.
    pub fn export_patch(&self) -> Option<&Patch> {
        match self.view.selected_library {
            Some(LibraryKey::Preset(index)) => return self.presets.get(index),
            Some(LibraryKey::Audio(_) | LibraryKey::Music(_)) => return None,
            _ => {}
        }
        match self.view.library_mode {
            LibraryMode::Project => self.patch(),
            LibraryMode::Presets => self
                .view
                .selected_preset
                .and_then(|index| self.presets.get(index)),
        }
    }

    pub fn select_preset(&mut self, index: usize) {
        if self.presets.get(index).is_some() {
            self.view.selected_preset = Some(index);
            self.view.selected_library = Some(LibraryKey::Preset(index));
            self.view.library_mode = LibraryMode::Presets;
            self.ensure_preset_visible();
            self.ensure_library_visible();
            self.bump_view();
        }
    }

    pub fn select_audio(&mut self, index: usize) {
        if self.audio_bank.get(index).is_some() {
            self.view.selected_library = Some(LibraryKey::Audio(index));
            self.view.selected_preset = None;
            self.view.library_mode = LibraryMode::Project;
            self.ensure_library_visible();
            self.bump_view();
        }
    }

    pub fn select_music(&mut self, index: usize) {
        if self.music_bank.get(index).is_none() {
            return;
        }
        self.view.tab = Tab::Music;
        self.view.selected_library = Some(LibraryKey::Music(index));
        self.view.selected_preset = None;
        self.view.library_mode = LibraryMode::Project;
        self.playback.defer_render = true;
        self.ensure_library_visible();
        self.bump_view();
    }

    pub fn ensure_library_visible(&mut self) {
        let Some(selected) = self.view.selected_library else {
            return;
        };
        let capacity = ui::library_capacity(self.view.viewport);
        let mut rows = self.visible_library_rows();
        let mut position = rows
            .iter()
            .position(|row| matches!(row, LibraryListRow::Entry(entry) if entry.key == selected));
        if position.is_none() {
            if let Some(folder) = self
                .library_entry(selected)
                .and_then(|entry| entry.folder_key())
            {
                self.view.folded_folders.remove(&folder);
                rows = self.visible_library_rows();
                position = rows.iter().position(
                    |row| matches!(row, LibraryListRow::Entry(entry) if entry.key == selected),
                );
            }
        }
        let position = position.unwrap_or(0);
        if position < self.view.library_scroll {
            self.view.library_scroll = position;
        } else if position >= self.view.library_scroll + capacity {
            self.view.library_scroll = position + 1 - capacity;
        }
        self.view.library_scroll = self
            .view
            .library_scroll
            .min(rows.len().saturating_sub(capacity));
    }

    fn ensure_preset_visible(&mut self) {
        let Some(selected) = self.view.selected_preset else {
            return;
        };
        let capacity = ui::library_capacity(self.view.viewport);
        let visible = self.visible_presets();
        let position = visible
            .iter()
            .position(|candidate| *candidate == selected)
            .unwrap_or(0);
        if position < self.view.preset_scroll {
            self.view.preset_scroll = position;
        } else if position >= self.view.preset_scroll + capacity {
            self.view.preset_scroll = position + 1 - capacity;
        }
        self.view.preset_scroll = self
            .view
            .preset_scroll
            .min(visible.len().saturating_sub(capacity));
    }

    pub fn mark_dirty(&mut self) {
        // Structural edits (add/duplicate) can introduce id-less objects;
        // assign stable IDs before the snapshot/version moves on.
        self.doc.project.ensure_ids();
        self.doc.version = self.doc.version.wrapping_add(1);
        self.view.view_version = self.view.view_version.wrapping_add(1);
        self.doc.redo.clear();
        self.doc.dirty = true;
    }

    /// Music edits only invalidate the music cache; patch previews stay warm.
    pub fn mark_music_dirty(&mut self) {
        self.doc.music_version = self.doc.music_version.wrapping_add(1);
        self.view.view_version = self.view.view_version.wrapping_add(1);
        self.doc.redo.clear();
        self.doc.dirty = true;
    }

    pub fn bump_view(&mut self) {
        self.view.view_version = self.view.view_version.wrapping_add(1);
    }

    pub fn select_patch(&mut self, index: usize) {
        let previous_layer = self.layer_index().unwrap_or(0);
        if let Some(patch) = self.doc.project.patches.get(index) {
            self.view.selected_patch = Some(patch.id);
            self.view.selected_library = Some(LibraryKey::Sound(patch.id));
            self.view.selected_preset = None;
            self.view.library_mode = LibraryMode::Project;
            self.view.inspector_scroll = 0.0;
            let keep_layer = self
                .view
                .selected_layer
                .filter(|id| patch.layers.iter().any(|layer| layer.id == *id));
            let clamped_slot = previous_layer.min(patch.layers.len().saturating_sub(1));
            self.view.selected_layer =
                keep_layer.or_else(|| patch.layers.get(clamped_slot).map(|layer| layer.id));
            self.bump_view();
            // Selecting a different patch must re-render the preview even
            // though the document itself is unchanged.
            self.playback.cache_version = 0;
            self.playback.defer_render = true;
            self.playback.playhead = 0.0;
            self.playback.seeked = false;
            self.view.selected_clip = None;
            self.view.selected_env_point = None;
            self.ensure_visible();
        }
    }

    pub fn select_layer(&mut self, index: usize) {
        self.view.inspector_scroll = 0.0;
        if let Some(patch) = self.patch() {
            if let Some(layer) = patch.layers.get(index) {
                self.view.selected_layer = Some(layer.id);
                self.view.selected_clip = None;
                self.view.selected_env_point = None;
                self.bump_view();
                self.ensure_visible();
            }
        }
    }

    pub fn select_clip(&mut self, index: usize) {
        self.view.selected_clip = self
            .layer()
            .and_then(|layer| layer.clips.get(index))
            .map(|clip| clip.id);
        self.view.selected_env_point = None;
        self.bump_view();
    }

    pub fn ensure_visible(&mut self) {
        let patch_capacity = ui::library_capacity(self.view.viewport);
        let visible = self.visible_patches();
        let position = self
            .view
            .selected_patch
            .and_then(|id| {
                visible
                    .iter()
                    .position(|index| self.doc.project.patches[*index].id == id)
            })
            .unwrap_or(0);
        if position < self.view.patch_scroll {
            self.view.patch_scroll = position;
        } else if position >= self.view.patch_scroll + patch_capacity {
            self.view.patch_scroll = position + 1 - patch_capacity;
        }
        self.view.patch_scroll = self
            .view
            .patch_scroll
            .min(visible.len().saturating_sub(patch_capacity));

        let layers = self.patch().map(|patch| patch.layers.len()).unwrap_or(1);
        let layer_capacity = ui::layer_capacity(self.view.viewport, layers);
        let layer_index = self.layer_index().unwrap_or(0);
        if layer_index < self.view.layer_scroll {
            self.view.layer_scroll = layer_index;
        } else if layer_index >= self.view.layer_scroll + layer_capacity {
            self.view.layer_scroll = layer_index + 1 - layer_capacity;
        }

        if self.view.library_mode == LibraryMode::Presets {
            self.ensure_preset_visible();
        }
        self.ensure_library_visible();
    }

    /// Capture the pre-edit state once per pointer gesture. The snapshot is
    /// only pushed if the project actually changes (see [`Self::commit_edit`]).
    pub fn begin_edit(&mut self) {
        if self.doc.pending_edit.is_none() {
            self.doc.pending_edit = Some(self.doc.project.clone());
        }
    }

    /// Commit a gesture snapshot. No-op clicks / drags that change nothing do
    /// not pollute the undo stack.
    pub fn commit_edit(&mut self) {
        let Some(before) = self.doc.pending_edit.take() else {
            return;
        };
        if before != self.doc.project {
            self.push_snapshot("edit", before);
        }
    }

    fn push_snapshot(&mut self, label: impl Into<String>, project: Project) {
        self.doc.undo.push((label.into(), project));
        if self.doc.undo.len() > 64 {
            self.doc.undo.remove(0);
        }
        self.doc.redo.clear();
    }

    pub fn push_undo(&mut self) {
        self.push_undo_label("edit");
    }

    pub fn push_undo_label(&mut self, label: impl Into<String>) {
        self.commit_edit();
        self.push_snapshot(label, self.doc.project.clone());
    }

    pub fn undo(&mut self) -> Option<String> {
        self.commit_edit();
        let (label, previous) = self.doc.undo.pop()?;
        self.doc
            .redo
            .push((label.clone(), self.doc.project.clone()));
        self.doc.project = previous;
        self.clamp_selection();
        self.invalidate_document();
        Some(label)
    }

    pub fn redo(&mut self) -> Option<String> {
        self.commit_edit();
        let (label, next) = self.doc.redo.pop()?;
        self.doc
            .undo
            .push((label.clone(), self.doc.project.clone()));
        if self.doc.undo.len() > 64 {
            self.doc.undo.remove(0);
        }
        self.doc.project = next;
        self.clamp_selection();
        self.invalidate_document();
        Some(label)
    }

    fn invalidate_document(&mut self) {
        // Undo/redo must keep the opposite stack alive, so this deliberately
        // does not go through `mark_dirty` (which clears redo).
        self.doc.version = self.doc.version.wrapping_add(1);
        self.doc.music_version = self.doc.music_version.wrapping_add(1);
        self.view.view_version = self.view.view_version.wrapping_add(1);
        self.doc.dirty = true;
        self.playback.cache_version = 0;
        self.playback.music_cache_version = 0;
        self.playback.defer_render = true;
        self.playback.playhead = 0.0;
        self.playback.seeked = false;
        self.ensure_visible();
    }

    fn clamp_selection(&mut self) {
        self.view.selected_patch = self
            .view
            .selected_patch
            .filter(|id| self.doc.project.patches.iter().any(|patch| patch.id == *id))
            .or_else(|| self.doc.project.patches.first().map(|patch| patch.id));
        let sound_key = self.view.selected_patch.map(LibraryKey::Sound);
        self.view.selected_library = match self.view.selected_library {
            None => sound_key,
            Some(LibraryKey::Sound(id))
                if !self.doc.project.patches.iter().any(|patch| patch.id == id) =>
            {
                sound_key
            }
            Some(LibraryKey::Preset(index)) if self.presets.get(index).is_none() => None,
            Some(LibraryKey::Audio(index)) if self.audio_bank.get(index).is_none() => None,
            other => other,
        };
        self.view.selected_layer = self.patch().and_then(|patch| {
            self.view
                .selected_layer
                .filter(|id| patch.layers.iter().any(|layer| layer.id == *id))
                .or_else(|| patch.layers.first().map(|layer| layer.id))
        });
        if self.clip_index().is_none() {
            self.view.selected_clip = None;
        }
        if self.view.selected_env_point.is_some() {
            let points = self
                .clip()
                .map(|clip| self.view.automation_lane.points(clip).len())
                .unwrap_or(0);
            if self.view.selected_env_point.unwrap_or(0) >= points {
                self.view.selected_env_point = None;
            }
        }
    }

    pub fn preview_rate(&self) -> u32 {
        // Preview always renders at the export rate; the audio callback
        // resamples to whatever the device asks for.
        synth::PREVIEW_RATE
    }

    pub fn ensure_cache(&mut self, rate: u32) {
        let want_patch = self.view.tab == Tab::Patches;
        let want_music = self.view.tab == Tab::Music;
        let patch_dirty = want_patch
            && (self.playback.cache_version != self.doc.version
                || self.playback.cache_rate != rate);
        let music_dirty = want_music
            && (self.playback.music_cache_version != self.doc.music_version
                || self.playback.music_cache_rate != rate);
        if !patch_dirty && !music_dirty {
            return;
        }
        if patch_dirty {
            if let Some(patch) = self.patch() {
                let stereo = fx::render_patch(patch, rate);
                let mono = stereo.mono();
                self.playback.cache_peak =
                    synth::peak(&stereo.left).max(synth::peak(&stereo.right));
                self.playback.cache_envelope = synth::envelope(&mono, synth::ENVELOPE_BINS);
                self.playback.cache = stereo.interleave();
                self.playback.cache_channels = 2;
            } else {
                self.playback.cache.clear();
                self.playback.cache_channels = 2;
                self.playback.cache_peak = 0.0;
                self.playback.cache_envelope = Envelope::default();
            }
            self.playback.cache_version = self.doc.version;
            self.playback.cache_rate = rate;
        }
        if music_dirty {
            self.playback.music_cache = synth::render_music(&self.doc.project.music, rate);
            self.playback.music_cache_peak = synth::peak(&self.playback.music_cache);
            self.playback.music_envelope =
                synth::envelope(&self.playback.music_cache, synth::ENVELOPE_BINS);
            self.playback.music_cache_version = self.doc.music_version;
            self.playback.music_cache_rate = rate;
        }
        self.playback.last_render = Instant::now();
    }

    /// Synchronous fallback used when the render worker is unavailable or
    /// stalled. Throttled to ~12×/s so dragging stays responsive.
    pub fn ensure_cache_soon(&mut self, rate: u32) {
        let already_rendered =
            self.playback.cache_version != 0 || self.playback.music_cache_version != 0;
        if already_rendered && self.playback.last_render.elapsed() < Duration::from_millis(80) {
            return;
        }
        self.ensure_cache(rate);
    }

    /// True when the active tab's preview cache is stale. Used by
    /// `needs_frame` so the deferred render gets a follow-up frame.
    pub fn preview_pending(&self) -> bool {
        let rate = self.preview_rate();
        match self.view.tab {
            Tab::Patches => {
                self.patch().is_some()
                    && (self.playback.cache_version != self.doc.version
                        || self.playback.cache_rate != rate)
            }
            Tab::Music => {
                self.playback.music_cache_version != self.doc.music_version
                    || self.playback.music_cache_rate != rate
            }
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.ui.status = text.clone();
        self.ui.toast = Some((text, Instant::now() + Duration::from_secs(4)));
    }
}

pub struct SfxLogic {
    state: State,
    measurer: Option<Rc<dyn TextMeasurer>>,
    audio: Option<AudioEngine>,
    /// Background preview renderer; `None` in tests/headless logic that do
    /// not need audio previews.
    render: Option<render::RenderWorker>,
}

/// Time under a drag; `snap = false` (Shift) keeps sub-grid precision.
fn snap_drag_time(value: f32, snap: bool) -> f32 {
    if snap {
        ui::snap_time(value)
    } else {
        value
    }
}

fn param_row_rect(layout: &ui::Layout, target: ui::Hit) -> Option<Rect> {
    for item in &layout.items {
        if let ui::Item::Param { hit, rect, .. } = item {
            if *hit == target {
                return Some(*rect);
            }
        }
    }
    None
}

fn randomize_layer(layer: &mut Layer, rng: &mut synth::Rng, amount: f32) {
    let vary = |rng: &mut synth::Rng| 1.0 + rng.next_f32() * amount;
    layer.duration = (layer.duration * vary(rng)).clamp(0.005, 3.0);
    layer.gain = (layer.gain * vary(rng)).clamp(0.02, 1.0);
    if layer.wave.is_tone() {
        let ratio = vary(rng);
        layer.freq_start = (layer.freq_start * ratio).clamp(20.0, 12000.0);
        layer.freq_end = (layer.freq_end * ratio).clamp(20.0, 12000.0);
        layer.harsh = (layer.harsh + rng.next_f32() * amount * 0.5).clamp(0.0, 1.0);
        layer.attack = (layer.attack * vary(rng)).clamp(0.0002, 1.0);
        layer.decay = (layer.decay * vary(rng)).clamp(0.005, 2.0);
        layer.release = (layer.release * vary(rng)).clamp(0.002, 2.0);
        layer.cutoff_start = (layer.cutoff_start * vary(rng)).clamp(20.0, 18000.0);
        layer.cutoff_end = (layer.cutoff_end * vary(rng)).clamp(20.0, 18000.0);
    }
    match layer.wave {
        Wave::Noise => {
            layer.lowpass_from =
                (layer.lowpass_from + rng.next_f32() * amount * 0.4).clamp(0.01, 1.0);
            layer.lowpass_to = (layer.lowpass_to + rng.next_f32() * amount * 0.4).clamp(0.01, 1.0);
            layer.noise_gain = (layer.noise_gain + rng.next_f32() * amount * 0.5).clamp(0.1, 1.0);
            layer.sweep_gain = (layer.sweep_gain + rng.next_f32() * amount * 0.4).clamp(0.0, 1.0);
            layer.decay = (layer.decay * vary(rng)).clamp(0.5, 30.0);
            layer.seed = layer.seed.wrapping_add(rng.next_u64() as u32);
        }
        Wave::Arp => {
            for freq in &mut layer.arp_freqs {
                *freq = (*freq * vary(rng)).clamp(20.0, 12000.0);
            }
            layer.step = (layer.step * vary(rng)).clamp(0.01, 0.5);
            layer.decay = (layer.decay * vary(rng)).clamp(0.5, 30.0);
        }
        Wave::Fm => {
            layer.fm_ratio = (layer.fm_ratio * vary(rng)).clamp(0.25, 12.0);
            layer.fm_index = (layer.fm_index + rng.next_f32() * amount * 4.0).clamp(0.0, 20.0);
            layer.fm_decay = (layer.fm_decay * vary(rng)).clamp(0.5, 30.0);
        }
        Wave::Pluck => {
            layer.pluck_damping =
                (layer.pluck_damping + rng.next_f32() * amount * 0.02).clamp(0.9, 0.9999);
            layer.pluck_brightness =
                (layer.pluck_brightness + rng.next_f32() * amount * 0.4).clamp(0.0, 1.0);
        }
        Wave::Kick | Wave::Snare | Wave::Hat | Wave::Clap => {
            layer.drum_tone = (layer.drum_tone * vary(rng)).clamp(0.25, 2.0);
            layer.drum_snap = (layer.drum_snap + rng.next_f32() * amount * 0.5).clamp(0.0, 2.0);
        }
        _ => {}
    }
}

fn default_duration(wave: Wave) -> f32 {
    match wave {
        Wave::Noise => 0.3,
        Wave::Arp => 0.18,
        Wave::Fm => 0.4,
        Wave::Pluck => 0.5,
        Wave::Kick | Wave::Snare => 0.18,
        Wave::Hat => 0.06,
        Wave::Clap => 0.2,
        _ => 0.15,
    }
}

fn apply_chord_preset(music: &mut model::Music, index: usize) {
    match index {
        1 => {
            music.bass_notes = vec![130.81, 110.0, 146.83, 98.0];
            music.chords = vec![
                vec![261.63, 329.63, 392.0, 493.88],
                vec![220.0, 261.63, 329.63, 392.0],
                vec![293.66, 349.23, 440.0, 523.25],
                vec![196.0, 246.94, 293.66, 349.23],
            ];
        }
        2 => {
            music.bass_notes = vec![82.41, 130.81, 98.0, 73.42];
            music.chords = vec![
                vec![164.81, 196.0, 246.94],
                vec![130.81, 164.81, 196.0],
                vec![196.0, 246.94, 293.66],
                vec![146.83, 185.0, 220.0],
            ];
        }
        _ => {
            music.bass_notes = vec![110.0, 87.31, 130.81, 98.0];
            music.chords = vec![
                vec![220.0, 261.63, 329.63],
                vec![174.61, 220.0, 261.63],
                vec![261.63, 329.63, 392.0],
                vec![196.0, 246.94, 293.66],
            ];
        }
    }
}

/// Which built-in chord preset (if any) matches the current progression.
/// Used by the MUSIC inspector so the active chip reflects the document.
pub(crate) fn chord_preset_index(music: &model::Music) -> Option<usize> {
    (0..3).find(|index| {
        let mut expected = model::Music::default();
        apply_chord_preset(&mut expected, *index);
        expected.bass_notes == music.bass_notes && expected.chords == music.chords
    })
}

fn toggle_pattern(music: &mut model::Music, row: ui::PatternRow, step: usize) {
    if step >= 16 {
        return;
    }
    music.use_patterns = true;
    while music.arp_pattern.len() < 16 {
        music.arp_pattern.push(-1);
    }
    while music.bass_pattern.len() < 16 {
        music.bass_pattern.push(-1);
    }
    match row {
        ui::PatternRow::Arp(tone) => {
            music.arp_pattern[step] = if music.arp_pattern[step] == tone as i32 {
                -1
            } else {
                tone as i32
            };
        }
        ui::PatternRow::Bass(octave) => {
            music.bass_pattern[step] = if music.bass_pattern[step] == octave as i32 {
                -1
            } else {
                octave as i32
            };
        }
        ui::PatternRow::Kick => {
            music.kick_steps ^= 1 << step;
        }
        ui::PatternRow::Hat => {
            music.hat_steps ^= 1 << step;
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("WAV · {:.1} MB", bytes as f32 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("WAV · {:.0} KB", bytes as f32 / 1024.0)
    } else {
        "WAV".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_no_project_or_presets() {
        let mut state = State::empty();
        assert!(state.doc.root.is_none());
        assert!(state.doc.project.patches.is_empty());
        assert!(state.presets.presets.is_empty());
        assert!(state.patch().is_none());
        assert!(state.layer().is_none());
        assert!(!state.preview_pending());
        state.ensure_cache(8000);
        assert!(state.playback.cache.is_empty());
        assert_eq!(state.playback.cache_peak, 0.0);
    }

    #[test]
    fn opening_a_project_loads_its_internal_presets() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-open-presets-{}-{stamp}",
            std::process::id()
        ));
        let (project, _) = crate::project_io::create_project(&root).expect("create project");
        assert_eq!(project.patches.len(), 1);
        let mut state = State::empty();
        let warnings = crate::project_io::open_folder(&mut state, root.clone()).expect("open");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(state.presets.len() >= 8);
        assert!(state
            .presets
            .presets
            .iter()
            .any(|preset| preset.name == "laser"));
        assert_eq!(state.music_bank.len(), crate::demo::tracks().len());
        assert!(root.join("music/neon_drive.json").exists());
        assert_eq!(state.doc.root.as_deref(), Some(root.as_path()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cursor_reflects_timeline_edges_and_drag() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.view.hover_hit = Some(ui::Hit::Timeline {
            layer: 0,
            clip: Some(0),
            mode: TimelineDrag::Resize,
        });
        assert_eq!(logic.cursor(), Some(Cursor::ColResize));
        logic.state.view.hover_hit = Some(ui::Hit::Timeline {
            layer: 0,
            clip: Some(0),
            mode: TimelineDrag::Move,
        });
        assert_eq!(logic.cursor(), Some(Cursor::Grab));
        logic.state.view.drag = Drag::Clip {
            layer: 0,
            clip: 0,
            mode: TimelineDrag::Move,
            start_x: 0.0,
            start: 0.0,
            duration: 0.1,
        };
        assert_eq!(logic.cursor(), Some(Cursor::Grabbing));
    }

    #[test]
    fn search_and_filters_narrow_the_library() {
        let mut state = State::demo();
        state.view.search = "wind".into();
        assert_eq!(state.visible_patches().len(), 1);
        state.view.search.clear();
        state.view.category_filter = Some(Category::Combat);
        let combat = state.visible_patches();
        assert_eq!(combat.len(), 2);
        assert!(combat
            .iter()
            .all(|index| state.doc.project.patches[*index].category == Category::Combat));
        state.view.category_filter = None;
        state.view.favorites_only = true;
        assert!(state
            .visible_patches()
            .iter()
            .all(|index| state.doc.project.patches[*index].favorite));
    }

    #[test]
    fn clicking_a_parameter_track_sets_its_value() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let layout = ui::Layout::new(&logic.state, logic.state.view.viewport);
        let hit = ui::Hit::LayerField(LayerField::Gain);
        let rect = param_row_rect(&layout, hit).expect("gain row");
        let track = ui::param_track(rect);
        let position = Vec2::new(track.right(), track.center().y);
        logic.set_param_from_x(position, hit, &layout);
        let gain = logic.state.layer().unwrap().gain;
        assert!(
            gain > 0.95,
            "gain should jump to the slider maximum: {gain}"
        );
    }

    #[test]
    fn clicking_the_space_track_sets_patch_space() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let layout = ui::Layout::new(&logic.state, logic.state.view.viewport);
        let hit = ui::Hit::PatchField(PatchField::Space);
        let rect = param_row_rect(&layout, hit).expect("space row");
        let track = ui::param_track(rect);
        let position = Vec2::new(track.right(), track.center().y);
        logic.set_param_from_x(position, hit, &layout);
        assert!(logic.state.patch().unwrap().space > 0.95);
    }

    #[test]
    fn upgrading_a_patch_switches_it_to_the_hd_chain() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.patch_mut().unwrap().dsp_version = 2;
        logic.activate(ui::Action::UpgradeDsp);
        assert_eq!(logic.state.patch().unwrap().dsp_version, model::DSP_VERSION);
        assert!(logic.state.doc.dirty);
        assert_eq!(logic.state.doc.undo.len(), 1);
    }

    #[test]
    fn inspector_wheel_scroll_moves_down_on_a_downward_wheel() {
        let mut state = State::demo();
        state.view.viewport = Size::new(1024.0, 400.0);
        let layout = ui::Layout::new(&state, state.view.viewport);
        let max = layout.inspector_max_scroll();
        assert!(max > 0.0);
        // igui_core: y > 0 scrolls down.
        assert!(super::input::inspector_wheel_scroll(0.0, 1.0, &layout) > 0.0);
        assert_eq!(super::input::inspector_wheel_scroll(max, 1.0, &layout), max);
        assert!(super::input::inspector_wheel_scroll(10.0, -1.0, &layout) < 10.0);
        assert_eq!(super::input::wheel_steps(1.0), 1);
        assert_eq!(super::input::wheel_steps(-1.0), -1);
    }

    #[test]
    fn chord_preset_index_follows_the_current_progression() {
        let mut music = model::Music::default();
        apply_chord_preset(&mut music, 1);
        assert_eq!(chord_preset_index(&music), Some(1));
        music.bass_notes.push(999.0);
        assert_eq!(chord_preset_index(&music), None);
    }

    #[test]
    fn loading_a_demo_track_replaces_the_music_and_is_undoable() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let before = logic.state.doc.project.music.clone();
        let track = crate::demo::tracks().remove(1);
        assert_ne!(before, track.music);
        let undo_before = logic.state.doc.undo.len();
        logic.activate(ui::Action::DemoTrack(1));
        assert_eq!(logic.state.doc.project.music, track.music);
        assert!(logic.state.doc.dirty);
        assert_eq!(logic.state.doc.undo.len(), undo_before + 1);
        logic.state.undo();
        assert_eq!(logic.state.doc.project.music, before);
    }

    #[test]
    fn variations_are_inserted_and_kept_in_order() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let base = logic.state.patch_index().expect("patch");
        let base_name = logic.state.patch().unwrap().name.clone();
        let original = logic.state.doc.project.patches.len();
        logic.activate(ui::Action::Vary);
        assert_eq!(logic.state.doc.project.patches.len(), original + 3);
        for (offset, suffix) in [" A", " B", " C"].iter().enumerate() {
            let patch = &logic.state.doc.project.patches[base + 1 + offset];
            assert_eq!(patch.name, format!("{base_name}{suffix}"));
            assert_ne!(patch.id, logic.state.doc.project.patches[base].id);
        }
        assert_eq!(logic.state.patch_index(), Some(base + 1));
        logic.state.undo();
        assert_eq!(logic.state.doc.project.patches.len(), original);
    }

    #[test]
    fn star_click_toggles_favorite_without_changing_selection() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let target = 2;
        let before = logic.state.doc.project.patches[target].favorite;
        logic.toggle_patch_favorite(target);
        assert_eq!(logic.state.doc.project.patches[target].favorite, !before);
        assert_eq!(logic.state.patch_index(), Some(0));
        assert_eq!(logic.state.doc.undo.len(), 1);
        logic.state.undo();
        assert_eq!(logic.state.doc.project.patches[target].favorite, before);
    }

    #[test]
    fn picking_a_category_closes_the_menu_and_filters() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.view.category_menu_open = true;
        logic.pick_category(Some(Category::Combat));
        assert_eq!(logic.state.view.category_filter, Some(Category::Combat));
        assert!(!logic.state.view.category_menu_open);
        assert!(logic
            .state
            .visible_patches()
            .iter()
            .all(|index| logic.state.doc.project.patches[*index].category == Category::Combat));
        logic.pick_category(None);
        assert_eq!(logic.state.view.category_filter, None);
    }

    #[test]
    fn shift_disables_the_clip_snap_grid() {
        assert!((snap_drag_time(0.1234, true) - 0.125).abs() < 0.0001);
        assert_eq!(snap_drag_time(0.1234, false), 0.1234);
    }

    #[test]
    fn selection_uses_stable_ids_across_reorders() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.select_patch(2);
        let patch_id = logic.state.view.selected_patch.expect("patch selected");
        let patch = logic.state.doc.project.patches.remove(2);
        logic.state.doc.project.patches.insert(0, patch);
        assert_eq!(logic.state.patch_index(), Some(0));
        assert_eq!(logic.state.view.selected_patch, Some(patch_id));

        logic.state.select_layer(1);
        let layer_id = logic.state.view.selected_layer.expect("layer selected");
        if let Some(patch) = logic.state.patch_mut() {
            patch.layers.insert(0, Layer::default());
        }
        logic.state.mark_dirty();
        assert_eq!(logic.state.layer_index(), Some(2));
        assert_eq!(logic.state.layer().map(|layer| layer.id), Some(layer_id));
    }

    #[test]
    fn deleting_the_selected_patch_selects_its_neighbour_by_id() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.select_patch(0);
        let neighbour = logic.state.doc.project.patches[1].id;
        logic.activate(ui::Action::DeletePatch);
        assert_eq!(logic.state.patch_index(), Some(0));
        assert_eq!(logic.state.view.selected_patch, Some(neighbour));
    }

    #[test]
    fn dropping_a_preset_inserts_layers_and_is_one_undo_step() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let before = logic.state.patch().unwrap().layers.len();
        let preset_layers = logic.state.preset(0).unwrap().layers.len();
        let layout = ui::Layout::new(&logic.state, logic.state.view.viewport);
        let row = layout.layer_rows[0].1;
        let pos = Vec2::new(layout.time_x(0.4), row.center().y);
        logic.drop_preset(0, pos, &layout);
        assert_eq!(
            logic.state.patch().unwrap().layers.len(),
            before + preset_layers
        );
        assert_eq!(logic.state.doc.undo.len(), 1);
        let selected = logic.state.layer().expect("new layer selected");
        assert_eq!(selected.clips.len(), 1);
        assert!((selected.clips[0].start - 0.4).abs() < 0.001);
        assert!(logic.state.view.selected_clip.is_some());
        assert!(logic.state.doc.dirty);

        let _ = logic.state.undo();
        assert_eq!(logic.state.patch().unwrap().layers.len(), before);
    }

    #[test]
    fn dropping_a_preset_on_the_left_copies_it_into_the_project() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.set_library_mode(LibraryMode::Presets);
        let before = logic.state.doc.project.patches.len();
        let source = logic.state.preset(1).unwrap().clone();
        let layout = ui::Layout::new(&logic.state, logic.state.view.viewport);
        logic.drop_preset(1, layout.left.center(), &layout);
        assert_eq!(logic.state.doc.project.patches.len(), before + 1);
        assert_eq!(logic.state.view.library_mode, LibraryMode::Project);
        let selected = logic.state.patch().expect("copied patch selected");
        assert_ne!(selected.name, source.name);
        assert!(selected.name.starts_with(&source.name));
        assert_eq!(logic.state.doc.undo.len(), 1);
    }

    #[test]
    fn preset_filters_and_selection_stay_out_of_the_project() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let project_id = logic.state.view.selected_patch;
        logic.state.set_library_mode(LibraryMode::Presets);
        logic.state.view.search = "laser".into();
        logic.refresh_filter();
        let visible = logic.state.visible_presets();
        assert_eq!(visible.len(), 1);
        assert_eq!(logic.state.preset(visible[0]).unwrap().name, "laser");
        assert_eq!(logic.state.view.selected_patch, project_id);

        logic.state.view.search.clear();
        let favorite = logic.state.preset(2).unwrap().clone();
        logic.state.presets.toggle_favorite(&favorite);
        logic.state.view.favorites_only = true;
        logic.refresh_filter();
        let favorites = logic.state.visible_presets();
        assert!(favorites.iter().all(|index| logic
            .state
            .preset(*index)
            .is_some_and(|p| p.name == favorite.name)));
        assert!(!favorites.is_empty());
    }

    #[test]
    fn installing_presets_fills_an_empty_bank_without_a_project_file() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-install-presets-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut logic = SfxLogic {
            state: State::empty(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.doc.root = Some(root.clone());
        logic.state.set_library_mode(LibraryMode::Presets);
        assert_eq!(logic.state.presets.len(), 0);

        logic.activate(ui::Action::InstallPresets);
        assert_eq!(
            logic.state.presets.len(),
            crate::builtin::PRESET_NAMES.len()
        );
        assert!(root.join("presets").is_dir());
        assert!(logic.state.ui.status.contains("installed"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn saving_the_current_patch_adds_a_project_preset() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-save-preset-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.doc.root = Some(root.clone());
        logic.state.presets = crate::presets::PresetBank::default();
        let name = logic.state.patch().expect("patch").name.clone();
        assert_eq!(logic.state.presets.len(), 0);

        logic.activate(ui::Action::SavePreset);
        assert_eq!(logic.state.presets.len(), 1);
        assert_eq!(logic.state.presets.presets[0].name, name);
        assert!(logic.state.ui.status.contains("saved preset"));
        assert!(root.join("presets").is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deleting_a_preset_goes_through_the_confirmation_modal() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-delete-preset-{}-{stamp}",
            std::process::id()
        ));
        let dir = root.join("presets");
        std::fs::create_dir_all(&dir).unwrap();
        let patch = Patch::new_layer("one", Wave::Sine);
        std::fs::write(dir.join("one.json"), serde_json::to_string(&patch).unwrap()).unwrap();
        let mut logic = SfxLogic {
            state: State::empty(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.doc.root = Some(root.clone());
        logic.state.presets.load_project(&root);
        logic.state.set_library_mode(LibraryMode::Presets);
        logic.state.select_preset(0);
        logic.activate(ui::Action::DeletePreset);
        assert!(logic.state.view.modal.is_some());

        assert!(logic.handle_modal(&InputEvent::KeyDown {
            key: igui_core::Key::Enter
        }));
        assert!(logic.state.presets.is_empty());
        assert!(!dir.join("one.json").exists());
        assert!(dir.join(".trash").is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn space_in_presets_targets_the_selected_preset() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.set_library_mode(LibraryMode::Presets);
        logic.toggle_play();
        assert!(logic.state.ui.status.contains("select a preset"));

        logic.state.select_preset(0);
        logic.toggle_play();
        assert!(logic.state.ui.status.contains("no audio device"));
        assert!(!logic.state.playback.was_playing);
        assert_eq!(logic.state.playback.playhead, 0.0);
    }

    #[test]
    fn wav_export_target_follows_the_active_library() {
        let mut state = State::demo();
        let project_name = state.export_patch().expect("project patch").name.clone();

        state.set_library_mode(LibraryMode::Presets);
        assert!(state.export_patch().is_none());
        state.select_preset(2);
        let preset_name = state.presets.get(2).expect("preset").name.clone();
        assert_eq!(
            state.export_patch().map(|patch| patch.name.as_str()),
            Some(preset_name.as_str())
        );
        assert_ne!(preset_name, project_name);

        state.set_library_mode(LibraryMode::Project);
        assert_eq!(
            state.export_patch().map(|patch| patch.name.as_str()),
            Some(project_name.as_str())
        );
    }

    #[test]
    fn display_names_keep_unicode() {
        assert_eq!(display_name("  Laser 01 "), "Laser 01");
        assert_eq!(display_name("金币 01"), "金币 01");
        assert_eq!(display_name("   "), "patch");
        assert_eq!(crate::model::sanitize("金币 01"), "sound");
    }

    #[test]
    fn startup_render_is_deferred_and_then_resolved() {
        let mut state = State::demo();
        assert!(state.playback.defer_render);
        assert!(state.preview_pending());
        state.ensure_cache(state.preview_rate());
        assert!(!state.preview_pending());
        state.select_patch(1);
        assert!(state.playback.defer_render);
        assert!(state.preview_pending());
    }

    #[test]
    fn view_changes_do_not_rerender_audio() {
        let mut state = State::demo();
        state.ensure_cache(8000);
        let rendered = state.playback.cache_version;
        let samples = state.playback.cache.len();
        state.bump_view();
        state.ensure_cache(8000);
        assert_eq!(state.playback.cache_version, rendered);
        assert_eq!(state.playback.cache.len(), samples);
        state.mark_dirty();
        state.ensure_cache(8000);
        assert_ne!(state.playback.cache_version, rendered);
    }

    #[test]
    fn clicking_the_cutoff_track_midpoint_uses_a_log_curve() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        let old_id = logic.state.doc.project.patches[0].layers[0].id;
        logic.state.doc.project.patches[0].layers[0] = Layer::new(Wave::Sine);
        logic.state.doc.project.patches[0].layers[0].id = old_id;
        let layout = ui::Layout::new(&logic.state, logic.state.view.viewport);
        let hit = ui::Hit::LayerField(LayerField::CutoffStart);
        let rect = param_row_rect(&layout, hit).expect("cutoff row");
        let track = ui::param_track(rect);
        let position = Vec2::new(track.center().x, track.center().y);
        logic.set_param_from_x(position, hit, &layout);
        let cutoff = logic.state.layer().unwrap().cutoff_start;
        // sqrt(20 * 18000) ≈ 600 Hz, far from the linear midpoint (9010).
        assert!((500.0..750.0).contains(&cutoff), "cutoff was {cutoff}");
    }

    #[test]
    fn undo_transactions_ignore_noop_gestures() {
        let mut state = State::demo();
        state.begin_edit();
        state.commit_edit();
        assert!(state.doc.undo.is_empty());
        let original = state.layer().unwrap().gain;
        state.begin_edit();
        state.layer_mut().unwrap().gain = 0.42;
        state.commit_edit();
        assert_eq!(state.doc.undo.len(), 1);
        assert!((state.layer().unwrap().gain - 0.42).abs() < 0.001);
        let _ = state.undo();
        assert!((state.layer().unwrap().gain - original).abs() < 0.001);
    }

    #[test]
    fn undo_redo_restores_clips() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.push_undo();
        state.layer_mut().unwrap().ensure_clips();
        assert_eq!(state.layer().unwrap().clips.len(), 1);
        let _ = state.undo();
        assert!(state.layer().unwrap().clips.is_empty());
        let _ = state.redo();
        assert_eq!(state.layer().unwrap().clips.len(), 1);
    }
    #[test]
    fn unified_library_lists_all_four_kinds_and_filters_them() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-unified-library-{}-{stamp}",
            std::process::id()
        ));
        let samples = root.join("audio/sfx/samples");
        std::fs::create_dir_all(&samples).unwrap();
        crate::synth::write_wav(&samples.join("click.wav"), &[0.0, 0.5, -0.5, 0.0], 48_000)
            .unwrap();
        let demo = crate::demo::tracks().remove(0);
        MusicBank::write_tracks(&root, &[(demo.key, &demo.music)]).unwrap();

        let mut state = State::demo();
        state.doc.root = Some(root.clone());
        state.audio_bank.load_project(&root);
        state.music_bank.load_project(&root);
        let entries = state.visible_library_entries();
        for kind in LibraryKind::ALL {
            assert!(
                entries.iter().any(|entry| entry.kind == kind),
                "missing {kind:?}"
            );
        }

        state.view.kind_filter = Some(LibraryKind::Audio);
        let audio = state.visible_library_entries();
        assert_eq!(audio.len(), 1);
        assert_eq!(audio[0].kind, LibraryKind::Audio);
        assert!(audio[0].name.ends_with("click.wav"));

        state.view.kind_filter = None;
        state.view.category_filter = Some(Category::Combat);
        let combat = state.visible_library_entries();
        assert!(combat
            .iter()
            .all(|entry| entry.category == Some(Category::Combat)));
        assert!(!combat.iter().any(|entry| entry.kind == LibraryKind::Audio));

        state.view.category_filter = None;
        state.view.search = "neon".into();
        let neon = state.visible_library_entries();
        assert_eq!(neon.len(), 1);
        assert_eq!(neon[0].kind, LibraryKind::Music);

        let mut logic = SfxLogic {
            state,
            measurer: None,
            audio: None,
            render: None,
        };
        logic.clear_filters();
        assert_eq!(logic.state.view.kind_filter, None);
        assert!(logic.state.view.search.is_empty());
        assert!(logic
            .state
            .visible_library_entries()
            .iter()
            .any(|entry| entry.kind == LibraryKind::Audio));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dropping_audio_on_the_timeline_creates_a_sample_layer() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-drop-audio-{}-{stamp}",
            std::process::id()
        ));
        let samples = root.join("audio/sfx/samples");
        std::fs::create_dir_all(&samples).unwrap();
        let path = samples.join("hit.wav");
        crate::synth::write_wav(&path, &[0.0, 0.4, -0.4, 0.0, 0.2], 48_000).unwrap();

        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.doc.root = Some(root.clone());
        logic.state.audio_bank.load_project(&root);
        assert_eq!(logic.state.audio_bank.assets().len(), 1);
        let before = logic.state.patch().unwrap().layers.len();
        let layout = ui::Layout::new(&logic.state, logic.state.view.viewport);
        let row = layout.layer_rows[0].1;
        let position = Vec2::new(layout.time_x(0.2), row.center().y);
        logic.drop_audio(0, position, &layout);

        let patch = logic.state.patch().unwrap();
        assert_eq!(patch.layers.len(), before + 1);
        let layer = patch
            .layers
            .iter()
            .find(|layer| layer.sample_path == "audio/sfx/samples/hit.wav")
            .expect("sample layer inserted");
        assert_eq!(layer.wave, Wave::Sample);
        assert!(layer.sample_data.is_some());
        assert!((layer.clips[0].start - 0.2).abs() < 0.001);
        assert_eq!(logic.state.doc.undo.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn music_rows_load_a_project_music_file() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-music-row-{}-{stamp}",
            std::process::id()
        ));
        let demo = crate::demo::tracks().remove(1);
        MusicBank::write_tracks(&root, &[(demo.key, &demo.music)]).unwrap();

        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.doc.root = Some(root.clone());
        logic.state.music_bank.load_project(&root);
        assert_eq!(logic.state.music_bank.len(), 1);

        logic.select_library_row(LibraryKey::Music(0));
        assert_eq!(logic.state.view.tab, Tab::Music);
        assert_eq!(
            logic.state.selected_library_kind(),
            Some(LibraryKind::Music)
        );
        assert_eq!(logic.state.selected_music(), Some(0));

        let before = logic.state.doc.project.music.clone();
        logic.activate_library_row(LibraryKey::Music(0));
        assert_eq!(logic.state.doc.project.music, demo.music);
        assert_ne!(logic.state.doc.project.music, before);
        assert_eq!(logic.state.view.tab, Tab::Music);
        assert!(logic.state.ui.status.contains(demo.key));
        logic.state.undo();
        assert_eq!(logic.state.doc.project.music, before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn preset_folders_group_in_the_library_and_can_collapse() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-library-folders-{}-{stamp}",
            std::process::id()
        ));
        let impact = root.join("presets/impact");
        let laser = root.join("presets/laser");
        std::fs::create_dir_all(&impact).unwrap();
        std::fs::create_dir_all(&laser).unwrap();
        let a = Patch::new_layer("boom", Wave::Noise);
        let b = Patch::new_layer("zap", Wave::Saw);
        std::fs::write(impact.join("boom.json"), serde_json::to_string(&a).unwrap()).unwrap();
        std::fs::write(laser.join("zap.json"), serde_json::to_string(&b).unwrap()).unwrap();

        let mut state = State::demo();
        state.doc.root = Some(root.clone());
        state.presets.load_project(&root);
        let rows = state.visible_library_rows();
        assert!(rows.iter().any(|row| matches!(
            row,
            LibraryListRow::Folder { key, .. } if key == "PRE:impact"
        )));
        assert!(rows.iter().any(|row| matches!(
            row,
            LibraryListRow::Entry(entry)
                if entry.key == LibraryKey::Preset(0)
                    && entry.folder.as_deref() == Some("impact")
        )));
        state.select_preset(0);
        state.toggle_library_folder("PRE:impact");
        let folded = state.visible_library_rows();
        assert!(folded.iter().any(|row| matches!(
            row,
            LibraryListRow::Folder { key, .. } if key == "PRE:impact"
        )));
        assert!(!folded.iter().any(|row| matches!(
            row,
            LibraryListRow::Entry(entry) if entry.folder.as_deref() == Some("impact")
        )));
        state.toggle_library_folder("PRE:impact");
        assert!(state.visible_library_rows().iter().any(|row| matches!(
            row,
            LibraryListRow::Entry(entry) if entry.folder.as_deref() == Some("impact")
        )));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clicking_a_music_row_switches_to_the_music_tab() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-music-hit-{}-{stamp}",
            std::process::id()
        ));
        let demo = crate::demo::tracks().remove(0);
        MusicBank::write_tracks(&root, &[(demo.key, &demo.music)]).unwrap();
        let mut state = State::demo();
        state.doc.root = Some(root.clone());
        state.music_bank.load_project(&root);
        let layout = ui::Layout::new(&state, state.view.viewport);
        let (key, rect) = layout
            .library_rows
            .iter()
            .find_map(|(row, rect)| match row {
                LibraryListRow::Entry(entry) if matches!(entry.key, LibraryKey::Music(_)) => {
                    Some((entry.key, *rect))
                }
                _ => None,
            })
            .expect("music row");
        assert_eq!(
            ui::hit(&state, &layout, rect.center()),
            Some(ui::Hit::LibraryRow(key))
        );
        let mut logic = SfxLogic {
            state: std::mem::replace(&mut state, State::empty()),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.select_library_row(key);
        assert_eq!(logic.state.view.tab, Tab::Music);
        assert_eq!(
            logic.state.selected_library_kind(),
            Some(LibraryKind::Music)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn kind_filter_keeps_selection_and_export_target_in_sync() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
        };
        logic.state.select_preset(0);
        let preset_name = logic.state.presets.get(0).unwrap().name.clone();
        assert_eq!(
            logic.state.export_patch().map(|patch| patch.name.as_str()),
            Some(preset_name.as_str())
        );

        logic.pick_kind(Some(LibraryKind::Sound));
        assert_eq!(logic.state.view.library_mode, LibraryMode::Project);
        assert_eq!(
            logic.state.selected_library_kind(),
            Some(LibraryKind::Sound)
        );
        let project_name = logic.state.patch().unwrap().name.clone();
        assert_eq!(
            logic.state.export_patch().map(|patch| patch.name.as_str()),
            Some(project_name.as_str())
        );

        logic.pick_kind(Some(LibraryKind::Preset));
        assert_eq!(logic.state.view.library_mode, LibraryMode::Presets);
        assert_eq!(
            logic.state.selected_library_kind(),
            Some(LibraryKind::Preset)
        );
        assert_eq!(
            logic.state.export_patch().map(|patch| patch.name.as_str()),
            Some(preset_name.as_str())
        );

        logic.pick_kind(Some(LibraryKind::Audio));
        assert_eq!(logic.state.selected_library_kind(), None);
        assert_eq!(
            logic.state.export_patch().map(|patch| patch.name.as_str()),
            Some(project_name.as_str())
        );
    }
    #[test]
    fn opening_an_empty_folder_bootstraps_presets_and_music() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-empty-open-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut state = State::empty();
        let (warnings, created) =
            crate::project_io::open_or_create(&mut state, root.clone()).expect("open or create");
        assert!(created);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(root.join(crate::model::PROJECT_FILE).exists());
        assert_eq!(state.presets.len(), 22);
        assert_eq!(state.music_bank.len(), 3);
        assert_eq!(state.doc.root.as_deref(), Some(root.as_path()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
