//! Input event handling, pointer gestures and the per-frame AppLogic bridge.

use super::*;

impl SfxLogic {
    /// Testable input entry point. `AppLogic::event` delegates here; tests can
    /// drive pointer/key sequences without constructing an `EventContext`.
    pub(super) fn handle_event(&mut self, event: &InputEvent) -> EventResult {
        if let InputEvent::ModifiersChanged(modifiers) = event {
            self.state.view.modifiers = *modifiers;
            return EventResult::Ignored;
        }
        if self.handle_modal(event) {
            return EventResult::Handled;
        }

        if self.state.view.search_focus {
            match event {
                InputEvent::TextInput { text } => {
                    for ch in text.chars() {
                        if !ch.is_control() && self.state.view.search.len() < 32 {
                            self.state.view.search.push(ch);
                        }
                    }
                    self.refresh_filter();
                    return EventResult::Handled;
                }
                InputEvent::KeyDown { key } => match key {
                    Key::Backspace => {
                        self.state.view.search.pop();
                        self.refresh_filter();
                        return EventResult::Handled;
                    }
                    Key::Enter => {
                        self.state.view.search_focus = false;
                        return EventResult::Handled;
                    }
                    Key::Escape => {
                        self.state.view.search_focus = false;
                        self.state.view.search.clear();
                        self.refresh_filter();
                        return EventResult::Handled;
                    }
                    _ if !(self.state.view.modifiers.meta || self.state.view.modifiers.ctrl) => {
                        return EventResult::Handled;
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        let layout_rc = self.layout();
        let layout = layout_rc.as_ref();
        match event {
            InputEvent::KeyDown { key } => {
                let cmd = self.state.view.modifiers.meta || self.state.view.modifiers.ctrl;
                match key {
                    Key::Space => {
                        self.toggle_play();
                        return EventResult::Handled;
                    }
                    Key::Tab => {
                        let next = match self.state.view.tab {
                            Tab::Patches => Tab::Music,
                            Tab::Music => Tab::Patches,
                        };
                        self.switch_tab(next);
                        return EventResult::Handled;
                    }
                    Key::Escape => {
                        if matches!(self.state.view.drag, Drag::LibraryItem { .. }) {
                            self.state.view.drag = Drag::None;
                            self.state.bump_view();
                        } else if self.state.view.overlays.close_top().is_some() {
                            self.state.bump_view();
                        } else {
                            self.stop();
                        }
                        return EventResult::Handled;
                    }
                    Key::Delete => {
                        // Delete acts on the focused object. When an asset row
                        // is focused there is no deletable document object, and
                        // silently deleting a layer/automation point would
                        // violate the read-only library boundary.
                        if matches!(
                            self.state.selected_library_kind(),
                            Some(
                                crate::library::LibraryKind::Audio
                                    | crate::library::LibraryKind::Preset
                                    | crate::library::LibraryKind::Music
                            )
                        ) {
                            self.state
                                .toast("选中的是资源条目 · 先点击图层或 SOUND 再编辑");
                            return EventResult::Handled;
                        }
                        if self.state.view.tab == Tab::Patches {
                            if let Some(index) = self
                                .state
                                .view
                                .selected_env_point
                                .filter(|_| self.state.clip_index().is_some())
                            {
                                self.push_undo();
                                let kind = self.state.view.automation_lane;
                                if let Some(clip) = self.state.clip_mut() {
                                    let points = kind.points_mut(clip);
                                    if index < points.len() {
                                        points.remove(index);
                                        self.state.view.selected_env_point = None;
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
                        self.navigate_library(-1);
                        return EventResult::Handled;
                    }
                    Key::ArrowDown => {
                        self.navigate_library(1);
                        return EventResult::Handled;
                    }
                    Key::ArrowLeft => {
                        let index = self.state.layer_index().unwrap_or(0).saturating_sub(1);
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Key::ArrowRight => {
                        let index = self.state.layer_index().unwrap_or(0) + 1;
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Key::Character(ch) if ('1'..='8').contains(ch) => {
                        let index = *ch as usize - '1' as usize;
                        self.trigger_pad(index);
                        return EventResult::Handled;
                    }
                    Key::Character('/') if !cmd => {
                        self.state.view.search_focus = true;
                        return EventResult::Handled;
                    }
                    Key::Character('f') if !cmd => {
                        match self.state.view.selected_library {
                            Some(crate::library::LibraryKey::Preset(id)) => {
                                if let Some(index) = self.state.presets.index_of(id) {
                                    self.toggle_preset_favorite(index);
                                }
                            }
                            Some(crate::library::LibraryKey::Sound(_)) | None => {
                                self.activate(ui::Action::Favorite)
                            }
                            _ => {}
                        }
                        return EventResult::Handled;
                    }
                    _ => {}
                }
                if let Some(action) = ui::shortcut_action(*key, self.state.view.modifiers) {
                    self.activate(action);
                    return EventResult::Handled;
                }
            }
            InputEvent::PointerDown { position, button } if *button == PointerButton::Left => {
                self.state.view.cursor = *position;
                let pointer_hit = ui::hit(&self.state, layout, *position);
                self.state.view.hover_hit = pointer_hit;
                if pointer_hit != Some(ui::Hit::Search) {
                    self.state.view.search_focus = false;
                }
                if let Some(overlay) = self.state.view.overlays.top() {
                    match overlay {
                        Overlay::Menu => match pointer_hit {
                            Some(ui::Hit::MenuToggle) => {
                                self.state.view.overlays.close(overlay);
                            }
                            Some(ui::Hit::MenuItem(action)) => {
                                self.state.view.overlays.close(overlay);
                                self.activate(action);
                            }
                            Some(ui::Hit::MenuPanel) => {}
                            _ => {
                                // Outside click closes the menu and is consumed.
                                self.state.view.overlays.close(overlay);
                            }
                        },
                        Overlay::KindFilter => match pointer_hit {
                            Some(ui::Hit::MenuToggle) => {
                                self.state.view.overlays.open(Overlay::Menu)
                            }
                            Some(ui::Hit::CategoryFilter) => {
                                self.state.view.overlays.open(Overlay::CategoryFilter)
                            }
                            Some(ui::Hit::KindPick(kind)) => self.pick_kind(kind),
                            Some(ui::Hit::KindMenu | ui::Hit::KindFilter) => {
                                self.state.view.overlays.close(overlay);
                            }
                            _ => {
                                self.state.view.overlays.close(overlay);
                            }
                        },
                        Overlay::CategoryFilter => match pointer_hit {
                            Some(ui::Hit::MenuToggle) => {
                                self.state.view.overlays.open(Overlay::Menu)
                            }
                            Some(ui::Hit::KindFilter) => {
                                self.state.view.overlays.open(Overlay::KindFilter)
                            }
                            Some(ui::Hit::CategoryPick(category)) => self.pick_category(category),
                            Some(ui::Hit::CategoryMenu | ui::Hit::CategoryFilter) => {
                                self.state.view.overlays.close(overlay);
                            }
                            _ => {
                                self.state.view.overlays.close(overlay);
                            }
                        },
                    }
                    self.state.bump_view();
                    return EventResult::Handled;
                }
                if pointer_hit == Some(ui::Hit::MenuToggle) {
                    self.state.view.overlays.open(Overlay::Menu);
                    self.state.bump_view();
                    return EventResult::Handled;
                }
                match pointer_hit {
                    Some(ui::Hit::Search) => {
                        self.state.view.search_focus = true;
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::InstallPresets) => {
                        self.activate(ui::Action::InstallPresets);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Curve(kind)) => {
                        self.state.focus_document();
                        self.state.begin_edit();
                        self.set_curve_from_pos(kind, *position, layout);
                        self.state.view.drag = Drag::Curve { kind };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Action(action)) => {
                        self.state.view.hover = Some(action);
                        self.activate(action);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::KindFilter) => {
                        self.state.view.overlays.toggle(Overlay::KindFilter);
                        self.state.bump_view();
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::KindPick(kind)) => {
                        self.pick_kind(kind);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::KindMenu) => {
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::ClearFilters) => {
                        self.clear_filters();
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::CategoryFilter) => {
                        self.state.view.overlays.toggle(Overlay::CategoryFilter);
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
                    Some(ui::Hit::LibraryRow(key)) => {
                        self.select_library_row(key);
                        if matches!(key, crate::library::LibraryKey::Music(_)) {
                            self.activate_library_row(key);
                        } else if matches!(
                            key,
                            crate::library::LibraryKey::Audio(_)
                                | crate::library::LibraryKey::Preset(_)
                        ) {
                            self.state.view.drag = Drag::LibraryItem {
                                key,
                                start: *position,
                                moved: false,
                            };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LibraryFolder(row)) => {
                        if let Some((LibraryListRow::Folder { key, .. }, _)) =
                            layout.library_rows.get(row)
                        {
                            self.state.toggle_library_folder(key);
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LibraryFavorite(key)) => {
                        match key {
                            crate::library::LibraryKey::Preset(id) => {
                                if let Some(index) = self.state.presets.index_of(id) {
                                    self.toggle_preset_favorite(index);
                                }
                            }
                            crate::library::LibraryKey::Sound(id) => {
                                if let Some(index) = self
                                    .state
                                    .doc
                                    .project
                                    .patches
                                    .iter()
                                    .position(|patch| patch.id == id)
                                {
                                    self.toggle_patch_favorite(index);
                                }
                            }
                            _ => {}
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::FavoriteFilter) => {
                        self.activate(ui::Action::FavoriteFilter);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LayerField(field)) => {
                        self.state.focus_document();
                        self.state.begin_edit();
                        self.set_param_from_x(*position, ui::Hit::LayerField(field), layout);
                        self.state.view.drag = Drag::Layer {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::PatchField(field)) => {
                        self.state.focus_document();
                        self.state.begin_edit();
                        self.set_param_from_x(*position, ui::Hit::PatchField(field), layout);
                        self.state.view.drag = Drag::PatchField {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::MusicField(field)) => {
                        self.state.focus_document();
                        self.state.begin_edit();
                        self.set_param_from_x(*position, ui::Hit::MusicField(field), layout);
                        self.state.view.drag = Drag::Music {
                            field,
                            last_x: position.x,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::ClipField(field)) => {
                        if let (Some(index), Some(layer)) =
                            (self.state.clip_index(), self.state.layer_index())
                        {
                            self.state.focus_document();
                            self.state.begin_edit();
                            self.set_param_from_x(*position, ui::Hit::ClipField(field), layout);
                            self.state.view.drag = Drag::ClipField {
                                layer,
                                clip: index,
                                field,
                                last_x: position.x,
                            };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LayerRow(index)) => {
                        self.state.select_layer(index);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Timeline { layer, clip, mode }) => {
                        self.state.begin_edit();
                        if self.state.view.timeline_pin.is_none() {
                            self.state.view.timeline_pin = Some(self.state.timeline_span());
                        }
                        self.state.select_layer(layer);
                        let mut index = clip;
                        if index.is_none() {
                            if let Some(layer) = self.state.layer_mut() {
                                layer.ensure_clips();
                                index = Some(0);
                            }
                        }
                        self.state.doc.project.ensure_ids();
                        if let Some(index) = index {
                            self.state.select_clip(index);
                        } else {
                            self.state.view.selected_clip = None;
                            self.state.view.selected_env_point = None;
                        }
                        if let Some(index) = index {
                            let (start, duration) = self
                                .state
                                .layer()
                                .and_then(|l| l.clips.get(index))
                                .map(|clip| (clip.start, clip.duration))
                                .unwrap_or((0.0, 0.1));
                            self.state.view.drag = Drag::Clip {
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
                        self.state.focus_document();
                        self.state.view.drag = Drag::Seek { waveform: false };
                        self.seek(time);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Waveform(time)) => {
                        self.state.focus_document();
                        self.state.view.drag = Drag::Seek { waveform: true };
                        self.seek(time);
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::LibraryScrollbar) => {
                        self.state.view.drag = Drag::LibraryScroll {
                            start_y: position.y,
                            start_scroll: self.state.library_scroll(),
                            travel: (layout.library_scrollbar.size.height
                                - layout.library_thumb.size.height)
                                .max(1.0),
                            max_start: layout.library_max_start,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::InspectorScrollbar) => {
                        self.state.view.drag = Drag::InspectorScroll {
                            start_y: position.y,
                            start_scroll: self.state.view.inspector_scroll,
                        };
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::Pattern(row, step)) => {
                        self.state.focus_document();
                        self.push_undo();
                        toggle_pattern(&mut self.state.doc.project.music, row, step);
                        self.state.mark_dirty();
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::AutomationPoint(index)) => {
                        if let (Some(clip), Some(layer)) =
                            (self.state.clip_index(), self.state.layer_index())
                        {
                            self.state.focus_document();
                            self.state.begin_edit();
                            self.state.view.selected_env_point = Some(index);
                            self.state.view.drag = Drag::AutomationPoint { layer, clip, index };
                        }
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::MenuToggle)
                    | Some(ui::Hit::MenuItem(_))
                    | Some(ui::Hit::MenuPanel) => {
                        return EventResult::Handled;
                    }
                    Some(ui::Hit::AutomationLane { time, value }) => {
                        if let (Some(clip_index), Some(layer_index)) =
                            (self.state.clip_index(), self.state.layer_index())
                        {
                            self.state.focus_document();
                            self.state.begin_edit();
                            let kind = self.state.view.automation_lane;
                            let mut added = None;
                            if let Some(clip) = self.state.clip_mut() {
                                added = Some(clip.add_point(kind, time, value));
                                self.state.mark_dirty();
                            }
                            if let Some(index) = added {
                                self.state.view.selected_env_point = Some(index);
                                self.state.view.drag = Drag::AutomationPoint {
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
                self.state.view.cursor = *position;
                let pointer_hit = ui::hit(&self.state, layout, *position);
                self.state.view.hover_hit = pointer_hit;
                self.state.view.hover = match pointer_hit {
                    Some(ui::Hit::Action(action)) => Some(action),
                    _ => None,
                };
                self.handle_drag(*position, layout);
                return EventResult::Handled;
            }
            InputEvent::PointerLeave => {
                if matches!(self.state.view.drag, Drag::LibraryItem { .. }) {
                    self.state.view.drag = Drag::None;
                } else {
                    self.state.commit_edit();
                }
                self.state.view.hover = None;
                self.state.view.hover_hit = None;
                return EventResult::Handled;
            }
            InputEvent::DoubleClick { position } => {
                let hit = ui::hit(&self.state, layout, *position);
                match hit {
                    Some(ui::Hit::LibraryRow(LibraryKey::Sound(id))) => {
                        if let Some(index) = self
                            .state
                            .doc
                            .project
                            .patches
                            .iter()
                            .position(|patch| patch.id == id)
                        {
                            self.state.select_patch(index);
                            if let Some(patch) = self.state.patch() {
                                self.state.view.modal = Some(Modal {
                                    title: "RENAME SOUND".into(),
                                    text: patch.name.clone(),
                                    action: ModalAction::RenamePatch,
                                });
                            }
                        }
                    }
                    Some(ui::Hit::LayerField(field)) => {
                        self.state.focus_document();
                        self.push_undo();
                        let default = Layer::default();
                        if let Some(layer) = self.state.layer_mut() {
                            field.set(layer, field.get(&default));
                            self.state.mark_dirty();
                        }
                    }
                    Some(ui::Hit::PatchField(field)) => {
                        self.state.focus_document();
                        self.push_undo();
                        let default = Patch::default();
                        if let Some(patch) = self.state.patch_mut() {
                            field.set(patch, field.get(&default));
                            self.state.mark_dirty();
                        }
                    }
                    Some(ui::Hit::ClipField(field)) => {
                        if self.state.clip_index().is_some() {
                            self.state.focus_document();
                            self.push_undo();
                            let max_duration = self
                                .state
                                .layer()
                                .map(|layer| layer.duration)
                                .unwrap_or(3.0);
                            let default = Clip::default();
                            if let Some(clip) = self.state.clip_mut() {
                                field.set(clip, field.get(&default));
                                clip.duration = clip.duration.clamp(0.005, max_duration);
                                self.state.mark_dirty();
                            }
                        }
                    }
                    Some(ui::Hit::Curve(kind)) => {
                        self.state.focus_document();
                        self.push_undo();
                        if let Some(layer) = self.state.layer_mut() {
                            match kind {
                                ui::CurveKind::Cycle => layer.curve.clear(),
                                ui::CurveKind::Amp => layer.amp_curve.clear(),
                            }
                        }
                        self.state.mark_dirty();
                    }
                    Some(ui::Hit::MusicField(field)) => {
                        self.state.focus_document();
                        self.push_undo();
                        let default = model::Music::default();
                        field.set(&mut self.state.doc.project.music, field.get(&default));
                        self.state.mark_music_dirty();
                    }
                    _ => {}
                }
                return EventResult::Handled;
            }
            InputEvent::PointerUp { position, .. } => {
                let drag = std::mem::replace(&mut self.state.view.drag, Drag::None);
                self.state.view.timeline_pin = None;
                if let Drag::LibraryItem { key, moved, .. } = drag {
                    if moved {
                        self.drop_library_item(key, *position, layout);
                    } else {
                        self.activate_library_row(key);
                    }
                }
                self.state.commit_edit();
                self.state.view.hover_hit = ui::hit(&self.state, layout, *position);
                return EventResult::Handled;
            }
            InputEvent::Wheel {
                position, delta, ..
            } => {
                let step = wheel_steps(delta.y);
                if layout.left.contains(*position) {
                    let capacity = ui::library_capacity(self.state.view.viewport);
                    let max_start = self
                        .state
                        .visible_library_rows()
                        .len()
                        .saturating_sub(capacity);
                    let next = (self.state.library_scroll() as i32 + step)
                        .clamp(0, max_start as i32) as usize;
                    self.state.set_library_scroll(next);
                    return EventResult::Handled;
                }
                if layout.right.contains(*position) {
                    self.state.view.inspector_scroll =
                        inspector_wheel_scroll(self.state.view.inspector_scroll, delta.y, layout);
                    return EventResult::Handled;
                }
                if layout.timeline.contains(*position) {
                    if self.state.view.modifiers.meta || self.state.view.modifiers.ctrl {
                        // Zoom around the cursor: keep the time under the
                        // pointer anchored while the scale changes.
                        let plot_w =
                            (layout.timeline.size.width - ui::TIMELINE_LABEL - 8.0).max(1.0);
                        let plot_left = layout.timeline.left() + ui::TIMELINE_LABEL;
                        let ratio = ((position.x - plot_left) / plot_w).clamp(0.0, 1.0);
                        let anchor = layout.x_time(position.x);
                        let factor = if delta.y > 0.0 { 1.25 } else { 0.8 };
                        self.state.view.timeline_zoom =
                            (self.state.view.timeline_zoom * factor).clamp(1.0, 32.0);
                        let view_total = layout.timeline_total + 0.08;
                        let scale =
                            ((plot_w / view_total) * self.state.view.timeline_zoom).max(8.0);
                        let visible = plot_w / scale;
                        self.state.view.timeline_offset =
                            (anchor - ratio * visible).clamp(0.0, (view_total - visible).max(0.0));
                        self.state.bump_view();
                        return EventResult::Handled;
                    }
                    if self.state.view.modifiers.shift {
                        let view_total = layout.timeline_total + 0.08;
                        let step =
                            layout.timeline_visible * 0.1 * if delta.y > 0.0 { 1.0 } else { -1.0 };
                        self.state.view.timeline_offset = (layout.timeline_offset + step)
                            .clamp(0.0, (view_total - layout.timeline_visible).max(0.0));
                        return EventResult::Handled;
                    }
                    if let Some(patch) = self.state.patch() {
                        let capacity =
                            ui::layer_capacity(self.state.view.viewport, patch.layers.len());
                        let max_start = patch.layers.len().saturating_sub(capacity);
                        self.state.view.layer_scroll = (self.state.view.layer_scroll as i32 + step)
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
}

impl AppLogic for SfxLogic {
    fn init(&mut self, ctx: &InitContext<'_>) {
        if let Some(measurer) = ctx.service::<Rc<dyn TextMeasurer>>() {
            self.measurer = Some(measurer.clone());
        }
    }

    fn event(&mut self, _ctx: &EventContext<'_>, event: &InputEvent) -> EventResult {
        self.handle_event(event)
    }

    fn update(&mut self, ctx: &FrameContext<'_>) {
        self.state.view.viewport = ctx.viewport().logical_size();
        if matches!(self.state.view.drag, Drag::None) {
            self.state.view.timeline_pin = None;
        }
        // A patch switch, tab switch or undo invalidates the preview cache with
        // version 0. Do not keep playing the old buffer while the UI shows the
        // new document. Asset auditions are excluded via `preset_playing`.
        let stale_project_playback = self.audio.as_ref().is_some_and(|audio| {
            project_playback_is_stale(
                self.state.view.tab,
                self.state.playback.cache_version,
                self.state.playback.music_cache_version,
                audio.is_playing(),
                self.state.playback.preset_playing,
            )
        });
        if stale_project_playback {
            self.stop();
        }
        if let Some((_, deadline)) = &self.state.ui.toast {
            if Instant::now() > *deadline {
                self.state.ui.toast = None;
            }
        }
        self.poll_render();
        let rate = self.state.preview_rate();
        if self.state.playback.defer_render {
            self.state.playback.defer_render = false;
        } else {
            self.request_preview(rate);
        }
        if let Some(audio) = &mut self.audio {
            audio.reclaim();
            let errors = audio.take_device_errors();
            let underruns = audio.take_underruns();
            if errors > 0 {
                self.state.toast("audio device error · reconnecting");
            } else if underruns > 0 {
                self.state.toast("audio underrun");
            }
            if !audio.is_alive() && audio.try_reconnect() {
                self.state.toast("audio device restored");
            }
            let playing = audio.is_playing();
            if playing {
                if !self.state.playback.preset_playing {
                    let mut time = audio.position();
                    if self.state.view.tab == Tab::Music {
                        let length = self.state.doc.project.music.length_seconds();
                        if length > 0.0 {
                            time %= length;
                        }
                    }
                    self.state.playback.playhead = time;
                }
            } else if self.state.playback.was_playing {
                if !self.state.playback.preset_playing {
                    self.state.playback.playhead = 0.0;
                    self.state.playback.seeked = false;
                }
                self.state.playback.preset_playing = false;
                self.state.playback.source = None;
            }
            self.state.playback.was_playing = playing;
        }
    }

    fn paint(&mut self, _ctx: &FrameContext<'_>, paint: &mut PaintContext) {
        let Some(measurer) = self.measurer.clone() else {
            return;
        };
        let layout = self.layout();
        ui::paint_with_layout(&self.state, measurer.as_ref(), paint, layout.as_ref());
    }

    fn needs_frame(&self) -> bool {
        self.state.playback.defer_render
            || self.state.preview_pending()
            || self
                .audio
                .as_ref()
                .map(|audio| audio.is_playing())
                .unwrap_or(false)
    }

    fn cursor(&self) -> Option<Cursor> {
        match &self.state.view.drag {
            Drag::Clip {
                mode: TimelineDrag::Resize | TimelineDrag::ResizeLeft,
                ..
            } => return Some(Cursor::ColResize),
            Drag::Clip { .. } => return Some(Cursor::Grabbing),
            Drag::Layer { .. }
            | Drag::PatchField { .. }
            | Drag::Music { .. }
            | Drag::ClipField { .. } => return Some(Cursor::ColResize),
            Drag::AutomationPoint { .. } => return Some(Cursor::Grabbing),
            Drag::LibraryItem { .. } => return Some(Cursor::Grabbing),
            Drag::Seek { .. } => return Some(Cursor::Grab),
            Drag::Curve { .. } => return Some(Cursor::Grabbing),
            Drag::InspectorScroll { .. } | Drag::LibraryScroll { .. } => {
                return Some(Cursor::RowResize)
            }
            Drag::None => {}
        }
        match self.state.view.hover_hit {
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
            Some(
                ui::Hit::LayerField(_)
                | ui::Hit::PatchField(_)
                | ui::Hit::ClipField(_)
                | ui::Hit::MusicField(_),
            ) => Some(Cursor::ColResize),
            Some(
                ui::Hit::Action(_)
                | ui::Hit::MenuToggle
                | ui::Hit::MenuItem(_)
                | ui::Hit::MenuPanel
                | ui::Hit::LibraryRow(_)
                | ui::Hit::LibraryFavorite(_)
                | ui::Hit::LibraryFolder(_)
                | ui::Hit::InstallPresets
                | ui::Hit::Curve(_)
                | ui::Hit::LayerRow(_)
                | ui::Hit::Pattern(_, _)
                | ui::Hit::KindFilter
                | ui::Hit::KindPick(_)
                | ui::Hit::KindMenu
                | ui::Hit::ClearFilters
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
    /// Build (or reuse) the frame layout. Input and paint share the same
    /// `Rc<Layout>` so library rows and inspector rows are only rebuilt when a
    /// relevant state field changes.
    fn layout(&mut self) -> Rc<ui::Layout> {
        let key = LayoutKey::of(&self.state);
        if let Some((cached_key, layout)) = &self.layout {
            if *cached_key == key {
                return layout.clone();
            }
        }
        let layout = Rc::new(ui::Layout::new(&self.state, self.state.view.viewport));
        self.layout = Some((key, layout.clone()));
        layout
    }
}

/// A patch/tab/undo switch invalidates the preview cache with version 0.
/// Project playback must stop so the old buffer cannot keep sounding while the
/// UI shows the new document. Asset auditions keep their own flag and are not
/// affected by document cache versions.
fn project_playback_is_stale(
    tab: Tab,
    patch_cache: u64,
    music_cache: u64,
    playing: bool,
    preset_playing: bool,
) -> bool {
    playing
        && !preset_playing
        && match tab {
            Tab::Patches => patch_cache == 0,
            Tab::Music => music_cache == 0,
        }
}

/// Core wheel convention: positive `delta.y` scrolls down.
pub(super) fn wheel_steps(delta_y: f32) -> i32 {
    let rows = (delta_y.abs() / 24.0).max(1.0) as i32;
    if delta_y > 0.0 {
        rows
    } else if delta_y < 0.0 {
        -rows
    } else {
        0
    }
}

/// One wheel notch scrolls `22px`; positive `delta.y` scrolls toward the bottom.
pub(super) fn inspector_wheel_scroll(current: f32, delta_y: f32, layout: &ui::Layout) -> f32 {
    (current + wheel_steps(delta_y) as f32 * 22.0).clamp(0.0, layout.inspector_max_scroll())
}

impl SfxLogic {
    /// Arrow keys walk whichever library source is active, without changing
    /// the project or starting playback.
    fn navigate_library(&mut self, direction: isize) {
        let visible: Vec<LibraryEntry> = self
            .state
            .visible_library_rows()
            .into_iter()
            .filter_map(|row| match row {
                LibraryListRow::Entry(entry) => Some(entry),
                LibraryListRow::Folder { .. } => None,
            })
            .collect();
        if visible.is_empty() {
            return;
        }
        let position = self
            .state
            .view
            .selected_library
            .and_then(|key| visible.iter().position(|entry| entry.key == key))
            .unwrap_or(0);
        let target = if direction < 0 {
            position.saturating_sub(1)
        } else {
            (position + 1).min(visible.len() - 1)
        };
        if let Some(entry) = visible.get(target) {
            let key = entry.key;
            // Arrow keys only move the selection; click/Enter auditions or
            // loads. Otherwise browsing would start playback on every key.
            self.select_library_row(key);
        }
    }

    fn set_curve_from_pos(&mut self, kind: ui::CurveKind, position: Vec2, layout: &ui::Layout) {
        let Some(rect) = layout.curve_rect(kind) else {
            return;
        };
        let Some(layer) = self.state.layer() else {
            return;
        };
        let mut values = ui::curve_values(layer, kind);
        if values.len() < 2 {
            return;
        }
        let index = ui::curve_index(kind, rect, position.x, values.len());
        values[index] = ui::curve_value_from_y(kind, rect, position.y);
        if let Some(layer) = self.state.layer_mut() {
            match kind {
                ui::CurveKind::Cycle => layer.curve = values,
                ui::CurveKind::Amp => layer.amp_curve = values,
            }
        }
        self.state.mark_dirty();
    }

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
            ui::Hit::PatchField(field) => {
                let spec = field.spec();
                let value = spec.snap(field.denorm(t)).clamp(spec.min, spec.max);
                if let Some(patch) = self.state.patch_mut() {
                    field.set(patch, value);
                    self.state.mark_dirty();
                }
            }
            ui::Hit::ClipField(field) => {
                if self.state.clip_index().is_some() {
                    let max_duration = self
                        .state
                        .layer()
                        .map(|layer| layer.duration)
                        .unwrap_or(3.0);
                    let spec = field.spec();
                    let value = spec.snap(field.denorm(t)).clamp(spec.min, spec.max);
                    if let Some(clip) = self.state.clip_mut() {
                        field.set(clip, value);
                        clip.duration = clip.duration.clamp(0.005, max_duration);
                        self.state.mark_dirty();
                    }
                }
            }
            ui::Hit::MusicField(field) => {
                let spec = field.spec();
                let value = spec.snap(field.denorm(t)).clamp(spec.min, spec.max);
                field.set(&mut self.state.doc.project.music, value);
                self.state.mark_music_dirty();
            }
            _ => {}
        }
    }

    fn handle_drag(&mut self, position: Vec2, layout: &ui::Layout) {
        let drag = std::mem::replace(&mut self.state.view.drag, Drag::None);
        let fine = if self.state.view.modifiers.shift {
            0.15
        } else if self.state.view.modifiers.alt {
            4.0
        } else {
            1.0
        };
        // Shift disables the time grid so clip edges can be placed freely.
        let snap = !self.state.view.modifiers.shift;
        match drag {
            Drag::None => {}
            Drag::LibraryItem { key, start, moved } => {
                let moved = moved || (position - start).length() > 4.0;
                self.state.view.drag = Drag::LibraryItem { key, start, moved };
            }
            Drag::Curve { kind } => {
                self.set_curve_from_pos(kind, position, layout);
                self.state.view.drag = Drag::Curve { kind };
            }
            Drag::Layer { field, last_x } => {
                if self.state.view.modifiers.shift || self.state.view.modifiers.alt {
                    let (_, _, step) = field.range();
                    let delta = (position.x - last_x) * step * fine;
                    if let Some(layer) = self.state.layer_mut() {
                        field.set(layer, field.get(layer) + delta);
                        self.state.mark_dirty();
                    }
                } else {
                    self.set_param_from_x(position, ui::Hit::LayerField(field), layout);
                }
                self.state.view.drag = Drag::Layer {
                    field,
                    last_x: position.x,
                };
            }
            Drag::PatchField { field, last_x } => {
                if self.state.view.modifiers.shift || self.state.view.modifiers.alt {
                    let (_, _, step) = field.range();
                    let delta = (position.x - last_x) * step * fine;
                    if let Some(patch) = self.state.patch_mut() {
                        field.set(patch, field.get(patch) + delta);
                        self.state.mark_dirty();
                    }
                } else {
                    self.set_param_from_x(position, ui::Hit::PatchField(field), layout);
                }
                self.state.view.drag = Drag::PatchField {
                    field,
                    last_x: position.x,
                };
            }
            Drag::Music { field, last_x } => {
                if self.state.view.modifiers.shift || self.state.view.modifiers.alt {
                    let (_, _, step) = field.range();
                    let delta = (position.x - last_x) * step * fine;
                    let old = field.get(&self.state.doc.project.music);
                    field.set(&mut self.state.doc.project.music, old + delta);
                    self.state.mark_music_dirty();
                } else {
                    self.set_param_from_x(position, ui::Hit::MusicField(field), layout);
                }
                self.state.view.drag = Drag::Music {
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
                if self.state.view.modifiers.shift || self.state.view.modifiers.alt {
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
                self.state.view.drag = Drag::ClipField {
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
                self.state.view.drag = Drag::Seek { waveform };
            }
            Drag::LibraryScroll {
                start_y,
                start_scroll,
                travel,
                max_start,
            } => {
                let delta = (position.y - start_y) / travel.max(1.0) * max_start as f32;
                let value = (start_scroll as f32 + delta)
                    .round()
                    .clamp(0.0, max_start as f32) as usize;
                self.state.set_library_scroll(value);
                self.state.view.drag = Drag::LibraryScroll {
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
                let max_scroll = layout.inspector_max_scroll();
                let travel = (layout.inspector_scrollbar.size.height
                    - layout.inspector_thumb.size.height)
                    .max(1.0);
                self.state.view.inspector_scroll = (start_scroll
                    + (position.y - start_y) * max_scroll / travel)
                    .clamp(0.0, max_scroll);
                self.state.bump_view();
                self.state.view.drag = Drag::InspectorScroll {
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
                    layout.automation_at(position, duration, self.state.view.automation_lane);
                let kind = self.state.view.automation_lane;
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
                self.state.view.drag = Drag::AutomationPoint { layer, clip, index };
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
                self.state.view.drag = Drag::Clip {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_logic() -> SfxLogic {
        SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
            layout: None,
        }
    }

    fn click(logic: &mut SfxLogic, position: Vec2) {
        logic.handle_event(&InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        });
        logic.handle_event(&InputEvent::PointerUp {
            position,
            button: PointerButton::Left,
        });
    }

    #[test]
    fn clicking_a_layer_reclaims_focus_from_a_preset_row() {
        let mut logic = test_logic();
        logic.state.select_preset(0);
        assert!(logic.state.preset_focus());
        let layout = logic.layout();
        let row = layout.layer_rows[0].1;
        click(
            &mut logic,
            Vec2::new(layout.timeline.left() + 4.0, row.center().y),
        );
        assert!(!logic.state.preset_focus());
        assert_eq!(
            logic.state.selected_library_kind(),
            Some(crate::library::LibraryKind::Sound)
        );
    }

    #[test]
    fn clicking_the_music_tab_reclaims_preset_focus() {
        let mut logic = test_logic();
        logic.state.select_preset(0);
        let layout = logic.layout();
        let tab = layout
            .tab_buttons
            .iter()
            .find(|button| button.action == ui::Action::TabMusic)
            .expect("music tab")
            .rect
            .center();
        click(&mut logic, tab);
        assert_eq!(logic.state.view.tab, Tab::Music);
        assert!(!logic.state.preset_focus());
        assert!(logic.state.view.selected_library.is_none());
    }

    #[test]
    fn outside_click_closes_the_overlay_and_is_consumed() {
        let mut logic = test_logic();
        let layout = logic.layout();
        click(&mut logic, layout.kind_rect.center());
        assert!(logic
            .state
            .view
            .overlays
            .is_open(crate::app::Overlay::KindFilter));

        let before = logic.state.view.selected_library;
        let layout = logic.layout();
        let row = layout
            .library_rows
            .iter()
            .rev()
            .find_map(|(row, rect)| match row {
                LibraryListRow::Entry(_) => Some(*rect),
                LibraryListRow::Folder { .. } => None,
            })
            .expect("entry row");
        click(&mut logic, row.center());
        assert!(!logic
            .state
            .view
            .overlays
            .is_open(crate::app::Overlay::KindFilter));
        assert_eq!(
            logic.state.view.selected_library, before,
            "outside click must not activate the row underneath"
        );
    }

    #[test]
    fn cmd_z_shortcut_runs_through_the_command_table() {
        let mut logic = test_logic();
        logic.state.push_undo();
        let before = logic.state.doc.undo.len();
        logic.handle_event(&InputEvent::ModifiersChanged(Modifiers {
            meta: true,
            ..Modifiers::NONE
        }));
        logic.handle_event(&InputEvent::KeyDown {
            key: Key::Character('z'),
        });
        assert_eq!(logic.state.doc.undo.len(), before - 1);
    }

    #[test]
    fn wheel_zoom_updates_the_timeline_scale() {
        let mut logic = test_logic();
        let layout = logic.layout();
        let position = layout.timeline.center();
        logic.handle_event(&InputEvent::ModifiersChanged(Modifiers {
            meta: true,
            ..Modifiers::NONE
        }));
        logic.handle_event(&InputEvent::Wheel {
            position,
            delta: Vec2::new(0.0, 1.0),
            phase: igui_core::TouchPhase::Moved,
            device: igui_core::ScrollDevice::Wheel,
        });
        assert!(logic.state.view.timeline_zoom > 1.0);
        assert!(logic.state.view.timeline_offset >= 0.0);
    }

    #[test]
    fn dragging_a_clip_moves_it_and_commits_one_undo() {
        let mut logic = test_logic();
        logic.state.layer_mut().expect("layer").ensure_clips();
        logic.state.mark_dirty();
        let layout = logic.layout();
        let row = layout.layer_rows[0].1;
        let clip = logic.state.layer().expect("layer").clips[0].clone();
        let start_x = layout.time_x(clip.start + clip.duration * 0.5);
        let position = Vec2::new(start_x, row.center().y);
        let undo_before = logic.state.doc.undo.len();

        logic.handle_event(&InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        });
        logic.handle_event(&InputEvent::PointerMove {
            position: Vec2::new(start_x + 24.0, row.center().y),
        });
        logic.handle_event(&InputEvent::PointerUp {
            position: Vec2::new(start_x + 24.0, row.center().y),
            button: PointerButton::Left,
        });

        let moved = logic.state.layer().expect("layer").clips[0].start;
        assert!(moved > clip.start, "clip did not move: {moved}");
        assert_eq!(logic.state.doc.undo.len(), undo_before + 1);
    }

    #[test]
    fn layout_is_reused_until_relevant_state_changes() {
        let mut logic = SfxLogic {
            state: State::demo(),
            measurer: None,
            audio: None,
            render: None,
            layout: None,
        };
        let first = logic.layout();
        let second = logic.layout();
        assert!(Rc::ptr_eq(&first, &second));

        logic.state.view.kind_filter = Some(crate::library::LibraryKind::Preset);
        let third = logic.layout();
        assert!(!Rc::ptr_eq(&first, &third));

        logic.state.view.library_scroll = 1;
        let fourth = logic.layout();
        assert!(!Rc::ptr_eq(&third, &fourth));
    }

    #[test]
    fn stale_project_playback_is_detected_but_auditions_are_kept() {
        assert!(project_playback_is_stale(Tab::Patches, 0, 7, true, false));
        assert!(project_playback_is_stale(Tab::Music, 7, 0, true, false));
        assert!(!project_playback_is_stale(Tab::Patches, 7, 0, true, false));
        assert!(!project_playback_is_stale(Tab::Patches, 0, 7, false, false));
        // A preset/audio audition is not tied to the project preview cache.
        assert!(!project_playback_is_stale(Tab::Patches, 0, 7, true, true));
    }
}
