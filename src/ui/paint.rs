//! All painting for the studio shell.

use super::layout::{automation_tabs, pattern_on, pattern_rows};
use super::*;

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
    if layout.library_scrollbar.size.height > 0.0 {
        ctx.fill_rounded_rect(layout.library_scrollbar, 2.0, RAISED);
        ctx.fill_rounded_rect(
            layout.library_thumb,
            2.0,
            if state.hover_hit == Some(Hit::LibraryScrollbar) {
                ACCENT
            } else {
                DIM
            },
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
            "{} Hz  ·  {:.3}s  ·  {} LAYERS  ·  peak {:.1} dB{}",
            state.cache_rate,
            patch.map(|p| p.end()).unwrap_or(0.0),
            patch.map(|p| p.layers.len()).unwrap_or(0),
            synth::db(state.cache_peak),
            if state.timeline_zoom > 1.01 {
                format!("  ·  {:.1}×", state.timeline_zoom)
            } else {
                String::new()
            }
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
    paint_category_menu(state, measurer, ctx, &layout);

    if state.modal.is_some() {
        paint_modal(state, measurer, ctx, &layout);
    } else if matches!(state.drag, Drag::None) && !state.menu_open {
        paint_hover_tip(state, measurer, ctx);
    }
}

fn paint_category_menu(
    state: &State,
    measurer: &dyn TextMeasurer,
    ctx: &mut PaintContext,
    layout: &Layout,
) {
    let Some(rect) = layout.category_menu else {
        return;
    };
    fill(ctx, rect, RAISED);
    stroke(ctx, rect, BORDER_HI);
    for (category, item) in &layout.category_items {
        let active = state.category_filter == *category;
        if item.contains(state.cursor) {
            fill(ctx, *item, ACTIVE);
        }
        text_top(
            ctx,
            measurer,
            category.map(Category::label).unwrap_or("ALL"),
            item.left() + 6.0,
            item.top() + 4.0,
            SMALL,
            if active { ACCENT } else { TEXT },
        );
        if active {
            icons::paint(
                ctx,
                Icon::Check,
                Rect::from_min_size(
                    Vec2::new(item.right() - 16.0, item.center().y - 5.0),
                    Size::splat(10.0),
                ),
                ACCENT,
            );
        }
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

    let start = layout.timeline_offset;
    let end = start + layout.timeline_visible;
    let step = tick_step(layout.timeline_visible);
    let mut time = (start / step).floor() * step;
    while time <= end + 0.001 {
        if time >= start - 0.001 {
            let x = layout.time_x(time);
            vline(
                ctx,
                x,
                ruler.bottom(),
                rect.bottom(),
                Color::new(1.0, 1.0, 1.0, 0.045),
            );
            vline(ctx, x, ruler.top(), ruler.bottom(), BORDER_HI);
            let label = format!("{time:.2}");
            if x + 3.0 + measurer.measure_run(&label, SMALL) <= rect.right() - 4.0 {
                text_top(
                    ctx,
                    measurer,
                    &label,
                    x + 3.0,
                    ruler.top() + 2.0,
                    SMALL,
                    DIM,
                );
            }
        }
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
    if px >= rect.left() && px <= rect.right() {
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
        let label_x = (px + 6.0)
            .min(rect.right() - label_w - 8.0)
            .max(rect.left() + 4.0);
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
    }
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

pub(crate) fn inspector_items(state: &State) -> Vec<RawItem> {
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
                for field in CLIP_FIELDS {
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
        for section in music_sections() {
            items.push(RawItem::Section(section.title.into()));
            for row in section.rows {
                match row {
                    MusicRowSpec::Field(field) => items.push(music_param(*field, music)),
                    MusicRowSpec::BassWaveOptions => items.push(RawItem::Options {
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
                    }),
                    MusicRowSpec::ArpWaveOptions => items.push(RawItem::Options {
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
                    }),
                    MusicRowSpec::ChordPresets => items.push(RawItem::Options {
                        label: "CHORDS".into(),
                        columns: 3,
                        options: vec![
                            ("Am F C G".into(), Action::ChordPreset(0), true),
                            ("Cmaj7".into(), Action::ChordPreset(1), false),
                            ("Em C G D".into(), Action::ChordPreset(2), false),
                        ],
                    }),
                    MusicRowSpec::TransposeOptions => items.push(RawItem::Options {
                        label: "TRANSPOSE".into(),
                        columns: 2,
                        options: vec![
                            ("-1".into(), Action::Transpose(-1), false),
                            ("+1".into(), Action::Transpose(1), false),
                        ],
                    }),
                    MusicRowSpec::PatternToggle => items.push(RawItem::Toggle {
                        label: "CUSTOM 16-STEP".into(),
                        value: music.use_patterns,
                        action: Action::TogglePatterns,
                    }),
                    MusicRowSpec::ResetOptions => items.push(RawItem::Options {
                        label: "RESET".into(),
                        columns: 2,
                        options: vec![
                            ("DEFAULT".into(), Action::ResetPatterns, false),
                            ("PRESETS".into(), Action::ChordPreset(0), false),
                        ],
                    }),
                    MusicRowSpec::PreviewNote => items.push(RawItem::Note(format!(
                        "PREVIEW {} Hz · BAKE {} Hz",
                        state.cache_rate, music.rate
                    ))),
                    MusicRowSpec::ChordNote => items.push(RawItem::Note(format!(
                        "{} chord bars  ·  {} notes",
                        music.chords.len(),
                        music.chords.iter().map(|c| c.len()).sum::<usize>()
                    ))),
                }
            }
        }
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
        Drag::LibraryScroll { .. } => Some("LIBRARY  scrolling".into()),
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
            let grid = if state.modifiers.shift {
                "FREE"
            } else {
                "SNAP 5ms"
            };
            Some(format!(
                "CLIP {:02}  {action}  START {:.3}s  LEN {:.3}s  GAIN {:.2}  ·  {grid}",
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
