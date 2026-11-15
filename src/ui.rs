//! Immediate-mode layout, paint and hit-testing for the compact studio shell.

use igui_core::{Color, Key, Modifiers, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_ui::TextMeasurer;

use crate::app::{Drag, Overlay, State, Tab, TimelineDrag};
use crate::icons::{self, Icon};
use crate::library::{LibraryKey, LibraryKind, LibraryListRow};
use crate::model::{
    layer_sections, music_sections, AutomationKind, Category, ClipField, EnvShape, FilterMode,
    Layer, LayerField, LayerRowSpec, MusicField, MusicRowSpec, MusicWave, PatchField, ToneEnv,
    Wave, CLIP_FIELDS,
};
use crate::synth;
use crate::theme::{self, *};

pub const TIMELINE_LABEL: f32 = 76.0;
pub const RULER_H: f32 = 18.0;
pub const TRACK_H: f32 = 22.0;
pub const INSPECTOR_LABEL: f32 = 82.0;
/// Inner breathing room for inspector rows so controls never touch the panel
/// card edges.
pub const INSPECTOR_PAD: f32 = 6.0;
/// Top inset of the first inspector row; included in the scroll maximum.
pub const INSPECTOR_TOP_PAD: f32 = 4.0;
/// Time grid used by clip/automation drags (5 ms).
pub const CLIP_SNAP: f32 = 0.005;

/// Shared layout metrics. `app.rs` uses these helpers instead of re-deriving
/// row capacity, so a metrics change cannot silently desync scrolling.
pub const LIBRARY_TOP: f32 = 86.0;
pub const LIBRARY_ROW_H: f32 = 24.0;
pub const LIBRARY_END_GAP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    NewProject,
    Open,
    Save,
    InstallPresets,
    SavePreset,
    RenamePreset,
    DeletePreset,
    RestorePreset,
    RenameAudio,
    DeleteAudio,
    RestoreAudio,
    RenameMusic,
    DeleteMusic,
    RestoreMusic,
    ExportWav,
    Bake,
    Tres,
    Randomize,
    Vary,
    Favorite,
    CategoryCycle,
    FavoriteFilter,
    TabPatches,
    TabMusic,
    NewPatch,
    DeletePatch,
    DuplicatePatch,
    RenamePatch,
    Play,
    Stop,
    Loop,
    AddLayer,
    DuplicateLayer,
    DeleteLayer,
    SetWave(Wave),
    SetFilter(FilterMode),
    SetEnv(EnvShape),
    SetToneEnv(ToneEnv),
    BassWave(MusicWave),
    ArpWave(MusicWave),
    TogglePatterns,
    ChordPreset(usize),
    DemoTrack(usize),
    Transpose(i32),
    ResetPatterns,
    AddClip,
    DeleteClip,
    LoadSample,
    ToggleSampleLoop,
    ToggleSampleReverse,
    UpgradeDsp,
    Undo,
    Redo,
    AutomationLane(AutomationKind),
}

/// Everything a command needs to participate in menu availability, undo and
/// focus handling without duplicating `matches!` lists in the call sites.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionScope {
    /// File/document lifecycle; never disabled by an asset selection.
    Document,
    /// Mutates the current project document.
    ProjectEdit,
    /// Operates on the selected read-only asset (preset lifecycle).
    AssetEdit,
    /// Playback transport.
    Transport,
    /// Pure view state (tabs, filters, automation lane).
    View,
}

#[derive(Clone, Copy, Debug)]
pub struct ActionMeta {
    pub scope: ActionScope,
    /// `Some(label)` means `activate` pushes one undo snapshot before running.
    /// Commands that open a modal or handle their own transaction use `None`.
    pub undo: Option<&'static str>,
    /// Menu/button label. The menu only stores order + section membership.
    pub label: &'static str,
    /// Status-line / tooltip explanation.
    pub tip: &'static str,
    pub icon: Option<Icon>,
}

pub fn action_meta(action: Action) -> ActionMeta {
    use Action as A;
    use ActionScope::{AssetEdit, Document, ProjectEdit, Transport, View};
    let label = action_label(action);
    let tip = action_tip(action);
    let icon = action_icon(action);
    let meta = |scope, undo| ActionMeta {
        scope,
        undo,
        label,
        tip,
        icon,
    };
    match action {
        A::NewProject | A::Open | A::Save | A::InstallPresets | A::Bake | A::Tres => {
            meta(Document, None)
        }
        // Target-driven: availability checks `export_patch()`.
        A::ExportWav => meta(Document, None),
        A::RenamePreset
        | A::DeletePreset
        | A::RestorePreset
        | A::RenameAudio
        | A::DeleteAudio
        | A::RestoreAudio
        | A::RenameMusic
        | A::DeleteMusic
        | A::RestoreMusic => meta(AssetEdit, None),
        A::SavePreset => meta(ProjectEdit, None),
        A::Randomize => meta(ProjectEdit, Some("randomize")),
        A::Vary => meta(ProjectEdit, Some("create variations")),
        A::Favorite => meta(ProjectEdit, Some("favorite")),
        A::CategoryCycle | A::FavoriteFilter | A::TabPatches | A::TabMusic => meta(View, None),
        A::AutomationLane(_) => meta(View, None),
        A::NewPatch | A::RenamePatch => meta(ProjectEdit, None),
        A::DeletePatch => meta(ProjectEdit, Some("delete sound")),
        A::DuplicatePatch => meta(ProjectEdit, Some("duplicate sound")),
        A::Play | A::Stop | A::Loop | A::Undo | A::Redo => meta(Transport, None),
        A::AddLayer => meta(ProjectEdit, Some("add layer")),
        A::DuplicateLayer => meta(ProjectEdit, Some("duplicate layer")),
        A::DeleteLayer => meta(ProjectEdit, Some("delete layer")),
        A::SetWave(_) => meta(ProjectEdit, Some("change wave")),
        A::SetFilter(_) => meta(ProjectEdit, Some("change filter")),
        A::SetEnv(_) => meta(ProjectEdit, Some("change envelope")),
        A::SetToneEnv(_) => meta(ProjectEdit, Some("change tone envelope")),
        A::BassWave(_) | A::ArpWave(_) => meta(ProjectEdit, Some("change music instrument")),
        A::TogglePatterns => meta(ProjectEdit, Some("toggle patterns")),
        A::ChordPreset(_) => meta(ProjectEdit, Some("chord preset")),
        A::DemoTrack(_) => meta(ProjectEdit, Some("load demo track")),
        A::Transpose(_) => meta(ProjectEdit, Some("transpose")),
        A::ResetPatterns => meta(ProjectEdit, Some("reset patterns")),
        A::AddClip => meta(ProjectEdit, Some("add clip")),
        A::DeleteClip => meta(ProjectEdit, Some("delete clip")),
        A::LoadSample => meta(ProjectEdit, None),
        A::ToggleSampleLoop => meta(ProjectEdit, Some("sample loop")),
        A::ToggleSampleReverse => meta(ProjectEdit, Some("sample reverse")),
        A::UpgradeDsp => meta(ProjectEdit, None),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PatternRow {
    Arp(usize),
    Bass(usize),
    Kick,
    Hat,
}

/// Editable curve canvases in the inspector.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CurveKind {
    Cycle,
    Amp,
}

impl CurveKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cycle => "SINGLE-CYCLE WAVE",
            Self::Amp => "AMP CURVE",
        }
    }
}

/// Fixed editor resolution for both curve canvases.
pub const CURVE_BINS: usize = 32;

/// Values shown by the editor; empty/malformed data falls back to sine for the
/// cycle and the current ADSR for amplitude.
pub(crate) fn curve_values(layer: &Layer, kind: CurveKind) -> Vec<f32> {
    match kind {
        CurveKind::Cycle => {
            if layer.curve.len() >= 2 {
                layer.curve.clone()
            } else {
                (0..CURVE_BINS)
                    .map(|index| (std::f32::consts::TAU * index as f32 / CURVE_BINS as f32).sin())
                    .collect()
            }
        }
        CurveKind::Amp => {
            if layer.amp_curve.len() >= 2 {
                layer.amp_curve.clone()
            } else {
                (0..CURVE_BINS)
                    .map(|index| {
                        let k = index as f32 / (CURVE_BINS - 1) as f32;
                        let t = k * layer.duration;
                        match layer.tone_env {
                            ToneEnv::Exp => {
                                (-layer.exp_decay * k).exp()
                                    * (t / layer.attack.max(0.0001)).min(1.0)
                            }
                            ToneEnv::Adsr => synth::adsr(
                                t,
                                layer.duration,
                                layer.attack,
                                layer.decay,
                                layer.sustain,
                                layer.release,
                            ),
                        }
                    })
                    .collect()
            }
        }
    }
}

pub fn curve_index(kind: CurveKind, rect: Rect, x: f32, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let t = ((x - rect.left()) / rect.size.width.max(1.0)).clamp(0.0, 1.0);
    match kind {
        CurveKind::Cycle => ((t * len as f32).floor() as usize).min(len - 1),
        CurveKind::Amp => ((t * (len - 1) as f32).round() as usize).min(len - 1),
    }
}

pub fn curve_value_from_y(kind: CurveKind, rect: Rect, y: f32) -> f32 {
    let t = ((rect.bottom() - y) / rect.size.height.max(1.0)).clamp(0.0, 1.0);
    match kind {
        CurveKind::Cycle => t * 2.0 - 1.0,
        CurveKind::Amp => t,
    }
}

pub fn curve_point(kind: CurveKind, rect: Rect, value: f32, index: usize, len: usize) -> Vec2 {
    let x = rect.left() + index as f32 / (len.saturating_sub(1).max(1)) as f32 * rect.size.width;
    let norm = match kind {
        CurveKind::Cycle => (value.clamp(-1.0, 1.0) + 1.0) * 0.5,
        CurveKind::Amp => value.clamp(0.0, 1.0),
    };
    Vec2::new(x, rect.bottom() - norm * rect.size.height)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    Action(Action),
    LibraryRow(LibraryKey),
    LibraryFavorite(LibraryKey),
    /// Index into `Layout::library_rows`; toggles a folder header.
    LibraryFolder(usize),
    LayerRow(usize),
    LayerField(LayerField),
    PatchField(PatchField),
    MusicField(MusicField),
    ClipField(ClipField),
    Timeline {
        layer: usize,
        clip: Option<usize>,
        mode: TimelineDrag,
    },
    Ruler(f32),
    AutomationPoint(usize),
    AutomationLane {
        time: f32,
        value: f32,
    },
    Pattern(PatternRow, usize),
    Curve(CurveKind),
    MenuToggle,
    MenuItem(Action),
    MenuPanel,
    CategoryFilter,
    CategoryPick(Option<Category>),
    CategoryMenu,
    KindFilter,
    KindPick(Option<LibraryKind>),
    KindMenu,
    ClearFilters,
    FavoriteFilter,
    InstallPresets,
    Search,
    LibraryScrollbar,
    Waveform(f32),
    InspectorScrollbar,
}

pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub action: Action,
    pub active: bool,
    pub icon: Option<Icon>,
}

#[derive(Clone, Copy)]
pub enum MenuEntry {
    Section(&'static str),
    /// The label comes from `action_meta`; this table only declares order and
    /// grouping so a command cannot have two different menu labels.
    Item(Action),
}

/// Top-bar actions are grouped into one system menu instead of a row of
/// buttons. Keyboard shortcuts remain available.
pub const MENU_ENTRIES: &[MenuEntry] = &[
    MenuEntry::Section("FILE"),
    MenuEntry::Item(Action::NewProject),
    MenuEntry::Item(Action::Open),
    MenuEntry::Item(Action::Save),
    MenuEntry::Item(Action::NewPatch),
    MenuEntry::Item(Action::RenamePatch),
    MenuEntry::Item(Action::DuplicatePatch),
    MenuEntry::Item(Action::DeletePatch),
    MenuEntry::Section("SOUND"),
    MenuEntry::Item(Action::Randomize),
    MenuEntry::Item(Action::Vary),
    MenuEntry::Item(Action::Favorite),
    MenuEntry::Section("PRESETS"),
    MenuEntry::Item(Action::SavePreset),
    MenuEntry::Item(Action::RenamePreset),
    MenuEntry::Item(Action::DeletePreset),
    MenuEntry::Item(Action::RestorePreset),
    MenuEntry::Section("AUDIO"),
    MenuEntry::Item(Action::RenameAudio),
    MenuEntry::Item(Action::DeleteAudio),
    MenuEntry::Item(Action::RestoreAudio),
    MenuEntry::Section("MUSIC"),
    MenuEntry::Item(Action::RenameMusic),
    MenuEntry::Item(Action::DeleteMusic),
    MenuEntry::Item(Action::RestoreMusic),
    MenuEntry::Section("EXPORT"),
    MenuEntry::Item(Action::ExportWav),
    MenuEntry::Item(Action::Bake),
    MenuEntry::Item(Action::Tres),
];

pub enum RawItem {
    Section(String),
    Param {
        label: String,
        value: String,
        desc: String,
        hit: Hit,
        norm: f32,
    },
    Options {
        label: String,
        options: Vec<(String, Action, bool)>,
        columns: usize,
    },
    Toggle {
        label: String,
        value: bool,
        action: Action,
    },
    Curve {
        kind: CurveKind,
        height: f32,
    },
    Note(String),
}

pub enum Item {
    Section {
        text: String,
        rect: Rect,
    },
    Param {
        label: String,
        value: String,
        desc: String,
        hit: Hit,
        norm: f32,
        rect: Rect,
    },
    Options {
        label: String,
        rect: Rect,
        buttons: Vec<Button>,
    },
    Toggle {
        label: String,
        value: bool,
        action: Action,
        rect: Rect,
    },
    Curve {
        kind: CurveKind,
        rect: Rect,
    },
    Note {
        text: String,
        rect: Rect,
    },
}

pub fn action_label(action: Action) -> &'static str {
    match action {
        Action::NewProject => "NEW PROJECT…",
        Action::Open => "OPEN…",
        Action::Save => "SAVE",
        Action::InstallPresets => "INSTALL BUILTIN PRESETS",
        Action::SavePreset => "SAVE AS PRESET",
        Action::RenamePreset => "RENAME PRESET",
        Action::DeletePreset => "DELETE PRESET",
        Action::RestorePreset => "RESTORE LAST PRESET",
        Action::RenameAudio => "RENAME AUDIO",
        Action::DeleteAudio => "DELETE AUDIO",
        Action::RestoreAudio => "RESTORE AUDIO",
        Action::RenameMusic => "RENAME MUSIC",
        Action::DeleteMusic => "DELETE MUSIC",
        Action::RestoreMusic => "RESTORE MUSIC",
        Action::ExportWav => "EXPORT WAV",
        Action::Bake => "BAKE GODOT",
        Action::Tres => "EXPORT .TRES",
        Action::Randomize => "RANDOMIZE",
        Action::Vary => "VARY A/B/C",
        Action::Favorite => "FAVORITE",
        Action::CategoryCycle => "CATEGORY",
        Action::FavoriteFilter => "FAVORITES",
        Action::TabPatches => "SOUND",
        Action::TabMusic => "MUSIC",
        Action::NewPatch => "NEW SOUND",
        Action::RenamePatch => "RENAME",
        Action::DuplicatePatch => "DUPLICATE",
        Action::DeletePatch => "DELETE",
        Action::Play => "PLAY",
        Action::Stop => "STOP",
        Action::Loop => "LOOP",
        Action::AddLayer => "LAYER",
        Action::DuplicateLayer => "DUP",
        Action::DeleteLayer => "DEL",
        Action::SetWave(_) => "WAVE",
        Action::SetFilter(_) => "FILTER",
        Action::SetEnv(_) => "ENV",
        Action::SetToneEnv(_) => "TONE ENV",
        Action::BassWave(_) => "BASS",
        Action::ArpWave(_) => "ARP",
        Action::TogglePatterns => "PATTERN",
        Action::ChordPreset(_) => "CHORDS",
        Action::DemoTrack(_) => "TRACK",
        Action::Transpose(_) => "TRANSPOSE",
        Action::ResetPatterns => "RESET",
        Action::AddClip => "CLIP",
        Action::DeleteClip => "DEL CLIP",
        Action::LoadSample => "LOAD WAV…",
        Action::ToggleSampleLoop => "LOOP",
        Action::ToggleSampleReverse => "REVERSE",
        Action::UpgradeDsp => "UPGRADE TO HD",
        Action::Undo => "UNDO",
        Action::Redo => "REDO",
        Action::AutomationLane(kind) => kind.label(),
    }
}

/// One keyboard binding. `shift: None` means "ignore Shift"; `Some` makes the
/// binding shift-sensitive (Undo/Redo share Cmd+Z).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: Key,
    pub cmd: bool,
    pub shift: Option<bool>,
}

pub const SHORTCUTS: &[(Shortcut, Action)] = &[
    (
        Shortcut {
            key: Key::Character('s'),
            cmd: true,
            shift: None,
        },
        Action::Save,
    ),
    (
        Shortcut {
            key: Key::Character('o'),
            cmd: true,
            shift: None,
        },
        Action::Open,
    ),
    (
        Shortcut {
            key: Key::Character('e'),
            cmd: true,
            shift: None,
        },
        Action::ExportWav,
    ),
    (
        Shortcut {
            key: Key::Character('b'),
            cmd: true,
            shift: None,
        },
        Action::Bake,
    ),
    (
        Shortcut {
            key: Key::Character('z'),
            cmd: true,
            shift: Some(false),
        },
        Action::Undo,
    ),
    (
        Shortcut {
            key: Key::Character('z'),
            cmd: true,
            shift: Some(true),
        },
        Action::Redo,
    ),
    (
        Shortcut {
            key: Key::Character('y'),
            cmd: true,
            shift: None,
        },
        Action::Redo,
    ),
    (
        Shortcut {
            key: Key::Character('r'),
            cmd: false,
            shift: None,
        },
        Action::Randomize,
    ),
    (
        Shortcut {
            key: Key::Character('v'),
            cmd: false,
            shift: None,
        },
        Action::Vary,
    ),
    (
        Shortcut {
            key: Key::Character('c'),
            cmd: false,
            shift: None,
        },
        Action::CategoryCycle,
    ),
    (
        Shortcut {
            key: Key::Character('l'),
            cmd: false,
            shift: None,
        },
        Action::Loop,
    ),
];

/// Resolve a key chord for the actions that are a straight 1:1 binding.
/// Space/Tab/Delete/arrows/search/pads stay explicit in `input.rs` because
/// they have gesture or focus semantics beyond a command dispatch.
pub fn shortcut_action(key: Key, modifiers: Modifiers) -> Option<Action> {
    let cmd = modifiers.meta || modifiers.ctrl;
    SHORTCUTS.iter().find_map(|(shortcut, action)| {
        let key_matches = match (shortcut.key, key) {
            (Key::Character(bound), Key::Character(actual)) => bound.eq_ignore_ascii_case(&actual),
            _ => shortcut.key == key,
        };
        if key_matches
            && shortcut.cmd == cmd
            && shortcut.shift.is_none_or(|shift| shift == modifiers.shift)
        {
            Some(*action)
        } else {
            None
        }
    })
}

fn action_icon(action: Action) -> Option<Icon> {
    match action {
        Action::NewProject => Some(Icon::FilePlus),
        Action::Open => Some(Icon::FolderOpen),
        Action::Save => Some(Icon::Save),
        Action::InstallPresets | Action::SavePreset => Some(Icon::FilePlus),
        Action::RenamePreset | Action::RenameAudio | Action::RenameMusic => Some(Icon::Pencil),
        Action::DeletePreset | Action::DeleteAudio | Action::DeleteMusic => Some(Icon::Trash),
        Action::RestoreAudio | Action::RestoreMusic => Some(Icon::Undo),
        Action::RestorePreset => Some(Icon::Undo),
        Action::NewPatch => Some(Icon::FilePlus),
        Action::RenamePatch => Some(Icon::Pencil),
        Action::DuplicatePatch | Action::DuplicateLayer => Some(Icon::Copy),
        Action::DeletePatch | Action::DeleteLayer | Action::DeleteClip => Some(Icon::Trash),
        Action::Randomize => Some(Icon::Wand),
        Action::Vary => Some(Icon::Layers),
        Action::Favorite => Some(Icon::Star),
        Action::ExportWav => Some(Icon::Download),
        Action::Bake => Some(Icon::Package),
        Action::Tres => Some(Icon::FileCode),
        Action::AddLayer | Action::AddClip => Some(Icon::Plus),
        Action::LoadSample => Some(Icon::FolderOpen),
        Action::Play => Some(Icon::Play),
        Action::Stop => Some(Icon::Stop),
        Action::Loop => Some(Icon::Loop),
        Action::Undo => Some(Icon::Undo),
        Action::Redo => Some(Icon::Redo),
        _ => None,
    }
}

fn action_tip(action: Action) -> &'static str {
    match action {
        Action::NewProject => "新建工程目录 + 内部预设 · New project",
        Action::Open => "打开项目文件夹 · Open",
        Action::Save => "保存工程 · Save（⌘S）",
        Action::InstallPresets => "安装内置预设到当前工程 · Install builtin presets",
        Action::SavePreset => "把当前音效保存到工程预设库 · Save as preset",
        Action::RenamePreset => "重命名选中预设 · Rename preset（PRESETS）",
        Action::DeletePreset => "删除选中预设（移到 presets/.trash/）· Delete preset",
        Action::RestorePreset => "从 presets/.trash/ 恢复最近删除的预设 · Restore",
        Action::RenameAudio => "重命名选中音频文件 · Rename audio",
        Action::DeleteAudio => "把选中音频移到 audio/.trash/ · Delete audio",
        Action::RestoreAudio => "恢复 audio/.trash/ 中最近的音频 · Restore audio",
        Action::RenameMusic => "重命名选中 music 文件 · Rename music",
        Action::DeleteMusic => "把选中 music 移到 music/.trash/ · Delete music",
        Action::RestoreMusic => "恢复 music/.trash/ 中最近的 music · Restore music",
        Action::ExportWav => "导出 48 kHz WAV · 预设模式导出选中预设（⌘E）",
        Action::Bake => "烘焙 Godot 音频资源 · Bake（⌘B）",
        Action::Tres => "导出 Godot .tres 参数 · Resource",
        Action::Randomize => "随机微调当前音效 · Randomize（R）",
        Action::Vary => "生成 A/B/C 变体 · Variation（V）",
        Action::Favorite => "收藏 / 取消收藏 · Favorite（F）",
        Action::CategoryCycle => "切换分类过滤 · Category（C）",
        Action::FavoriteFilter => "只看收藏 · Favorites",
        Action::NewPatch => "新建音效 · New",
        Action::DuplicatePatch => "复制当前音效 · Duplicate",
        Action::RenamePatch => "重命名当前音效 · Rename",
        Action::DeletePatch => "删除当前音效 · Delete",
        Action::TabPatches => "音效工作区 · Sound Studio",
        Action::TabMusic => "音乐 / 音序器 · Music",
        Action::Play => "播放 · Play（Space）",
        Action::Stop => "停止 · Stop（Esc）",
        Action::Loop => "循环播放 · Loop（L）",
        Action::Undo => "撤销 · Undo（⌘Z）",
        Action::Redo => "重做 · Redo（⇧⌘Z）",
        Action::AddLayer => "添加图层 · Add layer",
        Action::DuplicateLayer => "复制图层 · Duplicate layer",
        Action::DeleteLayer => "删除图层 · Delete layer",
        Action::AddClip => "添加 Clip · Add clip",
        Action::DeleteClip => "删除 Clip · Delete clip",
        Action::LoadSample => "载入 WAV 采样 · Load sample",
        Action::ToggleSampleLoop => "采样循环 · Sample loop",
        Action::ToggleSampleReverse => "反向播放采样 · Reverse",
        Action::UpgradeDsp => "升级到 HD 立体声链路 · Upgrade DSP v3",
        Action::SetWave(wave) => wave.desc(),
        Action::SetFilter(_) => "切换滤波器 · Filter",
        Action::SetEnv(_) => "切换包络 · Envelope",
        Action::SetToneEnv(_) => "音调包络 · Tone envelope",
        Action::AutomationLane(_) => "自动化轨道 · Automation lane",
        Action::BassWave(_) => "贝斯波形 · Bass wave",
        Action::ArpWave(_) => "琶音波形 · Arp wave",
        Action::TogglePatterns => "自定义 16 步节奏 · Pattern",
        Action::ChordPreset(_) => "和弦预设 · Chord preset",
        Action::DemoTrack(_) => "载入 demo 编曲 · Demo track",
        Action::Transpose(_) => "整体移调 · Transpose",
        Action::ResetPatterns => "重置节奏 · Reset pattern",
    }
}

mod hit;
mod layout;
mod paint;

/// Where a project-local preset can be dropped.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DropTarget {
    Timeline { layer: usize, time: f32 },
    ProjectLibrary,
}

/// Commands that mutate the project document. They are disabled while a
/// PRESET row owns the focus (the preset library is read-only) but remain
/// available when inspecting an AUDIO/MUSIC row: those rows are project
/// assets, not a different editing mode.
pub fn is_project_edit(action: Action) -> bool {
    action_meta(action).scope == ActionScope::ProjectEdit
}

/// Project-only commands are disabled while a PRESET row is focused so the
/// selected row and the command target cannot silently disagree. Document-level
/// commands (Save/Open/Bake/TRES/New Project) stay available; AUDIO/MUSIC
/// inspection never disables project editing.
///
/// WAV export is target-driven: it needs a project patch or a selected preset.
pub fn action_available(state: &State, action: Action) -> bool {
    match action {
        Action::RenamePreset | Action::DeletePreset => state.selected_preset_index().is_some(),
        Action::RenameAudio | Action::DeleteAudio => state.selected_audio_index().is_some(),
        Action::RestoreAudio | Action::RestoreMusic => state.doc.root.is_some(),
        Action::RenameMusic | Action::DeleteMusic => state.selected_music().is_some(),
        Action::RestorePreset => state.preset_library_focus(),
        Action::ExportWav => state.export_patch().is_some(),
        _ if is_project_edit(action) => !state.preset_focus(),
        _ => true,
    }
}

/// Human-readable reason for a greyed-out action, used by the tooltip and the
/// status line. Keeps the PRESETS read-only boundary separate from the benign
/// "select a preset first" case.
pub fn action_unavailable_message(state: &State, action: Action) -> &'static str {
    if action == Action::ExportWav {
        if state.preset_focus() {
            return "先选中一个预设 · select a preset";
        }
        if matches!(
            state.selected_library_kind(),
            Some(crate::library::LibraryKind::Audio | crate::library::LibraryKind::Music)
        ) {
            return "AUDIO/MUSIC 不能导出为音效 · 选 SOUND 或 PRESET";
        }
        return "没有可导出的音效";
    }
    if matches!(action, Action::RenameAudio | Action::DeleteAudio) {
        return "先选中一个音频文件 · select AUDIO";
    }
    if matches!(action, Action::RenameMusic | Action::DeleteMusic) {
        return "先选中一个 music 文件 · select MUSIC";
    }
    if matches!(action, Action::RestoreAudio | Action::RestoreMusic) {
        return "先打开一个工程目录 · open a project";
    }
    if matches!(action, Action::RenamePreset | Action::DeletePreset) {
        return "先选中一个预设 · select a preset";
    }
    if action == Action::RestorePreset {
        return "没有可恢复的预设 · select PRESETS";
    }
    if is_project_edit(action) && state.preset_focus() {
        return "PRESETS 只读 · 选 SOUND 后编辑";
    }
    "action unavailable"
}

pub use hit::hit;
pub use layout::{
    drop_target, layer_capacity, library_capacity, param_track, patch_star_rect, snap_time, Layout,
};
#[cfg(test)]
pub use paint::paint;
pub use paint::paint_with_layout;

#[cfg(test)]
mod tests {
    use super::paint::inspector_items;
    use super::*;
    use crate::app::State;

    fn first_entry_row(layout: &Layout) -> (LibraryKey, Rect) {
        layout
            .library_rows
            .iter()
            .find_map(|(row, rect)| match row {
                LibraryListRow::Entry(entry) => Some((entry.key, *rect)),
                LibraryListRow::Folder { .. } => None,
            })
            .expect("entry row")
    }

    #[test]
    fn layout_exposes_library_filters_and_system_menu() {
        let mut state = State::demo();
        state.view.overlays.open(Overlay::Menu);
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.search_rect.size.width > 0.0);
        assert!(layout.category_rect.size.width > 0.0);
        assert!(layout.favorite_rect.size.width > 0.0);
        assert!(layout.menu_rect.is_some());
        assert!(layout
            .menu_items
            .iter()
            .any(|(action, _, _)| *action == Action::Open));
        assert!(layout
            .menu_items
            .iter()
            .any(|(action, _, _)| *action == Action::NewProject));
        assert!(layout
            .menu_items
            .iter()
            .any(|(action, _, _)| *action == Action::ExportWav));
        // Badge counts are cached with the layout so paint never rescans the
        // library on idle frames.
        assert_eq!(layout.library_visible, state.visible_library_count());
        assert_eq!(layout.library_total, state.library_total_count());
        assert_eq!(layout.preset_visible, state.visible_preset_count());
    }

    #[test]
    fn system_menu_paints_and_hit_tests_actions() {
        let mut state = State::demo();
        state.view.overlays.open(Overlay::Menu);
        let layout = Layout::new(&state, state.view.viewport);
        let open = layout
            .menu_items
            .iter()
            .find(|(action, _, _)| *action == Action::Open)
            .map(|(_, _, rect)| *rect)
            .expect("open item");
        assert!(matches!(
            hit(&state, &layout, open.center()),
            Some(Hit::MenuItem(Action::Open))
        ));
        let mut ctx = PaintContext::new();
        paint(&state, &igui_ui::ApproxTextMeasurer, &mut ctx);
        let draw_list = ctx.into_draw_list();
        let labels: Vec<_> = draw_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                igui_render::DrawCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"OPEN…"), "menu labels: {labels:?}");
        assert!(labels.contains(&"EXPORT WAV"));
    }

    #[test]
    fn inspector_parameters_have_chinese_subtitles() {
        let state = State::demo();
        let layout = Layout::new(&state, state.view.viewport);
        let params: Vec<_> = layout
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Param { label, desc, .. } => Some((label, desc)),
                _ => None,
            })
            .collect();
        assert!(!params.is_empty());
        for (label, desc) in params {
            assert!(!desc.trim().is_empty(), "{label} has no description");
            assert!(
                !desc.is_ascii(),
                "{label} description is not Chinese: {desc}"
            );
        }
    }

    #[test]
    fn curve_wave_exposes_editable_canvases() {
        let mut state = State::demo();
        if let Some(layer) = state.layer_mut() {
            layer.wave = Wave::Curve;
            layer.curve = vec![0.0, 1.0, -1.0, 0.0];
        }
        let layout = Layout::new(&state, state.view.viewport);
        let cycle = layout.curve_rect(CurveKind::Cycle).expect("cycle canvas");
        let amp = layout.curve_rect(CurveKind::Amp).expect("amp canvas");
        assert_eq!(
            hit(&state, &layout, cycle.center()),
            Some(Hit::Curve(CurveKind::Cycle))
        );
        assert_eq!(
            hit(&state, &layout, amp.center()),
            Some(Hit::Curve(CurveKind::Amp))
        );
        let values = curve_values(state.layer().unwrap(), CurveKind::Cycle);
        assert_eq!(values, vec![0.0, 1.0, -1.0, 0.0]);
        assert_eq!(curve_index(CurveKind::Cycle, cycle, cycle.left(), 4), 0);
        assert_eq!(curve_index(CurveKind::Cycle, cycle, cycle.right(), 4), 3);
        assert!((curve_value_from_y(CurveKind::Cycle, cycle, cycle.top()) - 1.0).abs() < 0.001);
        assert!((curve_value_from_y(CurveKind::Cycle, cycle, cycle.bottom()) + 1.0).abs() < 0.001);
    }

    #[test]
    fn wave_guide_lists_every_wave_type() {
        let state = State::demo();
        let items = inspector_items(&state);
        let notes: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                RawItem::Note(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for wave in Wave::ALL {
            assert!(
                notes
                    .iter()
                    .any(|text| text.contains(wave.label()) && text.contains(wave.desc())),
                "missing guide for {}",
                wave.label()
            );
        }
    }

    #[test]
    fn inspector_scroll_reaches_the_last_row() {
        let mut state = State::demo();
        state.view.viewport = Size::new(1024.0, 400.0);
        let base = Layout::new(&state, state.view.viewport);
        assert!(base.inspector_max_scroll() > 0.0);
        state.view.inspector_scroll = base.inspector_max_scroll();
        let layout = Layout::new(&state, state.view.viewport);
        let last = layout
            .items
            .last()
            .map(|item| match item {
                Item::Section { rect, .. }
                | Item::Param { rect, .. }
                | Item::Options { rect, .. }
                | Item::Toggle { rect, .. }
                | Item::Curve { rect, .. }
                | Item::Note { rect, .. } => *rect,
            })
            .expect("inspector row");
        assert!((last.bottom() - layout.inspector.bottom()).abs() < 0.01);
    }

    #[test]
    fn compact_layout_stays_inside_a_1024_window() {
        let mut state = State::demo();
        let viewport = Size::new(1024.0, 720.0);
        state.view.viewport = viewport;
        let layout = Layout::new(&state, viewport);
        for button in layout
            .tab_buttons
            .iter()
            .chain(layout.header_buttons.iter())
        {
            assert!(
                button.rect.left() >= 0.0 && button.rect.right() <= viewport.width,
                "{:?} at {:?} overflows {}",
                button.action,
                button.rect,
                viewport.width
            );
        }
        assert!(layout.left.right() <= layout.header.left() + 0.001);
        assert!(layout.header.right() <= layout.right.left() + 0.001);
    }

    #[test]
    fn inspector_controls_keep_panel_margins() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        let layout = Layout::new(&state, state.view.viewport);
        let panel = layout.right;
        for item in &layout.items {
            let rect = match item {
                Item::Section { rect, .. }
                | Item::Param { rect, .. }
                | Item::Options { rect, .. }
                | Item::Toggle { rect, .. }
                | Item::Curve { rect, .. }
                | Item::Note { rect, .. } => *rect,
            };
            assert!(
                rect.left() >= panel.left() + 8.0 && rect.right() <= panel.right() - 8.0,
                "inspector row {:?} touches the panel edge of {:?}",
                rect,
                panel
            );
        }
        for item in &layout.items {
            if let Item::Options { buttons, .. } = item {
                for button in buttons {
                    assert!(
                        button.rect.right() <= panel.right() - 8.0,
                        "inspector button {} touches the panel edge",
                        button.label
                    );
                }
            }
        }
    }

    #[test]
    fn zoom_and_scroll_map_times_both_ways() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        let total = state.timeline_span();
        state.view.timeline_zoom = 2.0;
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.timeline_visible < total + 0.08);
        let x = layout.time_x(0.2);
        assert!((layout.x_time(x) - 0.2).abs() < 0.002);
        state.view.timeline_offset = 99.0;
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.timeline_offset + layout.timeline_visible <= total + 0.09);
        assert!((layout.x_time(layout.time_x(0.3)) - 0.3).abs() < 0.002);
    }

    #[test]
    fn library_scrollbar_reserves_the_right_gutter() {
        let mut state = State::demo();
        let layout = Layout::new(&state, state.view.viewport);
        assert_eq!(layout.library_scrollbar.size.height, 0.0);
        let patch = state.doc.project.patches[0].clone();
        for _ in 0..40 {
            state.doc.project.patches.push(patch.clone());
        }
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.library_scrollbar.size.height > 0.0);
        assert!(layout.library_max_start > 0);
        let (_, row) = layout.library_rows[0];
        assert!(patch_star_rect(row).right() < layout.library_scrollbar.left());
        assert_eq!(
            hit(&state, &layout, layout.library_scrollbar.center()),
            Some(Hit::LibraryScrollbar)
        );
    }

    #[test]
    fn empty_project_paints_open_hints() {
        let state = State::empty();
        let mut ctx = PaintContext::new();
        paint(&state, &igui_ui::ApproxTextMeasurer, &mut ctx);
        let draw_list = ctx.into_draw_list();
        let labels: Vec<_> = draw_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                igui_render::DrawCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"NO PROJECT"));
        assert!(labels.iter().any(|label| label.contains("OPEN A PROJECT")));
    }

    #[test]
    fn preset_filter_swaps_rows_and_hit_targets() {
        let mut state = State::demo();
        state.view.kind_filter = Some(LibraryKind::Preset);
        let layout = Layout::new(&state, state.view.viewport);
        assert!(!layout.library_rows.is_empty());
        let (key, row) = first_entry_row(&layout);
        assert_eq!(key.kind(), LibraryKind::Preset);
        let body = Vec2::new(row.left() + 40.0, row.center().y);
        assert_eq!(hit(&state, &layout, body), Some(Hit::LibraryRow(key)));
        assert_eq!(
            hit(&state, &layout, patch_star_rect(row).center()),
            Some(Hit::LibraryFavorite(key))
        );
        assert_eq!(
            hit(&state, &layout, layout.kind_rect.center()),
            Some(Hit::KindFilter)
        );
    }

    #[test]
    fn empty_presets_offer_an_install_action() {
        let mut state = State::empty();
        state.doc.root = Some(std::path::PathBuf::from("/tmp/sfx-studio-install"));
        state.view.kind_filter = Some(LibraryKind::Preset);
        let layout = Layout::new(&state, state.view.viewport);
        let button = layout.install_presets.expect("install button");
        assert_eq!(
            hit(&state, &layout, button.center()),
            Some(Hit::InstallPresets)
        );

        state.doc.root = None;
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.install_presets.is_none());
    }

    #[test]
    fn preset_mode_inspector_describes_the_selected_preset() {
        let mut state = State::demo();
        state.select_preset(1);
        let name = state.preset(1).expect("preset").name.clone();
        let items = inspector_items(&state);
        assert!(items
            .iter()
            .any(|item| matches!(item, RawItem::Note(text) if text.contains(name.as_str()))));
        assert!(items
            .iter()
            .any(|item| matches!(item, RawItem::Section(text) if text == "READ-ONLY")));
        assert!(!items
            .iter()
            .any(|item| matches!(item, RawItem::Param { .. })));
    }

    #[test]
    fn shortcuts_and_menu_labels_are_centralized() {
        use igui_core::Modifiers;
        let cmd = Modifiers {
            meta: true,
            ..Modifiers::NONE
        };
        let cmd_shift = Modifiers {
            meta: true,
            shift: true,
            ..Modifiers::NONE
        };
        let plain = Modifiers::NONE;
        assert_eq!(
            shortcut_action(Key::Character('s'), cmd),
            Some(Action::Save)
        );
        assert_eq!(
            shortcut_action(Key::Character('z'), cmd),
            Some(Action::Undo)
        );
        assert_eq!(
            shortcut_action(Key::Character('z'), cmd_shift),
            Some(Action::Redo)
        );
        assert_eq!(
            shortcut_action(Key::Character('r'), plain),
            Some(Action::Randomize)
        );
        assert_eq!(
            shortcut_action(Key::Character('R'), plain),
            Some(Action::Randomize)
        );
        // F has mode-specific behavior (preset vs sound) and stays explicit.
        assert_eq!(shortcut_action(Key::Character('f'), plain), None);
        assert_eq!(action_meta(Action::Bake).label, "BAKE GODOT");
        assert_eq!(action_meta(Action::DeleteLayer).label, "DEL");
        for entry in MENU_ENTRIES {
            if let MenuEntry::Item(action) = entry {
                assert_eq!(action_meta(*action).label, action_label(*action));
                assert!(!action_label(*action).is_empty());
            }
        }
    }

    #[test]
    fn action_meta_is_the_single_scope_and_undo_source() {
        use ActionScope::{AssetEdit, Document, ProjectEdit, Transport, View};
        assert_eq!(action_meta(Action::Bake).scope, Document);
        assert_eq!(action_meta(Action::RenamePreset).scope, AssetEdit);
        assert_eq!(action_meta(Action::Randomize).scope, ProjectEdit);
        assert_eq!(action_meta(Action::Randomize).undo, Some("randomize"));
        assert_eq!(action_meta(Action::NewPatch).undo, None);
        assert_eq!(action_meta(Action::LoadSample).undo, None);
        assert_eq!(action_meta(Action::Play).scope, Transport);
        assert_eq!(action_meta(Action::TabMusic).scope, View);
        assert!(is_project_edit(Action::AddLayer));
        assert!(!is_project_edit(Action::Bake));
    }

    #[test]
    fn preset_focus_disables_project_edits_but_not_document_commands() {
        let mut state = State::demo();
        state.select_preset(0);
        for action in [
            Action::NewPatch,
            Action::RenamePatch,
            Action::DeletePatch,
            Action::DuplicatePatch,
            Action::Randomize,
            Action::Vary,
            Action::Favorite,
            Action::SavePreset,
            Action::AddLayer,
            Action::DuplicateLayer,
            Action::DeleteLayer,
            Action::AddClip,
            Action::DeleteClip,
            Action::LoadSample,
            Action::ToggleSampleLoop,
            Action::ToggleSampleReverse,
            Action::UpgradeDsp,
        ] {
            assert!(!action_available(&state, action), "{action:?}");
        }
        // Document-level commands are not project edits: baking/exporting the
        // project while auditioning a preset is valid.
        assert!(action_available(&state, Action::Save));
        assert!(action_available(&state, Action::Open));
        assert!(action_available(&state, Action::Bake));
        assert!(action_available(&state, Action::Tres));
        assert!(action_available(&state, Action::InstallPresets));

        // WAV export targets the selected preset.
        assert!(action_available(&state, Action::ExportWav));
        assert!(action_available(&state, Action::RenamePreset));
        assert!(action_available(&state, Action::DeletePreset));
        assert!(action_available(&state, Action::RestorePreset));

        state.focus_document();
        assert!(action_available(&state, Action::ExportWav));
        assert!(!action_available(&state, Action::RenamePreset));
        assert!(!action_available(&state, Action::DeletePreset));
        assert!(!action_available(&state, Action::RestorePreset));
    }

    #[test]
    fn audio_and_music_lifecycle_actions_need_a_selection() {
        let mut state = State::demo();
        for action in [
            Action::RenameAudio,
            Action::DeleteAudio,
            Action::RenameMusic,
            Action::DeleteMusic,
        ] {
            assert!(!action_available(&state, action), "{action:?}");
            assert!(!action_unavailable_message(&state, action).is_empty());
        }
        assert!(!action_available(&state, Action::RestoreAudio));
        state.doc.root = Some(std::path::PathBuf::from("/tmp/sfx-studio-audio"));
        assert!(action_available(&state, Action::RestoreAudio));
        assert!(action_available(&state, Action::RestoreMusic));
    }

    #[test]
    fn audio_focus_does_not_disable_document_commands() {
        let mut state = State::demo();
        state.view.selected_library = Some(LibraryKey::Audio(0));

        for action in [
            Action::Bake,
            Action::Tres,
            Action::SavePreset,
            Action::Randomize,
            Action::AddLayer,
            Action::DeleteLayer,
            Action::Favorite,
        ] {
            assert!(action_available(&state, action), "{action:?}");
        }
        // There is no patch target for an AUDIO row.
        assert!(!action_available(&state, Action::ExportWav));
        assert!(action_unavailable_message(&state, Action::ExportWav).contains("AUDIO/MUSIC"));
        assert!(!action_available(&state, Action::RenamePreset));
        assert!(!action_available(&state, Action::DeletePreset));

        let patch_id = state.patch().expect("patch").id;
        state.view.selected_library = Some(LibraryKey::Sound(patch_id));
        assert!(action_available(&state, Action::ExportWav));
    }

    #[test]
    fn preset_mode_paints_rows_and_the_drag_ghost() {
        let mut state = State::demo();
        let layout = Layout::new(&state, state.view.viewport);
        let row = layout.layer_rows[0].1;
        state.view.cursor = Vec2::new(layout.time_x(0.3), row.center().y);
        state.view.drag = Drag::LibraryItem {
            key: LibraryKey::Preset(0),
            start: Vec2::ZERO,
            moved: true,
        };
        let mut ctx = PaintContext::new();
        paint(&state, &igui_ui::ApproxTextMeasurer, &mut ctx);
        let draw_list = ctx.into_draw_list();
        let labels: Vec<_> = draw_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                igui_render::DrawCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"PRE"));
        assert!(labels.iter().any(|label| label.contains("INSERT")));
    }

    #[test]
    fn drop_target_maps_timeline_rows_and_the_left_panel() {
        let state = State::demo();
        let layout = Layout::new(&state, state.view.viewport);
        let row = layout.layer_rows[0].1;
        let pos = Vec2::new(layout.time_x(0.2), row.center().y);
        match drop_target(&state, &layout, pos) {
            Some(DropTarget::Timeline { layer, time }) => {
                assert_eq!(layer, 0);
                assert!((time - 0.2).abs() < 0.01);
            }
            other => panic!("expected timeline target, got {other:?}"),
        }
        let below = Vec2::new(layout.timeline.center().x, layout.timeline.bottom() - 2.0);
        match drop_target(&state, &layout, below) {
            Some(DropTarget::Timeline { layer, .. }) => {
                assert_eq!(layer, state.patch().unwrap().layers.len());
            }
            other => panic!("expected append target, got {other:?}"),
        }
        let left = layout.left.center();
        assert_eq!(
            drop_target(&state, &layout, left),
            Some(DropTarget::ProjectLibrary)
        );
    }

    #[test]
    fn category_menu_lists_and_hits_every_category() {
        let mut state = State::demo();
        state.view.overlays.open(Overlay::CategoryFilter);
        let layout = Layout::new(&state, state.view.viewport);
        let menu = layout.category_menu.expect("menu open");
        assert_eq!(layout.category_items.len(), Category::ALL.len() + 1);
        for (category, rect) in &layout.category_items {
            assert_eq!(
                hit(&state, &layout, rect.center()),
                Some(Hit::CategoryPick(*category))
            );
        }
        assert_eq!(
            hit(
                &state,
                &layout,
                Vec2::new(menu.right() - 1.0, menu.bottom() - 1.0)
            ),
            Some(Hit::CategoryMenu)
        );
        assert_eq!(
            hit(&state, &layout, layout.category_rect.center()),
            Some(Hit::CategoryFilter)
        );
    }

    #[test]
    fn sample_layer_exposes_load_pitch_and_loop() {
        let mut state = State::demo();
        if let Some(layer) = state.layer_mut() {
            layer.wave = Wave::Sample;
            layer.sample_path = "audio/sfx/samples/x.wav".into();
        }
        let items = inspector_items(&state);
        let mut has_load = false;
        let mut has_loop = false;
        let mut has_pitch = false;
        for item in &items {
            match item {
                RawItem::Options { options, .. } => {
                    has_load |= options
                        .iter()
                        .any(|(_, action, _)| *action == Action::LoadSample);
                }
                RawItem::Toggle { action, .. } => {
                    has_loop |= *action == Action::ToggleSampleLoop;
                }
                RawItem::Param { label, .. } => has_pitch |= label == "PITCH",
                _ => {}
            }
        }
        assert!(has_load && has_loop && has_pitch);
    }

    #[test]
    fn sample_inspector_exposes_start_and_reverse() {
        let mut state = State::demo();
        state.layer_mut().unwrap().wave = Wave::Sample;
        state.layer_mut().unwrap().sample_reverse = true;
        let items = inspector_items(&state);
        let mut start = false;
        let mut reverse = false;
        for item in &items {
            match item {
                RawItem::Param { label, .. } => start |= label == "START",
                RawItem::Toggle { action, value, .. } if *action == Action::ToggleSampleReverse => {
                    reverse = true;
                    assert!(*value);
                }
                _ => {}
            }
        }
        assert!(start && reverse);
    }

    #[test]
    fn music_inspector_follows_the_registry() {
        let mut state = State::demo();
        state.view.tab = Tab::Music;
        let items = inspector_items(&state);
        let mut sections = Vec::new();
        let mut params = Vec::new();
        for item in &items {
            match item {
                RawItem::Section(text) => sections.push(text.clone()),
                RawItem::Param { label, .. } => params.push(label.clone()),
                _ => {}
            }
        }
        assert_eq!(
            sections,
            vec![
                "MUSIC",
                "INSTRUMENTS",
                "MIX",
                "DEMO TRACKS",
                "HARMONY",
                "PATTERN"
            ]
        );
        assert!(params.contains(&"BPM".to_string()));
        assert!(params.contains(&"SEED".to_string()));
        let demo_tracks: Vec<_> = items
            .iter()
            .flat_map(|item| match item {
                RawItem::Options { options, .. } => options.as_slice(),
                _ => &[],
            })
            .filter_map(|(label, action, _)| match action {
                Action::DemoTrack(_) => Some(label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(demo_tracks, vec!["NEON DRIVE", "BOSS THEME", "STAR MENU"]);
    }

    #[test]
    fn inspector_renders_the_layer_registry() {
        let mut state = State::demo();
        state.layer_mut().unwrap().wave = Wave::Sine;
        let items = inspector_items(&state);
        let mut sections = Vec::new();
        let mut params = Vec::new();
        for item in &items {
            match item {
                RawItem::Section(text) => sections.push(text.trim().to_string()),
                RawItem::Param { label, .. } => params.push(label.clone()),
                _ => {}
            }
        }
        assert!(sections.contains(&"PITCH".to_string()));
        assert!(sections.contains(&"AMP ENV".to_string()));
        assert!(sections.contains(&"FILTER".to_string()));
        assert!(params.contains(&"HARSH".to_string()));
        assert!(!params.contains(&"DUTY".to_string()));
        let harsh = params.iter().position(|label| label == "HARSH").unwrap();
        let vibrato = params.iter().position(|label| label == "VIB RATE").unwrap();
        assert!(harsh < vibrato, "registry order must drive the rows");
    }

    #[test]
    fn inspector_exposes_pan_and_the_space_block() {
        let mut state = State::demo();
        let items = inspector_items(&state);
        let mut sections = Vec::new();
        let mut params = Vec::new();
        let mut upgrade = false;
        for item in &items {
            match item {
                RawItem::Section(text) => sections.push(text.trim().to_string()),
                RawItem::Param { label, .. } => params.push(label.clone()),
                RawItem::Options { options, .. } => {
                    upgrade |= options
                        .iter()
                        .any(|(_, action, _)| *action == Action::UpgradeDsp);
                }
                _ => {}
            }
        }
        assert!(sections.contains(&"MIX".to_string()));
        assert!(sections.contains(&"SPACE".to_string()));
        assert!(params.contains(&"PAN".to_string()));
        assert!(params.contains(&"SPACE".to_string()));
        assert!(params.contains(&"ROOM".to_string()));
        // Demo patches ship on v3, so no upgrade prompt.
        assert!(!upgrade);

        // A legacy patch gets the explicit opt-in instead.
        state.patch_mut().unwrap().dsp_version = 2;
        let items = inspector_items(&state);
        assert!(items.iter().any(|item| matches!(
            item,
            RawItem::Options { options, .. }
                if options
                    .iter()
                    .any(|(_, action, _)| *action == Action::UpgradeDsp)
        )));
    }

    #[test]
    fn favorite_star_hit_is_split_from_the_row() {
        let state = State::demo();
        let layout = Layout::new(&state, state.view.viewport);
        let (key, row) = first_entry_row(&layout);
        assert_eq!(
            hit(&state, &layout, patch_star_rect(row).center()),
            Some(Hit::LibraryFavorite(key))
        );
        let body = Vec2::new(row.left() + 40.0, row.center().y);
        assert_eq!(hit(&state, &layout, body), Some(Hit::LibraryRow(key)));
    }

    #[test]
    fn pinned_timeline_span_keeps_the_scale_stable() {
        let mut state = State::demo();
        let span = state.timeline_span();
        let base = Layout::new(&state, state.view.viewport).timeline_scale;
        state.view.timeline_pin = Some(span);
        let layer = state.layer_mut().unwrap();
        layer.ensure_clips();
        layer.clips[0].duration += 1.5;
        let pinned = Layout::new(&state, state.view.viewport).timeline_scale;
        assert!(
            (pinned - base).abs() < 0.001,
            "pinned scale moved from {base} to {pinned}"
        );
        state.view.timeline_pin = None;
        let refit = Layout::new(&state, state.view.viewport).timeline_scale;
        assert!(refit < pinned, "release should refit the wider timeline");
    }

    #[test]
    fn timeline_grows_with_layers_and_gives_space_to_the_waveform() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        let few = Layout::new(&state, state.view.viewport);
        assert!(
            few.timeline.size.height <= 200.0,
            "3 layers should not claim the whole column"
        );
        assert!(few.waveform.size.height > 128.0);
        let layer = state.doc.project.patches[0].layers[0].clone();
        for _ in 0..30 {
            state.doc.project.patches[0].layers.push(layer.clone());
        }
        let many = Layout::new(&state, state.view.viewport);
        assert!(many.timeline.size.height > few.timeline.size.height);
        assert!(many.waveform.size.height < few.waveform.size.height);
        assert!(many.layer_rows.len() <= layer_capacity(state.view.viewport, 31));
        state.view.tab = Tab::Music;
        let music = Layout::new(&state, state.view.viewport);
        assert!(music.timeline.size.height < many.timeline.size.height);
        assert!(music.pattern_grid.size.height <= music.timeline.size.height);
    }

    #[test]
    fn layout_exposes_undo_and_clip_fields() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.layer_mut().unwrap().ensure_clips();
        state.mark_dirty();
        state.select_clip(0);
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout
            .header_buttons
            .iter()
            .any(|b| b.action == Action::Undo));
        assert!(layout
            .header_buttons
            .iter()
            .any(|b| b.action == Action::Redo));
        assert!(layout.items.iter().any(|item| matches!(
            item,
            Item::Param {
                hit: Hit::ClipField(_),
                ..
            }
        )));
    }

    #[test]
    fn automation_lane_adds_and_hits_points() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.layer_mut().unwrap().ensure_clips();
        state.mark_dirty();
        state.select_clip(0);
        state.layer_mut().unwrap().clips[0].add_point(AutomationKind::Gain, 0.05, 1.5);
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.automation.size.height > 0.0);
        let clip = &state.layer().unwrap().clips[0];
        let point = text_point(&layout, clip, 0.05, 1.5);
        assert!(matches!(
            hit(&state, &layout, point),
            Some(Hit::AutomationPoint(0))
        ));
        let lane = layout.automation_xy(0.02, 0.5, AutomationKind::Gain, clip.duration);
        assert!(matches!(
            hit(&state, &layout, lane),
            Some(Hit::AutomationLane { .. })
        ));
    }

    fn text_point(layout: &Layout, clip: &crate::model::Clip, time: f32, value: f32) -> Vec2 {
        layout.automation_xy(time, value, AutomationKind::Gain, clip.duration)
    }

    #[test]
    fn clip_blocks_are_hit_testable() {
        let mut state = State::demo();
        state.view.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.layer_mut().unwrap().ensure_clips();
        let layout = Layout::new(&state, state.view.viewport);
        let clip = &state.layer().unwrap().clips[0];
        let x = layout.time_x(clip.start + clip.duration * 0.5);
        let row = layout
            .layer_rows
            .iter()
            .find(|(index, _)| {
                state
                    .patch()
                    .and_then(|patch| patch.layers.get(*index))
                    .map(|layer| layer.id)
                    == state.view.selected_layer
            })
            .map(|(_, rect)| *rect)
            .unwrap();
        let hit = hit(&state, &layout, Vec2::new(x, row.center().y));
        assert!(matches!(hit, Some(Hit::Timeline { clip: Some(0), .. })));
    }
    #[test]
    fn kind_menu_lists_every_resource_and_clear_is_clickable() {
        let mut state = State::demo();
        state.view.overlays.open(Overlay::KindFilter);
        let layout = Layout::new(&state, state.view.viewport);
        assert_eq!(layout.kind_items.len(), LibraryKind::ALL.len() + 1);
        for (kind, rect) in &layout.kind_items {
            assert_eq!(
                hit(&state, &layout, rect.center()),
                Some(Hit::KindPick(*kind))
            );
        }
        assert_eq!(
            hit(&state, &layout, layout.kind_rect.center()),
            Some(Hit::KindFilter)
        );
        assert_eq!(
            hit(&state, &layout, layout.clear_rect.center()),
            Some(Hit::ClearFilters)
        );
    }
    #[test]
    fn folder_headers_are_hit_testable() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-ui-folders-{}-{stamp}",
            std::process::id()
        ));
        let dir = root.join("presets/impact");
        std::fs::create_dir_all(&dir).unwrap();
        let patch = crate::model::Patch::new_layer("boom", crate::model::Wave::Noise);
        std::fs::write(
            dir.join("boom.json"),
            serde_json::to_string(&patch).unwrap(),
        )
        .unwrap();
        let mut state = State::demo();
        state.doc.root = Some(root.clone());
        state.presets.load_project(&root);
        let layout = Layout::new(&state, state.view.viewport);
        let (index, rect) = layout
            .library_rows
            .iter()
            .enumerate()
            .find_map(|(index, (row, rect))| match row {
                LibraryListRow::Folder { key, .. } if key == "PRE:impact" => Some((index, *rect)),
                _ => None,
            })
            .expect("impact folder");
        assert_eq!(
            hit(&state, &layout, rect.center()),
            Some(Hit::LibraryFolder(index))
        );
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn stale_library_scroll_cannot_hide_every_row() {
        let mut state = State::demo();
        state.view.library_scroll = 999;
        let layout = Layout::new(&state, state.view.viewport);
        assert!(!layout.library_rows.is_empty());
        assert!(matches!(layout.library_rows[0].0, LibraryListRow::Entry(_)));
    }
}
