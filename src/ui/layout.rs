//! Layout metrics, capacity helpers and the shared `Layout` geometry.

use super::paint::inspector_items;
use super::*;

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
    pub timeline_offset: f32,
    pub timeline_visible: f32,
    pub category_rect: Rect,
    pub category_menu: Option<Rect>,
    pub category_items: Vec<(Option<Category>, Rect)>,
    pub favorite_rect: Rect,
    pub search_rect: Rect,
    pub library_mode_buttons: Vec<(LibraryMode, Rect)>,
    pub preset_rows: Vec<(usize, Rect)>,
    /// One-click recovery when a project has no `presets/` files yet.
    pub install_presets: Option<Rect>,
    pub library_scrollbar: Rect,
    pub library_thumb: Rect,
    pub library_max_start: usize,
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

        // PROJECT / PRESETS live in the left title strip so they do not steal
        // a list row from the compact library.
        let mode_w = 54.0;
        let mode_h = 20.0;
        let mode_y = left.top() + 3.0;
        let presets_mode = Rect::from_min_size(
            Vec2::new(left.right() - 10.0 - mode_w, mode_y),
            Size::new(mode_w, mode_h),
        );
        let project_mode = Rect::from_min_size(
            Vec2::new(presets_mode.left() - 4.0 - mode_w, mode_y),
            Size::new(mode_w, mode_h),
        );
        let library_mode_buttons = vec![
            (LibraryMode::Project, project_mode),
            (LibraryMode::Presets, presets_mode),
        ];

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
                active: action == Action::Loop && state.playback.loop_play,
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
                active: state.view.tab == tab,
                icon: Some(icon),
            });
            tab_x += 76.0;
        }

        let mut menu_rect = None;
        let mut menu_items = Vec::new();
        let mut menu_sections = Vec::new();
        if state.view.menu_open {
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
        let mut category_menu = None;
        let mut category_items = Vec::new();
        if state.view.category_menu_open {
            let row_h = 22.0;
            let choices = Category::ALL.len() + 1;
            let rect = Rect::from_min_size(
                Vec2::new(category_rect.left(), category_rect.bottom() + 4.0),
                Size::new(category_rect.size.width, choices as f32 * row_h + 8.0),
            );
            let mut y = rect.top() + 4.0;
            for category in std::iter::once(None).chain(Category::ALL.iter().copied().map(Some)) {
                category_items.push((
                    category,
                    Rect::from_min_size(
                        Vec2::new(rect.left() + 4.0, y),
                        Size::new(rect.size.width - 8.0, row_h),
                    ),
                ));
                y += row_h;
            }
            category_menu = Some(rect);
        }
        let music_row = Rect::from_min_size(
            Vec2::new(left.left(), left.bottom() - MUSIC_ROW_OFFSET),
            Size::new(left.size.width, LIBRARY_ROW_H),
        );
        let row_h = LIBRARY_ROW_H;
        let capacity = library_capacity(viewport);
        let visible_rows = state.visible_library_rows();
        let library_start = state
            .library_scroll()
            .min(visible_rows.len().saturating_sub(1));
        let mut patch_rows = Vec::new();
        let mut preset_rows = Vec::new();
        for visible in 0..capacity {
            let Some(index) = visible_rows.get(library_start + visible).copied() else {
                break;
            };
            let rect = Rect::from_min_size(
                Vec2::new(left.left(), list_top + visible as f32 * row_h),
                Size::new(left.size.width - 8.0, row_h),
            );
            match state.view.library_mode {
                LibraryMode::Project => patch_rows.push((index, rect)),
                LibraryMode::Presets => preset_rows.push((index, rect)),
            }
        }

        let install_presets = (state.view.library_mode == LibraryMode::Presets
            && state.presets.presets.is_empty()
            && state.doc.root.is_some())
        .then(|| {
            Rect::from_min_size(
                Vec2::new(left.left() + 12.0, left.top() + 114.0),
                Size::new(left.size.width - 24.0, 24.0),
            )
        });

        let layer_count = state.patch().map(|p| p.layers.len()).unwrap_or(0);
        let mut library_scrollbar = Rect::from_min_size(Vec2::ZERO, Size::ZERO);
        let mut library_thumb = Rect::from_min_size(Vec2::ZERO, Size::ZERO);
        let library_max_start = visible_rows.len().saturating_sub(capacity);
        if library_max_start > 0 {
            if let Some((_, first)) = patch_rows.first().or_else(|| preset_rows.first()) {
                let track_top = first.top();
                let track_bottom = music_row.top() - LIBRARY_END_GAP;
                let track_h = (track_bottom - track_top).max(1.0);
                let thumb_h =
                    ((capacity as f32 / visible_rows.len() as f32) * track_h).clamp(18.0, track_h);
                let progress = library_start as f32 / library_max_start as f32;
                library_scrollbar = Rect::from_min_size(
                    Vec2::new(left.right() - 6.0, track_top),
                    Size::new(4.0, track_h),
                );
                library_thumb = Rect::from_min_size(
                    Vec2::new(
                        left.right() - 6.0,
                        track_top + progress * (track_h - thumb_h),
                    ),
                    Size::new(4.0, thumb_h),
                );
            }
        }

        let timeline_h = if state.view.tab == Tab::Music {
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
        let view_total = total + 0.08;
        let plot_w = (timeline.size.width - TIMELINE_LABEL - 8.0).max(1.0);
        // Fit is zoom 1.0; ⌘/Ctrl + wheel scales beyond it.
        let timeline_scale =
            ((plot_w / view_total) * state.view.timeline_zoom.clamp(1.0, 32.0)).max(8.0);
        let timeline_visible = plot_w / timeline_scale;
        let timeline_offset = state
            .view
            .timeline_offset
            .clamp(0.0, (view_total - timeline_visible).max(0.0));
        let mut layer_rows = Vec::new();
        if let Some(patch) = state.patch() {
            let visible = layer_capacity(viewport, layer_count);
            let start = state
                .view
                .layer_scroll
                .min(patch.layers.len().saturating_sub(1));
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
        let mut y = inspector.top() + 4.0 - state.view.inspector_scroll;
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
            let t = (state.view.inspector_scroll / max_scroll).clamp(0.0, 1.0);
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
        if state.view.tab == Tab::Music {
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
            timeline_offset,
            timeline_visible,
            category_rect,
            category_menu,
            category_items,
            favorite_rect,
            search_rect,
            library_mode_buttons,
            preset_rows,
            install_presets,
            library_scrollbar,
            library_thumb,
            library_max_start,
            project_anchor,
            inspector,
            inspector_content_h,
            inspector_scrollbar,
            inspector_thumb,
        }
    }

    pub fn time_x(&self, time: f32) -> f32 {
        self.timeline.left() + TIMELINE_LABEL + (time - self.timeline_offset) * self.timeline_scale
    }

    pub fn x_time(&self, x: f32) -> f32 {
        (self.timeline_offset + (x - self.timeline.left() - TIMELINE_LABEL) / self.timeline_scale)
            .max(0.0)
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

/// Resolve where a dragged preset would land. Timeline rows win over the left
/// panel so a drop on the track area always inserts audio.
pub fn drop_target(state: &State, layout: &Layout, pos: Vec2) -> Option<DropTarget> {
    if state.view.tab == Tab::Patches && layout.timeline.contains(pos) {
        let count = state.patch().map(|patch| patch.layers.len()).unwrap_or(0);
        let mut layer = count;
        for (index, row) in &layout.layer_rows {
            if pos.y < row.bottom() {
                layer = *index;
                break;
            }
        }
        if pos.y < layout.timeline.top() + RULER_H {
            layer = layout
                .layer_rows
                .first()
                .map(|(index, _)| *index)
                .unwrap_or(0);
        }
        let plot_left = layout.timeline.left() + TIMELINE_LABEL;
        let time = if pos.x >= plot_left {
            layout.x_time(pos.x)
        } else {
            state.playback.playhead
        };
        return Some(DropTarget::Timeline {
            layer,
            time: time.clamp(0.0, 8.0),
        });
    }
    if layout.left.contains(pos) {
        return Some(DropTarget::ProjectLibrary);
    }
    None
}

pub(super) fn automation_tabs(layout: &Layout) -> [(AutomationKind, Rect); 2] {
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

pub(super) fn pattern_rows(state: &State) -> Vec<PatternRow> {
    let music = &state.doc.project.music;
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

pub(super) fn pattern_on(music: &crate::model::Music, row: PatternRow, step: usize) -> bool {
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
