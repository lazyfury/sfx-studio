//! Immediate-mode layout, paint and hit-testing for the compact studio shell.

use igui_core::{Color, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_ui::TextMeasurer;

use crate::app::{Drag, LibraryMode, State, Tab, TimelineDrag};
use crate::icons::{self, Icon};
use crate::model::{
    layer_sections, music_sections, AutomationKind, Category, ClipField, EnvShape, FilterMode,
    LayerField, LayerRowSpec, MusicField, MusicRowSpec, MusicWave, PatchField, ToneEnv, Wave,
    CLIP_FIELDS,
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
/// Time grid used by clip/automation drags (5 ms).
pub const CLIP_SNAP: f32 = 0.005;

/// Shared layout metrics. `app.rs` uses these helpers instead of re-deriving
/// row capacity, so a metrics change cannot silently desync scrolling.
pub const LIBRARY_TOP: f32 = 86.0;
pub const LIBRARY_ROW_H: f32 = 24.0;
pub const LIBRARY_END_GAP: f32 = 8.0;
pub const MUSIC_ROW_OFFSET: f32 = 28.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    NewProject,
    Open,
    Save,
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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    Action(Action),
    PatchRow(usize),
    PatchFavorite(usize),
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
    MenuToggle,
    MenuItem(Action),
    MenuPanel,
    CategoryFilter,
    CategoryPick(Option<Category>),
    CategoryMenu,
    FavoriteFilter,
    LibraryMode(LibraryMode),
    PresetRow(usize),
    PresetFavorite(usize),
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
        Action::ExportWav => "导出 48 kHz WAV · Export（⌘E）",
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
        Action::SetWave(_) => "切换波形 · Wave",
        Action::SetFilter(_) => "切换滤波器 · Filter",
        Action::SetEnv(_) => "切换包络 · Envelope",
        Action::SetToneEnv(_) => "音调包络 · Tone envelope",
        Action::AutomationLane(_) => "自动化轨道 · Automation lane",
        Action::BassWave(_) => "贝斯波形 · Bass wave",
        Action::ArpWave(_) => "琶音波形 · Arp wave",
        Action::TogglePatterns => "自定义 16 步节奏 · Pattern",
        Action::ChordPreset(_) => "和弦预设 · Chord preset",
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
/// the selected row and the command target cannot silently disagree.
pub fn action_available(state: &State, action: Action) -> bool {
    if state.view.library_mode != LibraryMode::Presets {
        return true;
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
            | Action::ExportWav
            | Action::Bake
            | Action::Tres
    )
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
        let (_, row) = layout.patch_rows[0];
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
    fn preset_mode_swaps_rows_and_hit_targets() {
        let mut state = State::demo();
        state.view.library_mode = LibraryMode::Presets;
        let layout = Layout::new(&state, state.view.viewport);
        assert!(layout.patch_rows.is_empty());
        assert!(!layout.preset_rows.is_empty());
        let (index, row) = layout.preset_rows[0];
        let body = Vec2::new(row.left() + 40.0, row.center().y);
        assert_eq!(hit(&state, &layout, body), Some(Hit::PresetRow(index)));
        assert_eq!(
            hit(&state, &layout, patch_star_rect(row).center()),
            Some(Hit::PresetFavorite(index))
        );
        let (_, project_tab) = layout
            .library_mode_buttons
            .iter()
            .find(|(mode, _)| *mode == LibraryMode::Project)
            .expect("project tab");
        assert_eq!(
            hit(&state, &layout, project_tab.center()),
            Some(Hit::LibraryMode(LibraryMode::Project))
        );
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
            Action::ExportWav,
            Action::Bake,
            Action::Tres,
        ] {
            assert!(!action_available(&state, action), "{action:?}");
        }
        assert!(action_available(&state, Action::Save));
        assert!(action_available(&state, Action::Open));
        assert!(action_available(&state, Action::AddLayer));
        state.view.library_mode = LibraryMode::Project;
        assert!(action_available(&state, Action::ExportWav));
    }

    #[test]
    fn preset_mode_paints_rows_and_the_drag_ghost() {
        let mut state = State::demo();
        state.view.library_mode = LibraryMode::Presets;
        let layout = Layout::new(&state, state.view.viewport);
        let row = layout.layer_rows[0].1;
        state.view.cursor = Vec2::new(layout.time_x(0.3), row.center().y);
        state.view.drag = Drag::LibraryItem {
            index: 0,
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
        assert!(labels.contains(&"PROJECT"));
        assert!(labels.contains(&"PRESETS"));
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
            vec!["MUSIC", "INSTRUMENTS", "MIX", "HARMONY", "PATTERN"]
        );
        assert!(params.contains(&"BPM".to_string()));
        assert!(params.contains(&"SEED".to_string()));
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
        let (index, row) = layout.patch_rows[0];
        assert_eq!(
            hit(&state, &layout, patch_star_rect(row).center()),
            Some(Hit::PatchFavorite(index))
        );
        let body = Vec2::new(row.left() + 40.0, row.center().y);
        assert_eq!(hit(&state, &layout, body), Some(Hit::PatchRow(index)));
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
}
