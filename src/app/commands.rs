//! File/export/edit commands plus the audio bridge helpers.

use super::*;

impl SfxLogic {
    pub(crate) fn new(mut state: State, background_preview: bool) -> Self {
        let audio = AudioEngine::new();
        if let Some(engine) = &audio {
            state.playback.audio_ok = true;
            state.playback.audio_rate = engine.sample_rate();
            state.ui.status = format!("audio {} Hz · OPEN a project folder", engine.sample_rate());
        } else {
            state.ui.status = "no audio device · UI only".into();
        }
        Self {
            state,
            measurer: Some(Rc::new(ApproxTextMeasurer)),
            audio,
            render: background_preview
                .then(render::RenderWorker::spawn)
                .flatten(),
        }
    }

    pub(super) fn activate(&mut self, action: ui::Action) {
        use ui::Action as A;
        self.state.view.menu_open = false;
        self.state.commit_edit();
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
                | A::SetToneEnv(_)
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
                self.state.view.tab = Tab::Patches;
                self.state.view.inspector_scroll = 0.0;
                self.state.playback.defer_render = true;
                self.state.bump_view();
                self.state.playback.playhead = 0.0;
            }
            A::TabMusic => {
                self.state.view.tab = Tab::Music;
                self.state.view.inspector_scroll = 0.0;
                self.state.playback.defer_render = true;
                self.state.bump_view();
                self.state.playback.playhead = 0.0;
            }
            A::NewPatch => {
                let name = self.state.doc.project.next_patch_name();
                self.state.view.modal = Some(Modal {
                    title: "NEW SOUND".into(),
                    text: name,
                    action: ModalAction::NewPatch,
                });
            }
            A::DeletePatch => {
                if self.state.doc.project.patches.len() > 1 {
                    self.state
                        .doc
                        .project
                        .patches
                        .remove(self.state.view.selected_patch);
                    self.state.view.selected_patch = self
                        .state
                        .view
                        .selected_patch
                        .min(self.state.doc.project.patches.len() - 1);
                    self.state.view.selected_layer = 0;
                    self.state.mark_dirty();
                    self.state.bump_view();
                }
            }
            A::DuplicatePatch => {
                if let Some(patch) = self.state.patch().cloned() {
                    let mut copy = patch;
                    copy.name = format!("{}_copy", copy.name);
                    self.state
                        .doc
                        .project
                        .patches
                        .insert(self.state.view.selected_patch + 1, copy);
                    self.state.view.selected_patch += 1;
                    self.state.view.selected_layer = 0;
                    self.state.mark_dirty();
                    self.state.bump_view();
                }
            }
            A::RenamePatch => {
                if let Some(patch) = self.state.patch() {
                    self.state.view.modal = Some(Modal {
                        title: "RENAME SOUND".into(),
                        text: patch.name.clone(),
                        action: ModalAction::RenamePatch,
                    });
                }
            }
            A::Play => self.play(),
            A::Stop => self.stop(),
            A::Loop => self.state.playback.loop_play = !self.state.playback.loop_play,
            A::AddLayer => {
                if let Some(patch) = self.state.patch_mut() {
                    patch.layers.push(Layer::default());
                    self.state.view.selected_layer = patch.layers.len() - 1;
                    self.state.mark_dirty();
                }
            }
            A::DuplicateLayer => {
                let selected = self.state.view.selected_layer;
                if let Some(patch) = self.state.patch_mut() {
                    if let Some(layer) = patch.layers.get(selected).cloned() {
                        patch.layers.insert(selected + 1, layer);
                        self.state.view.selected_layer = selected + 1;
                        self.state.mark_dirty();
                    }
                }
            }
            A::DeleteLayer => {
                let selected = self.state.view.selected_layer;
                if let Some(patch) = self.state.patch_mut() {
                    if patch.layers.len() > 1 {
                        patch.layers.remove(selected);
                        self.state.view.selected_layer =
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
            A::SetToneEnv(tone_env) => {
                if let Some(layer) = self.state.layer_mut() {
                    layer.tone_env = tone_env;
                    self.state.mark_dirty();
                }
            }
            A::BassWave(wave) => {
                self.state.doc.project.music.bass_wave = wave;
                self.state.mark_music_dirty();
            }
            A::ArpWave(wave) => {
                self.state.doc.project.music.arp_wave = wave;
                self.state.mark_music_dirty();
            }
            A::TogglePatterns => {
                self.state.doc.project.music.use_patterns =
                    !self.state.doc.project.music.use_patterns;
                self.state.mark_music_dirty();
            }
            A::ChordPreset(index) => {
                apply_chord_preset(&mut self.state.doc.project.music, index);
                self.state.mark_music_dirty();
            }
            A::Transpose(delta) => {
                self.state.doc.project.music.transpose(delta);
                self.state.mark_music_dirty();
            }
            A::ResetPatterns => {
                self.state.doc.project.music.reset_patterns();
                self.state.mark_music_dirty();
            }
            A::AddClip => {
                let playhead = self.state.playback.playhead;
                if let Some(layer) = self.state.layer_mut() {
                    layer.ensure_clips();
                    let duration = layer.duration.clamp(0.02, 0.5);
                    layer.clips.push(Clip {
                        id: 0,
                        start: playhead.max(0.0),
                        duration,
                        gain: 1.0,
                        pitch: 0.0,
                        gain_points: Vec::new(),
                        cutoff_points: Vec::new(),
                    });
                    self.state.view.selected_clip = Some(layer.clips.len() - 1);
                    self.state.view.selected_env_point = None;
                    self.state.mark_dirty();
                }
            }
            A::DeleteClip => {
                if let Some(index) = self.state.view.selected_clip {
                    if let Some(layer) = self.state.layer_mut() {
                        if index < layer.clips.len() {
                            layer.clips.remove(index);
                            self.state.view.selected_clip = None;
                            self.state.view.selected_env_point = None;
                            self.state.mark_dirty();
                        }
                    }
                }
            }
            A::AutomationLane(kind) => {
                self.state.view.automation_lane = kind;
                self.state.view.selected_env_point = None;
                self.state.bump_view();
            }
            A::Undo => {
                if let Some(label) = self.state.undo() {
                    self.state.toast(format!("undo {label}"));
                }
            }
            A::Redo => {
                if let Some(label) = self.state.redo() {
                    self.state.toast(format!("redo {label}"));
                }
            }
        }
        self.state.ensure_visible();
    }

    pub(super) fn push_undo(&mut self) {
        self.state.push_undo();
    }

    pub(super) fn open(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Open a Godot project folder")
            .pick_folder()
        else {
            return;
        };
        if self.state.doc.dirty {
            self.state.view.modal = Some(Modal {
                title: "DISCARD CHANGES?".into(),
                text: folder.display().to_string(),
                action: ModalAction::ConfirmOpen(folder),
            });
            self.state.bump_view();
            return;
        }
        self.open_folder_now(folder);
    }

    fn open_folder_now(&mut self, folder: PathBuf) {
        self.state.view.timeline_zoom = 1.0;
        self.state.view.timeline_offset = 0.0;
        match open_folder(&mut self.state, folder) {
            Ok(warnings) => {
                let message = if warnings.is_empty() {
                    "project loaded".to_string()
                } else {
                    format!("project loaded · {}", warnings.join("; "))
                };
                self.state.toast(message);
            }
            Err(error) => self.state.toast(format!("open failed: {error}")),
        }
    }

    pub(super) fn save(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        self.state.doc.project.migrate();
        let warnings = self.state.doc.project.validate();
        // Validation may clamp values; make the rendered preview match what
        // was just persisted.
        self.state.playback.cache_version = 0;
        self.state.playback.music_cache_version = 0;
        match model::save_project(&root, &self.state.doc.project) {
            Ok(()) => {
                self.state.doc.dirty = false;
                let message = if warnings.is_empty() {
                    format!("saved {}", root.join(model::PROJECT_FILE).display())
                } else {
                    format!(
                        "saved {} · {}",
                        root.join(model::PROJECT_FILE).display(),
                        warnings.join("; ")
                    )
                };
                self.state.toast(message);
            }
            Err(error) => self.state.toast(format!("save failed: {error}")),
        }
    }

    pub(super) fn bake(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        match export::bake(&self.state.doc.project, &root) {
            Ok(message) => self.state.toast(message),
            Err(error) => self.state.toast(format!("bake failed: {error}")),
        }
    }

    fn export_tres(&mut self) {
        let Some(root) = self.ensure_root() else {
            return;
        };
        match export::export_tres(&self.state.doc.project, &root) {
            Ok(message) => self.state.toast(message),
            Err(error) => self.state.toast(format!("export failed: {error}")),
        }
    }

    pub(super) fn export_wav(&mut self) {
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
            ^ self.state.doc.version.wrapping_mul(0x9E3779B97F4A7C15);
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
        let start = self.state.view.selected_patch;
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
            self.state
                .doc
                .project
                .patches
                .insert(start + 1 + index, copy);
        }
        self.state.view.selected_patch = start + 1;
        self.state.view.selected_layer = 0;
        self.state.view.selected_clip = None;
        self.state.mark_dirty();
        self.state.bump_view();
        self.state.toast("created 3 variations (A/B/C)");
    }

    fn toggle_favorite(&mut self) {
        if let Some(patch) = self.state.patch_mut() {
            patch.favorite = !patch.favorite;
            let favorite = patch.favorite;
            self.state.mark_dirty();
            if self.state.view.favorites_only {
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

    pub(super) fn toggle_patch_favorite(&mut self, index: usize) {
        let Some(patch) = self.state.doc.project.patches.get(index) else {
            return;
        };
        let favorite = !patch.favorite;
        self.state.begin_edit();
        self.state.doc.project.patches[index].favorite = favorite;
        self.state.mark_dirty();
        if self.state.view.favorites_only {
            self.refresh_filter();
        } else {
            self.state.bump_view();
        }
        self.state.toast(if favorite {
            "added to favorites"
        } else {
            "removed from favorites"
        });
        self.state.commit_edit();
    }

    fn cycle_category(&mut self) {
        let next = match self.state.view.category_filter {
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
        self.state.view.category_filter = next;
        self.refresh_filter();
        self.state.toast(match next {
            Some(category) => format!("filter: {}", category.label()),
            None => "filter: all sounds".into(),
        });
    }

    pub(super) fn pick_category(&mut self, category: Option<Category>) {
        self.state.view.category_filter = category;
        self.state.view.category_menu_open = false;
        self.refresh_filter();
    }

    fn toggle_favorite_filter(&mut self) {
        self.state.view.favorites_only = !self.state.view.favorites_only;
        self.refresh_filter();
        self.state.toast(if self.state.view.favorites_only {
            "showing favorites"
        } else {
            "showing all sounds"
        });
    }

    pub(super) fn refresh_filter(&mut self) {
        self.state.view.patch_scroll = 0;
        self.keep_selection_visible();
        self.state.bump_view();
    }

    fn keep_selection_visible(&mut self) {
        if self.state.patch_visible(self.state.view.selected_patch) {
            return;
        }
        if let Some(index) = self.state.visible_patches().first().copied() {
            self.state.select_patch(index);
        }
    }

    pub(super) fn trigger_pad(&mut self, index: usize) {
        if index >= self.state.doc.project.patches.len() {
            return;
        }
        if !self.state.patch_visible(index) {
            self.state.view.category_filter = None;
            self.state.view.favorites_only = false;
            self.state.view.patch_scroll = 0;
        }
        self.state.view.tab = Tab::Patches;
        self.state.select_patch(index);
        self.play();
    }

    fn ensure_root(&mut self) -> Option<PathBuf> {
        if let Some(root) = &self.state.doc.root {
            return Some(root.clone());
        }
        let folder = rfd::FileDialog::new()
            .set_title("Choose the Godot project folder for export")
            .pick_folder()?;
        if let Some(name) = folder.file_name().and_then(|name| name.to_str()) {
            if self.state.doc.project.name == "untitled" {
                self.state.doc.project.name = name.to_string();
            }
        }
        self.state.doc.root = Some(folder.clone());
        Some(folder)
    }

    fn confirm_modal(&mut self) {
        let Some(modal) = self.state.view.modal.take() else {
            return;
        };
        let name = display_name(&modal.text);
        match modal.action {
            ModalAction::ConfirmOpen(folder) => self.open_folder_now(folder),
            ModalAction::NewPatch => {
                self.push_undo();
                self.state
                    .doc
                    .project
                    .patches
                    .push(Patch::new_layer(name.clone(), Wave::Sine));
                self.state.view.selected_patch = self.state.doc.project.patches.len() - 1;
                self.state.view.selected_layer = 0;
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

    pub(super) fn handle_modal(&mut self, event: &InputEvent) -> bool {
        let Some(input) = self.state.view.modal.as_ref().map(Modal::is_input) else {
            return false;
        };
        match event {
            InputEvent::TextInput { text } => {
                if input {
                    if let Some(modal) = self.state.view.modal.as_mut() {
                        for ch in text.chars() {
                            if !ch.is_control() && modal.text.len() < 48 {
                                modal.text.push(ch);
                            }
                        }
                    }
                }
                true
            }
            InputEvent::KeyDown { key } => match key {
                Key::Backspace if input => {
                    if let Some(modal) = self.state.view.modal.as_mut() {
                        modal.text.pop();
                    }
                    true
                }
                Key::Enter => {
                    self.confirm_modal();
                    true
                }
                Key::Escape => {
                    self.state.view.modal = None;
                    true
                }
                _ => true,
            },
            _ => true,
        }
    }
}
