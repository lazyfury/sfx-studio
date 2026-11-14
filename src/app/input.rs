//! Input event handling, pointer gestures and the per-frame AppLogic bridge.

use super::*;

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
    pub(super) fn set_param_from_x(
        &mut self,
        position: Vec2,
        target: ui::Hit,
        layout: &ui::Layout,
    ) {
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
