//! Immediate-mode layout, paint and hit-testing for the compact studio shell.

use igui_core::{Color, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_ui::TextMeasurer;

use crate::app::{Drag, State, Tab, TimelineDrag};
use crate::icons::{self, Icon};
use crate::model::{
    layer_sections, AutomationKind, Category, ClipField, EnvShape, FilterMode, LayerField,
    LayerRowSpec, MusicField, MusicWave, ToneEnv, Wave,
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

pub fn body_height(viewport: Size) -> f32 {
    (viewport.height - theme::TOP_H - theme::STATUS_H).max(1.0)
}

pub fn library_capacity(viewport: Size) -> usize {
    let list_top = theme::TOP_H + LIBRARY_TOP;
    let music_top = theme::TOP_H + body_height(viewport) - MUSIC_ROW_OFFSET;
    (((music_top - LIBRARY_END_GAP - list_top) / LIBRARY_ROW_H).max(1.0) as usize).max(1)
}

pub fn timeline_height(viewport: Size) -> f32 {
    (body_height(viewport) - theme::HEADER_H - theme::AUTOMATION_H - 152.0).max(180.0)
}

/// Timeline height for the current layer count: grows with content but never
/// eats the space reserved for the waveform. The floor keeps three tracks
/// visible; any extra height goes to the waveform.
pub fn timeline_height_for(viewport: Size, layers: usize) -> f32 {
    let min_h = RULER_H + 3.0 * TRACK_H + 8.0;
    let wanted = RULER_H + layers.max(1) as f32 * TRACK_H + 8.0;
    wanted.clamp(min_h, timeline_height(viewport))
}

pub fn layer_capacity(viewport: Size, layers: usize) -> usize {
    (((timeline_height_for(viewport, layers) - RULER_H) / TRACK_H).max(1.0) as usize).max(1)
}

/// Snaps a drag time onto the shared clip grid.
pub fn snap_time(time: f32) -> f32 {
    (time / CLIP_SNAP).round() * CLIP_SNAP
}

/// Clickable favourite slot at the right edge of a library row.
pub fn patch_star_rect(row: Rect) -> Rect {
    Rect::from_min_size(
        Vec2::new(row.right() - 22.0, row.top()),
        Size::new(20.0, row.size.height),
    )
}

/// Hit/drag area for a parameter slider row (shared by paint and input).
pub fn param_track(rect: Rect) -> Rect {
    Rect::from_min_size(
        Vec2::new(rect.left() + 4.0, rect.top() + 16.0),
        Size::new((rect.size.width - 8.0).max(1.0), 4.0),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
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
    FavoriteFilter,
    Search,
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

pub struct Layout {
    pub viewport: Size,
    pub top: Rect,
    pub left: Rect,
    pub header: Rect,
    pub right: Rect,
    pub status: Rect,
    pub menu_button: Rect,
    pub menu_rect: Option<Rect>,
    pub menu_items: Vec<(Action, &'static str, Rect)>,
    pub menu_sections: Vec<(&'static str, Rect)>,
    pub tab_buttons: Vec<Button>,
    pub header_buttons: Vec<Button>,
    pub patch_rows: Vec<(usize, Rect)>,
    pub music_row: Rect,
    pub timeline: Rect,
    pub automation: Rect,
    pub waveform: Rect,
    pub items: Vec<Item>,
    pub pattern_grid: Rect,
    pub pattern_cells: Vec<(PatternRow, usize, Rect)>,
    pub layer_rows: Vec<(usize, Rect)>,
    pub timeline_scale: f32,
    pub timeline_total: f32,
    pub category_rect: Rect,
    pub favorite_rect: Rect,
    pub search_rect: Rect,
    pub project_anchor: f32,
    pub inspector: Rect,
    pub inspector_content_h: f32,
    pub inspector_scrollbar: Rect,
    pub inspector_thumb: Rect,
}

impl Layout {
    pub fn new(state: &State, viewport: Size) -> Self {
        let top = Rect::from_min_size(Vec2::ZERO, Size::new(viewport.width, TOP_H));
        let status = Rect::from_min_size(
            Vec2::new(0.0, viewport.height - STATUS_H),
            Size::new(viewport.width, STATUS_H),
        );
        let body_h = body_height(viewport);
        let left = Rect::from_min_size(Vec2::new(0.0, TOP_H), Size::new(LEFT_W, body_h));
        let right = Rect::from_min_size(
            Vec2::new(viewport.width - RIGHT_W, TOP_H),
            Size::new(RIGHT_W, body_h),
        );
        let center = Rect::from_min_size(
            Vec2::new(LEFT_W, TOP_H),
            Size::new((viewport.width - LEFT_W - RIGHT_W).max(1.0), body_h),
        );

        let pad = 8.0;
        let header = Rect::from_min_size(
            Vec2::new(center.left() + pad, center.top() + 6.0),
            Size::new(center.size.width - pad * 2.0, HEADER_H),
        );

        // Below ~1260 px the top bar cannot fit icon+label buttons; switch to
        // icon-only buttons instead of letting them crowd the patch title.
        let compact = viewport.width < 1260.0;
        let mut header_buttons = Vec::new();
        let mut x = header.right() - 6.0;
        for (label, action, width) in [
            ("DEL", Action::DeleteLayer, 44.0),
            ("DUP", Action::DuplicateLayer, 46.0),
            ("LAYER", Action::AddLayer, 62.0),
            ("LOOP", Action::Loop, 52.0),
            ("STOP", Action::Stop, 50.0),
            ("PLAY", Action::Play, 48.0),
            ("REDO", Action::Redo, 52.0),
            ("UNDO", Action::Undo, 52.0),
        ] {
            let width = if compact { 28.0 } else { width };
            let gap = if compact { 3.0 } else { 4.0 };
            x -= width;
            header_buttons.push(Button {
                rect: Rect::from_min_size(Vec2::new(x, header.top() + 6.0), Size::new(width, 22.0)),
                label: if compact { String::new() } else { label.into() },
                action,
                active: action == Action::Loop && state.loop_play,
                icon: action_icon(action),
            });
            x -= gap;
        }
        header_buttons.reverse();

        let menu_button = Rect::from_min_size(Vec2::new(92.0, 5.0), Size::new(62.0, 22.0));
        let project_anchor = 170.0;
        let mut tab_buttons = Vec::new();
        let mut tab_x = viewport.width - 8.0 - 72.0 * 2.0 - 4.0;
        for (label, action, tab, icon) in [
            ("SOUND", Action::TabPatches, Tab::Patches, Icon::Waveform),
            ("MUSIC", Action::TabMusic, Tab::Music, Icon::Music),
        ] {
            tab_buttons.push(Button {
                rect: Rect::from_min_size(Vec2::new(tab_x, 4.0), Size::new(72.0, 22.0)),
                label: label.into(),
                action,
                active: state.tab == tab,
                icon: Some(icon),
            });
            tab_x += 76.0;
        }

        let mut menu_rect = None;
        let mut menu_items = Vec::new();
        let mut menu_sections = Vec::new();
        if state.menu_open {
            let width = 224.0;
            let mut height = 10.0;
            for entry in MENU_ENTRIES {
                height += match entry {
                    MenuEntry::Section(_) => 20.0,
                    MenuEntry::Item(_, _) => 24.0,
                };
            }
            let rect = Rect::from_min_size(
                Vec2::new(menu_button.left(), TOP_H + 4.0),
                Size::new(width, height),
            );
            let mut y = rect.top() + 5.0;
            for entry in MENU_ENTRIES {
                match entry {
                    MenuEntry::Section(text) => {
                        menu_sections.push((
                            *text,
                            Rect::from_min_size(
                                Vec2::new(rect.left() + 10.0, y),
                                Size::new(width - 20.0, 20.0),
                            ),
                        ));
                        y += 20.0;
                    }
                    MenuEntry::Item(label, action) => {
                        menu_items.push((
                            *action,
                            *label,
                            Rect::from_min_size(
                                Vec2::new(rect.left() + 4.0, y),
                                Size::new(width - 8.0, 24.0),
                            ),
                        ));
                        y += 24.0;
                    }
                }
            }
            menu_rect = Some(rect);
        }

        // Left: category filter, patch rows + MUSIC row.
        let list_top = left.top() + LIBRARY_TOP;
        let category_rect = Rect::from_min_size(
            Vec2::new(left.left() + 8.0, left.top() + 34.0),
            Size::new(118.0, 20.0),
        );
        let favorite_rect = Rect::from_min_size(
            Vec2::new(left.right() - 32.0, left.top() + 34.0),
            Size::new(24.0, 20.0),
        );
        let search_rect = Rect::from_min_size(
            Vec2::new(left.left() + 8.0, left.top() + 58.0),
            Size::new(left.size.width - 16.0, 20.0),
        );
        let music_row = Rect::from_min_size(
            Vec2::new(left.left(), left.bottom() - MUSIC_ROW_OFFSET),
            Size::new(left.size.width, LIBRARY_ROW_H),
        );
        let row_h = LIBRARY_ROW_H;
        let capacity = library_capacity(viewport);
        let visible_patches = state.visible_patches();
        let patch_start = state
            .patch_scroll
            .min(visible_patches.len().saturating_sub(1));
        let mut patch_rows = Vec::new();
        for visible in 0..capacity {
            let Some(index) = visible_patches.get(patch_start + visible).copied() else {
                break;
            };
            patch_rows.push((
                index,
                Rect::from_min_size(
                    Vec2::new(left.left(), list_top + visible as f32 * row_h),
                    Size::new(left.size.width, row_h),
                ),
            ));
        }

        let layer_count = state.patch().map(|p| p.layers.len()).unwrap_or(0);
        let timeline_h = if state.tab == Tab::Music {
            // The pattern grid needs its own height; layer count is irrelevant.
            let rows = pattern_rows(state).len() as f32;
            (22.0 + rows * 19.0 + 8.0)
                .clamp(RULER_H + 3.0 * TRACK_H + 8.0, timeline_height(viewport))
        } else {
            timeline_height_for(viewport, layer_count)
        };
        let timeline = Rect::from_min_size(
            Vec2::new(center.left() + pad, header.bottom() + 6.0),
            Size::new(center.size.width - pad * 2.0, timeline_h),
        );
        let automation = Rect::from_min_size(
            Vec2::new(center.left() + pad, timeline.bottom() + 6.0),
            Size::new(center.size.width - pad * 2.0, AUTOMATION_H),
        );
        // Card gaps + header + automation consume 120px; the waveform takes
        // every remaining pixel so the shell always fills the body.
        let waveform_h = (body_h - 120.0 - timeline_h).max(96.0);
        let waveform = Rect::from_min_size(
            Vec2::new(center.left() + pad, automation.bottom() + 6.0),
            Size::new(center.size.width - pad * 2.0, waveform_h),
        );

        let total = state.timeline_span();
        let timeline_scale =
            ((timeline.size.width - TIMELINE_LABEL - 8.0) / (total + 0.08)).max(8.0);
        let mut layer_rows = Vec::new();
        if let Some(patch) = state.patch() {
            let visible = layer_capacity(viewport, layer_count);
            let start = state.layer_scroll.min(patch.layers.len().saturating_sub(1));
            for offset in 0..visible {
                let index = start + offset;
                if index >= patch.layers.len() {
                    break;
                }
                layer_rows.push((
                    index,
                    Rect::from_min_size(
                        Vec2::new(
                            timeline.left(),
                            timeline.top() + RULER_H + offset as f32 * TRACK_H,
                        ),
                        Size::new(timeline.size.width, TRACK_H),
                    ),
                ));
            }
        }

        let inspector = Rect::from_min_size(
            Vec2::new(right.left() + 8.0, right.top() + 32.0),
            Size::new(right.size.width - 16.0, (right.size.height - 40.0).max(1.0)),
        );
        let raw = inspector_items(state);
        let mut items = Vec::new();
        let mut y = inspector.top() + 4.0 - state.inspector_scroll;
        let content_start = y;
        let inner_left = inspector.left() + INSPECTOR_PAD;
        let inner_width = (inspector.size.width - INSPECTOR_PAD * 2.0).max(1.0);
        let inner_rect = |top: f32, height: f32| {
            Rect::from_min_size(Vec2::new(inner_left, top), Size::new(inner_width, height))
        };
        for raw in raw {
            match raw {
                RawItem::Section(text) => {
                    items.push(Item::Section {
                        text,
                        rect: inner_rect(y, 24.0),
                    });
                    y += 24.0;
                }
                RawItem::Param {
                    label,
                    value,
                    hit,
                    norm,
                } => {
                    items.push(Item::Param {
                        label,
                        value,
                        hit,
                        norm,
                        rect: inner_rect(y, 26.0),
                    });
                    y += 26.0;
                }
                RawItem::Options {
                    label,
                    options,
                    columns,
                } => {
                    let cols = columns.max(1) as f32;
                    let gap = 2.0;
                    let width = (inner_width - INSPECTOR_LABEL - gap * (cols - 1.0)) / cols;
                    let rows = (options.len() as f32 / cols).ceil().max(1.0);
                    let height = rows * 19.0 + 7.0;
                    let mut buttons = Vec::new();
                    for (index, (text, action, active)) in options.into_iter().enumerate() {
                        let col = index % columns;
                        let row = index / columns;
                        buttons.push(Button {
                            rect: Rect::from_min_size(
                                Vec2::new(
                                    inner_left + INSPECTOR_LABEL + col as f32 * (width + gap),
                                    y + 3.0 + row as f32 * 19.0,
                                ),
                                Size::new(width, 18.0),
                            ),
                            label: text,
                            action,
                            active,
                            icon: action_icon(action),
                        });
                    }
                    items.push(Item::Options {
                        label,
                        rect: inner_rect(y, height),
                        buttons,
                    });
                    y += height + 2.0;
                }
                RawItem::Toggle {
                    label,
                    value,
                    action,
                } => {
                    items.push(Item::Toggle {
                        label,
                        value,
                        action,
                        rect: inner_rect(y, 26.0),
                    });
                    y += 26.0;
                }
                RawItem::Note(text) => {
                    items.push(Item::Note {
                        text,
                        rect: inner_rect(y, 18.0),
                    });
                    y += 18.0;
                }
            }
        }

        let inspector_content_h = (y - content_start).max(0.0);
        let mut inspector_scrollbar = Rect::from_min_size(Vec2::ZERO, Size::ZERO);
        let mut inspector_thumb = Rect::from_min_size(Vec2::ZERO, Size::ZERO);
        if inspector_content_h > inspector.size.height + 1.0 {
            let track = Rect::from_min_size(
                Vec2::new(right.right() - 9.0, inspector.top() + 2.0),
                Size::new(5.0, (inspector.size.height - 4.0).max(1.0)),
            );
            let max_scroll = (inspector_content_h - inspector.size.height).max(0.001);
            let thumb_h = ((inspector.size.height / inspector_content_h) * track.size.height)
                .clamp(18.0, track.size.height);
            let t = (state.inspector_scroll / max_scroll).clamp(0.0, 1.0);
            inspector_scrollbar = track;
            inspector_thumb = Rect::from_min_size(
                Vec2::new(
                    track.left(),
                    track.top() + t * (track.size.height - thumb_h),
                ),
                Size::new(5.0, thumb_h),
            );
        }

        // Music pattern grid occupies the timeline slot.
        let mut pattern_grid = Rect::from_min_size(Vec2::ZERO, Size::ZERO);
        let mut pattern_cells = Vec::new();
        if state.tab == Tab::Music {
            let rows = pattern_rows(state);
            let height = (rows.len() as f32 * 19.0 + 22.0).min(timeline.size.height);
            pattern_grid = Rect::from_min_size(
                Vec2::new(timeline.left(), timeline.top()),
                Size::new(timeline.size.width, height),
            );
            let label_w = TIMELINE_LABEL;
            let col_w = (pattern_grid.size.width - label_w - 8.0) / 16.0;
            for (row_index, row) in rows.iter().enumerate() {
                for step in 0..16 {
                    pattern_cells.push((
                        *row,
                        step,
                        Rect::from_min_size(
                            Vec2::new(
                                pattern_grid.left() + label_w + step as f32 * col_w,
                                pattern_grid.top() + 22.0 + row_index as f32 * 19.0,
                            ),
                            Size::new(col_w.max(1.0) - 1.0, 18.0),
                        ),
                    ));
                }
            }
        }

        Self {
            viewport,
            top,
            left,
            header,
            right,
            status,
            menu_button,
            menu_rect,
            menu_items,
            menu_sections,
            tab_buttons,
            header_buttons,
            patch_rows,
            music_row,
            timeline,
            automation,
            waveform,
            items,
            pattern_grid,
            pattern_cells,
            layer_rows,
            timeline_scale,
            timeline_total: total,
            category_rect,
            favorite_rect,
            search_rect,
            project_anchor,
            inspector,
            inspector_content_h,
            inspector_scrollbar,
            inspector_thumb,
        }
    }

    pub fn time_x(&self, time: f32) -> f32 {
        self.timeline.left() + TIMELINE_LABEL + time * self.timeline_scale
    }

    pub fn x_time(&self, x: f32) -> f32 {
        ((x - self.timeline.left() - TIMELINE_LABEL) / self.timeline_scale).max(0.0)
    }

    pub fn automation_plot(&self) -> Rect {
        Rect::from_min_size(
            Vec2::new(self.automation.left() + 6.0, self.automation.top() + 16.0),
            Size::new(
                (self.automation.size.width - 12.0).max(1.0),
                (self.automation.size.height - 20.0).max(1.0),
            ),
        )
    }

    pub fn automation_xy(
        &self,
        time: f32,
        value: f32,
        kind: AutomationKind,
        duration: f32,
    ) -> Vec2 {
        let plot = self.automation_plot();
        let x = plot.left() + (time / duration.max(0.0001)).clamp(0.0, 1.0) * plot.size.width;
        let y = plot.bottom() - kind.norm(value) * plot.size.height;
        Vec2::new(x, y)
    }

    pub fn automation_at(&self, pos: Vec2, duration: f32, kind: AutomationKind) -> (f32, f32) {
        let plot = self.automation_plot();
        let time =
            ((pos.x - plot.left()) / plot.size.width.max(1.0) * duration).clamp(0.0, duration);
        let norm = ((plot.bottom() - pos.y) / plot.size.height.max(1.0)).clamp(0.0, 1.0);
        (time, kind.denorm(norm))
    }
}

fn automation_tabs(layout: &Layout) -> [(AutomationKind, Rect); 2] {
    let w = 54.0;
    let y = layout.automation.top() + 4.0;
    [
        (
            AutomationKind::Gain,
            Rect::from_min_size(
                Vec2::new(layout.automation.right() - w * 2.0 - 12.0, y),
                Size::new(w, 16.0),
            ),
        ),
        (
            AutomationKind::Cutoff,
            Rect::from_min_size(
                Vec2::new(layout.automation.right() - w - 8.0, y),
                Size::new(w, 16.0),
            ),
        ),
    ]
}

pub fn hit(state: &State, layout: &Layout, pos: Vec2) -> Option<Hit> {
    if layout.menu_button.contains(pos) {
        return Some(Hit::MenuToggle);
    }
    if let Some(menu) = layout.menu_rect {
        if menu.contains(pos) {
            for (action, _, rect) in &layout.menu_items {
                if rect.contains(pos) {
                    return Some(Hit::MenuItem(*action));
                }
            }
            return Some(Hit::MenuPanel);
        }
    }
    if layout.category_rect.contains(pos) {
        return Some(Hit::CategoryFilter);
    }
    if layout.favorite_rect.contains(pos) {
        return Some(Hit::FavoriteFilter);
    }
    if layout.search_rect.contains(pos) {
        return Some(Hit::Search);
    }
    if layout.inspector_scrollbar.contains(pos) {
        return Some(Hit::InspectorScrollbar);
    }
    for button in layout
        .tab_buttons
        .iter()
        .chain(layout.header_buttons.iter())
    {
        if button.rect.contains(pos) {
            return Some(Hit::Action(button.action));
        }
    }
    for (index, rect) in &layout.patch_rows {
        if rect.contains(pos) {
            if patch_star_rect(*rect).contains(pos) {
                return Some(Hit::PatchFavorite(*index));
            }
            return Some(Hit::PatchRow(*index));
        }
    }
    if layout.music_row.contains(pos) {
        return Some(Hit::Action(Action::TabMusic));
    }

    if layout.inspector.contains(pos) {
        for item in &layout.items {
            match item {
                Item::Param { hit, rect, .. } => {
                    if rect.contains(pos) {
                        return Some(*hit);
                    }
                }
                Item::Options { buttons, .. } => {
                    for button in buttons {
                        if button.rect.contains(pos) {
                            return Some(Hit::Action(button.action));
                        }
                    }
                }
                Item::Toggle { action, rect, .. } if rect.contains(pos) => {
                    return Some(Hit::Action(*action));
                }
                _ => {}
            }
        }
    }

    if state.tab == Tab::Patches && layout.waveform.contains(pos) {
        let duration = state.patch().map(|patch| patch.end()).unwrap_or(0.0);
        let t = ((pos.x - layout.waveform.left()) / layout.waveform.size.width.max(1.0))
            .clamp(0.0, 1.0)
            * duration;
        return Some(Hit::Waveform(t));
    }
    if state.tab == Tab::Patches && layout.timeline.contains(pos) {
        if pos.y < layout.timeline.top() + RULER_H {
            return Some(Hit::Ruler(layout.x_time(pos.x)));
        }
        for (index, rect) in &layout.layer_rows {
            if rect.contains(pos) {
                if let Some(layer) = state.patch().and_then(|p| p.layers.get(*index)) {
                    if layer.clips.is_empty() {
                        let x0 = layout.time_x(layer.delay);
                        let x1 = layout.time_x(layer.delay + layer.duration);
                        if pos.x >= x0 && pos.x <= x1 {
                            let mode = if pos.x >= x1 - 7.0 {
                                TimelineDrag::Resize
                            } else if pos.x <= x0 + 7.0 {
                                TimelineDrag::ResizeLeft
                            } else {
                                TimelineDrag::Move
                            };
                            return Some(Hit::Timeline {
                                layer: *index,
                                clip: None,
                                mode,
                            });
                        }
                    } else {
                        for (clip_index, clip) in layer.clips.iter().enumerate() {
                            let x0 = layout.time_x(clip.start);
                            let x1 = layout.time_x(clip.start + clip.duration);
                            if pos.x >= x0 && pos.x <= x1 {
                                let mode = if pos.x >= x1 - 7.0 {
                                    TimelineDrag::Resize
                                } else if pos.x <= x0 + 7.0 {
                                    TimelineDrag::ResizeLeft
                                } else {
                                    TimelineDrag::Move
                                };
                                return Some(Hit::Timeline {
                                    layer: *index,
                                    clip: Some(clip_index),
                                    mode,
                                });
                            }
                        }
                    }
                }
                return Some(Hit::LayerRow(*index));
            }
        }
    }
    if state.tab == Tab::Patches && layout.automation.contains(pos) {
        for (kind, rect) in automation_tabs(layout) {
            if rect.contains(pos) {
                return Some(Hit::Action(Action::AutomationLane(kind)));
            }
        }
        if let (Some(clip), Some(layer)) = (
            state.selected_clip,
            state
                .patch()
                .and_then(|p| p.layers.get(state.selected_layer)),
        ) {
            if let Some(clip) = layer.clips.get(clip) {
                let kind = state.automation_lane;
                for (index, point) in kind.points(clip).iter().enumerate() {
                    let xy = layout.automation_xy(point.time, point.value, kind, clip.duration);
                    if (xy - pos).length() <= 7.0 {
                        return Some(Hit::AutomationPoint(index));
                    }
                }
                let (time, value) = layout.automation_at(pos, clip.duration, kind);
                return Some(Hit::AutomationLane { time, value });
            }
        }
    }
    for (row, step, rect) in &layout.pattern_cells {
        if rect.contains(pos) {
            return Some(Hit::Pattern(*row, *step));
        }
    }
    None
}

fn action_icon(action: Action) -> Option<Icon> {
    match action {
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

pub fn paint(state: &State, measurer: &dyn TextMeasurer, ctx: &mut PaintContext) {
    let layout = Layout::new(state, state.viewport);
    fill(ctx, Rect::from_min_size(Vec2::ZERO, layout.viewport), BG);

    // Top bar
    fill(ctx, layout.top, PANEL);
    hline(ctx, 0.0, layout.top.right(), layout.top.bottom(), BORDER);
    icons::paint(
        ctx,
        Icon::Library,
        Rect::from_min_size(Vec2::new(12.0, 9.0), Size::splat(14.0)),
        ACCENT,
    );
    text(ctx, "SFX LAB", Vec2::new(31.0, 20.0), TITLE, TEXT);
    let project = state
        .root
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "UNTITLED PROJECT".into());
    let tabs_left = layout.viewport.width - 156.0;
    let project_text = ellipsis(
        measurer,
        &project,
        SMALL,
        (tabs_left - layout.project_anchor - 14.0).max(40.0),
    );
    text(
        ctx,
        &project_text,
        Vec2::new(layout.project_anchor, 20.0),
        SMALL,
        DIM,
    );
    if state.dirty {
        text(
            ctx,
            "*",
            Vec2::new(layout.project_anchor - 9.0, 20.0),
            TITLE,
            WARN,
        );
    }
    theme::button_icon(
        ctx,
        measurer,
        layout.menu_button,
        "MENU",
        icons::document(Icon::Menu),
        state.hover_hit == Some(Hit::MenuToggle),
        state.menu_open,
    );
    for button in &layout.tab_buttons {
        paint_button(state, measurer, ctx, button);
    }

    // Left panel
    fill(ctx, layout.left, PANEL);
    vline(
        ctx,
        layout.left.right(),
        layout.left.top(),
        layout.left.bottom(),
        BORDER,
    );
    // Title bar mirrors the right inspector header: raised strip, hairline
    // rule, icon + title, so both side panels read as the same chrome.
    fill(
        ctx,
        Rect::from_min_size(layout.left.origin, Size::new(layout.left.size.width, 26.0)),
        RAISED,
    );
    hline(
        ctx,
        layout.left.left(),
        layout.left.right(),
        layout.left.top() + 26.0,
        BORDER,
    );
    icons::paint(
        ctx,
        Icon::Library,
        Rect::from_min_size(
            Vec2::new(layout.left.left() + 10.0, layout.left.top() + 7.0),
            Size::splat(13.0),
        ),
        DIM,
    );
    text_top(
        ctx,
        measurer,
        "LIBRARY",
        layout.left.left() + 28.0,
        layout.left.top() + 6.0,
        TITLE,
        TEXT,
    );
    text_right(
        ctx,
        measurer,
        &format!("{:02}", state.visible_patches().len()),
        layout.left.right() - 10.0,
        layout.left.top() + 13.0,
        SMALL,
        FAINT,
    );
    let category_label = state.category_filter.map(Category::short).unwrap_or("ALL");
    theme::button_icon(
        ctx,
        measurer,
        layout.category_rect,
        category_label,
        icons::document(Icon::Tag),
        state.hover_hit == Some(Hit::CategoryFilter),
        state.category_filter.is_some(),
    );
    theme::button_icon(
        ctx,
        measurer,
        layout.favorite_rect,
        "",
        icons::document(Icon::Star),
        state.hover_hit == Some(Hit::FavoriteFilter),
        state.favorites_only,
    );
    ctx.fill_rounded_rect(layout.search_rect, 5.0, PANEL);
    ctx.stroke_rounded_rect(
        layout.search_rect,
        5.0,
        1.0,
        if state.search_focus { ACCENT } else { BORDER },
    );
    icons::paint(
        ctx,
        Icon::Search,
        Rect::from_min_size(
            Vec2::new(
                layout.search_rect.left() + 6.0,
                layout.search_rect.center().y - 6.0,
            ),
            Size::splat(12.0),
        ),
        FAINT,
    );
    let search_text = if state.search.is_empty() {
        "Search sounds…"
    } else {
        state.search.as_str()
    };
    let search_color = if state.search.is_empty() { FAINT } else { TEXT };
    text_top(
        ctx,
        measurer,
        search_text,
        layout.search_rect.left() + 23.0,
        layout.search_rect.top() + 3.0,
        SMALL,
        search_color,
    );
    if state.search_focus {
        let caret_x = layout.search_rect.left()
            + 23.0
            + measurer.measure_run(state.search.as_str(), SMALL)
            + 1.0;
        vline(
            ctx,
            caret_x,
            layout.search_rect.top() + 4.0,
            layout.search_rect.bottom() - 4.0,
            ACCENT_DARK,
        );
    }
    for (index, rect) in &layout.patch_rows {
        let patch = &state.project.patches[*index];
        let selected = *index == state.selected_patch && state.tab == Tab::Patches;
        if selected {
            fill(ctx, *rect, ACTIVE);
            fill(
                ctx,
                Rect::from_min_size(rect.origin, Size::new(2.0, rect.size.height)),
                ACCENT,
            );
        } else if rect.contains(state.cursor) {
            fill(ctx, *rect, HOVER);
        }
        let wave = patch.layers.first().map(|l| l.wave).unwrap_or(Wave::Sine);
        let wave_label = wave.label();
        // The favorite star owns a fixed slot at the far right; the wave tag is
        // right-aligned before it so the two never share pixels.
        let label_right = rect.right() - 26.0;
        if patch.favorite {
            let star = patch_star_rect(*rect);
            let hovered = state.hover_hit == Some(Hit::PatchFavorite(*index));
            icons::paint(
                ctx,
                Icon::Star,
                Rect::from_min_size(
                    Vec2::new(star.center().x - 5.0, star.center().y - 5.0),
                    Size::splat(10.0),
                ),
                if hovered {
                    ACCENT
                } else if selected {
                    ACCENT_DARK
                } else {
                    WARN
                },
            );
        }
        let name_width =
            (label_right - measurer.measure_run(wave_label, SMALL) - 8.0 - (rect.left() + 12.0))
                .max(24.0);
        text_top(
            ctx,
            measurer,
            &ellipsis(measurer, &patch.name, FONT, name_width),
            rect.left() + 12.0,
            rect.top() + 4.0,
            FONT,
            if selected { TEXT } else { DIM },
        );
        text_right(
            ctx,
            measurer,
            wave_label,
            label_right,
            rect.center().y,
            SMALL,
            if selected { DIM } else { FAINT },
        );
    }
    if state.visible_patches().is_empty() {
        text_top(
            ctx,
            measurer,
            "NO MATCHES",
            layout.left.left() + 12.0,
            layout.left.top() + 96.0,
            SMALL,
            FAINT,
        );
    }
    let music_selected = state.tab == Tab::Music;
    if music_selected {
        fill(ctx, layout.music_row, ACTIVE);
        fill(
            ctx,
            Rect::from_min_size(
                layout.music_row.origin,
                Size::new(2.0, layout.music_row.size.height),
            ),
            ACCENT,
        );
    }
    text_top(
        ctx,
        measurer,
        "MUSIC",
        layout.music_row.left() + 12.0,
        layout.music_row.top() + 4.0,
        FONT,
        if music_selected { TEXT } else { DIM },
    );
    text_right(
        ctx,
        measurer,
        "16 STEP",
        layout.music_row.right() - 8.0,
        layout.music_row.center().y,
        SMALL,
        FAINT,
    );

    // Center header
    paint_header(state, measurer, ctx, &layout);

    if state.tab == Tab::Patches {
        paint_timeline(state, measurer, ctx, &layout);
        paint_automation(state, measurer, ctx, &layout);
        paint_waveform(
            state,
            measurer,
            ctx,
            layout.waveform,
            WaveformData {
                samples: &state.cache,
                envelope: &state.cache_envelope,
                peak: state.cache_peak,
                rate: state.cache_rate,
            },
        );
    } else {
        paint_patterns(state, measurer, ctx, &layout);
        paint_waveform(
            state,
            measurer,
            ctx,
            layout.waveform,
            WaveformData {
                samples: &state.music_cache,
                envelope: &state.music_envelope,
                peak: state.music_cache_peak,
                rate: state.cache_rate,
            },
        );
    }

    // Right inspector
    fill(ctx, layout.right, PANEL);
    vline(
        ctx,
        layout.right.left(),
        layout.right.top(),
        layout.right.bottom(),
        BORDER,
    );
    fill(
        ctx,
        Rect::from_min_size(
            layout.right.origin,
            Size::new(layout.right.size.width, 26.0),
        ),
        RAISED,
    );
    hline(
        ctx,
        layout.right.left(),
        layout.right.right(),
        layout.right.top() + 26.0,
        BORDER,
    );
    icons::paint(
        ctx,
        Icon::Settings,
        Rect::from_min_size(
            Vec2::new(layout.right.left() + 10.0, layout.right.top() + 7.0),
            Size::splat(13.0),
        ),
        DIM,
    );
    text_top(
        ctx,
        measurer,
        "PARAMETERS",
        layout.right.left() + 28.0,
        layout.right.top() + 6.0,
        TITLE,
        TEXT,
    );
    ctx.save();
    ctx.clip_rect(layout.inspector);
    paint_inspector(state, measurer, ctx, &layout);
    ctx.restore();

    if layout.inspector_scrollbar.size.height > 0.0 {
        ctx.fill_rounded_rect(layout.inspector_scrollbar, 2.5, RAISED);
        ctx.fill_rounded_rect(
            layout.inspector_thumb,
            2.5,
            if state.hover_hit == Some(Hit::InspectorScrollbar) {
                ACCENT
            } else {
                DIM
            },
        );
    }

    // Status bar
    fill(ctx, layout.status, PANEL);
    hline(ctx, 0.0, layout.status.right(), layout.status.top(), BORDER);
    ctx.fill_rounded_rect(
        Rect::from_min_size(
            Vec2::new(10.0, layout.status.center().y - 3.0),
            Size::splat(6.0),
        ),
        3.0,
        if state.dirty { WARN } else { ACCENT },
    );
    let toast = state.toast.as_ref().map(|(text, _)| text.as_str());
    let live = drag_info(state).or_else(|| hover_info(state));
    let status = toast.or(live.as_deref()).unwrap_or(state.status.as_str());
    text_top(
        ctx,
        measurer,
        status,
        22.0,
        layout.status.top() + 5.0,
        SMALL,
        if state.toast.is_some() {
            ACCENT_DARK
        } else {
            DIM
        },
    );
    let info = if state.tab == Tab::Patches {
        let patch = state.patch();
        format!(
            "{} Hz  ·  {:.3}s  ·  {} LAYERS  ·  peak {:.1} dB",
            state.cache_rate,
            patch.map(|p| p.end()).unwrap_or(0.0),
            patch.map(|p| p.layers.len()).unwrap_or(0),
            synth::db(state.cache_peak)
        )
    } else {
        format!(
            "{} BPM  ·  {} BARS  ·  {:.1}s  ·  peak {:.1} dB",
            state.project.music.bpm,
            state.project.music.bars,
            state.project.music.length_seconds(),
            synth::db(state.music_cache_peak)
        )
    };
    text_right(
        ctx,
        measurer,
        &info,
        layout.status.right() - 10.0,
        layout.status.center().y,
        SMALL,
        FAINT,
    );

    paint_menu(state, measurer, ctx, &layout);

    if state.modal.is_some() {
        paint_modal(state, measurer, ctx, &layout);
    } else if matches!(state.drag, Drag::None) && !state.menu_open {
        paint_hover_tip(state, measurer, ctx);
    }
}

fn paint_button(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    button: &Button,
) {
    theme::button_icon(
        ctx,
        measurer,
        button.rect,
        &button.label,
        button.icon.and_then(icons::document),
        state.hover == Some(button.action) || state.hover_hit == Some(Hit::Action(button.action)),
        button.active,
    );
}

fn paint_menu(state: &State, measurer: &dyn TextMeasurer, ctx: &mut PaintContext, layout: &Layout) {
    let Some(rect) = layout.menu_rect else {
        return;
    };
    fill(ctx, rect, RAISED);
    stroke(ctx, rect, BORDER_HI);
    for (text, section) in &layout.menu_sections {
        text_top(
            ctx,
            measurer,
            text,
            section.left(),
            section.top() + 3.0,
            SMALL,
            ACCENT2,
        );
    }
    for (action, label, item) in &layout.menu_items {
        if item.contains(state.cursor) {
            fill(ctx, *item, ACTIVE);
        }
        let active =
            *action == Action::Favorite && state.patch().is_some_and(|patch| patch.favorite);
        let mut text_x = item.left() + 10.0;
        if let Some(icon) = action_icon(*action) {
            icons::paint(
                ctx,
                icon,
                Rect::from_min_size(
                    Vec2::new(item.left() + 9.0, item.center().y - 6.5),
                    Size::splat(13.0),
                ),
                if active { ACCENT } else { DIM },
            );
            text_x = item.left() + 30.0;
        }
        text_top(
            ctx,
            measurer,
            label,
            text_x,
            item.top() + 5.0,
            FONT,
            if active { ACCENT } else { TEXT },
        );
        if active {
            text_right(
                ctx,
                measurer,
                "ON",
                item.right() - 8.0,
                item.center().y,
                SMALL,
                ACCENT_DARK,
            );
        }
    }
}

fn paint_hover_tip(state: &State, measurer: &dyn TextMeasurer, ctx: &mut PaintContext) {
    let tip = match state.hover_hit {
        Some(Hit::Action(action)) => action_tip(action),
        Some(Hit::Search) => "搜索音效 · Search（/）",
        Some(Hit::CategoryFilter) => "切换分类过滤 · Category（C）",
        Some(Hit::FavoriteFilter) => "只看收藏 · Favorites",
        Some(Hit::InspectorScrollbar) => "拖动滚动参数面板 · Scroll",
        _ => return,
    };
    if tip.is_empty() {
        return;
    }
    let width = measurer.measure_run(tip, FONT) + 16.0;
    let height = 24.0;
    let mut origin = state.cursor + Vec2::new(14.0, 18.0);
    origin.x = origin.x.min(state.viewport.width - width - 6.0).max(6.0);
    origin.y = origin.y.min(state.viewport.height - height - 6.0).max(6.0);
    let rect = Rect::from_min_size(origin, Size::new(width, height));
    ctx.fill_rounded_rect(rect, 5.0, PANEL);
    ctx.stroke_rounded_rect(rect, 5.0, 1.0, ACCENT_DARK);
    text_top(
        ctx,
        measurer,
        tip,
        rect.left() + 8.0,
        rect.top() + 4.0,
        FONT,
        TEXT,
    );
}

fn paint_header(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let header = layout.header;
    fill(ctx, header, PANEL);
    stroke(ctx, header, BORDER);
    for button in &layout.header_buttons {
        paint_button(state, measurer, ctx, button);
    }
    // Keep the title block clear of the transport buttons at any width.
    let title_right = layout
        .header_buttons
        .first()
        .map(|button| button.rect.left() - 8.0)
        .unwrap_or(header.right() - 10.0);
    let title_width = (title_right - (header.left() + 10.0)).max(24.0);
    if state.tab == Tab::Patches {
        if let Some(patch) = state.patch() {
            text_top(
                ctx,
                measurer,
                &ellipsis(measurer, &patch.name, TITLE, title_width),
                header.left() + 10.0,
                header.top() + 4.0,
                TITLE,
                ACCENT,
            );
            let layer = patch.layers.get(state.selected_layer);
            let meta = format!(
                "{}  /  {}  ·  {:.3}s  ·  gain {:.2}",
                patch.layers.len(),
                layer.map(|l| l.wave.label()).unwrap_or("—"),
                patch.end(),
                layer.map(|l| l.gain).unwrap_or(0.0)
            );
            text_top(
                ctx,
                measurer,
                &ellipsis(measurer, &meta, SMALL, title_width),
                header.left() + 10.0,
                header.top() + 18.0,
                SMALL,
                FAINT,
            );
        }
    } else {
        text_top(
            ctx,
            measurer,
            "MUSIC",
            header.left() + 10.0,
            header.top() + 4.0,
            TITLE,
            ACCENT,
        );
        text_top(
            ctx,
            measurer,
            &ellipsis(
                measurer,
                &format!(
                    "{} BPM  ·  {} BARS  ·  {} CHORD BARS  ·  {}",
                    state.project.music.bpm,
                    state.project.music.bars,
                    state.project.music.chords.len(),
                    if state.project.music.use_patterns {
                        "PATTERN"
                    } else {
                        "DEFAULT"
                    }
                ),
                SMALL,
                title_width,
            ),
            header.left() + 10.0,
            header.top() + 18.0,
            SMALL,
            FAINT,
        );
    }
}

fn paint_timeline(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let rect = layout.timeline;
    fill(ctx, rect, TRACK_BG);
    stroke(ctx, rect, BORDER);
    let ruler = Rect::from_min_size(rect.origin, Size::new(rect.size.width, RULER_H));
    fill(ctx, ruler, RAISED);
    hline(ctx, rect.left(), rect.right(), ruler.bottom(), BORDER);
    ctx.save();
    ctx.clip_rect(rect);

    let step = tick_step(layout.timeline_total);
    let mut time = 0.0;
    while time <= layout.timeline_total + 0.001 {
        let x = layout.time_x(time);
        vline(
            ctx,
            x,
            ruler.bottom(),
            rect.bottom(),
            Color::new(1.0, 1.0, 1.0, 0.045),
        );
        vline(ctx, x, ruler.top(), ruler.bottom(), BORDER_HI);
        text_top(
            ctx,
            measurer,
            &format!("{time:.2}"),
            x + 3.0,
            ruler.top() + 2.0,
            SMALL,
            DIM,
        );
        time += step;
    }

    if let Some(patch) = state.patch() {
        for (index, row_rect) in &layout.layer_rows {
            if let Some(layer) = patch.layers.get(*index) {
                let selected = *index == state.selected_layer;
                let hovered_row = matches!(
                    state.hover_hit,
                    Some(Hit::Timeline { layer, .. }) if layer == *index
                );
                if selected {
                    fill(ctx, *row_rect, ACTIVE);
                } else if hovered_row {
                    fill(ctx, *row_rect, HOVER);
                }
                hline(ctx, rect.left(), rect.right(), row_rect.bottom(), BORDER);
                let label = format!("{:02} {}", index + 1, layer.wave.label());
                text_top(
                    ctx,
                    measurer,
                    &ellipsis(measurer, &label, SMALL, TIMELINE_LABEL - 8.0),
                    rect.left() + 5.0,
                    row_rect.top() + 4.0,
                    SMALL,
                    if selected { TEXT } else { DIM },
                );
                if layer.clips.is_empty() {
                    paint_clip_block(
                        state,
                        measurer,
                        ctx,
                        layout,
                        row_rect,
                        layer.delay,
                        layer.duration,
                        layer.gain,
                        layer,
                        selected,
                        false,
                        *index,
                        None,
                    );
                } else {
                    for (clip_index, clip) in layer.clips.iter().enumerate() {
                        paint_clip_block(
                            state,
                            measurer,
                            ctx,
                            layout,
                            row_rect,
                            clip.start,
                            clip.duration,
                            layer.gain * clip.gain,
                            layer,
                            selected,
                            state.selected_clip == Some(clip_index),
                            *index,
                            Some(clip_index),
                        );
                    }
                }
            }
        }
    }
    if let Some(Hit::Ruler(time)) = state.hover_hit {
        let hx = layout.time_x(time);
        vline(ctx, hx, ruler.bottom(), rect.bottom(), BORDER_HI);
    }
    let px = layout.time_x(state.playhead.min(layout.timeline_total + 0.05));
    vline(ctx, px, rect.top(), rect.bottom(), WARN);
    ctx.draw_line(
        Vec2::new(px, rect.top()),
        Vec2::new(px - 4.0, rect.top() + 6.0),
        1.0,
        WARN,
    );
    ctx.draw_line(
        Vec2::new(px, rect.top()),
        Vec2::new(px + 4.0, rect.top() + 6.0),
        1.0,
        WARN,
    );
    let label = format!("{:.3}", state.playhead);
    let label_w = measurer.measure_run(&label, SMALL);
    let label_x = (px + 6.0).min(rect.right() - label_w - 8.0);
    ctx.fill_rounded_rect(
        Rect::from_min_size(
            Vec2::new(label_x - 4.0, rect.top() + 1.0),
            Size::new(label_w + 8.0, 14.0),
        ),
        3.0,
        Color::new(BG.r, BG.g, BG.b, 0.92),
    );
    text_top(
        ctx,
        measurer,
        &label,
        label_x,
        rect.top() + 2.0,
        SMALL,
        WARN,
    );
    ctx.restore();
}

fn paint_automation(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let rect = layout.automation;
    fill(ctx, rect, PANEL);
    stroke(ctx, rect, BORDER);
    for (kind, tab) in automation_tabs(layout) {
        theme::button(
            ctx,
            measurer,
            tab,
            kind.label(),
            state.hover == Some(Action::AutomationLane(kind)),
            state.automation_lane == kind,
        );
    }
    let Some(clip_index) = state.selected_clip else {
        text_top(
            ctx,
            measurer,
            "AUTOMATION · select a clip",
            rect.left() + 8.0,
            rect.top() + 4.0,
            SMALL,
            FAINT,
        );
        return;
    };
    let Some(clip) = state
        .patch()
        .and_then(|p| p.layers.get(state.selected_layer))
        .and_then(|l| l.clips.get(clip_index))
    else {
        return;
    };
    let kind = state.automation_lane;
    text_top(
        ctx,
        measurer,
        &format!(
            "CLIP {:02} · {} · {} POINTS",
            clip_index + 1,
            kind.label(),
            kind.points(clip).len()
        ),
        rect.left() + 8.0,
        rect.top() + 4.0,
        SMALL,
        DIM,
    );
    let plot = layout.automation_plot();
    for i in 0..=4 {
        let y = plot.top() + plot.size.height * i as f32 / 4.0;
        hline(ctx, plot.left(), plot.right(), y, BORDER);
    }
    for i in 0..=4 {
        let x = plot.left() + plot.size.width * i as f32 / 4.0;
        vline(ctx, x, plot.top(), plot.bottom(), BORDER);
    }
    let points_data = kind.points(clip);
    if points_data.is_empty() {
        let flat = if kind == AutomationKind::Gain {
            1.0
        } else {
            12000.0
        };
        let a = layout.automation_xy(0.0, flat, kind, clip.duration);
        let b = layout.automation_xy(clip.duration, flat, kind, clip.duration);
        ctx.draw_line(a, b, 1.0, FAINT);
        text_top(
            ctx,
            measurer,
            "click to add a point · drag · Delete",
            plot.left() + 6.0,
            plot.bottom() - 14.0,
            SMALL,
            FAINT,
        );
        return;
    }
    let points: Vec<Vec2> = points_data
        .iter()
        .map(|point| layout.automation_xy(point.time, point.value, kind, clip.duration))
        .collect();
    let first = layout.automation_xy(
        0.0,
        points_data.first().map(|p| p.value).unwrap_or(1.0),
        kind,
        clip.duration,
    );
    let last = layout.automation_xy(
        clip.duration,
        points_data.last().map(|p| p.value).unwrap_or(1.0),
        kind,
        clip.duration,
    );
    ctx.draw_line(first, points[0], 1.5, ACCENT);
    for pair in points.windows(2) {
        ctx.draw_line(pair[0], pair[1], 1.5, ACCENT);
    }
    ctx.draw_line(points.last().copied().unwrap_or(last), last, 1.5, ACCENT);
    for (index, point) in points.iter().enumerate() {
        let selected = state.selected_env_point == Some(index);
        let hovered = matches!(state.hover_hit, Some(Hit::AutomationPoint(i)) if i == index);
        let radius = if selected || hovered { 4.0 } else { 3.0 };
        ctx.fill_circle(*point, radius, if selected { ACCENT2 } else { ACCENT });
        ctx.stroke_circle(*point, radius, 1.0, BG);
    }
}

struct WaveformData<'a> {
    samples: &'a [f32],
    envelope: &'a synth::Envelope,
    peak: f32,
    rate: u32,
}

fn paint_waveform(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    rect: Rect,
    data: WaveformData<'_>,
) {
    let WaveformData {
        samples,
        envelope,
        peak,
        rate,
    } = data;
    fill(ctx, rect, DARK);
    stroke(ctx, rect, Color::new(1.0, 1.0, 1.0, 0.10));
    hline(
        ctx,
        rect.left(),
        rect.right(),
        rect.center().y,
        Color::new(1.0, 1.0, 1.0, 0.10),
    );
    if samples.is_empty() {
        text_center(ctx, measurer, "NO SIGNAL", rect, SMALL, DARK_DIM);
        return;
    }
    let width = rect.size.width.max(1.0) as usize;
    let half = rect.size.height * 0.45;
    let mid = rect.center().y;
    let color = Color::new(0.72, 0.95, 0.42, 0.90);
    if envelope.is_empty() {
        // Defensive fallback for a cache that was populated before the
        // envelope existed (e.g. a freshly loaded session).
        for x in 0..width {
            let first = (x * samples.len() / width).min(samples.len() - 1);
            let last = (((x + 1) * samples.len()) / width)
                .max(first + 1)
                .min(samples.len());
            let mut lo = 0.0f32;
            let mut hi = 0.0f32;
            for value in &samples[first..last] {
                lo = lo.min(*value);
                hi = hi.max(*value);
            }
            ctx.draw_line(
                Vec2::new(rect.left() + x as f32, mid - hi * half),
                Vec2::new(rect.left() + x as f32, mid - lo * half),
                1.0,
                color,
            );
        }
    } else {
        let bins = envelope.min.len();
        for x in 0..width {
            let first = (x * bins / width).min(bins - 1);
            let last = (((x + 1) * bins) / width).max(first + 1).min(bins);
            let lo = envelope.min[first..last]
                .iter()
                .copied()
                .fold(0.0, f32::min);
            let hi = envelope.max[first..last]
                .iter()
                .copied()
                .fold(0.0, f32::max);
            ctx.draw_line(
                Vec2::new(rect.left() + x as f32, mid - hi * half),
                Vec2::new(rect.left() + x as f32, mid - lo * half),
                1.0,
                color,
            );
        }
    }
    let duration = samples.len() as f32 / rate.max(1) as f32;
    let px = rect.left() + (state.playhead / duration.max(0.001)).clamp(0.0, 1.0) * rect.size.width;
    vline(ctx, px, rect.top(), rect.bottom(), WARN);
    let info = format!(
        "{:.2}s  ·  {} Hz  ·  peak {:.1} dB",
        duration,
        rate,
        synth::db(peak)
    );
    text_right(
        ctx,
        measurer,
        &info,
        rect.right() - 6.0,
        rect.top() + 10.0,
        SMALL,
        DARK_DIM,
    );
}

fn paint_patterns(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let grid = layout.pattern_grid;
    fill(ctx, grid, PANEL);
    stroke(ctx, grid, BORDER);
    let rows = pattern_rows(state);
    for (index, row) in rows.iter().enumerate() {
        let y = grid.top() + 22.0 + index as f32 * 19.0;
        let label = match row {
            PatternRow::Arp(tone) => format!("ARP {}", tone + 1),
            PatternRow::Bass(0) => "BASS ROOT".into(),
            PatternRow::Bass(_) => "BASS OCT".into(),
            PatternRow::Kick => "KICK".into(),
            PatternRow::Hat => "HAT".into(),
        };
        text_top(
            ctx,
            measurer,
            &label,
            grid.left() + 6.0,
            y + 3.0,
            SMALL,
            DIM,
        );
        hline(ctx, grid.left(), grid.right(), y + 18.0, BORDER);
    }
    for step in 0..16 {
        let x = grid.left()
            + TIMELINE_LABEL
            + step as f32 * ((grid.size.width - TIMELINE_LABEL - 8.0) / 16.0);
        vline(ctx, x, grid.top() + 20.0, grid.bottom(), BORDER);
    }
    for (row, step, rect) in &layout.pattern_cells {
        let on = pattern_on(&state.project.music, *row, *step);
        let hovered = rect.contains(state.cursor);
        let color = if on {
            CELL_ON
        } else if hovered {
            BORDER_HI
        } else {
            RAISED
        };
        fill(ctx, *rect, color);
        if on {
            fill(
                ctx,
                Rect::from_min_size(rect.origin, Size::new(rect.size.width, 2.0)),
                ACCENT2,
            );
        }
    }
}

fn paint_inspector(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let mut clip_bottom = layout.right.bottom() - 8.0;
    for item in &layout.items {
        let rect = match item {
            Item::Section { rect, .. }
            | Item::Param { rect, .. }
            | Item::Options { rect, .. }
            | Item::Toggle { rect, .. }
            | Item::Note { rect, .. } => *rect,
        };
        if rect.top() >= clip_bottom {
            break;
        }
        match item {
            Item::Section { text, rect } => {
                fill(ctx, *rect, PANEL);
                hline(ctx, rect.left(), rect.right(), rect.top(), BORDER);
                text_top(
                    ctx,
                    measurer,
                    text,
                    rect.left() + 2.0,
                    rect.top() + 5.0,
                    SMALL,
                    ACCENT2,
                );
            }
            Item::Param {
                label,
                value,
                hit,
                norm,
                rect,
            } => {
                let hovered = rect.contains(state.cursor);
                if hovered {
                    fill(ctx, *rect, HOVER);
                }
                text_top(
                    ctx,
                    measurer,
                    label,
                    rect.left() + 2.0,
                    rect.top() + 4.0,
                    SMALL,
                    DIM,
                );
                text_right(
                    ctx,
                    measurer,
                    value,
                    rect.right() - 4.0,
                    rect.top() + 9.0,
                    SMALL,
                    if hovered { TEXT } else { DIM },
                );
                let track = param_track(*rect);
                ctx.fill_rounded_rect(track, 2.0, BORDER);
                let norm = norm.clamp(0.0, 1.0);
                let fill_w = (track.size.width * norm).max(0.0);
                if fill_w > 0.5 {
                    let color = if matches!(
                        hit,
                        Hit::LayerField(_) | Hit::MusicField(_) | Hit::ClipField(_)
                    ) {
                        ACCENT
                    } else {
                        ACCENT2
                    };
                    ctx.fill_rounded_rect(
                        Rect::from_min_size(track.origin, Size::new(fill_w, track.size.height)),
                        2.0,
                        color,
                    );
                }
                let knob = Vec2::new(track.left() + track.size.width * norm, track.center().y);
                ctx.fill_circle(knob, if hovered { 4.5 } else { 3.5 }, PANEL);
                ctx.fill_circle(knob, if hovered { 3.5 } else { 2.5 }, ACCENT);
            }
            Item::Options {
                label,
                rect,
                buttons,
            } => {
                text_top(
                    ctx,
                    measurer,
                    label,
                    rect.left() + 2.0,
                    rect.top() + 3.0,
                    SMALL,
                    DIM,
                );
                for button in buttons {
                    paint_button(state, measurer, ctx, button);
                }
            }
            Item::Toggle {
                label,
                value,
                action,
                rect,
            } => {
                if state.hover == Some(*action) {
                    fill(ctx, *rect, HOVER);
                }
                let box_rect = Rect::from_min_size(
                    Vec2::new(rect.left() + 2.0, rect.center().y - 5.0),
                    Size::new(10.0, 10.0),
                );
                fill(ctx, box_rect, if *value { ACCENT } else { RAISED });
                stroke(ctx, box_rect, BORDER_HI);
                text_top(
                    ctx,
                    measurer,
                    label,
                    rect.left() + 20.0,
                    rect.top() + 4.0,
                    SMALL,
                    TEXT,
                );
            }
            Item::Note { text, rect } => {
                text_top(
                    ctx,
                    measurer,
                    &ellipsis(measurer, text, SMALL, rect.size.width - 4.0),
                    rect.left() + 2.0,
                    rect.top() + 2.0,
                    SMALL,
                    FAINT,
                );
            }
        }
        clip_bottom = clip_bottom.max(rect.bottom());
    }
}

fn paint_modal(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let Some(modal) = &state.modal else {
        return;
    };
    fill(
        ctx,
        Rect::from_min_size(Vec2::ZERO, layout.viewport),
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    let rect = Rect::from_min_size(
        Vec2::new(
            (layout.viewport.width - 440.0) * 0.5,
            (layout.viewport.height - 108.0) * 0.5,
        ),
        Size::new(440.0, 108.0),
    );
    fill(ctx, rect, RAISED);
    stroke(ctx, rect, BORDER_HI);
    text_top(
        ctx,
        measurer,
        &modal.title,
        rect.left() + 12.0,
        rect.top() + 10.0,
        TITLE,
        ACCENT,
    );
    hline(ctx, rect.left(), rect.right(), rect.top() + 28.0, BORDER);
    let field = Rect::from_min_size(
        Vec2::new(rect.left() + 12.0, rect.top() + 38.0),
        Size::new(rect.size.width - 24.0, 26.0),
    );
    fill(ctx, field, PANEL);
    stroke(ctx, field, BORDER_HI);
    if modal.is_input() {
        let text = format!("{}{}", modal.text, "▌");
        text_top(
            ctx,
            measurer,
            &text,
            field.left() + 8.0,
            field.top() + 5.0,
            FONT,
            TEXT,
        );
    } else {
        let text = theme::ellipsis(measurer, &modal.text, SMALL, field.size.width - 16.0);
        text_top(
            ctx,
            measurer,
            &text,
            field.left() + 8.0,
            field.top() + 6.0,
            SMALL,
            DIM,
        );
    }
    text_top(
        ctx,
        measurer,
        if modal.is_input() {
            "ENTER CONFIRM   ESC CANCEL"
        } else {
            "ENTER DISCARD & OPEN   ESC CANCEL"
        },
        rect.left() + 12.0,
        rect.top() + 78.0,
        SMALL,
        FAINT,
    );
}

fn inspector_items(state: &State) -> Vec<RawItem> {
    let mut items = Vec::new();
    if state.tab == Tab::Patches {
        let Some(layer) = state.layer() else {
            items.push(RawItem::Note("NO LAYER".into()));
            return items;
        };
        items.push(RawItem::Section(format!(
            "LAYER {:02}  /  {}",
            state.selected_layer + 1,
            layer.wave.label()
        )));
        items.push(RawItem::Options {
            label: "WAVE".into(),
            columns: 5,
            options: Wave::ALL
                .iter()
                .map(|wave| {
                    (
                        wave.label().into(),
                        Action::SetWave(*wave),
                        layer.wave == *wave,
                    )
                })
                .collect(),
        });
        items.push(RawItem::Options {
            label: "LAYER".into(),
            columns: 3,
            options: vec![
                ("LAYER".into(), Action::AddLayer, false),
                ("DUP".into(), Action::DuplicateLayer, false),
                ("DEL".into(), Action::DeleteLayer, false),
            ],
        });
        items.push(RawItem::Section("TIMING".into()));
        for field in [LayerField::Delay, LayerField::Duration, LayerField::Gain] {
            items.push(layer_param(field, layer));
        }
        if let Some(index) = state.selected_clip {
            if let Some(clip) = layer.clips.get(index) {
                items.push(RawItem::Section(format!("CLIP {:02}", index + 1)));
                for field in [
                    ClipField::Start,
                    ClipField::Duration,
                    ClipField::Gain,
                    ClipField::Pitch,
                ] {
                    items.push(clip_param(field, clip));
                }
            }
        }
        items.push(RawItem::Options {
            label: "CLIPS".into(),
            columns: 2,
            options: vec![
                ("CLIP".into(), Action::AddClip, false),
                ("DEL CLIP".into(), Action::DeleteClip, false),
            ],
        });
        if layer.clips.is_empty() {
            items.push(RawItem::Note("IMPLICIT CLIP · layer delay/length".into()));
        } else {
            items.push(RawItem::Note(format!(
                "{} CLIP(S) · drag blocks on the timeline",
                layer.clips.len()
            )));
        }

        for section in layer_sections(layer.wave) {
            items.push(RawItem::Section(section.title.into()));
            for row in section.rows {
                if !(row.visible)(layer) {
                    continue;
                }
                match row.spec {
                    LayerRowSpec::Field(field) => items.push(layer_param(field, layer)),
                    LayerRowSpec::ToneEnvOptions => {
                        items.push(RawItem::Options {
                            label: "TONE ENV".into(),
                            columns: 2,
                            options: vec![
                                (
                                    ToneEnv::Adsr.label().into(),
                                    Action::SetToneEnv(ToneEnv::Adsr),
                                    layer.tone_env == ToneEnv::Adsr,
                                ),
                                (
                                    ToneEnv::Exp.label().into(),
                                    Action::SetToneEnv(ToneEnv::Exp),
                                    layer.tone_env == ToneEnv::Exp,
                                ),
                            ],
                        });
                    }
                    LayerRowSpec::EnvShapeOptions => {
                        items.push(RawItem::Options {
                            label: "ENV".into(),
                            columns: 2,
                            options: vec![
                                (
                                    "EXP".into(),
                                    Action::SetEnv(EnvShape::Exp),
                                    layer.env_shape == EnvShape::Exp,
                                ),
                                (
                                    "SINE".into(),
                                    Action::SetEnv(EnvShape::Sine),
                                    layer.env_shape == EnvShape::Sine,
                                ),
                            ],
                        });
                    }
                    LayerRowSpec::FilterOptions => items.push(filter_options(layer.filter)),
                }
            }
        }
        items.push(RawItem::Note(format!(
            "{} · {:.1} dB · bake {} Hz",
            layer.wave.label(),
            synth::db(layer.gain.max(0.0001)),
            state.patch().map(|patch| patch.rate).unwrap_or(22050)
        )));
    } else {
        let music = &state.project.music;
        items.push(RawItem::Section("MUSIC".into()));
        for field in [
            MusicField::Bpm,
            MusicField::Bars,
            MusicField::Rate,
            MusicField::Volume,
            MusicField::Swing,
        ] {
            items.push(music_param(field, music));
        }
        items.push(RawItem::Note(format!(
            "PREVIEW {} Hz · BAKE {} Hz",
            state.cache_rate, music.rate
        )));
        items.push(RawItem::Section("INSTRUMENTS".into()));
        items.push(RawItem::Options {
            label: "BASS".into(),
            columns: 5,
            options: MusicWave::ALL
                .iter()
                .map(|wave| {
                    (
                        wave.label().into(),
                        Action::BassWave(*wave),
                        music.bass_wave == *wave,
                    )
                })
                .collect(),
        });
        items.push(RawItem::Options {
            label: "ARP".into(),
            columns: 5,
            options: MusicWave::ALL
                .iter()
                .map(|wave| {
                    (
                        wave.label().into(),
                        Action::ArpWave(*wave),
                        music.arp_wave == *wave,
                    )
                })
                .collect(),
        });
        items.push(RawItem::Section("MIX".into()));
        for field in [
            MusicField::BassGain,
            MusicField::BassHarsh,
            MusicField::BassDecay,
            MusicField::ArpGain,
            MusicField::ArpHarsh,
            MusicField::ArpDecay,
            MusicField::KickGain,
            MusicField::HatGain,
            MusicField::Seed,
        ] {
            items.push(music_param(field, music));
        }
        items.push(RawItem::Section("HARMONY".into()));
        items.push(RawItem::Options {
            label: "CHORDS".into(),
            columns: 3,
            options: vec![
                ("Am F C G".into(), Action::ChordPreset(0), true),
                ("Cmaj7".into(), Action::ChordPreset(1), false),
                ("Em C G D".into(), Action::ChordPreset(2), false),
            ],
        });
        items.push(RawItem::Options {
            label: "TRANSPOSE".into(),
            columns: 2,
            options: vec![
                ("-1".into(), Action::Transpose(-1), false),
                ("+1".into(), Action::Transpose(1), false),
            ],
        });
        items.push(RawItem::Section("PATTERN".into()));
        items.push(RawItem::Toggle {
            label: "CUSTOM 16-STEP".into(),
            value: music.use_patterns,
            action: Action::TogglePatterns,
        });
        items.push(RawItem::Options {
            label: "RESET".into(),
            columns: 2,
            options: vec![
                ("DEFAULT".into(), Action::ResetPatterns, false),
                ("PRESETS".into(), Action::ChordPreset(0), false),
            ],
        });
        items.push(RawItem::Note(format!(
            "{} chord bars  ·  {} notes",
            music.chords.len(),
            music.chords.iter().map(|c| c.len()).sum::<usize>()
        )));
    }
    items
}

fn layer_param(field: LayerField, layer: &crate::model::Layer) -> RawItem {
    let value = field.get(layer);
    RawItem::Param {
        label: field.label().into(),
        value: field.format(value),
        hit: Hit::LayerField(field),
        norm: field.norm(value),
    }
}

fn clip_param(field: ClipField, clip: &crate::model::Clip) -> RawItem {
    let value = field.get(clip);
    RawItem::Param {
        label: field.label().into(),
        value: field.format(value),
        hit: Hit::ClipField(field),
        norm: field.norm(value),
    }
}

fn music_param(field: MusicField, music: &crate::model::Music) -> RawItem {
    let value = field.get(music);
    RawItem::Param {
        label: field.label().into(),
        value: field.format(value),
        hit: Hit::MusicField(field),
        norm: field.norm(value),
    }
}

fn filter_options(mode: FilterMode) -> RawItem {
    RawItem::Options {
        label: "MODE".into(),
        columns: 4,
        options: vec![
            (
                "OFF".into(),
                Action::SetFilter(FilterMode::None),
                mode == FilterMode::None,
            ),
            (
                "LP".into(),
                Action::SetFilter(FilterMode::Lowpass),
                mode == FilterMode::Lowpass,
            ),
            (
                "HP".into(),
                Action::SetFilter(FilterMode::Highpass),
                mode == FilterMode::Highpass,
            ),
            (
                "BP".into(),
                Action::SetFilter(FilterMode::Bandpass),
                mode == FilterMode::Bandpass,
            ),
        ],
    }
}

fn pattern_rows(state: &State) -> Vec<PatternRow> {
    let music = &state.project.music;
    let tones = music
        .chords
        .first()
        .map(|c| c.len().clamp(1, 4))
        .unwrap_or(3);
    let mut rows = Vec::new();
    for tone in 0..tones {
        rows.push(PatternRow::Arp(tone));
    }
    rows.push(PatternRow::Bass(0));
    rows.push(PatternRow::Bass(1));
    rows.push(PatternRow::Kick);
    rows.push(PatternRow::Hat);
    rows
}

fn pattern_on(music: &crate::model::Music, row: PatternRow, step: usize) -> bool {
    match row {
        PatternRow::Arp(tone) => music
            .arp_pattern
            .get(step)
            .map(|value| *value == tone as i32)
            .unwrap_or(false),
        PatternRow::Bass(octave) => music
            .bass_pattern
            .get(step)
            .map(|value| *value == octave as i32)
            .unwrap_or(false),
        PatternRow::Kick => music.kick_steps & (1 << step) != 0,
        PatternRow::Hat => music.hat_steps & (1 << step) != 0,
    }
}

fn tick_step(total: f32) -> f32 {
    for candidate in [0.05, 0.1, 0.25, 0.5, 1.0, 2.0] {
        if total / candidate <= 10.0 {
            return candidate;
        }
    }
    5.0
}

fn wave_color(wave: Wave) -> Color {
    match wave {
        Wave::Sine => hex(0x39E0AE),
        Wave::Triangle => hex(0x7DD3FC),
        Wave::Saw => hex(0xF6C177),
        Wave::Pulse => hex(0xF07178),
        Wave::Square => hex(0xC792EA),
        Wave::Noise => hex(0x8494A8),
        Wave::Arp => hex(0x82E2A6),
        Wave::Fm => hex(0xFF9E64),
        Wave::Pluck => hex(0x9CDCFE),
        Wave::Kick => hex(0xF78C6C),
        Wave::Snare => hex(0xE6C889),
        Wave::Hat => hex(0xB8C7DA),
        Wave::Clap => hex(0xFF6E9C),
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_clip_block(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
    row_rect: &Rect,
    start: f32,
    duration: f32,
    gain: f32,
    layer: &crate::model::Layer,
    layer_selected: bool,
    clip_selected: bool,
    layer_index: usize,
    clip_index: Option<usize>,
) {
    let x0 = layout.time_x(start);
    let x1 = layout.time_x(start + duration);
    let block = Rect::from_min_size(
        Vec2::new(x0, row_rect.top() + 3.0),
        Size::new((x1 - x0).max(3.0), TRACK_H - 6.0),
    );
    let hovered = matches!(
        state.hover_hit,
        Some(Hit::Timeline {
            layer,
            clip,
            ..
        }) if layer == layer_index && clip == clip_index
    );
    let dragging = match &state.drag {
        Drag::Clip { layer, clip, .. } => *layer == layer_index && Some(*clip) == clip_index,
        _ => false,
    };
    let color = wave_color(layer.wave);
    let alpha = (if layer.enabled { 1.0 } else { 0.45 }) * gain.clamp(0.5, 1.0);
    let alpha = if hovered || dragging {
        (alpha + 0.05).min(1.0)
    } else {
        alpha
    };
    fill(ctx, block, Color::new(color.r, color.g, color.b, alpha));

    // Always-visible trim handles; brighter while hovered / selected / dragging.
    let handle = 4.0;
    let active = hovered || dragging || clip_selected;
    let handle_color = if active {
        ACCENT2
    } else {
        Color::new(
            (color.r + 0.2).min(1.0),
            (color.g + 0.2).min(1.0),
            (color.b + 0.2).min(1.0),
            0.55,
        )
    };
    fill(
        ctx,
        Rect::from_min_size(block.origin, Size::new(handle, block.size.height)),
        Color::new(
            handle_color.r,
            handle_color.g,
            handle_color.b,
            if active { 0.95 } else { 0.45 },
        ),
    );
    fill(
        ctx,
        Rect::from_min_size(
            Vec2::new(block.right() - handle, block.top()),
            Size::new(handle, block.size.height),
        ),
        Color::new(
            handle_color.r,
            handle_color.g,
            handle_color.b,
            if active { 0.95 } else { 0.45 },
        ),
    );
    let grip = if active {
        BG
    } else {
        Color::new(0.0, 0.0, 0.0, 0.45)
    };
    let grip_y = block.center().y;
    ctx.draw_line(
        Vec2::new(block.left() + 1.0, grip_y - 2.0),
        Vec2::new(block.left() + handle - 1.0, grip_y - 2.0),
        1.0,
        grip,
    );
    ctx.draw_line(
        Vec2::new(block.left() + 1.0, grip_y + 2.0),
        Vec2::new(block.left() + handle - 1.0, grip_y + 2.0),
        1.0,
        grip,
    );
    ctx.draw_line(
        Vec2::new(block.right() - handle + 1.0, grip_y - 2.0),
        Vec2::new(block.right() - 1.0, grip_y - 2.0),
        1.0,
        grip,
    );
    ctx.draw_line(
        Vec2::new(block.right() - handle + 1.0, grip_y + 2.0),
        Vec2::new(block.right() - 1.0, grip_y + 2.0),
        1.0,
        grip,
    );

    if clip_selected || hovered || dragging {
        stroke(ctx, block, ACCENT2);
    } else if layer_selected {
        stroke(ctx, block, ACCENT);
    }
    if block.size.width > 52.0 {
        let label = match clip_index {
            Some(index) => format!("{} · {}", layer.wave.label(), index + 1),
            None => layer.wave.label().to_string(),
        };
        text_top(
            ctx,
            measurer,
            &label,
            block.left() + handle + 3.0,
            block.top() + 3.0,
            SMALL,
            BG,
        );
    }
}

fn drag_info(state: &State) -> Option<String> {
    match &state.drag {
        Drag::Seek { .. } => Some(format!(
            "PLAYHEAD  {:.3}s  ·  drag to scrub",
            state.playhead
        )),
        Drag::InspectorScroll { .. } => Some("PARAMETERS  scrolling".into()),
        Drag::AutomationPoint { layer, clip, index } => {
            let data = state
                .patch()
                .and_then(|p| p.layers.get(*layer))
                .and_then(|l| l.clips.get(*clip))
                .and_then(|c| state.automation_lane.points(c).get(*index))?;
            Some(format!(
                "AUTOMATION {:02}  {}  T {:.3}s  {:.2}",
                index + 1,
                state.automation_lane.label(),
                data.time,
                data.value
            ))
        }
        Drag::Clip {
            layer, clip, mode, ..
        } => {
            let data = state
                .patch()
                .and_then(|p| p.layers.get(*layer))
                .and_then(|l| l.clips.get(*clip))?;
            let action = match mode {
                TimelineDrag::Move => "MOVE",
                TimelineDrag::Resize => "RESIZE",
                TimelineDrag::ResizeLeft => "TRIM",
            };
            Some(format!(
                "CLIP {:02}  {action}  START {:.3}s  LEN {:.3}s  GAIN {:.2}  ·  SNAP 5ms",
                clip + 1,
                data.start,
                data.duration,
                data.gain
            ))
        }
        Drag::ClipField {
            layer, clip, field, ..
        } => {
            let data = state
                .patch()
                .and_then(|p| p.layers.get(*layer))
                .and_then(|l| l.clips.get(*clip))?;
            Some(format!(
                "CLIP {:02}  {}  {}",
                clip + 1,
                field.label(),
                field.format(field.get(data))
            ))
        }
        Drag::Layer { field, .. } => {
            let layer = state.layer()?;
            Some(format!(
                "LAYER  {}  {}",
                field.label(),
                field.format(field.get(layer))
            ))
        }
        Drag::Music { field, .. } => Some(format!(
            "MUSIC  {}  {}",
            field.label(),
            field.format(field.get(&state.project.music))
        )),
        Drag::None => None,
    }
}

fn hover_info(state: &State) -> Option<String> {
    match state.hover_hit {
        Some(Hit::Action(action)) => {
            let tip = action_tip(action);
            if tip.is_empty() {
                None
            } else {
                Some(tip.to_string())
            }
        }
        Some(Hit::Search) => Some("搜索音效 · Search（/）".into()),
        Some(Hit::CategoryFilter) => Some("切换分类过滤 · Category（C）".into()),
        Some(Hit::FavoriteFilter) => Some("只看收藏 · Favorites".into()),
        Some(Hit::InspectorScrollbar) => Some("拖动滚动参数面板 · Scroll".into()),
        Some(Hit::MenuItem(action)) => {
            let tip = action_tip(action);
            if tip.is_empty() {
                None
            } else {
                Some(tip.to_string())
            }
        }
        Some(Hit::Waveform(time)) => Some(format!("拖动定位播放头 · Playhead {time:.3}s")),
        Some(Hit::Timeline { layer, clip, .. }) => {
            let layer_data = state.patch()?.layers.get(layer)?;
            if let Some(index) = clip {
                let data = layer_data.clips.get(index)?;
                Some(format!(
                    "CLIP {:02}  START {:.3}s  LEN {:.3}s  GAIN {:.2}  ·  drag body / edges",
                    index + 1,
                    data.start,
                    data.duration,
                    data.gain
                ))
            } else {
                Some(format!(
                    "IMPLICIT CLIP  START {:.3}s  LEN {:.3}s  ·  drag to make it explicit",
                    layer_data.delay, layer_data.duration
                ))
            }
        }
        Some(Hit::Ruler(time)) => Some(format!("PLAYHEAD  {time:.3}s  ·  click to seek")),
        Some(Hit::AutomationPoint(index)) => {
            let clip = state.selected_clip?;
            let layer = state.patch()?.layers.get(state.selected_layer)?;
            let point = state
                .automation_lane
                .points(layer.clips.get(clip)?)
                .get(index)?;
            Some(format!(
                "AUTOMATION {:02}  {}  T {:.3}s  {:.2}  ·  drag / Delete",
                index + 1,
                state.automation_lane.label(),
                point.time,
                point.value
            ))
        }
        Some(Hit::AutomationLane { time, value }) => {
            let kind = state.automation_lane;
            let value = if kind == AutomationKind::Gain {
                format!("{value:.2}")
            } else {
                format!("{value:.0} Hz")
            };
            Some(format!(
                "AUTOMATION {}  T {time:.3}s  {value}  ·  click to add point",
                kind.label()
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::State;

    #[test]
    fn layout_exposes_library_filters_and_system_menu() {
        let mut state = State::new();
        state.menu_open = true;
        let layout = Layout::new(&state, state.viewport);
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
            .any(|(action, _, _)| *action == Action::ExportWav));
    }

    #[test]
    fn system_menu_paints_and_hit_tests_actions() {
        let mut state = State::new();
        state.menu_open = true;
        let layout = Layout::new(&state, state.viewport);
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
        let mut state = State::new();
        let viewport = Size::new(1024.0, 720.0);
        state.viewport = viewport;
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
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        let layout = Layout::new(&state, state.viewport);
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
    fn inspector_renders_the_layer_registry() {
        let mut state = State::new();
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
    fn favorite_star_hit_is_split_from_the_row() {
        let state = State::new();
        let layout = Layout::new(&state, state.viewport);
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
        let mut state = State::new();
        let span = state.timeline_span();
        let base = Layout::new(&state, state.viewport).timeline_scale;
        state.timeline_pin = Some(span);
        let layer = state.layer_mut().unwrap();
        layer.ensure_clips();
        layer.clips[0].duration += 1.5;
        let pinned = Layout::new(&state, state.viewport).timeline_scale;
        assert!(
            (pinned - base).abs() < 0.001,
            "pinned scale moved from {base} to {pinned}"
        );
        state.timeline_pin = None;
        let refit = Layout::new(&state, state.viewport).timeline_scale;
        assert!(refit < pinned, "release should refit the wider timeline");
    }

    #[test]
    fn timeline_grows_with_layers_and_gives_space_to_the_waveform() {
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        let few = Layout::new(&state, state.viewport);
        assert!(
            few.timeline.size.height <= 200.0,
            "3 layers should not claim the whole column"
        );
        assert!(few.waveform.size.height > 128.0);
        let layer = state.project.patches[0].layers[0].clone();
        for _ in 0..30 {
            state.project.patches[0].layers.push(layer.clone());
        }
        let many = Layout::new(&state, state.viewport);
        assert!(many.timeline.size.height > few.timeline.size.height);
        assert!(many.waveform.size.height < few.waveform.size.height);
        assert!(many.layer_rows.len() <= layer_capacity(state.viewport, 31));
        state.tab = Tab::Music;
        let music = Layout::new(&state, state.viewport);
        assert!(music.timeline.size.height < many.timeline.size.height);
        assert!(music.pattern_grid.size.height <= music.timeline.size.height);
    }

    #[test]
    fn layout_exposes_undo_and_clip_fields() {
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.layer_mut().unwrap().ensure_clips();
        state.selected_clip = Some(0);
        let layout = Layout::new(&state, state.viewport);
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
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.layer_mut().unwrap().ensure_clips();
        state.selected_clip = Some(0);
        state.layer_mut().unwrap().clips[0].add_point(AutomationKind::Gain, 0.05, 1.5);
        let layout = Layout::new(&state, state.viewport);
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
        let mut state = State::new();
        state.viewport = Size::new(crate::theme::VIEW_W, crate::theme::VIEW_H);
        state.layer_mut().unwrap().ensure_clips();
        let layout = Layout::new(&state, state.viewport);
        let clip = &state.layer().unwrap().clips[0];
        let x = layout.time_x(clip.start + clip.duration * 0.5);
        let row = layout
            .layer_rows
            .iter()
            .find(|(index, _)| *index == state.selected_layer)
            .map(|(_, rect)| *rect)
            .unwrap();
        let hit = hit(&state, &layout, Vec2::new(x, row.center().y));
        assert!(matches!(hit, Some(Hit::Timeline { clip: Some(0), .. })));
    }
}
