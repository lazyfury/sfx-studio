//! Hit-testing shared by painting and input.

use super::layout::automation_tabs;
use super::*;

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
    if let Some(menu) = layout.category_menu {
        if menu.contains(pos) {
            for (category, rect) in &layout.category_items {
                if rect.contains(pos) {
                    return Some(Hit::CategoryPick(*category));
                }
            }
            return Some(Hit::CategoryMenu);
        }
    }
    for (mode, rect) in &layout.library_mode_buttons {
        if rect.contains(pos) {
            return Some(Hit::LibraryMode(*mode));
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
    if layout
        .install_presets
        .is_some_and(|rect| rect.contains(pos))
    {
        return Some(Hit::InstallPresets);
    }
    if layout.library_scrollbar.contains(pos) {
        return Some(Hit::LibraryScrollbar);
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
    for (index, rect) in &layout.preset_rows {
        if rect.contains(pos) {
            if patch_star_rect(*rect).contains(pos) {
                return Some(Hit::PresetFavorite(*index));
            }
            return Some(Hit::PresetRow(*index));
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
                Item::Curve { kind, rect } if rect.contains(pos) => {
                    return Some(Hit::Curve(*kind));
                }
                _ => {}
            }
        }
    }

    if state.view.tab == Tab::Patches && layout.waveform.contains(pos) {
        let duration = state.patch().map(|patch| patch.end()).unwrap_or(0.0);
        let t = ((pos.x - layout.waveform.left()) / layout.waveform.size.width.max(1.0))
            .clamp(0.0, 1.0)
            * duration;
        return Some(Hit::Waveform(t));
    }
    if state.view.tab == Tab::Patches && layout.timeline.contains(pos) {
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
    if state.view.tab == Tab::Patches && layout.automation.contains(pos) {
        for (kind, rect) in automation_tabs(layout) {
            if rect.contains(pos) {
                return Some(Hit::Action(Action::AutomationLane(kind)));
            }
        }
        if let Some(clip) = state.clip() {
            let kind = state.view.automation_lane;
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
    for (row, step, rect) in &layout.pattern_cells {
        if rect.contains(pos) {
            return Some(Hit::Pattern(*row, *step));
        }
    }
    None
}
