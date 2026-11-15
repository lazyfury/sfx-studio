//! Immediate-mode layout, paint and hit-testing for the compact studio shell.

use igui_core::{Color, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_ui::TextMeasurer;

use crate::app::{Drag, LibraryMode, State, Tab, TimelineDrag};
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
    Item(&'static str, Action),
}

/// Top-bar actions are grouped into one system menu instead of a row of
/// buttons. Keyboard shortcuts remain available.
pub const MENU_ENTRIES: &[MenuEntry] = &[
    MenuEntry::Section("FILE"),
    MenuEntry::Item("NEW PROJECT…", Action::NewProject),
    MenuEntry::Item("OPEN…", Action::Open),
    MenuEntry::Item("SAVE", Action::Save),
    MenuEntry::Item("NEW SOUND", Action::NewPatch),
    MenuEntry::Item("RENAME", Action::RenamePatch),
    MenuEntry::Item("DUPLICATE", Action::DuplicatePatch),
    MenuEntry::Item("DELETE", Action::DeletePatch),
    MenuEntry::Section("SOUND"),
    MenuEntry::Item("RANDOMIZE", Action::Randomize),
    MenuEntry::Item("VARY A/B/C", Action::Vary),
    MenuEntry::Item("FAVORITE", Action::Favorite),
    MenuEntry::Section("PRESETS"),
    MenuEntry::Item("SAVE AS PRESET", Action::SavePreset),
    MenuEntry::Item("RENAME PRESET", Action::RenamePreset),
    MenuEntry::Item("DELETE PRESET", Action::DeletePreset),
    MenuEntry::Item("RESTORE LAST PRESET", Action::RestorePreset),
    MenuEntry::Section("EXPORT"),
    MenuEntry::Item("EXPORT WAV", Action::ExportWav),
    MenuEntry::Item("BAKE GODOT", Action::Bake),
    MenuEntry::Item("EXPORT .TRES", Action::Tres),
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

fn action_icon(action: Action) -> Option<Icon> {
    match action {
        Action::NewProject => Some(Icon::FilePlus),
        Action::Open => Some(Icon::FolderOpen),
        Action::Save => Some(Icon::Save),
        Action::InstallPresets | Action::SavePreset => Some(Icon::FilePlus),
        Action::RenamePreset => Some(Icon::Pencil),
        Action::DeletePreset => Some(Icon::Trash),
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

/// Project-only commands are disabled while the PRESETS source is active so
/// the selected row and the command target cannot silently disagree. WAV
/// export stays available, but it needs a selected preset to write.
pub fn action_available(state: &State, action: Action) -> bool {
    let selected_preset = state
        .view
        .selected_preset
        .and_then(|index| state.presets.get(index))
        .is_some();
    if matches!(action, Action::RenamePreset | Action::DeletePreset) {
        return state.view.library_mode == LibraryMode::Presets && selected_preset;
    }
    if action == Action::RestorePreset {
        return state.view.library_mode == LibraryMode::Presets;
    }
    let focused_read_only = matches!(
        state.selected_library_kind(),
        Some(crate::library::LibraryKind::Audio | crate::library::LibraryKind::Music)
    );
    if state.view.library_mode != LibraryMode::Presets && !focused_read_only {
        return true;
    }
    if action == Action::ExportWav {
        return matches!(
            state.view.selected_library,
            Some(crate::library::LibraryKey::Preset(_))
        );
    }
    !matches!(
        action,
        Action::NewPatch
            | Action::RenamePatch
            | Action::DeletePatch
            | Action::DuplicatePatch
            | Action::Randomize
            | Action::Vary
            | Action::Favorite
            | Action::SavePreset
            | Action::Bake
            | Action::Tres
            | Action::AddLayer
            | Action::DuplicateLayer
            | Action::DeleteLayer
            | Action::AddClip
            | Action::DeleteClip
            | Action::LoadSample
            | Action::ToggleSampleLoop
            | Action::ToggleSampleReverse
            | Action::UpgradeDsp
    )
}

/// Human-readable reason for a greyed-out action, used by the tooltip and the
/// status line. Keeps the PRESETS read-only boundary separate from the benign
/// "select a preset first" case.
pub fn action_unavailable_message(state: &State, action: Action) -> &'static str {
    if matches!(
        action,
        Action::RenamePreset | Action::DeletePreset | Action::RestorePreset
    ) && state.view.library_mode != LibraryMode::Presets
    {
        return "切到 PRESETS 后管理预设 · switch to PRESETS";
    }
    if matches!(
        state.selected_library_kind(),
        Some(crate::library::LibraryKind::Audio | crate::library::LibraryKind::Music)
    ) && state.view.library_mode != LibraryMode::Presets
    {
        return "AUDIO/MUSIC 只读 · 选 SOUND 编辑";
    }
    if state.view.library_mode == LibraryMode::Presets {
        if matches!(
            action,
            Action::ExportWav | Action::RenamePreset | Action::DeletePreset
        ) {
            return "先选中一个预设 · select a preset";
        }
        if action == Action::SavePreset {
            return "切回 PROJECT 保存当前音效为预设 · switch to PROJECT";
        }
        return "PRESETS 只读 · 切回 PROJECT 编辑";
    }
    "action unavailable"
}

pub use hit::hit;
pub use layout::{
    drop_target, layer_capacity, library_capacity, param_track, patch_star_rect, snap_time, Layout,
};
pub use paint::paint;

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
        state.view.menu_open = true;
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
    }

    #[test]
    fn system_menu_paints_and_hit_tests_actions() {
        let mut state = State::demo();
        state.view.menu_open = true;
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
        state.view.library_mode = LibraryMode::Presets;
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
        state.view.library_mode = LibraryMode::Presets;
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
        state.view.library_mode = LibraryMode::Presets;
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
    fn preset_mode_disables_project_only_actions() {
        let mut state = State::demo();
        state.view.library_mode = LibraryMode::Presets;
        for action in [
            Action::NewPatch,
            Action::RenamePatch,
            Action::DeletePatch,
            Action::DuplicatePatch,
            Action::Randomize,
            Action::Vary,
            Action::Favorite,
            Action::SavePreset,
            Action::Bake,
            Action::Tres,
            Action::AddLayer,
            Action::DuplicateLayer,
            Action::DeleteLayer,
            Action::AddClip,
            Action::DeleteClip,
            Action::LoadSample,
            Action::ToggleSampleLoop,
            Action::ToggleSampleReverse,
            Action::UpgradeDsp,
            Action::RenamePreset,
            Action::DeletePreset,
        ] {
            assert!(!action_available(&state, action), "{action:?}");
        }
        assert!(action_available(&state, Action::Save));
        assert!(action_available(&state, Action::Open));
        assert!(action_available(&state, Action::InstallPresets));

        // WAV export is the one project-style command that stays useful in
        // PRESETS mode; it targets the selected preset and needs a selection.
        assert!(!action_available(&state, Action::ExportWav));
        assert!(action_unavailable_message(&state, Action::ExportWav).contains("先选中一个预设"));
        state.select_preset(0);
        assert!(action_available(&state, Action::ExportWav));
        assert!(action_available(&state, Action::RenamePreset));
        assert!(action_available(&state, Action::DeletePreset));
        assert!(action_available(&state, Action::RestorePreset));

        state.view.library_mode = LibraryMode::Project;
        assert!(action_available(&state, Action::ExportWav));
        assert!(!action_available(&state, Action::RenamePreset));
        assert!(!action_available(&state, Action::DeletePreset));
        assert!(!action_available(&state, Action::RestorePreset));
    }

    #[test]
    fn preset_mode_paints_rows_and_the_drag_ghost() {
        let mut state = State::demo();
        state.view.library_mode = LibraryMode::Presets;
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
        state.view.category_menu_open = true;
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
        state.view.kind_menu_open = true;
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
