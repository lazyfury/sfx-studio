//! App state, input handling, audio bridging and the per-frame entry points.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use igui_app::{AppLogic, EventContext, FrameContext, InitContext};
use igui_core::{Cursor, EventResult, InputEvent, Key, Modifiers, PointerButton, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_ui::{ApproxTextMeasurer, TextMeasurer};

use crate::audio::AudioEngine;
use crate::model::{
    AutomationKind, Category, Clip, ClipField, Layer, LayerField, MusicField, Patch, Project, Wave,
};
use crate::project_io::{display_name, open_folder};
use crate::synth::{self, Envelope};
use crate::{export, model, theme, ui};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Patches,
    Music,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ModalAction {
    NewPatch,
    RenamePatch,
    /// Discard the current dirty project and open this folder.
    ConfirmOpen(PathBuf),
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
            ModalAction::NewPatch | ModalAction::RenamePatch
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
    InspectorScroll {
        start_y: f32,
        start_scroll: f32,
    },
    LibraryScroll {
        start_y: f32,
        start_scroll: usize,
        travel: f32,
        max_start: usize,
    },
}

pub struct State {
    pub project: Project,
    pub root: Option<PathBuf>,
    pub tab: Tab,
    pub selected_patch: usize,
    pub selected_layer: usize,
    pub selected_clip: Option<usize>,
    pub selected_env_point: Option<usize>,
    pub automation_lane: AutomationKind,
    pub patch_scroll: usize,
    pub layer_scroll: usize,
    pub undo: Vec<(String, Project)>,
    pub redo: Vec<(String, Project)>,
    /// Project clone captured at pointer-down; committed on pointer-up only if
    /// something actually changed.
    pub pending_edit: Option<Project>,
    pub status: String,
    pub dirty: bool,
    /// Patch/document revision; invalidates the patch render cache.
    pub version: u64,
    /// Music revision; invalidates the music render cache independently.
    pub music_version: u64,
    /// Pure view revision (selection/scroll/playhead); never renders audio.
    pub view_version: u64,
    pub cache_version: u64,
    pub music_cache_version: u64,
    pub cache_rate: u32,
    pub music_cache_rate: u32,
    pub cache: Vec<f32>,
    pub cache_peak: f32,
    pub cache_envelope: Envelope,
    pub music_cache: Vec<f32>,
    pub music_cache_peak: f32,
    pub music_envelope: Envelope,
    pub loop_play: bool,
    pub playhead: f32,
    pub seeked: bool,
    pub was_playing: bool,
    pub modifiers: Modifiers,
    pub drag: Drag,
    pub modal: Option<Modal>,
    pub hover: Option<ui::Action>,
    pub hover_hit: Option<ui::Hit>,
    pub viewport: Size,
    pub audio_ok: bool,
    pub audio_rate: u32,
    pub cursor: Vec2,
    pub toast: Option<(String, Instant)>,
    /// Time of the last actual preview render; throttles drag-time renders.
    pub last_render: Instant,
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
    /// Defer the next preview render by one frame so the shell/tab paints
    /// before a potentially slow music/patch render blocks the UI thread.
    pub defer_render: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            project: Project::demo("untitled"),
            root: None,
            tab: Tab::Patches,
            selected_patch: 0,
            selected_layer: 0,
            selected_clip: None,
            selected_env_point: None,
            automation_lane: AutomationKind::Gain,
            patch_scroll: 0,
            layer_scroll: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_edit: None,
            status: "OPEN a Godot project folder · demo loaded".into(),
            dirty: false,
            version: 1,
            music_version: 1,
            view_version: 1,
            cache_version: 0,
            music_cache_version: 0,
            cache_rate: 22050,
            music_cache_rate: 11025,
            cache: Vec::new(),
            cache_peak: 0.0,
            cache_envelope: Envelope::default(),
            music_cache: Vec::new(),
            music_cache_peak: 0.0,
            music_envelope: Envelope::default(),
            loop_play: false,
            playhead: 0.0,
            seeked: false,
            was_playing: false,
            modifiers: Modifiers::NONE,
            drag: Drag::None,
            modal: None,
            hover: None,
            hover_hit: None,
            viewport: Size::new(theme::VIEW_W, theme::VIEW_H),
            audio_ok: false,
            audio_rate: 22050,
            cursor: Vec2::ZERO,
            toast: None,
            last_render: Instant::now(),
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
            defer_render: true,
        }
    }

    pub fn patch(&self) -> Option<&Patch> {
        self.project.patches.get(self.selected_patch)
    }

    /// Seconds represented by the timeline. Pinned during clip drags.
    pub fn timeline_span(&self) -> f32 {
        self.timeline_pin.unwrap_or_else(|| {
            self.patch()
                .map(|patch| patch.end().max(0.05))
                .unwrap_or(0.2)
        })
    }

    pub fn patch_mut(&mut self) -> Option<&mut Patch> {
        self.project.patches.get_mut(self.selected_patch)
    }

    pub fn layer(&self) -> Option<&Layer> {
        self.patch().and_then(|p| p.layers.get(self.selected_layer))
    }

    pub fn layer_mut(&mut self) -> Option<&mut Layer> {
        let index = self.selected_layer;
        self.patch_mut().and_then(|p| p.layers.get_mut(index))
    }

    pub fn patch_visible(&self, index: usize) -> bool {
        let Some(patch) = self.project.patches.get(index) else {
            return false;
        };
        let category_ok = self
            .category_filter
            .map(|category| patch.category == category)
            .unwrap_or(true);
        let favorite_ok = !self.favorites_only || patch.favorite;
        let search = self.search.trim().to_lowercase();
        let search_ok = search.is_empty() || patch.name.to_lowercase().contains(&search);
        category_ok && favorite_ok && search_ok
    }

    pub fn visible_patches(&self) -> Vec<usize> {
        self.project
            .patches
            .iter()
            .enumerate()
            .filter(|(index, _)| self.patch_visible(*index))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn mark_dirty(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.view_version = self.view_version.wrapping_add(1);
        self.redo.clear();
        self.dirty = true;
    }

    /// Music edits only invalidate the music cache; patch previews stay warm.
    pub fn mark_music_dirty(&mut self) {
        self.music_version = self.music_version.wrapping_add(1);
        self.view_version = self.view_version.wrapping_add(1);
        self.redo.clear();
        self.dirty = true;
    }

    pub fn bump_view(&mut self) {
        self.view_version = self.view_version.wrapping_add(1);
    }

    pub fn select_patch(&mut self, index: usize) {
        if index < self.project.patches.len() {
            self.selected_patch = index;
            self.inspector_scroll = 0.0;
            self.selected_layer = self
                .selected_layer
                .min(self.project.patches[index].layers.len().saturating_sub(1));
            self.bump_view();
            // Selecting a different patch must re-render the preview even
            // though the document itself is unchanged.
            self.cache_version = 0;
            self.defer_render = true;
            self.playhead = 0.0;
            self.seeked = false;
            self.selected_clip = None;
            self.selected_env_point = None;
            self.ensure_visible();
        }
    }

    pub fn select_layer(&mut self, index: usize) {
        self.inspector_scroll = 0.0;
        if let Some(patch) = self.patch() {
            if index < patch.layers.len() {
                self.selected_layer = index;
                self.selected_clip = None;
                self.selected_env_point = None;
                self.bump_view();
                self.ensure_visible();
            }
        }
    }

    pub fn ensure_visible(&mut self) {
        let patch_capacity = ui::library_capacity(self.viewport);
        let visible = self.visible_patches();
        let position = visible
            .iter()
            .position(|index| *index == self.selected_patch)
            .unwrap_or(0);
        if position < self.patch_scroll {
            self.patch_scroll = position;
        } else if position >= self.patch_scroll + patch_capacity {
            self.patch_scroll = position + 1 - patch_capacity;
        }
        self.patch_scroll = self
            .patch_scroll
            .min(visible.len().saturating_sub(patch_capacity));

        let layers = self.patch().map(|patch| patch.layers.len()).unwrap_or(1);
        let layer_capacity = ui::layer_capacity(self.viewport, layers);
        if self.selected_layer < self.layer_scroll {
            self.layer_scroll = self.selected_layer;
        } else if self.selected_layer >= self.layer_scroll + layer_capacity {
            self.layer_scroll = self.selected_layer + 1 - layer_capacity;
        }
    }

    /// Capture the pre-edit state once per pointer gesture. The snapshot is
    /// only pushed if the project actually changes (see [`Self::commit_edit`]).
    pub fn begin_edit(&mut self) {
        if self.pending_edit.is_none() {
            self.pending_edit = Some(self.project.clone());
        }
    }

    /// Commit a gesture snapshot. No-op clicks / drags that change nothing do
    /// not pollute the undo stack.
    pub fn commit_edit(&mut self) {
        let Some(before) = self.pending_edit.take() else {
            return;
        };
        if before != self.project {
            self.push_snapshot("edit", before);
        }
    }

    fn push_snapshot(&mut self, label: impl Into<String>, project: Project) {
        self.undo.push((label.into(), project));
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn push_undo(&mut self) {
        self.push_undo_label("edit");
    }

    pub fn push_undo_label(&mut self, label: impl Into<String>) {
        self.commit_edit();
        self.push_snapshot(label, self.project.clone());
    }

    pub fn undo(&mut self) -> Option<String> {
        self.commit_edit();
        let (label, previous) = self.undo.pop()?;
        self.redo.push((label.clone(), self.project.clone()));
        self.project = previous;
        self.clamp_selection();
        self.invalidate_document();
        Some(label)
    }

    pub fn redo(&mut self) -> Option<String> {
        self.commit_edit();
        let (label, next) = self.redo.pop()?;
        self.undo.push((label.clone(), self.project.clone()));
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.project = next;
        self.clamp_selection();
        self.invalidate_document();
        Some(label)
    }

    fn invalidate_document(&mut self) {
        // Undo/redo must keep the opposite stack alive, so this deliberately
        // does not go through `mark_dirty` (which clears redo).
        self.version = self.version.wrapping_add(1);
        self.music_version = self.music_version.wrapping_add(1);
        self.view_version = self.view_version.wrapping_add(1);
        self.dirty = true;
        self.cache_version = 0;
        self.music_cache_version = 0;
        self.defer_render = true;
        self.playhead = 0.0;
        self.seeked = false;
        self.ensure_visible();
    }

    fn clamp_selection(&mut self) {
        self.selected_patch = self
            .selected_patch
            .min(self.project.patches.len().saturating_sub(1));
        if let Some(patch) = self.project.patches.get(self.selected_patch) {
            self.selected_layer = self
                .selected_layer
                .min(patch.layers.len().saturating_sub(1));
            if let Some(index) = self.selected_clip {
                let clips = patch
                    .layers
                    .get(self.selected_layer)
                    .map(|l| l.clips.len())
                    .unwrap_or(0);
                self.selected_clip = if index < clips { Some(index) } else { None };
            }
            if let Some(index) = self.selected_env_point {
                let points = patch
                    .layers
                    .get(self.selected_layer)
                    .and_then(|l| self.selected_clip.and_then(|c| l.clips.get(c)))
                    .map(|c| self.automation_lane.points(c).len())
                    .unwrap_or(0);
                self.selected_env_point = if index < points { Some(index) } else { None };
            }
        }
    }

    pub fn preview_rate(&self) -> u32 {
        if self.audio_ok {
            self.audio_rate
        } else {
            match self.tab {
                Tab::Patches => self.patch().map(|p| p.rate).unwrap_or(22050),
                Tab::Music => self.project.music.rate,
            }
        }
    }

    pub fn ensure_cache(&mut self, rate: u32) {
        const ENVELOPE_BINS: usize = 2048;
        let want_patch = self.tab == Tab::Patches;
        let want_music = self.tab == Tab::Music;
        let patch_dirty =
            want_patch && (self.cache_version != self.version || self.cache_rate != rate);
        let music_dirty = want_music
            && (self.music_cache_version != self.music_version || self.music_cache_rate != rate);
        if !patch_dirty && !music_dirty {
            return;
        }
        if patch_dirty {
            self.cache = self
                .patch()
                .map(|patch| synth::render_patch(patch, rate))
                .unwrap_or_default();
            self.cache_peak = synth::peak(&self.cache);
            self.cache_envelope = synth::envelope(&self.cache, ENVELOPE_BINS);
            self.cache_version = self.version;
            self.cache_rate = rate;
        }
        if music_dirty {
            self.music_cache = synth::render_music(&self.project.music, rate);
            self.music_cache_peak = synth::peak(&self.music_cache);
            self.music_envelope = synth::envelope(&self.music_cache, ENVELOPE_BINS);
            self.music_cache_version = self.music_version;
            self.music_cache_rate = rate;
        }
        self.last_render = Instant::now();
    }

    /// Render at most ~12×/s while a slider is being dragged. `play()` always
    /// calls [`Self::ensure_cache`] directly, so pressing play never waits.
    pub fn ensure_cache_soon(&mut self, rate: u32) {
        let already_rendered = self.cache_version != 0 || self.music_cache_version != 0;
        if already_rendered && self.last_render.elapsed() < Duration::from_millis(80) {
            return;
        }
        self.ensure_cache(rate);
    }

    /// True when the active tab's preview cache is stale. Used by
    /// `needs_frame` so the deferred render gets a follow-up frame.
    pub fn preview_pending(&self) -> bool {
        let rate = self.preview_rate();
        match self.tab {
            Tab::Patches => self.cache_version != self.version || self.cache_rate != rate,
            Tab::Music => {
                self.music_cache_version != self.music_version || self.music_cache_rate != rate
            }
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.status = text.clone();
        self.toast = Some((text, Instant::now() + Duration::from_secs(4)));
    }
}

pub struct SfxLogic {
    state: State,
    measurer: Option<Rc<dyn TextMeasurer>>,
    audio: Option<AudioEngine>,
}

impl SfxLogic {
    pub(crate) fn new(mut state: State) -> Self {
        let audio = AudioEngine::new();
        if let Some(engine) = &audio {
            state.audio_ok = true;
            state.audio_rate = engine.sample_rate();
            state.status = format!("audio {} Hz · OPEN a project folder", engine.sample_rate());
        } else {
            state.status = "no audio device · UI only".into();
        }
        Self {
            state,
            measurer: Some(Rc::new(ApproxTextMeasurer)),
            audio,
        }
    }

    fn activate(&mut self, action: ui::Action) {
        use ui::Action as A;
        self.state.menu_open = false;
        self.state.commit_edit();
        let snapshot = matches!(
            action,
            A::DeletePatch
                | A::DuplicatePatch
                | A::AddLayer
                | A::DuplicateLayer
                | A::DeleteLayer
                | A::SetWave(_)
                | A::SetFilter(_)
                | A::SetEnv(_)
                | A::SetToneEnv(_)
                | A::BassWave(_)
                | A::ArpWave(_)
                | A::TogglePatterns
                | A::ChordPreset(_)
                | A::Transpose(_)
                | A::ResetPatterns
                | A::AddClip
                | A::DeleteClip
                | A::Randomize
                | A::Vary
        );
        if snapshot {
            self.push_undo();
        }
        match action {
            A::Open => self.open(),
            A::Save => self.save(),
            A::Bake => self.bake(),
            A::Tres => self.export_tres(),
            A::ExportWav => self.export_wav(),
            A::Randomize => self.randomize(),
            A::Vary => self.vary(),
            A::Favorite => self.toggle_favorite(),
            A::CategoryCycle => self.cycle_category(),
            A::FavoriteFilter => self.toggle_favorite_filter(),
            A::TabPatches => {
                self.state.tab = Tab::Patches;
                self.state.inspector_scroll = 0.0;
                self.state.defer_render = true;
                self.state.bump_view();
                self.state.playhead = 0.0;
            }
            A::TabMusic => {
                self.state.tab = Tab::Music;
                self.state.inspector_scroll = 0.0;
                self.state.defer_render = true;
                self.state.bump_view();
                self.state.playhead = 0.0;
            }
            A::NewPatch => {
                let name = self.state.project.next_patch_name();
                self.state.modal = Some(Modal {
                    title: "NEW SOUND".into(),
                    text: name,
                    action: ModalAction::NewPatch,
                });
            }
            A::DeletePatch => {
                if self.state.project.patches.len() > 1 {
                    self.state.project.patches.remove(self.state.selected_patch);
                    self.state.selected_patch = self
                        .state
                        .selected_patch
                        .min(self.state.project.patches.len() - 1);
                    self.state.selected_layer = 0;
                    self.state.mark_dirty();
                    self.state.bump_view();
                }
            }
            A::DuplicatePatch => {
                if let Some(patch) = self.state.patch().cloned() {
                    let mut copy = patch;
                    copy.name = format!("{}_copy", copy.name);
                    self.state
                        .project
                        .patches
                        .insert(self.state.selected_patch + 1, copy);
                    self.state.selected_patch += 1;
                    self.state.selected_layer = 0;
                    self.state.mark_dirty();
                    self.state.bump_view();
                }
            }
            A::RenamePatch => {
                if let Some(patch) = self.state.patch() {
                    self.state.modal = Some(Modal {
                        title: "RENAME SOUND".into(),
                        text: patch.name.clone(),
                        action: ModalAction::RenamePatch,
                    });
                }
            }
            A::Play => self.play(),
            A::Stop => self.stop(),
            A::Loop => self.state.loop_play = !self.state.loop_play,
            A::AddLayer => {
                if let Some(patch) = self.state.patch_mut() {
                    patch.layers.push(Layer::default());
                    self.state.selected_layer = patch.layers.len() - 1;
                    self.state.mark_dirty();
                }
            }
            A::DuplicateLayer => {
                let selected = self.state.selected_layer;
                if let Some(patch) = self.state.patch_mut() {
                    if let Some(layer) = patch.layers.get(selected).cloned() {
                        patch.layers.insert(selected + 1, layer);
                        self.state.selected_layer = selected + 1;
                        self.state.mark_dirty();
                    }
                }
            }
            A::DeleteLayer => {
                let selected = self.state.selected_layer;
                if let Some(patch) = self.state.patch_mut() {
                    if patch.layers.len() > 1 {
                        patch.layers.remove(selected);
                        self.state.selected_layer =
                            selected.min(patch.layers.len().saturating_sub(1));
                        self.state.mark_dirty();
                    }
                }
            }
            A::SetWave(wave) => {
                if let Some(layer) = self.state.layer_mut() {
                    layer.wave = wave;
                    layer.duration = default_duration(wave);
                    self.state.mark_dirty();
                }
            }
            A::SetFilter(mode) => {
                if let Some(layer) = self.state.layer_mut() {
                    layer.filter = mode;
                    self.state.mark_dirty();
                }
            }
            A::SetEnv(shape) => {
                if let Some(layer) = self.state.layer_mut() {
                    layer.env_shape = shape;
                    self.state.mark_dirty();
                }
            }
            A::SetToneEnv(tone_env) => {
                if let Some(layer) = self.state.layer_mut() {
                    layer.tone_env = tone_env;
                    self.state.mark_dirty();
                }
            }
            A::BassWave(wave) => {
                self.state.project.music.bass_wave = wave;
                self.state.mark_music_dirty();
            }
            A::ArpWave(wave) => {
                self.state.project.music.arp_wave = wave;
                self.state.mark_music_dirty();
            }
            A::TogglePatterns => {
                self.state.project.music.use_patterns = !self.state.project.music.use_patterns;
                self.state.mark_music_dirty();
            }
            A::ChordPreset(index) => {
                apply_chord_preset(&mut self.state.project.music, index);
                self.state.mark_music_dirty();
            }
            A::Transpose(delta) => {
                self.state.project.music.transpose(delta);
                self.state.mark_music_dirty();
            }
            A::ResetPatterns => {
                self.state.project.music.reset_patterns();
                self.state.mark_music_dirty();
            }
            A::AddClip => {
                let playhead = self.state.playhead;
                if let Some(layer) = self.state.layer_mut() {
                    layer.ensure_clips();
                    let duration = layer.duration.clamp(0.02, 0.5);
                    layer.clips.push(Clip {
                        start: playhead.max(0.0),
                        duration,
                        gain: 1.0,
                        pitch: 0.0,
                        gain_points: Vec::new(),
                        cutoff_points: Vec::new(),
                    });
                    self.state.selected_clip = Some(layer.clips.len() - 1);
                    self.state.selected_env_point = None;
                    self.state.mark_dirty();
                }
            }
            A::DeleteClip => {
                if let Some(index) = self.state.selected_clip {
                    if let Some(layer) = self.state.layer_mut() {
                        if index < layer.clips.len() {
                            layer.clips.remove(index);
                            self.state.selected_clip = None;
                            self.state.selected_env_point = None;
                            self.state.mark_dirty();
                        }
                    }
                }
            }
            A::AutomationLane(kind) => {
                self.state.automation_lane = kind;
                self.state.selected_env_point = None;
                self.state.bump_view();
            }
            A::Undo => {
                if let Some(label) = self.state.undo() {
                    self.state.toast(format!("undo {label}"));
                }
            }
            A::Redo => {
                if let Some(label) = self.state.redo() {
                    self.state.toast(format!("redo {label}"));
                }
            }
        }
        self.state.ensure_visible();
    }

    fn push_undo(&mut self) {
        self.state.push_undo();
    }

    fn open(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Open a Godot project folder")
            .pick_folder()
        else {
            return;
        };
        if self.state.dirty {
            self.state.modal = Some(Modal {
                title: "DISCARD CHANGES?".into(),
                text: folder.display().to_string(),
                action: ModalAction::ConfirmOpen(folder),
            });
            self.state.bump_view();
            return;
        }
        self.open_folder_now(folder);
    }

    fn open_folder_now(&mut self, folder: PathBuf) {
        self.state.timeline_zoom = 1.0;
        self.state.timeline_offset = 0.0;
        match open_folder(&mut self.state, folder) {
            Ok(warnings) => {
                let message = if warnings.is_empty() {
                    "project loaded".to_string()
                } else {
                    format!("project loaded · {}", warnings.join("; "))
                };
                self.state.toast(message);
            }
            Err(error) => self.state.toast(format!("open failed: {error}")),
        }
    }

    fn save(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        self.state.project.migrate();
        let warnings = self.state.project.validate();
        // Validation may clamp values; make the rendered preview match what
        // was just persisted.
        self.state.cache_version = 0;
        self.state.music_cache_version = 0;
        match model::save_project(&root, &self.state.project) {
            Ok(()) => {
                self.state.dirty = false;
                let message = if warnings.is_empty() {
                    format!("saved {}", root.join(model::PROJECT_FILE).display())
                } else {
                    format!(
                        "saved {} · {}",
                        root.join(model::PROJECT_FILE).display(),
                        warnings.join("; ")
                    )
                };
                self.state.toast(message);
            }
            Err(error) => self.state.toast(format!("save failed: {error}")),
        }
    }

    fn bake(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        match export::bake(&self.state.project, &root) {
            Ok(message) => self.state.toast(message),
            Err(error) => self.state.toast(format!("bake failed: {error}")),
        }
    }

    fn export_tres(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        match export::export_tres(&self.state.project, &root) {
            Ok(message) => self.state.toast(message),
            Err(error) => self.state.toast(format!("export failed: {error}")),
        }
    }

    fn export_wav(&mut self) {
        let Some(patch) = self.state.patch() else {
            return;
        };
        let default_name = format!("{}.wav", patch.safe_name());
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export WAV")
            .set_file_name(&default_name)
            .add_filter("WAV audio", &["wav"])
            .save_file()
        else {
            return;
        };
        let rate = 48_000;
        let samples = synth::render_patch(patch, rate);
        match synth::write_wav(&path, &samples, rate) {
            Ok(()) => self
                .state
                .toast(format!("exported {} (48 kHz)", path.display())),
            Err(error) => self.state.toast(format!("export failed: {error}")),
        }
    }

    fn randomize(&mut self) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x5EED)
            ^ self.state.version.wrapping_mul(0x9E3779B97F4A7C15);
        let mut rng = synth::Rng::new(seed);
        if let Some(patch) = self.state.patch_mut() {
            for layer in &mut patch.layers {
                randomize_layer(layer, &mut rng, 0.22);
            }
        }
        self.state.mark_dirty();
        self.state.bump_view();
        self.state.toast("randomized current sound");
    }

    fn vary(&mut self) {
        let Some(base) = self.state.patch().cloned() else {
            return;
        };
        let start = self.state.selected_patch;
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0xA11CE);
        for (index, suffix) in [" A", " B", " C"].iter().enumerate() {
            let mut copy = base.clone();
            copy.name = format!("{}{suffix}", base.name);
            copy.favorite = base.favorite;
            let mut rng = synth::Rng::new(seed.wrapping_add(index as u64 * 0x9E3779B9));
            for layer in &mut copy.layers {
                randomize_layer(layer, &mut rng, 0.30);
            }
            self.state.project.patches.insert(start + 1 + index, copy);
        }
        self.state.selected_patch = start + 1;
        self.state.selected_layer = 0;
        self.state.selected_clip = None;
        self.state.mark_dirty();
        self.state.bump_view();
        self.state.toast("created 3 variations (A/B/C)");
    }

    fn toggle_favorite(&mut self) {
        if let Some(patch) = self.state.patch_mut() {
            patch.favorite = !patch.favorite;
            let favorite = patch.favorite;
            self.state.mark_dirty();
            if self.state.favorites_only {
                self.refresh_filter();
            } else {
                self.state.bump_view();
            }
            self.state.toast(if favorite {
                "added to favorites"
            } else {
                "removed from favorites"
            });
        }
    }

    fn toggle_patch_favorite(&mut self, index: usize) {
        let Some(patch) = self.state.project.patches.get(index) else {
            return;
        };
        let favorite = !patch.favorite;
        self.state.begin_edit();
        self.state.project.patches[index].favorite = favorite;
        self.state.mark_dirty();
        if self.state.favorites_only {
            self.refresh_filter();
        } else {
            self.state.bump_view();
        }
        self.state.toast(if favorite {
            "added to favorites"
        } else {
            "removed from favorites"
        });
        self.state.commit_edit();
    }

    fn cycle_category(&mut self) {
        let next = match self.state.category_filter {
            None => Some(Category::ALL[0]),
            Some(category) => {
                let next = category.next();
                if next == Category::ALL[0] {
                    None
                } else {
                    Some(next)
                }
            }
        };
        self.state.category_filter = next;
        self.refresh_filter();
        self.state.toast(match next {
            Some(category) => format!("filter: {}", category.label()),
            None => "filter: all sounds".into(),
        });
    }

    fn pick_category(&mut self, category: Option<Category>) {
        self.state.category_filter = category;
        self.state.category_menu_open = false;
        self.refresh_filter();
    }

    fn toggle_favorite_filter(&mut self) {
        self.state.favorites_only = !self.state.favorites_only;
        self.refresh_filter();
        self.state.toast(if self.state.favorites_only {
            "showing favorites"
        } else {
            "showing all sounds"
        });
    }

    fn refresh_filter(&mut self) {
        self.state.patch_scroll = 0;
        self.keep_selection_visible();
        self.state.bump_view();
    }

    fn keep_selection_visible(&mut self) {
        if self.state.patch_visible(self.state.selected_patch) {
            return;
        }
        if let Some(index) = self.state.visible_patches().first().copied() {
            self.state.select_patch(index);
        }
    }

    fn trigger_pad(&mut self, index: usize) {
        if index >= self.state.project.patches.len() {
            return;
        }
        if !self.state.patch_visible(index) {
            self.state.category_filter = None;
            self.state.favorites_only = false;
            self.state.patch_scroll = 0;
        }
        self.state.tab = Tab::Patches;
        self.state.select_patch(index);
        self.play();
    }

    fn ensure_root(&mut self) -> Option<PathBuf> {
        if let Some(root) = &self.state.root {
            return Some(root.clone());
        }
        let folder = rfd::FileDialog::new()
            .set_title("Choose the Godot project folder for export")
            .pick_folder()?;
        if let Some(name) = folder.file_name().and_then(|name| name.to_str()) {
            if self.state.project.name == "untitled" {
                self.state.project.name = name.to_string();
            }
        }
        self.state.root = Some(folder.clone());
        Some(folder)
    }

    fn play(&mut self) {
        let rate = self.state.preview_rate();
        self.state.ensure_cache(rate);
        self.state.defer_render = false;
        let Some(audio) = &self.audio else {
            self.state.toast("no audio device");
            return;
        };
        match self.state.tab {
            Tab::Patches => {
                let samples = self.state.cache.clone();
                let offset = if self.state.seeked {
                    (self.state.playhead * rate as f32) as usize
                } else {
                    0
                };
                if samples.is_empty() || offset >= samples.len() {
                    self.state.toast("nothing to play");
                    return;
                }
                audio.play(samples, offset, self.state.loop_play);
                if !self.state.seeked {
                    self.state.playhead = 0.0;
                }
            }
            Tab::Music => {
                let mut samples = self.state.music_cache.clone();
                let gain = 10f32.powf(self.state.project.music.volume_db / 20.0);
                for sample in &mut samples {
                    *sample *= gain;
                }
                audio.play(samples, 0, true);
                self.state.playhead = 0.0;
            }
        }
        self.state.seeked = false;
        self.state.was_playing = true;
    }

    /// Seek while paused. Stopping the stream avoids the UI and the audio
    /// thread disagreeing about the current position.
    fn seek(&mut self, time: f32) {
        if let Some(audio) = &self.audio {
            if audio.is_playing() {
                audio.stop();
            }
        }
        self.state.was_playing = false;
        self.state.playhead = time.max(0.0);
        self.state.seeked = true;
        self.state.bump_view();
    }

    fn stop(&mut self) {
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        self.state.playhead = 0.0;
        self.state.seeked = false;
        self.state.was_playing = false;
    }

    fn confirm_modal(&mut self) {
        let Some(modal) = self.state.modal.take() else {
            return;
        };
        let name = display_name(&modal.text);
        match modal.action {
            ModalAction::ConfirmOpen(folder) => self.open_folder_now(folder),
            ModalAction::NewPatch => {
                self.push_undo();
                self.state
                    .project
                    .patches
                    .push(Patch::new_layer(name.clone(), Wave::Sine));
                self.state.selected_patch = self.state.project.patches.len() - 1;
                self.state.selected_layer = 0;
                self.state.mark_dirty();
                self.state.bump_view();
            }
            ModalAction::RenamePatch => {
                self.push_undo();
                if let Some(patch) = self.state.patch_mut() {
                    patch.name = name;
                    self.state.mark_dirty();
                }
            }
        }
    }

    fn handle_modal(&mut self, event: &InputEvent) -> bool {
        let Some(input) = self.state.modal.as_ref().map(Modal::is_input) else {
            return false;
        };
        match event {
            InputEvent::TextInput { text } => {
                if input {
                    if let Some(modal) = self.state.modal.as_mut() {
                        for ch in text.chars() {
                            if !ch.is_control() && modal.text.len() < 48 {
                                modal.text.push(ch);
                            }
                        }
                    }
                }
                true
            }
            InputEvent::KeyDown { key } => match key {
                Key::Backspace if input => {
                    if let Some(modal) = self.state.modal.as_mut() {
                        modal.text.pop();
                    }
                    true
                }
                Key::Enter => {
                    self.confirm_modal();
                    true
                }
                Key::Escape => {
                    self.state.modal = None;
                    true
                }
                _ => true,
            },
            _ => true,
        }
    }
}

impl AppLogic for SfxLogic {
    fn init(&mut self, ctx: &InitContext<'_>) {
        if let Some(measurer) = ctx.service::<Rc<dyn TextMeasurer>>() {
            self.measurer = Some(measurer.clone());
        }
    }

    fn event(&mut self, _ctx: &EventContext<'_>, event: &InputEvent) -> EventResult {
        if let InputEvent::ModifiersChanged(modifiers) = event {
            self.state.modifiers = *modifiers;
            return EventResult::Ignored;
        }
        if self.handle_modal(event) {
            return EventResult::Handled;
        }

        if self.state.search_focus {
            match event {
                InputEvent::TextInput { text } => {
                    for ch in text.chars() {
                        if !ch.is_control() && self.state.search.len() < 32 {
                            self.state.search.push(ch);
                        }
                    }
                    self.refresh_filter();
                    return EventResult::Handled;
                }
                InputEvent::KeyDown { key } => match key {
                    Key::Backspace => {
                        self.state.search.pop();
                        self.refresh_filter();
                        return EventResult::Handled;
                    }
                    Key::Enter => {
                        self.state.search_focus = false;
                        return EventResult::Handled;
                    }
                    Key::Escape => {
                        self.state.search_focus = false;
                        self.state.search.clear();
                        self.refresh_filter();
                        return EventResult::Handled;
                    }
                    _ if !(self.state.modifiers.meta || self.state.modifiers.ctrl) => {
                        return EventResult::Handled;
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        let layout = ui::Layout::new(&self.state, self.state.viewport);
        match event {
            InputEvent::KeyDown { key } => {
                let cmd = self.state.modifiers.meta || self.state.modifiers.ctrl;
                match key {
                    Key::Space => {
                        if self.state.was_playing
                            || self.audio.as_ref().is_some_and(|a| a.is_playing())
                        {
                            self.stop();
                        } else {
                            self.play();
                        }
                        return EventResult::Handled;
                    }
                    Key::Tab => {
                        self.state.tab = match self.state.tab {
                            Tab::Patches => Tab::Music,
                            Tab::Music => Tab::Patches,
                        };
                        self.state.bump_view();
                        return EventResult::Handled;
                    }
                    Key::Escape => {
                        if self.state.category_menu_open {
                            self.state.category_menu_open = false;
                            self.state.bump_view();
                        } else if self.state.menu_open {
                            self.state.menu_open = false;
                            self.state.bump_view();
                        } else {
                            self.stop();
                        }
                        return EventResult::Handled;
                    }
                    Key::Delete => {
                        if self.state.tab == Tab::Patches {
                            if let (Some(index), Some(clip_index)) =
                                (self.state.selected_env_point, self.state.selected_clip)
                            {
                                self.push_undo();
                                let layer_index = self.state.selected_layer;
                                let kind = self.state.automation_lane;
                                if let Some(clip) = self
                                    .state
                                    .patch_mut()
                                    .and_then(|p| p.layers.get_mut(layer_index))
                                    .and_then(|l| l.clips.get_mut(clip_index))
                                {
                                    let points = kind.points_mut(clip);
                                    if index < points.len() {
                                        points.remove(index);
                                        self.state.selected_env_point = None;
                                        self.state.mark_dirty();
                                    }
                                }
                            } else {
                                self.activate(ui::Action::DeleteLayer);
                            }
                        }
                        return EventResult::Handled;
                    }
                    Key::ArrowUp => {
                        let visible = self.state.visible_patches();
                        let position = visible
                            .iter()
                            .position(|index| *index == self.state.selected_patch)
                            .unwrap_or(0);
                        if let Some(index) = visible.get(position.saturating_sub(1)) {
                            self.state.select_patch(*index);
                        }
                        return EventResult::Handled;
                    }
                    Key::ArrowDown => {
                        let visible = self.state.visible_patches();
                        let position = visible
                            .iter()
                            .position(|index| *index == self.state.selected_patch)
                            .unwrap_or(0);
                        if let Some(index) = visible.get(position + 1) {
                            self.state.select_patch(*index);
                        }
                        return EventResult::Handled;
                    }
                    Key::ArrowLeft => {
                        let index = self.state.selected_layer.saturating_sub(1);
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Key::ArrowRight => {
                        let index = self.state.selected_layer + 1;
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Key::Character(ch) if ('1'..='8').contains(ch) => {
                        let index = *ch as usize - '1' as usize;
                        self.trigger_pad(index);
                        return EventResult::Handled;
                    }
                    Key::Character('/') if !cmd => {
                        self.state.search_focus = true;
                        return EventResult::Handled;
                    }
                    Key::Character('r') if !cmd => {
                        self.activate(ui::Action::Randomize);
                        return EventResult::Handled;
                    }
                    Key::Character('f') if !cmd => {
                        self.activate(ui::Action::Favorite);
                        return EventResult::Handled;
                    }
                    Key::Character('v') if !cmd => {
                        self.activate(ui::Action::Vary);
                        return EventResult::Handled;
                    }
                    Key::Character('c') if !cmd => {
                        self.activate(ui::Action::CategoryCycle);
                        return EventResult::Handled;
                    }
                    Key::Character('l') if !cmd => {
                        self.state.loop_play = !self.state.loop_play;
                        return EventResult::Handled;
                    }
                    Key::Character('z') if cmd => {
                        if self.state.modifiers.shift {
                            self.state.redo();
                        } else {
                            self.state.undo();
                        }
                        return EventResult::Handled;
                    }
                    Key::Character('y') if cmd => {
                        self.state.redo();
                        return EventResult::Handled;
                    }
                    Key::Character('s') if cmd => {
                        self.save();
                        return EventResult::Handled;
                    }
                    Key::Character('o') if cmd => {
                        self.open();
                        return EventResult::Handled;
                    }
                    Key::Character('b') if cmd => {
                        self.bake();
                        return EventResult::Handled;
                    }
                    Key::Character('e') if cmd => {
                        self.export_wav();
                        return EventResult::Handled;
                    }
                    _ => {}
                }
            }
            InputEvent::PointerDown { position, button } if *button == PointerButton::Left => {
                self.state.cursor = *position;
                let pointer_hit = ui::hit(&self.state, &layout, *position);
                self.state.hover_hit = pointer_hit;
                if pointer_hit != Some(ui::Hit::Search) {
                    self.state.search_focus = false;
                }
                if self.state.menu_open {
                    match pointer_hit {
                        Some(ui::Hit::MenuToggle) => {
                            self.state.menu_open = false;
                        }
                        Some(ui::Hit::MenuItem(action)) => {
                            self.state.menu_open = false;
                            self.activate(action);
                        }
                        Some(ui::Hit::MenuPanel) => {}
                        _ => {
                            // Click outside closes the menu without also
                            // activating whatever is underneath it.
                            self.state.menu_open = false;
                        }
                    }
                    self.state.bump_view();
                    return EventResult::Handled;
                }
                if pointer_hit == Some(ui::Hit::MenuToggle) {
                    self.state.menu_open = true;
                    self.state.category_menu_open = false;
                    self.state.bump_view();
                    return EventResult::Handled;
                }
                if self.state.category_menu_open {
                    match pointer_hit {
                        Some(ui::Hit::CategoryPick(category)) => {
                            self.pick_category(category);
                            return EventResult::Handled;
                        }
                        Some(ui::Hit::CategoryMenu | ui::Hit::CategoryFilter) => {
                            self.state.category_menu_open = false;
                            self.state.bump_view();
                            return EventResult::Handled;
                        }
                        _ => {
                            self.state.category_menu_open = false;
                            self.state.bump_view();
                        }
                    }
                }
                match pointer_hit {
                    Some(ui::Hit::Search) => {
                        self.state.search_focus = true;
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Action(action)) => {
                        self.state.hover = Some(action);
                        self.activate(action);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::CategoryFilter) => {
                        self.state.category_menu_open = !self.state.category_menu_open;
                        self.state.menu_open = false;
                        self.state.bump_view();
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::CategoryPick(category)) => {
                        self.pick_category(category);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::CategoryMenu) => {
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::FavoriteFilter) => {
                        self.activate(ui::Action::FavoriteFilter);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LayerField(field)) => {
                        self.state.begin_edit();
                        self.set_param_from_x(*position, ui::Hit::LayerField(field), &layout);
                        self.state.drag = Drag::Layer {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::MusicField(field)) => {
                        self.state.begin_edit();
                        self.set_param_from_x(*position, ui::Hit::MusicField(field), &layout);
                        self.state.drag = Drag::Music {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::ClipField(field)) => {
                        if let Some(index) = self.state.selected_clip {
                            self.state.begin_edit();
                            self.set_param_from_x(*position, ui::Hit::ClipField(field), &layout);
                            self.state.drag = Drag::ClipField {
                                layer: self.state.selected_layer,
                                clip: index,
                                field,
                                last_x: position.x,
                            };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::PatchFavorite(index)) => {
                        self.toggle_patch_favorite(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::PatchRow(index)) => {
                        self.state.select_patch(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LayerRow(index)) => {
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Timeline { layer, clip, mode }) => {
                        self.state.begin_edit();
                        if self.state.timeline_pin.is_none() {
                            self.state.timeline_pin = Some(self.state.timeline_span());
                        }
                        self.state.select_layer(layer);
                        let mut index = clip;
                        if index.is_none() {
                            if let Some(layer) = self.state.layer_mut() {
                                layer.ensure_clips();
                                index = Some(0);
                            }
                        }
                        self.state.selected_clip = index;
                        self.state.selected_env_point = None;
                        if let Some(index) = index {
                            let (start, duration) = self
                                .state
                                .layer()
                                .and_then(|l| l.clips.get(index))
                                .map(|clip| (clip.start, clip.duration))
                                .unwrap_or((0.0, 0.1));
                            self.state.drag = Drag::Clip {
                                layer,
                                clip: index,
                                mode,
                                start_x: position.x,
                                start,
                                duration,
                            };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Ruler(time)) => {
                        self.state.drag = Drag::Seek { waveform: false };
                        self.seek(time);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Waveform(time)) => {
                        self.state.drag = Drag::Seek { waveform: true };
                        self.seek(time);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LibraryScrollbar) => {
                        self.state.drag = Drag::LibraryScroll {
                            start_y: position.y,
                            start_scroll: self.state.patch_scroll,
                            travel: (layout.library_scrollbar.size.height
                                - layout.library_thumb.size.height)
                                .max(1.0),
                            max_start: layout.library_max_start,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::InspectorScrollbar) => {
                        self.state.drag = Drag::InspectorScroll {
                            start_y: position.y,
                            start_scroll: self.state.inspector_scroll,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Pattern(row, step)) => {
                        self.push_undo();
                        toggle_pattern(&mut self.state.project.music, row, step);
                        self.state.mark_dirty();
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::AutomationPoint(index)) => {
                        if let Some(clip) = self.state.selected_clip {
                            self.state.begin_edit();
                            self.state.selected_env_point = Some(index);
                            self.state.drag = Drag::AutomationPoint {
                                layer: self.state.selected_layer,
                                clip,
                                index,
                            };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::MenuToggle)
                    | Some(ui::Hit::MenuItem(_))
                    | Some(ui::Hit::MenuPanel) => {
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::AutomationLane { time, value }) => {
                        if let Some(clip_index) = self.state.selected_clip {
                            self.state.begin_edit();
                            let layer_index = self.state.selected_layer;
                            let kind = self.state.automation_lane;
                            let mut added = None;
                            if let Some(clip) = self
                                .state
                                .patch_mut()
                                .and_then(|p| p.layers.get_mut(layer_index))
                                .and_then(|l| l.clips.get_mut(clip_index))
                            {
                                added = Some(clip.add_point(kind, time, value));
                                self.state.mark_dirty();
                            }
                            if let Some(index) = added {
                                self.state.selected_env_point = Some(index);
                                self.state.drag = Drag::AutomationPoint {
                                    layer: layer_index,
                                    clip: clip_index,
                                    index,
                                };
                            }
                        }
                        return EventResult::Handled;
                    }
                    None => {}
                }
            }
            InputEvent::PointerMove { position } => {
                self.state.cursor = *position;
                let pointer_hit = ui::hit(&self.state, &layout, *position);
                self.state.hover_hit = pointer_hit;
                self.state.hover = match pointer_hit {
                    Some(ui::Hit::Action(action)) => Some(action),
                    _ => None,
                };
                self.handle_drag(*position, &layout);
                return EventResult::Handled;
            }
            InputEvent::PointerLeave => {
                self.state.commit_edit();
                self.state.hover = None;
                self.state.hover_hit = None;
                return EventResult::Handled;
            }
            InputEvent::DoubleClick { position } => {
                let hit = ui::hit(&self.state, &layout, *position);
                match hit {
                    Some(ui::Hit::PatchRow(index)) => {
                        self.state.select_patch(index);
                        if let Some(patch) = self.state.patch() {
                            self.state.modal = Some(Modal {
                                title: "RENAME SOUND".into(),
                                text: patch.name.clone(),
                                action: ModalAction::RenamePatch,
                            });
                        }
                    }
                    Some(ui::Hit::LayerField(field)) => {
                        self.push_undo();
                        let default = Layer::default();
                        if let Some(layer) = self.state.layer_mut() {
                            field.set(layer, field.get(&default));
                            self.state.mark_dirty();
                        }
                    }
                    Some(ui::Hit::ClipField(field)) => {
                        if let Some(index) = self.state.selected_clip {
                            self.push_undo();
                            let layer_index = self.state.selected_layer;
                            let max_duration = self
                                .state
                                .patch()
                                .and_then(|patch| patch.layers.get(layer_index))
                                .map(|layer| layer.duration)
                                .unwrap_or(3.0);
                            let default = Clip::default();
                            if let Some(clip) = self
                                .state
                                .patch_mut()
                                .and_then(|patch| patch.layers.get_mut(layer_index))
                                .and_then(|layer| layer.clips.get_mut(index))
                            {
                                field.set(clip, field.get(&default));
                                clip.duration = clip.duration.clamp(0.005, max_duration);
                                self.state.mark_dirty();
                            }
                        }
                    }
                    Some(ui::Hit::MusicField(field)) => {
                        self.push_undo();
                        let default = model::Music::default();
                        field.set(&mut self.state.project.music, field.get(&default));
                        self.state.mark_music_dirty();
                    }
                    _ => {}
                }
                return EventResult::Handled;
            }
            InputEvent::PointerUp { position, .. } => {
                self.state.commit_edit();
                self.state.drag = Drag::None;
                self.state.timeline_pin = None;
                self.state.hover_hit = ui::hit(&self.state, &layout, *position);
                return EventResult::Handled;
            }
            InputEvent::Wheel {
                position, delta, ..
            } => {
                let rows = (delta.y.abs() / 24.0).max(1.0) as i32;
                let step = if delta.y > 0.0 { -rows } else { rows };
                if layout.left.contains(*position) {
                    let capacity = ui::library_capacity(self.state.viewport);
                    let max_start = self.state.visible_patches().len().saturating_sub(capacity);
                    self.state.patch_scroll =
                        (self.state.patch_scroll as i32 + step).clamp(0, max_start as i32) as usize;
                    return EventResult::Handled;
                }
                if layout.right.contains(*position) {
                    let max_scroll =
                        (layout.inspector_content_h - layout.inspector.size.height).max(0.0);
                    self.state.inspector_scroll =
                        (self.state.inspector_scroll - step as f32 * 22.0).clamp(0.0, max_scroll);
                    return EventResult::Handled;
                }
                if layout.timeline.contains(*position) {
                    if self.state.modifiers.meta || self.state.modifiers.ctrl {
                        // Zoom around the cursor: keep the time under the
                        // pointer anchored while the scale changes.
                        let plot_w =
                            (layout.timeline.size.width - ui::TIMELINE_LABEL - 8.0).max(1.0);
                        let plot_left = layout.timeline.left() + ui::TIMELINE_LABEL;
                        let ratio = ((position.x - plot_left) / plot_w).clamp(0.0, 1.0);
                        let anchor = layout.x_time(position.x);
                        let factor = if delta.y > 0.0 { 1.25 } else { 0.8 };
                        self.state.timeline_zoom =
                            (self.state.timeline_zoom * factor).clamp(1.0, 32.0);
                        let view_total = layout.timeline_total + 0.08;
                        let scale = ((plot_w / view_total) * self.state.timeline_zoom).max(8.0);
                        let visible = plot_w / scale;
                        self.state.timeline_offset =
                            (anchor - ratio * visible).clamp(0.0, (view_total - visible).max(0.0));
                        self.state.bump_view();
                        return EventResult::Handled;
                    }
                    if self.state.modifiers.shift {
                        let view_total = layout.timeline_total + 0.08;
                        let step =
                            layout.timeline_visible * 0.1 * if delta.y > 0.0 { -1.0 } else { 1.0 };
                        self.state.timeline_offset = (layout.timeline_offset + step)
                            .clamp(0.0, (view_total - layout.timeline_visible).max(0.0));
                        return EventResult::Handled;
                    }
                    if let Some(patch) = self.state.patch() {
                        let capacity = ui::layer_capacity(self.state.viewport, patch.layers.len());
                        let max_start = patch.layers.len().saturating_sub(capacity);
                        self.state.layer_scroll = (self.state.layer_scroll as i32 + rows)
                            .clamp(0, max_start as i32)
                            as usize;
                    }
                    return EventResult::Handled;
                }
                return EventResult::Handled;
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn update(&mut self, ctx: &FrameContext<'_>) {
        self.state.viewport = ctx.viewport().logical_size();
        if matches!(self.state.drag, Drag::None) {
            self.state.timeline_pin = None;
        }
        if let Some((_, deadline)) = &self.state.toast {
            if Instant::now() > *deadline {
                self.state.toast = None;
            }
        }
        let rate = self.state.preview_rate();
        if self.state.defer_render {
            self.state.defer_render = false;
        } else {
            self.state.ensure_cache_soon(rate);
        }
        if let Some(audio) = &self.audio {
            let playing = audio.is_playing();
            if playing {
                let mut time = audio.position();
                if self.state.tab == Tab::Music {
                    let length = self.state.project.music.length_seconds();
                    if length > 0.0 {
                        time %= length;
                    }
                }
                self.state.playhead = time;
            } else if self.state.was_playing {
                self.state.playhead = 0.0;
                self.state.seeked = false;
            }
            self.state.was_playing = playing;
        }
    }

    fn paint(&mut self, _ctx: &FrameContext<'_>, paint: &mut PaintContext) {
        let Some(measurer) = &self.measurer else {
            return;
        };
        ui::paint(&self.state, measurer.as_ref(), paint);
    }

    fn needs_frame(&self) -> bool {
        self.state.defer_render
            || self.state.preview_pending()
            || self
                .audio
                .as_ref()
                .map(|audio| audio.is_playing())
                .unwrap_or(false)
    }

    fn cursor(&self) -> Option<Cursor> {
        match &self.state.drag {
            Drag::Clip {
                mode: TimelineDrag::Resize | TimelineDrag::ResizeLeft,
                ..
            } => return Some(Cursor::ColResize),
            Drag::Clip { .. } => return Some(Cursor::Grabbing),
            Drag::Layer { .. } | Drag::Music { .. } | Drag::ClipField { .. } => {
                return Some(Cursor::ColResize)
            }
            Drag::AutomationPoint { .. } => return Some(Cursor::Grabbing),
            Drag::Seek { .. } => return Some(Cursor::Grab),
            Drag::InspectorScroll { .. } | Drag::LibraryScroll { .. } => {
                return Some(Cursor::RowResize)
            }
            Drag::None => {}
        }
        match self.state.hover_hit {
            Some(ui::Hit::Timeline {
                mode: TimelineDrag::Resize | TimelineDrag::ResizeLeft,
                ..
            }) => Some(Cursor::ColResize),
            Some(ui::Hit::Timeline { .. }) => Some(Cursor::Grab),
            Some(ui::Hit::Ruler(_) | ui::Hit::Waveform(_)) => Some(Cursor::Pointer),
            Some(ui::Hit::Search) => Some(Cursor::Text),
            Some(ui::Hit::InspectorScrollbar | ui::Hit::LibraryScrollbar) => {
                Some(Cursor::RowResize)
            }
            Some(ui::Hit::AutomationPoint(_)) => Some(Cursor::Grab),
            Some(ui::Hit::AutomationLane { .. }) => Some(Cursor::Pointer),
            Some(ui::Hit::LayerField(_) | ui::Hit::ClipField(_) | ui::Hit::MusicField(_)) => {
                Some(Cursor::ColResize)
            }
            Some(
                ui::Hit::Action(_)
                | ui::Hit::MenuToggle
                | ui::Hit::MenuItem(_)
                | ui::Hit::MenuPanel
                | ui::Hit::PatchRow(_)
                | ui::Hit::PatchFavorite(_)
                | ui::Hit::LayerRow(_)
                | ui::Hit::Pattern(_, _)
                | ui::Hit::CategoryFilter
                | ui::Hit::CategoryPick(_)
                | ui::Hit::CategoryMenu
                | ui::Hit::FavoriteFilter,
            ) => Some(Cursor::Pointer),
            None => Some(Cursor::Default),
        }
    }
}

impl SfxLogic {
    fn set_param_from_x(&mut self, position: Vec2, target: ui::Hit, layout: &ui::Layout) {
        let Some(rect) = param_row_rect(layout, target) else {
            return;
        };
        let track = ui::param_track(rect);
        let t = ((position.x - track.left()) / track.size.width.max(1.0)).clamp(0.0, 1.0);
        match target {
            ui::Hit::LayerField(field) => {
                let spec = field.spec();
                let value = spec.snap(field.denorm(t)).clamp(spec.min, spec.max);
                if let Some(layer) = self.state.layer_mut() {
                    field.set(layer, value);
                    self.state.mark_dirty();
                }
            }
            ui::Hit::ClipField(field) => {
                if let Some(index) = self.state.selected_clip {
                    let layer_index = self.state.selected_layer;
                    let max_duration = self
                        .state
                        .patch()
                        .and_then(|patch| patch.layers.get(layer_index))
                        .map(|layer| layer.duration)
                        .unwrap_or(3.0);
                    let spec = field.spec();
                    let value = spec.snap(field.denorm(t)).clamp(spec.min, spec.max);
                    if let Some(clip) = self
                        .state
                        .patch_mut()
                        .and_then(|patch| patch.layers.get_mut(layer_index))
                        .and_then(|layer| layer.clips.get_mut(index))
                    {
                        field.set(clip, value);
                        clip.duration = clip.duration.clamp(0.005, max_duration);
                        self.state.mark_dirty();
                    }
                }
            }
            ui::Hit::MusicField(field) => {
                let spec = field.spec();
                let value = spec.snap(field.denorm(t)).clamp(spec.min, spec.max);
                field.set(&mut self.state.project.music, value);
                self.state.mark_music_dirty();
            }
            _ => {}
        }
    }

    fn handle_drag(&mut self, position: Vec2, layout: &ui::Layout) {
        let drag = std::mem::replace(&mut self.state.drag, Drag::None);
        let fine = if self.state.modifiers.shift {
            0.15
        } else if self.state.modifiers.alt {
            4.0
        } else {
            1.0
        };
        // Shift disables the time grid so clip edges can be placed freely.
        let snap = !self.state.modifiers.shift;
        match drag {
            Drag::None => {}
            Drag::Layer { field, last_x } => {
                if self.state.modifiers.shift || self.state.modifiers.alt {
                    let (_, _, step) = field.range();
                    let delta = (position.x - last_x) * step * fine;
                    if let Some(layer) = self.state.layer_mut() {
                        field.set(layer, field.get(layer) + delta);
                        self.state.mark_dirty();
                    }
                } else {
                    self.set_param_from_x(position, ui::Hit::LayerField(field), layout);
                }
                self.state.drag = Drag::Layer {
                    field,
                    last_x: position.x,
                };
            }
            Drag::Music { field, last_x } => {
                if self.state.modifiers.shift || self.state.modifiers.alt {
                    let (_, _, step) = field.range();
                    let delta = (position.x - last_x) * step * fine;
                    let old = field.get(&self.state.project.music);
                    field.set(&mut self.state.project.music, old + delta);
                    self.state.mark_music_dirty();
                } else {
                    self.set_param_from_x(position, ui::Hit::MusicField(field), layout);
                }
                self.state.drag = Drag::Music {
                    field,
                    last_x: position.x,
                };
            }
            Drag::ClipField {
                layer,
                clip,
                field,
                last_x,
            } => {
                if self.state.modifiers.shift || self.state.modifiers.alt {
                    let (_, _, step) = field.range();
                    let delta = (position.x - last_x) * step * fine;
                    if let Some(patch) = self.state.patch_mut() {
                        let max_duration =
                            patch.layers.get(layer).map(|l| l.duration).unwrap_or(3.0);
                        if let Some(clip) = patch
                            .layers
                            .get_mut(layer)
                            .and_then(|l| l.clips.get_mut(clip))
                        {
                            let old = field.get(clip);
                            field.set(clip, old + delta);
                            clip.duration = clip.duration.min(max_duration).max(0.005);
                            self.state.mark_dirty();
                        }
                    }
                } else {
                    self.set_param_from_x(position, ui::Hit::ClipField(field), layout);
                }
                self.state.drag = Drag::ClipField {
                    layer,
                    clip,
                    field,
                    last_x: position.x,
                };
            }
            Drag::Seek { waveform } => {
                let total = self.state.patch().map(|patch| patch.end()).unwrap_or(0.0);
                let time = if waveform {
                    ((position.x - layout.waveform.left()) / layout.waveform.size.width.max(1.0))
                        .clamp(0.0, 1.0)
                        * total
                } else {
                    layout.x_time(position.x).clamp(0.0, total)
                };
                self.seek(time);
                self.state.drag = Drag::Seek { waveform };
            }
            Drag::LibraryScroll {
                start_y,
                start_scroll,
                travel,
                max_start,
            } => {
                let delta = (position.y - start_y) / travel.max(1.0) * max_start as f32;
                self.state.patch_scroll = (start_scroll as f32 + delta)
                    .round()
                    .clamp(0.0, max_start as f32)
                    as usize;
                self.state.drag = Drag::LibraryScroll {
                    start_y,
                    start_scroll,
                    travel,
                    max_start,
                };
            }
            Drag::InspectorScroll {
                start_y,
                start_scroll,
            } => {
                let max_scroll =
                    (layout.inspector_content_h - layout.inspector.size.height).max(0.0);
                let travel = (layout.inspector_scrollbar.size.height
                    - layout.inspector_thumb.size.height)
                    .max(1.0);
                self.state.inspector_scroll = (start_scroll
                    + (position.y - start_y) * max_scroll / travel)
                    .clamp(0.0, max_scroll);
                self.state.bump_view();
                self.state.drag = Drag::InspectorScroll {
                    start_y,
                    start_scroll,
                };
            }
            Drag::AutomationPoint { layer, clip, index } => {
                let duration = self
                    .state
                    .patch()
                    .and_then(|p| p.layers.get(layer))
                    .and_then(|l| l.clips.get(clip))
                    .map(|c| c.duration)
                    .unwrap_or(0.1);
                let (time, value) =
                    layout.automation_at(position, duration, self.state.automation_lane);
                let kind = self.state.automation_lane;
                if let Some(clip_data) = self
                    .state
                    .patch_mut()
                    .and_then(|p| p.layers.get_mut(layer))
                    .and_then(|l| l.clips.get_mut(clip))
                {
                    let duration_max = clip_data.duration;
                    let points = kind.points_mut(clip_data);
                    let min = if index > 0 {
                        points[index - 1].time + 0.002
                    } else {
                        0.0
                    };
                    let max = if index + 1 < points.len() {
                        points[index + 1].time - 0.002
                    } else {
                        duration_max
                    };
                    let (lo, hi, _) = kind.range();
                    if let Some(point) = points.get_mut(index) {
                        point.time = snap_drag_time(time, snap).clamp(min, max);
                        let decimals = if kind == AutomationKind::Gain {
                            100.0
                        } else {
                            10.0
                        };
                        point.value = ((value * decimals).round() / decimals).clamp(lo, hi);
                        self.state.mark_dirty();
                    }
                }
                self.state.drag = Drag::AutomationPoint { layer, clip, index };
            }
            Drag::Clip {
                layer,
                clip,
                mode,
                start_x,
                start,
                duration,
            } => {
                let scale = layout.timeline_scale;
                let dt = (position.x - start_x) / scale;
                if let Some(patch) = self.state.patch_mut() {
                    let max_duration = patch.layers.get(layer).map(|l| l.duration).unwrap_or(3.0);
                    if let Some(clip) = patch
                        .layers
                        .get_mut(layer)
                        .and_then(|l| l.clips.get_mut(clip))
                    {
                        let snap_time = |value: f32| {
                            if snap {
                                ui::snap_time(value)
                            } else {
                                value
                            }
                        };
                        let max_start = ClipField::Start.spec().max;
                        match mode {
                            TimelineDrag::Move => {
                                clip.start = snap_time(start + dt).clamp(0.0, max_start);
                            }
                            TimelineDrag::Resize => {
                                clip.duration = snap_time(duration + dt).clamp(0.005, max_duration);
                            }
                            TimelineDrag::ResizeLeft => {
                                let new_start = snap_time(start + dt).clamp(0.0, max_start);
                                let delta = new_start - start;
                                clip.start = new_start;
                                clip.duration = (duration - delta).clamp(0.005, max_duration);
                            }
                        }
                        self.state.mark_dirty();
                    }
                }
                self.state.drag = Drag::Clip {
                    layer,
                    clip,
                    mode,
                    start_x,
                    start,
                    duration,
                };
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_reflects_timeline_edges_and_drag() {
        let mut logic = SfxLogic {
            state: State::new(),
            measurer: None,
            audio: None,
        };
        logic.state.hover_hit = Some(ui::Hit::Timeline {
            layer: 0,
            clip: Some(0),
            mode: TimelineDrag::Resize,
        });
        assert_eq!(logic.cursor(), Some(Cursor::ColResize));
        logic.state.hover_hit = Some(ui::Hit::Timeline {
            layer: 0,
            clip: Some(0),
            mode: TimelineDrag::Move,
        });
        assert_eq!(logic.cursor(), Some(Cursor::Grab));
        logic.state.drag = Drag::Clip {
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
        let mut state = State::new();
        state.search = "wind".into();
        assert_eq!(state.visible_patches().len(), 1);
        state.search.clear();
        state.category_filter = Some(Category::Combat);
        let combat = state.visible_patches();
        assert_eq!(combat.len(), 2);
        assert!(combat
            .iter()
            .all(|index| state.project.patches[*index].category == Category::Combat));
        state.category_filter = None;
        state.favorites_only = true;
        assert!(state
            .visible_patches()
            .iter()
            .all(|index| state.project.patches[*index].favorite));
    }

    #[test]
    fn clicking_a_parameter_track_sets_its_value() {
        let mut logic = SfxLogic {
            state: State::new(),
            measurer: None,
            audio: None,
        };
        let layout = ui::Layout::new(&logic.state, logic.state.viewport);
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
    fn star_click_toggles_favorite_without_changing_selection() {
        let mut logic = SfxLogic {
            state: State::new(),
            measurer: None,
            audio: None,
        };
        logic.state.selected_patch = 0;
        let target = 2;
        let before = logic.state.project.patches[target].favorite;
        logic.toggle_patch_favorite(target);
        assert_eq!(logic.state.project.patches[target].favorite, !before);
        assert_eq!(logic.state.selected_patch, 0);
        assert_eq!(logic.state.undo.len(), 1);
        logic.state.undo();
        assert_eq!(logic.state.project.patches[target].favorite, before);
    }

    #[test]
    fn picking_a_category_closes_the_menu_and_filters() {
        let mut logic = SfxLogic {
            state: State::new(),
            measurer: None,
            audio: None,
        };
        logic.state.category_menu_open = true;
        logic.pick_category(Some(Category::Combat));
        assert_eq!(logic.state.category_filter, Some(Category::Combat));
        assert!(!logic.state.category_menu_open);
        assert!(logic
            .state
            .visible_patches()
            .iter()
            .all(|index| logic.state.project.patches[*index].category == Category::Combat));
        logic.pick_category(None);
        assert_eq!(logic.state.category_filter, None);
    }

    #[test]
    fn shift_disables_the_clip_snap_grid() {
        assert!((snap_drag_time(0.1234, true) - 0.125).abs() < 0.0001);
        assert_eq!(snap_drag_time(0.1234, false), 0.1234);
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
        let mut state = State::new();
        assert!(state.defer_render);
        assert!(state.preview_pending());
        state.ensure_cache(state.preview_rate());
        assert!(!state.preview_pending());
        state.select_patch(1);
        assert!(state.defer_render);
        assert!(state.preview_pending());
    }

    #[test]
    fn view_changes_do_not_rerender_audio() {
        let mut state = State::new();
        state.ensure_cache(8000);
        let rendered = state.cache_version;
        let samples = state.cache.len();
        state.bump_view();
        state.ensure_cache(8000);
        assert_eq!(state.cache_version, rendered);
        assert_eq!(state.cache.len(), samples);
        state.mark_dirty();
        state.ensure_cache(8000);
        assert_ne!(state.cache_version, rendered);
    }

    #[test]
    fn clicking_the_cutoff_track_midpoint_uses_a_log_curve() {
        let mut logic = SfxLogic {
            state: State::new(),
            measurer: None,
            audio: None,
        };
        logic.state.project.patches[0].layers[0] = Layer::new(Wave::Sine);
        let layout = ui::Layout::new(&logic.state, logic.state.viewport);
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
        let mut state = State::new();
        state.begin_edit();
        state.commit_edit();
        assert!(state.undo.is_empty());
        let original = state.layer().unwrap().gain;
        state.begin_edit();
        state.layer_mut().unwrap().gain = 0.42;
        state.commit_edit();
        assert_eq!(state.undo.len(), 1);
        assert!((state.layer().unwrap().gain - 0.42).abs() < 0.001);
        let _ = state.undo();
        assert!((state.layer().unwrap().gain - original).abs() < 0.001);
    }

    #[test]
    fn undo_redo_restores_clips() {
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.push_undo();
        state.layer_mut().unwrap().ensure_clips();
        assert_eq!(state.layer().unwrap().clips.len(), 1);
        let _ = state.undo();
        assert!(state.layer().unwrap().clips.is_empty());
        let _ = state.redo();
        assert_eq!(state.layer().unwrap().clips.len(), 1);
    }
}
