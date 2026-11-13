//! App state, input handling, audio bridging and the per-frame entry points.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use igui_app::{App, AppConfig, AppLogic, EventContext, FrameContext, InitContext};
use igui_backend_wgpu::FontConfig;
use igui_core::{
    Color, Cursor, EventResult, InputEvent, Key, Modifiers, PointerButton, Rect, Size, Vec2,
};
use igui_headless::{HeadlessPlugin, RecordingHandle};
use igui_render::PaintContext;
use igui_ui::{ApproxTextMeasurer, TextMeasurer};
use igui_winit::{
    ClipboardPlugin, GpuConfig, KeyboardPlugin, PointerPlugin, TextMeasurePlugin, TitlebarMode,
    WgpuPlugin, WindowConfig, WinitPlugin,
};

use crate::audio::AudioEngine;
use crate::model::{
    AutomationKind, Category, Clip, ClipField, Layer, LayerField, MusicField, MusicWave, Patch,
    Project, Wave,
};
use crate::synth;
use crate::{export, model, theme, tres, ui};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Patches,
    Music,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModalAction {
    NewPatch,
    RenamePatch,
}

pub struct Modal {
    pub title: String,
    pub text: String,
    pub action: ModalAction,
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
    pub undo: Vec<Project>,
    pub redo: Vec<Project>,
    pub status: String,
    pub dirty: bool,
    pub version: u64,
    pub cache_version: u64,
    pub cache_rate: u32,
    pub cache: Vec<f32>,
    pub music_cache: Vec<f32>,
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
    pub category_filter: Option<Category>,
    pub favorites_only: bool,
    pub search: String,
    pub search_focus: bool,
    pub inspector_scroll: f32,
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
            status: "OPEN a Godot project folder · demo loaded".into(),
            dirty: false,
            version: 1,
            cache_version: 0,
            cache_rate: 0,
            cache: Vec::new(),
            music_cache: Vec::new(),
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
            category_filter: None,
            favorites_only: false,
            search: String::new(),
            search_focus: false,
            inspector_scroll: 0.0,
        }
    }

    pub fn patch(&self) -> Option<&Patch> {
        self.project.patches.get(self.selected_patch)
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
        self.dirty = true;
    }

    pub fn bump_view(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub fn select_patch(&mut self, index: usize) {
        if index < self.project.patches.len() {
            self.selected_patch = index;
            self.inspector_scroll = 0.0;
            self.selected_layer = self
                .selected_layer
                .min(self.project.patches[index].layers.len().saturating_sub(1));
            self.bump_view();
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
        let body_h = self.viewport.height - theme::TOP_H - theme::STATUS_H - theme::PAD_H;
        let patch_capacity = (((body_h - 76.0 - 28.0 - 8.0) / 24.0).floor() as usize).max(1);
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

        let timeline_h = (body_h - theme::HEADER_H - theme::AUTOMATION_H - 152.0).max(180.0);
        let layer_capacity = (((timeline_h - ui::RULER_H) / ui::TRACK_H).floor() as usize).max(1);
        if self.selected_layer < self.layer_scroll {
            self.layer_scroll = self.selected_layer;
        } else if self.selected_layer >= self.layer_scroll + layer_capacity {
            self.layer_scroll = self.selected_layer + 1 - layer_capacity;
        }
    }

    pub fn push_undo(&mut self) {
        self.undo.push(self.project.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(self.project.clone());
            self.project = previous;
            self.clamp_selection();
            self.mark_dirty();
            self.cache_version = 0;
            self.playhead = 0.0;
            self.seeked = false;
            self.ensure_visible();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.project.clone());
            self.project = next;
            self.clamp_selection();
            self.mark_dirty();
            self.cache_version = 0;
            self.playhead = 0.0;
            self.seeked = false;
            self.ensure_visible();
        }
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
        if self.cache_version == self.version && self.cache_rate == rate {
            return;
        }
        self.cache = self
            .patch()
            .map(|patch| synth::render_patch(patch, rate))
            .unwrap_or_default();
        self.music_cache = synth::render_music(&self.project.music, rate);
        self.cache_version = self.version;
        self.cache_rate = rate;
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.status = text.clone();
        self.toast = Some((text, Instant::now() + Duration::from_secs(4)));
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--selfcheck") {
        selfcheck();
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--bake") {
        if let Some(path) = args.get(index + 1) {
            let folder = PathBuf::from(path);
            let project =
                model::load_project(&folder).unwrap_or_else(|| tres::import_project(&folder));
            match export::bake(&project, &folder) {
                Ok(message) => println!("{message}"),
                Err(error) => {
                    eprintln!("bake failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--export") {
        if let Some(path) = args.get(index + 1) {
            let folder = PathBuf::from(path);
            let project =
                model::load_project(&folder).unwrap_or_else(|| tres::import_project(&folder));
            match export::export_tres(&project, &folder) {
                Ok(message) => println!("{message}"),
                Err(error) => {
                    eprintln!("export failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--dump") {
        if let Some(path) = args.get(index + 1) {
            let folder = PathBuf::from(path);
            let project =
                model::load_project(&folder).unwrap_or_else(|| tres::import_project(&folder));
            println!(
                "{}: {} patches, music {} bpm / {} bars",
                project.name,
                project.patches.len(),
                project.music.bpm,
                project.music.bars
            );
            for patch in &project.patches {
                let layer = patch.layers.first();
                println!(
                    "  {:<14} layers={} wave={} duration={:.3}s gain={:.2}",
                    patch.name,
                    patch.layers.len(),
                    layer.map(|l| l.wave.label()).unwrap_or("—"),
                    patch.end(),
                    layer.map(|l| l.gain).unwrap_or(0.0)
                );
            }
        } else {
            eprintln!("--dump needs a project folder");
            std::process::exit(2);
        }
        return;
    }
    let mut state = State::new();
    if let Some(index) = args.iter().position(|arg| arg == "--project") {
        if let Some(path) = args.get(index + 1) {
            open_folder(&mut state, PathBuf::from(path));
        }
    }
    build_app(state, false).run();
}

fn build_app(state: State, headless: bool) -> App {
    let builder = App::new(AppConfig {
        title: "SFX LAB".into(),
        size: (theme::VIEW_W as f64, theme::VIEW_H as f64),
        ..Default::default()
    });
    let builder = if headless {
        builder.plugin(HeadlessPlugin::new(theme::VIEW_W, theme::VIEW_H))
    } else {
        builder
            .plugin(WinitPlugin::new(WindowConfig {
                title: "SFX LAB".into(),
                size: (theme::VIEW_W as f64, theme::VIEW_H as f64),
                titlebar: TitlebarMode::Native,
                ime: false,
            }))
            .plugin(WgpuPlugin::new(GpuConfig {
                clear_color: theme::BG,
                font: FontConfig {
                    default_family: Some("Helvetica Neue".into()),
                    device_pixel_rasterization: true,
                    em_size: true,
                    ..Default::default()
                },
                ..Default::default()
            }))
            .plugin(PointerPlugin)
            .plugin(KeyboardPlugin)
            .plugin(TextMeasurePlugin)
            .plugin(ClipboardPlugin)
    };
    builder.logic(SfxLogic::new(state)).build()
}

fn selfcheck() {
    let mut app = build_app(State::new(), true);
    let recording = app
        .services()
        .get::<RecordingHandle>()
        .expect("headless recording handle")
        .0
        .clone();
    app.run_headless(2);
    let recording = recording.borrow();
    let frames = recording.frame_count();
    let (commands, text_commands, labels, in_bounds, parameters_visible) = recording
        .last_frame()
        .map(|frame| {
            let mut texts = 0;
            let mut labels = Vec::new();
            let mut in_bounds = true;
            let mut parameters_visible = false;
            for command in frame.commands() {
                if let igui_render::DrawCommand::DrawText { text, position, .. } = command {
                    texts += 1;
                    labels.push(text.clone());
                    if text == "PARAMETERS" && position.x >= theme::VIEW_W - theme::RIGHT_W {
                        parameters_visible = true;
                    }
                    if position.x < 0.0
                        || position.y < 0.0
                        || position.x > theme::VIEW_W
                        || position.y > theme::VIEW_H
                    {
                        in_bounds = false;
                    }
                }
            }
            (
                frame.command_count(),
                texts,
                labels,
                in_bounds,
                parameters_visible,
            )
        })
        .unwrap_or((0, 0, Vec::new(), false, false));
    let required = [
        "SFX LAB",
        "LIBRARY",
        "PARAMETERS",
        "TRIGGER PAD",
        "SOUND",
        "MUSIC",
    ];
    let labels_ok = required
        .iter()
        .all(|needle| labels.iter().any(|text| text == needle));
    println!(
        "sfx-studio selfcheck: {frames} frames, {commands} commands, {text_commands} text runs"
    );
    if frames == 0
        || commands < 100
        || text_commands < 20
        || !labels_ok
        || !in_bounds
        || !parameters_visible
    {
        eprintln!(
            "selfcheck failed: labels_ok={labels_ok} in_bounds={in_bounds} parameters_visible={parameters_visible}"
        );
        std::process::exit(1);
    }
}

pub struct SfxLogic {
    state: State,
    measurer: Option<Rc<dyn TextMeasurer>>,
    audio: Option<AudioEngine>,
}

impl SfxLogic {
    fn new(mut state: State) -> Self {
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
                self.state.bump_view();
                self.state.playhead = 0.0;
            }
            A::TabMusic => {
                self.state.tab = Tab::Music;
                self.state.inspector_scroll = 0.0;
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
            A::BassWave(wave) => {
                self.state.project.music.bass_wave = wave;
                self.state.mark_dirty();
            }
            A::ArpWave(wave) => {
                self.state.project.music.arp_wave = wave;
                self.state.mark_dirty();
            }
            A::TogglePatterns => {
                self.state.project.music.use_patterns = !self.state.project.music.use_patterns;
                self.state.mark_dirty();
            }
            A::ChordPreset(index) => {
                apply_chord_preset(&mut self.state.project.music, index);
                self.state.mark_dirty();
            }
            A::Transpose(delta) => {
                self.state.project.music.transpose(delta);
                self.state.mark_dirty();
            }
            A::ResetPatterns => {
                self.state.project.music.reset_patterns();
                self.state.mark_dirty();
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
            A::Undo => self.state.undo(),
            A::Redo => self.state.redo(),
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
        open_folder(&mut self.state, folder);
        self.state.toast("project loaded");
    }

    fn save(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        match model::save_project(&root, &self.state.project) {
            Ok(()) => {
                self.state.dirty = false;
                self.state.toast(format!(
                    "saved {}",
                    root.join(model::PROJECT_FILE).display()
                ));
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
        self.push_undo();
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
        let Some(audio) = &self.audio else {
            self.state.toast("no audio device");
            return;
        };
        match self.state.tab {
            Tab::Patches => {
                let mut samples = self.state.cache.clone();
                let offset = if self.state.seeked {
                    (self.state.playhead * rate as f32) as usize
                } else {
                    0
                };
                if offset < samples.len() {
                    samples.drain(0..offset);
                }
                if samples.is_empty() {
                    self.state.toast("nothing to play");
                    return;
                }
                audio.play(samples, self.state.loop_play);
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
                audio.play(samples, true);
                self.state.playhead = 0.0;
            }
        }
        self.state.seeked = false;
        self.state.was_playing = true;
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
        let name = model::sanitize(&modal.text);
        match modal.action {
            ModalAction::NewPatch => {
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
        if self.state.modal.is_none() {
            return false;
        }
        match event {
            InputEvent::TextInput { text } => {
                if let Some(modal) = self.state.modal.as_mut() {
                    for ch in text.chars() {
                        if !ch.is_control() && modal.text.len() < 48 {
                            modal.text.push(ch);
                        }
                    }
                }
                true
            }
            InputEvent::KeyDown { key } => match key {
                Key::Backspace => {
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

    fn timeline_geometry(&self, layout: &ui::Layout) -> (f32, f32) {
        let total = self.state.patch().map(|p| p.end().max(0.05)).unwrap_or(0.2);
        let scale =
            ((layout.timeline.size.width - ui::TIMELINE_LABEL - 8.0) / (total + 0.08)).max(8.0);
        (scale, total)
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
                        self.stop();
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
                    Some(ui::Hit::Pad(index)) => {
                        self.trigger_pad(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::CategoryFilter) => {
                        self.activate(ui::Action::CategoryCycle);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::FavoriteFilter) => {
                        self.activate(ui::Action::FavoriteFilter);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LayerField(field)) => {
                        self.push_undo();
                        self.set_param_from_x(*position, ui::Hit::LayerField(field), &layout);
                        self.state.drag = Drag::Layer {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::MusicField(field)) => {
                        self.push_undo();
                        self.set_param_from_x(*position, ui::Hit::MusicField(field), &layout);
                        self.state.drag = Drag::Music {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::ClipField(field)) => {
                        if let Some(index) = self.state.selected_clip {
                            self.push_undo();
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
                    Some(ui::Hit::PatchRow(index)) => {
                        self.state.select_patch(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LayerRow(index)) => {
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Timeline { layer, clip, mode }) => {
                        self.push_undo();
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
                        self.state.playhead = time;
                        self.state.seeked = true;
                        self.state.drag = Drag::Seek { waveform: false };
                        self.state.bump_view();
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Waveform(time)) => {
                        self.state.playhead = time;
                        self.state.seeked = true;
                        self.state.drag = Drag::Seek { waveform: true };
                        self.state.bump_view();
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
                            self.push_undo();
                            self.state.selected_env_point = Some(index);
                            self.state.drag = Drag::AutomationPoint {
                                layer: self.state.selected_layer,
                                clip,
                                index,
                            };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::AutomationLane { time, value }) => {
                        if let Some(clip_index) = self.state.selected_clip {
                            self.push_undo();
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
                        self.state.mark_dirty();
                    }
                    _ => {}
                }
                return EventResult::Handled;
            }
            InputEvent::PointerUp { position, .. } => {
                self.state.drag = Drag::None;
                self.state.hover_hit = ui::hit(&self.state, &layout, *position);
                return EventResult::Handled;
            }
            InputEvent::Wheel {
                position, delta, ..
            } => {
                let rows = (delta.y.abs() / 24.0).max(1.0) as i32;
                let step = if delta.y > 0.0 { -rows } else { rows };
                if layout.left.contains(*position) {
                    let body_h =
                        self.state.viewport.height - theme::TOP_H - theme::STATUS_H - theme::PAD_H;
                    let capacity = (((body_h - 76.0 - 28.0 - 8.0) / 24.0).floor() as usize).max(1);
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
                    if let Some(patch) = self.state.patch() {
                        let max_start = patch.layers.len().saturating_sub(9);
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
        if let Some((_, deadline)) = &self.state.toast {
            if Instant::now() > *deadline {
                self.state.toast = None;
            }
        }
        let rate = self.state.preview_rate();
        self.state.ensure_cache(rate);
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
        self.audio
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
            Drag::InspectorScroll { .. } => return Some(Cursor::RowResize),
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
            Some(ui::Hit::InspectorScrollbar) => Some(Cursor::RowResize),
            Some(ui::Hit::AutomationPoint(_)) => Some(Cursor::Grab),
            Some(ui::Hit::AutomationLane { .. }) => Some(Cursor::Pointer),
            Some(ui::Hit::LayerField(_) | ui::Hit::ClipField(_) | ui::Hit::MusicField(_)) => {
                Some(Cursor::ColResize)
            }
            Some(
                ui::Hit::Action(_)
                | ui::Hit::PatchRow(_)
                | ui::Hit::LayerRow(_)
                | ui::Hit::Pattern(_, _)
                | ui::Hit::Pad(_)
                | ui::Hit::CategoryFilter
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
                let (min, max, step) = field.range();
                let value = snap_to(min + t * (max - min), step).clamp(min, max);
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
                    let (min, max, step) = field.range();
                    let value = snap_to(min + t * (max - min), step).clamp(min, max);
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
                let (min, max, step) = field.range();
                let value = snap_to(min + t * (max - min), step).clamp(min, max);
                field.set(&mut self.state.project.music, value);
                self.state.mark_dirty();
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
                    self.state.mark_dirty();
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
                self.state.playhead = time;
                self.state.seeked = true;
                self.state.bump_view();
                self.state.drag = Drag::Seek { waveform };
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
                        point.time = ((time * 200.0).round() / 200.0).clamp(min, max);
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
                let (scale, _) = self.timeline_geometry(layout);
                let dt = (position.x - start_x) / scale;
                if let Some(patch) = self.state.patch_mut() {
                    let max_duration = patch.layers.get(layer).map(|l| l.duration).unwrap_or(3.0);
                    if let Some(clip) = patch
                        .layers
                        .get_mut(layer)
                        .and_then(|l| l.clips.get_mut(clip))
                    {
                        match mode {
                            TimelineDrag::Move => {
                                clip.start = ((start + dt) * 200.0).round() / 200.0;
                                clip.start = clip.start.max(0.0);
                            }
                            TimelineDrag::Resize => {
                                clip.duration = ((duration + dt) * 200.0).round() / 200.0;
                                clip.duration = clip.duration.clamp(0.005, max_duration);
                            }
                            TimelineDrag::ResizeLeft => {
                                let new_start = ((start + dt) * 200.0).round() / 200.0;
                                let new_start = new_start.max(0.0);
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

fn open_folder(state: &mut State, folder: PathBuf) {
    let project = model::load_project(&folder).unwrap_or_else(|| tres::import_project(&folder));
    state.project = project;
    state.root = Some(folder);
    state.selected_patch = 0;
    state.selected_layer = 0;
    state.tab = Tab::Patches;
    state.playhead = 0.0;
    state.seeked = false;
    state.dirty = false;
    state.version = state.version.wrapping_add(1);
    state.cache_version = 0;
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

fn snap_to(value: f32, step: f32) -> f32 {
    if step > 0.0 {
        (value / step).round() * step
    } else {
        value
    }
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

#[allow(dead_code)]
fn _keep(_: Color, _: MusicWave) {}

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
    fn undo_redo_restores_clips() {
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.push_undo();
        state.layer_mut().unwrap().ensure_clips();
        assert_eq!(state.layer().unwrap().clips.len(), 1);
        state.undo();
        assert!(state.layer().unwrap().clips.is_empty());
        state.redo();
        assert_eq!(state.layer().unwrap().clips.len(), 1);
    }
}
