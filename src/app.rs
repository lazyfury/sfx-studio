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

mod audio_bridge;
mod commands;
mod input;

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
