//! File/export/edit commands plus the audio bridge helpers.

use super::*;

impl SfxLogic {
    pub(crate) fn new(mut state: State, background_preview: bool) -> Self {
        let audio = AudioEngine::new();
        if let Some(engine) = &audio {
            state.playback.audio_ok = true;
            state.playback.audio_rate = engine.sample_rate();
            state.ui.status = if state.doc.root.is_some() {
                format!("audio {} Hz · project loaded", engine.sample_rate())
            } else {
                format!(
                    "audio {} Hz · OPEN a project folder (⌘O) · MENU → NEW PROJECT",
                    engine.sample_rate()
                )
            };
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
            layout: None,
        }
    }

    pub(super) fn activate(&mut self, action: ui::Action) {
        use ui::Action as A;
        self.state.view.overlays.clear();
        if !ui::action_available(&self.state, action) {
            self.state
                .toast(ui::action_unavailable_message(&self.state, action));
            return;
        }
        self.state.commit_edit();
        // Menu/keyboard edits act on the project. If the user was inspecting
        // an AUDIO/MUSIC row, reclaim the document focus first so the result
        // is visible and the inspector follows the command. PRESET focus is
        // still read-only (`action_available` already rejected these actions).
        if ui::is_project_edit(action)
            && matches!(
                self.state.selected_library_kind(),
                Some(crate::library::LibraryKind::Audio | crate::library::LibraryKind::Music)
            )
        {
            self.state.focus_document();
        }
        if let Some(label) = ui::action_meta(action).undo {
            self.state.push_undo_label(label);
        }
        match action {
            A::NewProject => self.new_project(),
            A::Open => self.open(),
            A::Save => self.save(),
            A::InstallPresets => self.install_presets(),
            A::SavePreset => self.save_as_preset(),
            A::RenamePreset => self.open_rename_preset(),
            A::DeletePreset => self.open_delete_preset(),
            A::RestorePreset => self.restore_preset(),
            A::RenameAudio => self.open_rename_audio(),
            A::DeleteAudio => self.open_delete_audio(),
            A::RestoreAudio => self.restore_audio(),
            A::RenameMusic => self.open_rename_music(),
            A::DeleteMusic => self.open_delete_music(),
            A::RestoreMusic => self.restore_music(),
            A::Bake => self.bake(),
            A::Tres => self.export_tres(),
            A::ExportWav => self.export_wav(),
            A::Randomize => self.randomize(),
            A::UpgradeDsp => self.upgrade_dsp(),
            A::Vary => self.vary(),
            A::Favorite => self.toggle_favorite(),
            A::CategoryCycle => self.cycle_category(),
            A::FavoriteFilter => self.toggle_favorite_filter(),
            A::TabPatches => self.switch_tab(Tab::Patches),
            A::TabMusic => self.switch_tab(Tab::Music),
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
                    let index = self.state.patch_index().unwrap_or(0);
                    self.state.doc.project.patches.remove(index);
                    self.state.mark_dirty();
                    let next = index.min(self.state.doc.project.patches.len() - 1);
                    self.state.select_patch(next);
                }
            }
            A::DuplicatePatch => {
                let index = self.state.patch_index().unwrap_or(0);
                if let Some(patch) = self.state.patch().cloned() {
                    let mut copy = patch;
                    copy.name = format!("{}_copy", copy.name);
                    self.state.doc.project.patches.insert(index + 1, copy);
                    self.state.mark_dirty();
                    self.state.select_patch(index + 1);
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
            A::Play => self.play_focused(),
            A::Stop => self.stop(),
            A::Loop => self.state.playback.loop_play = !self.state.playback.loop_play,
            A::AddLayer => {
                if let Some(patch) = self.state.patch_mut() {
                    patch.layers.push(Layer::default());
                }
                self.state.mark_dirty();
                let index = self
                    .state
                    .layer()
                    .map(|_| {
                        self.state
                            .patch()
                            .map(|p| p.layers.len())
                            .unwrap_or(0)
                            .saturating_sub(1)
                    })
                    .unwrap_or(0);
                self.state.select_layer(index);
            }
            A::DuplicateLayer => {
                let selected = self.state.layer_index().unwrap_or(0);
                if let Some(patch) = self.state.patch_mut() {
                    if let Some(layer) = patch.layers.get(selected).cloned() {
                        patch.layers.insert(selected + 1, layer);
                        self.state.mark_dirty();
                        self.state.select_layer(selected + 1);
                    }
                }
            }
            A::DeleteLayer => {
                let selected = self.state.layer_index().unwrap_or(0);
                if let Some(patch) = self.state.patch_mut() {
                    if patch.layers.len() > 1 {
                        patch.layers.remove(selected);
                        self.state.mark_dirty();
                        let remaining = self.state.patch().map(|p| p.layers.len()).unwrap_or(1);
                        self.state.select_layer(selected.min(remaining - 1));
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
            A::DemoTrack(index) => {
                if let Some(track) = crate::demo::tracks().get(index) {
                    self.state.doc.project.music = track.music.clone();
                    self.state.mark_music_dirty();
                    self.state.playback.playhead = 0.0;
                    let title = track.title;
                    self.state.toast(format!("demo track · {title}"));
                }
            }
            A::Transpose(delta) => {
                self.state.doc.project.music.transpose(delta);
                self.state.mark_music_dirty();
            }
            A::ResetPatterns => {
                self.state.doc.project.music.reset_patterns();
                self.state.mark_music_dirty();
            }
            A::LoadSample => self.load_sample(),
            A::ToggleSampleLoop => self.toggle_sample_loop(),
            A::ToggleSampleReverse => self.toggle_sample_reverse(),
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
                }
                self.state.mark_dirty();
                let last = self
                    .state
                    .layer()
                    .map(|layer| layer.clips.len().saturating_sub(1))
                    .unwrap_or(0);
                self.state.select_clip(last);
            }
            A::DeleteClip => {
                if let Some(index) = self.state.clip_index() {
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

    /// Switch the centre view and return focus to the document. Tab switching
    /// must not leave a PRESET/AUDIO row owning the inspector or the transport,
    /// otherwise the tab appears to do nothing.
    pub(super) fn switch_tab(&mut self, tab: Tab) {
        self.stop();
        self.state.view.tab = tab;
        self.state.focus_document();
        self.state.view.inspector_scroll = 0.0;
        self.state.playback.defer_render = true;
        self.state.bump_view();
    }

    fn new_project(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose a folder for the new SFX project")
            .pick_folder()
        else {
            return;
        };
        if folder.join(model::PROJECT_FILE).exists() {
            self.state.toast(format!(
                "{} already contains {} · use OPEN",
                folder.display(),
                model::PROJECT_FILE
            ));
            return;
        }
        if self.state.doc.dirty {
            self.state.view.modal = Some(Modal {
                title: "DISCARD CHANGES AND CREATE A NEW PROJECT?".into(),
                text: folder.display().to_string(),
                action: ModalAction::ConfirmNewProject(folder),
            });
            self.state.bump_view();
            return;
        }
        self.create_project_now(folder);
    }

    fn create_project_now(&mut self, folder: PathBuf) {
        if self.state.playback.preset_playing {
            self.stop();
        }
        match crate::project_io::create_project(&folder) {
            Ok((_, mut warnings)) => {
                self.state.view.timeline_zoom = 1.0;
                self.state.view.timeline_offset = 0.0;
                match open_folder(&mut self.state, folder) {
                    Ok(load_warnings) => {
                        warnings.extend(load_warnings);
                        let summary = format!(
                            "project created · {} presets · {} music",
                            self.state.presets.len(),
                            self.state.music_bank.len()
                        );
                        self.state.toast(if warnings.is_empty() {
                            summary
                        } else {
                            format!("{summary} · {}", warnings.join("; "))
                        });
                    }
                    Err(error) => self
                        .state
                        .toast(format!("project created but open failed: {error}")),
                }
            }
            Err(error) => self.state.toast(format!("new project failed: {error}")),
        }
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
        if self.state.playback.preset_playing {
            self.stop();
        }
        self.state.view.timeline_zoom = 1.0;
        self.state.view.timeline_offset = 0.0;
        match crate::project_io::open_or_create(&mut self.state, folder) {
            Ok((warnings, created)) => {
                let summary = if created {
                    format!(
                        "project created · {} presets · {} music",
                        self.state.presets.len(),
                        self.state.music_bank.len()
                    )
                } else {
                    "project loaded".to_string()
                };
                self.state.toast(if warnings.is_empty() {
                    summary
                } else {
                    format!("{summary} · {}", warnings.join("; "))
                });
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

    /// Seed `<project>/presets/` with the builtin bank when the library is
    /// empty. This is the GUI equivalent of `--install-presets` and never
    /// overwrites an existing file.
    fn install_presets(&mut self) {
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        match crate::presets::PresetBank::install_missing(&root, &crate::builtin::bank(20_240_607))
        {
            Ok(0) => {
                let warnings = self.state.presets.load_project(&root);
                self.state.bump_view();
                if self.state.presets.is_empty() {
                    self.state
                        .toast("no usable presets in presets/ · check the JSON files");
                } else if warnings.is_empty() {
                    self.state
                        .toast("all builtin presets are already installed");
                } else {
                    self.state
                        .toast(format!("presets reloaded · {}", warnings.join("; ")));
                }
            }
            Ok(written) => {
                let warnings = self.state.presets.load_project(&root);
                self.state.view.selected_library = None;
                self.state.bump_view();
                self.state.toast(if warnings.is_empty() {
                    format!("installed {written} builtin presets")
                } else {
                    format!(
                        "installed {written} builtin presets · {}",
                        warnings.join("; ")
                    )
                });
            }
            Err(error) => self.state.toast(format!("install presets failed: {error}")),
        }
    }

    /// Persist the current project patch into `<project>/presets/` so it can
    /// be auditioned and dragged like the bundled library. Existing preset
    /// names are never overwritten.
    fn save_as_preset(&mut self) {
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        let Some(patch) = self.state.patch().cloned() else {
            self.state.toast("select a sound first");
            return;
        };
        let name = patch.name.clone();
        match crate::presets::PresetBank::install_missing(&root, &[patch]) {
            Ok(0) => self.state.toast(format!("preset '{name}' already exists")),
            Ok(_) => {
                let warnings = self.state.presets.load_project(&root);
                if let Some(index) = self
                    .state
                    .presets
                    .presets
                    .iter()
                    .position(|preset| preset.name == name)
                {
                    self.state.select_preset(index);
                }
                self.state.bump_view();
                self.state.toast(if warnings.is_empty() {
                    format!("saved preset '{name}'")
                } else {
                    format!("saved preset '{name}' · {}", warnings.join("; "))
                });
            }
            Err(error) => self.state.toast(format!("save preset failed: {error}")),
        }
    }

    fn selected_preset(&self) -> Option<(usize, String)> {
        let index = self.state.selected_preset_index()?;
        let name = self.state.presets.get(index)?.name.clone();
        Some((index, name))
    }

    fn open_rename_preset(&mut self) {
        let Some((_, name)) = self.selected_preset() else {
            self.state.toast("select a preset first");
            return;
        };
        self.state.view.modal = Some(Modal {
            title: "RENAME PRESET".into(),
            text: name,
            action: ModalAction::RenamePreset,
        });
        self.state.bump_view();
    }

    fn open_delete_preset(&mut self) {
        let Some((_, name)) = self.selected_preset() else {
            self.state.toast("select a preset first");
            return;
        };
        self.state.view.modal = Some(Modal {
            title: "DELETE PRESET?".into(),
            text: format!("{name} → presets/.trash/"),
            action: ModalAction::ConfirmDeletePreset,
        });
        self.state.bump_view();
    }

    fn restore_audio(&mut self) {
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        match self.state.audio_bank.restore_latest(&root) {
            Ok(Some(name)) => {
                self.state.audio_bank.load_project(&root);
                if let Some(index) = self
                    .state
                    .audio_bank
                    .assets()
                    .iter()
                    .position(|asset| asset.name == name)
                {
                    self.state.select_audio(index);
                }
                self.state.bump_view();
                self.state.toast(format!("restored audio '{name}'"));
            }
            Ok(None) => self.state.toast("no deleted audio in audio/.trash/"),
            Err(error) => self.state.toast(format!("restore audio failed: {error}")),
        }
    }

    fn restore_music(&mut self) {
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        match self.state.music_bank.restore_latest(&root) {
            Ok(Some(name)) => {
                self.state.music_bank.load_project(&root);
                if let Some(index) = self
                    .state
                    .music_bank
                    .assets()
                    .iter()
                    .position(|asset| asset.name == name)
                {
                    self.state.select_music(index);
                }
                self.state.bump_view();
                self.state.toast(format!("restored music '{name}'"));
            }
            Ok(None) => self.state.toast("no deleted music in music/.trash/"),
            Err(error) => self.state.toast(format!("restore music failed: {error}")),
        }
    }

    fn open_rename_audio(&mut self) {
        let Some(asset) = self.state.selected_audio() else {
            self.state.toast("select an audio file first");
            return;
        };
        let stem = std::path::Path::new(&asset.name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("audio")
            .to_string();
        self.state.view.modal = Some(Modal {
            title: "RENAME AUDIO".into(),
            text: stem,
            action: ModalAction::RenameAudio,
        });
        self.state.bump_view();
    }

    fn open_delete_audio(&mut self) {
        let Some(asset) = self.state.selected_audio() else {
            self.state.toast("select an audio file first");
            return;
        };
        let referenced = self.state.doc.project.patches.iter().any(|patch| {
            patch
                .layers
                .iter()
                .any(|layer| layer.sample_path == asset.path)
        }) || self.state.presets.presets.iter().any(|patch| {
            patch
                .layers
                .iter()
                .any(|layer| layer.sample_path == asset.path)
        });
        let text = if referenced {
            format!("{} (被 SAMPLE 层引用) → audio/.trash/", asset.name)
        } else {
            format!("{} → audio/.trash/", asset.name)
        };
        self.state.view.modal = Some(Modal {
            title: "DELETE AUDIO?".into(),
            text,
            action: ModalAction::ConfirmDeleteAudio,
        });
        self.state.bump_view();
    }

    fn open_rename_music(&mut self) {
        let Some(index) = self.state.selected_music() else {
            self.state.toast("select a music file first");
            return;
        };
        let Some(asset) = self.state.music_bank.get(index) else {
            return;
        };
        self.state.view.modal = Some(Modal {
            title: "RENAME MUSIC".into(),
            text: asset.name.clone(),
            action: ModalAction::RenameMusic,
        });
        self.state.bump_view();
    }

    fn open_delete_music(&mut self) {
        let Some(index) = self.state.selected_music() else {
            self.state.toast("select a music file first");
            return;
        };
        let Some(asset) = self.state.music_bank.get(index) else {
            return;
        };
        self.state.view.modal = Some(Modal {
            title: "DELETE MUSIC?".into(),
            text: format!("{} → music/.trash/", asset.name),
            action: ModalAction::ConfirmDeleteMusic,
        });
        self.state.bump_view();
    }

    fn restore_preset(&mut self) {
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        match self.state.presets.restore_latest() {
            Ok(Some(name)) => {
                let warnings = self.state.presets.load_project(&root);
                if let Some(index) = self
                    .state
                    .presets
                    .presets
                    .iter()
                    .position(|preset| preset.name == name)
                {
                    self.state.select_preset(index);
                }
                self.state.bump_view();
                self.state.toast(if warnings.is_empty() {
                    format!("restored preset '{name}'")
                } else {
                    format!("restored preset '{name}' · {}", warnings.join("; "))
                });
            }
            Ok(None) => self.state.toast("no deleted presets in presets/.trash/"),
            Err(error) => self.state.toast(format!("restore preset failed: {error}")),
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
        let mut sample_warnings = Vec::new();
        if let Some(LibraryKey::Preset(id)) = self.state.view.selected_library {
            if let Some(index) = self.state.presets.index_of(id) {
                sample_warnings = self.state.presets.hydrate(index);
            }
        }
        let Some(patch) = self.state.export_patch().cloned() else {
            self.state.toast("select a sound or preset to export");
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
        match export::write_patch_wav(&patch, &path) {
            Ok(seconds) => {
                let warnings = if sample_warnings.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", sample_warnings.join("; "))
                };
                self.state.toast(format!(
                    "exported {} · {} · {seconds:.2}s · 48 kHz / 24-bit stereo{warnings}",
                    patch.name,
                    path.display()
                ));
            }
            Err(error) => self.state.toast(format!("export failed: {error}")),
        }
    }

    /// Opt an older project into the v3 stereo/space chain.
    fn upgrade_dsp(&mut self) {
        let Some(current) = self.state.patch().map(|patch| patch.dsp_version) else {
            return;
        };
        if current >= model::DSP_VERSION {
            return;
        }
        self.push_undo();
        if let Some(patch) = self.state.patch_mut() {
            patch.dsp_version = model::DSP_VERSION;
        }
        self.state.mark_dirty();
        self.state
            .toast("upgraded to HD DSP v3 · pan + space + limiter");
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

    fn load_sample(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Load a WAV sample")
            .add_filter("WAV audio", &["wav"])
            .pick_file()
        else {
            return;
        };
        let data = match crate::wav::load(&path) {
            Ok(data) => data,
            Err(error) => {
                self.state.toast(format!("sample load failed: {error}"));
                return;
            }
        };
        self.apply_sample(&path, data);
    }

    /// Apply a decoded sample as a layer edit. Kept separate from the file
    /// dialog so the undo transaction is testable and only fires after a
    /// successful load (a cancelled picker must not create an undo entry).
    pub(super) fn apply_sample(&mut self, source: &std::path::Path, data: crate::wav::SampleData) {
        self.state.begin_edit();
        let stored = self.store_sample(source);
        if let Some(layer) = self.state.layer_mut() {
            layer.wave = Wave::Sample;
            layer.sample_pitch = 0.0;
            layer.sample_loop = false;
            layer.sample_start = 0.0;
            layer.sample_reverse = false;
            layer.duration = (data.samples.len() as f32 / data.rate as f32).clamp(0.005, 3.0);
            layer.sample_path = stored;
            layer.sample_data = Some(std::sync::Arc::new(data));
        }
        if let Some(root) = self.state.doc.root.clone() {
            let selected = self.state.selected_audio().map(|asset| asset.id);
            self.state.audio_bank.load_project(&root);
            if let Some(id) = selected {
                if self.state.audio_bank.get_id(id).is_some() {
                    self.state.view.selected_library = Some(LibraryKey::Audio(id));
                }
            }
        }
        self.state.mark_dirty();
        self.state.commit_edit();
        self.state.bump_view();
        self.state.toast("sample loaded");
    }

    /// Copy the WAV into the project when possible so the JSON stays portable.
    fn store_sample(&self, source: &std::path::Path) -> String {
        let Some(root) = &self.state.doc.root else {
            return source.display().to_string();
        };
        let dir = root.join("audio").join("sfx").join("samples");
        if std::fs::create_dir_all(&dir).is_err() {
            return source.display().to_string();
        }
        let stem = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(model::sanitize)
            .unwrap_or_else(|| "sample".into());
        let mut name = format!("{stem}.wav");
        let mut index = 2;
        while dir.join(&name).exists() {
            name = format!("{stem}_{index}.wav");
            index += 1;
        }
        let target = dir.join(&name);
        if std::fs::copy(source, &target).is_err() {
            return source.display().to_string();
        }
        format!("audio/sfx/samples/{name}")
    }

    fn toggle_sample_loop(&mut self) {
        self.push_undo();
        if let Some(layer) = self.state.layer_mut() {
            layer.sample_loop = !layer.sample_loop;
        }
        self.state.mark_dirty();
        self.state.bump_view();
    }

    fn toggle_sample_reverse(&mut self) {
        self.push_undo();
        if let Some(layer) = self.state.layer_mut() {
            layer.sample_reverse = !layer.sample_reverse;
        }
        self.state.mark_dirty();
        self.state.bump_view();
    }

    fn vary(&mut self) {
        let Some(base) = self.state.patch().cloned() else {
            return;
        };
        let start = self.state.patch_index().unwrap_or(0);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0xA11CE);
        for (index, suffix) in [" A", " B", " C"].iter().enumerate() {
            let mut copy = base.clone();
            copy.name = format!("{}{suffix}", base.name);
            copy.favorite = base.favorite;
            let mut rng = synth::Rng::new(seed.wrapping_add(index as u64 * 0x9E3779B9));
            copy.space = (copy.space + rng.next_f32().abs() * 0.10).clamp(0.0, 1.0);
            copy.room = (copy.room + rng.next_f32() * 0.10).clamp(0.0, 1.0);
            for layer in &mut copy.layers {
                randomize_layer(layer, &mut rng, 0.30);
                if copy.dsp_version >= 3 {
                    layer.pan = (layer.pan + rng.next_f32() * 0.25).clamp(-1.0, 1.0);
                }
            }
            self.state
                .doc
                .project
                .patches
                .insert(start + 1 + index, copy);
        }
        self.state.mark_dirty();
        self.state.select_patch(start + 1);
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

    pub(super) fn toggle_preset_favorite(&mut self, index: usize) {
        let Some(patch) = self.state.presets.get(index).cloned() else {
            return;
        };
        let favorite = self.state.presets.toggle_favorite(&patch);
        self.state.presets.save_favorites();
        if self.state.view.favorites_only {
            self.refresh_filter();
        } else {
            self.state.bump_view();
        }
        self.state.toast(if favorite {
            "preset added to favorites"
        } else {
            "preset removed from favorites"
        });
    }

    /// Audition a project-local preset without touching the project document.
    pub(super) fn audition_preset(&mut self, index: usize) {
        let sample_warnings = self.state.presets.hydrate(index);
        let Some(patch) = self.state.presets.get(index).cloned() else {
            return;
        };
        let rate = self.state.preview_rate();
        let Some(audio) = &self.audio else {
            self.state.toast("no audio device");
            return;
        };
        if audio.is_playing() {
            audio.stop();
        }
        let samples = fx::render_patch(&patch, rate).interleave();
        if samples.is_empty() {
            self.state.toast("nothing to play");
            return;
        }
        audio.play(samples, 2, 0, self.state.playback.loop_play);
        self.state.playback.source = Some(PlaybackSource::Preset);
        self.state.playback.preset_playing = true;
        self.state.playback.was_playing = true;
        self.state.playback.seeked = false;
        let warnings = if sample_warnings.is_empty() {
            String::new()
        } else {
            format!(" · {}", sample_warnings.join("; "))
        };
        self.state.toast(format!(
            "preview · {} · drag to timeline{warnings}",
            patch.name
        ));
    }

    /// Select a row on pointer-down. Preset/audio rows also arm a drag; music
    /// rows only switch the editor until the click completes.
    pub(super) fn select_library_row(&mut self, key: LibraryKey) {
        match key {
            LibraryKey::Sound(id) => {
                if let Some(index) = self
                    .state
                    .doc
                    .project
                    .patches
                    .iter()
                    .position(|patch| patch.id == id)
                {
                    self.state.view.tab = Tab::Patches;
                    self.state.select_patch(index);
                }
            }
            LibraryKey::Audio(id) => {
                if let Some(index) = self.state.audio_bank.index_of(id) {
                    self.state.view.tab = Tab::Patches;
                    self.state.playback.defer_render = true;
                    self.state.select_audio(index);
                }
            }
            LibraryKey::Preset(id) => {
                if let Some(index) = self.state.presets.index_of(id) {
                    self.state.select_preset(index);
                }
            }
            LibraryKey::Music(id) => {
                if let Some(index) = self.state.music_bank.index_of(id) {
                    self.state.view.tab = Tab::Music;
                    self.state.select_music(index);
                    self.state.playback.defer_render = true;
                }
            }
        }
    }

    /// Complete a click on a library row: sound selects, audio/preset auditions,
    /// music loads the project bed or a demo arrangement.
    pub(super) fn activate_library_row(&mut self, key: LibraryKey) {
        match key {
            LibraryKey::Sound(id) => {
                if let Some(index) = self
                    .state
                    .doc
                    .project
                    .patches
                    .iter()
                    .position(|patch| patch.id == id)
                {
                    self.state.view.tab = Tab::Patches;
                    self.state.select_patch(index);
                }
            }
            LibraryKey::Audio(id) => {
                if let Some(index) = self.state.audio_bank.index_of(id) {
                    self.state.view.tab = Tab::Patches;
                    self.state.playback.defer_render = true;
                    self.audition_audio(index);
                }
            }
            LibraryKey::Preset(id) => {
                if let Some(index) = self.state.presets.index_of(id) {
                    self.audition_preset(index);
                }
            }
            LibraryKey::Music(id) => {
                if let Some(index) = self.state.music_bank.index_of(id) {
                    self.load_music(index);
                }
            }
        }
    }

    /// Load a `<project>/music/*.json` arrangement into the current project.
    /// This is an undoable document edit, not a preview.
    pub(super) fn load_music(&mut self, index: usize) {
        let Some(asset) = self.state.music_bank.get(index) else {
            return;
        };
        let name = asset.name.clone();
        let music = asset.music.clone();
        self.state.select_music(index);
        let changed = self.state.doc.project.music != music;
        if changed {
            self.state.push_undo_label("load music");
            self.state.doc.project.music = music;
            self.state.mark_music_dirty();
            self.state.playback.playhead = 0.0;
        }
        self.state.view.tab = Tab::Music;
        self.state.view.inspector_scroll = 0.0;
        self.state.playback.defer_render = true;
        self.state.bump_view();
        self.state.toast(if changed {
            format!("music · {name}")
        } else {
            format!("music · {name} · already loaded")
        });
    }

    /// Audition a WAV asset from the project audio folder. The audio callback
    /// expects 48 kHz source frames, so native-rate files are resampled here.
    pub(super) fn audition_audio(&mut self, index: usize) {
        let Some(asset) = self.state.audio_bank.get(index).cloned() else {
            return;
        };
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        let path = asset.absolute(&root);
        let data = match crate::wav::load(&path) {
            Ok(data) => data,
            Err(error) => {
                self.state.toast(format!("audio load failed: {error}"));
                return;
            }
        };
        let mut samples = data.samples;
        if data.rate != synth::PREVIEW_RATE {
            samples = resample_linear(&samples, data.rate, synth::PREVIEW_RATE);
        }
        let Some(audio) = &self.audio else {
            self.state.toast("no audio device");
            return;
        };
        if audio.is_playing() {
            audio.stop();
        }
        if samples.is_empty() {
            self.state.toast("nothing to play");
            return;
        }
        audio.play(samples, 1, 0, false);
        self.state.playback.source = Some(PlaybackSource::Audio);
        self.state.playback.preset_playing = true;
        self.state.playback.was_playing = true;
        self.state.playback.seeked = false;
        self.state
            .toast(format!("audio · {} · drag to timeline", asset.name));
    }

    /// Insert a WAV asset as a `Wave::Sample` layer at the drop time.
    pub(super) fn drop_audio(&mut self, index: usize, position: Vec2, layout: &ui::Layout) {
        let Some(asset) = self.state.audio_bank.get(index).cloned() else {
            return;
        };
        let Some(root) = self.state.doc.root.clone() else {
            self.state.toast("open a project first");
            return;
        };
        let Some(target) = ui::drop_target(&self.state, layout, position) else {
            self.state.toast("drop audio on the timeline");
            return;
        };
        let (layer, time) = match target {
            ui::DropTarget::Timeline { layer, time } => (layer, time),
            ui::DropTarget::ProjectLibrary => {
                self.state.toast("drop audio on the timeline");
                return;
            }
        };
        if self.state.view.tab != Tab::Patches {
            self.state.toast("switch to the SOUND tab to drop audio");
            return;
        }
        let Some(patch_index) = self.state.patch_index() else {
            self.state.toast("select a project patch first");
            return;
        };
        let current_layers = self
            .state
            .patch()
            .map(|patch| patch.layers.len())
            .unwrap_or(0);
        if current_layers >= 32 {
            self.state.toast("layer limit is 32 · delete a layer first");
            return;
        }
        let path = asset.absolute(&root);
        let data = match crate::wav::load(&path) {
            Ok(data) => data,
            Err(error) => {
                self.state.toast(format!("audio load failed: {error}"));
                return;
            }
        };
        let time = if self.state.view.modifiers.shift {
            time
        } else {
            ui::snap_time(time)
        };
        let duration = (data.samples.len() as f32 / data.rate as f32).clamp(0.005, 3.0);
        let mut new_layer = Layer::new(Wave::Sample);
        new_layer.sample_path = asset.path.clone();
        new_layer.sample_data = Some(std::sync::Arc::new(data));
        new_layer.duration = duration;
        new_layer.clips = vec![Clip {
            start: time,
            duration,
            ..Clip::default()
        }];
        self.state.begin_edit();
        let at = self
            .state
            .patch()
            .map(|patch| patch.layers.len())
            .unwrap_or(0)
            .min(layer);
        if let Some(patch) = self.state.doc.project.patches.get_mut(patch_index) {
            patch.layers.insert(at, new_layer);
        }
        self.state.mark_dirty();
        self.state.select_layer(at);
        self.state.select_clip(0);
        self.state.commit_edit();
        self.state
            .toast(format!("inserted {} at {time:.3}s", asset.name));
    }

    pub(super) fn drop_library_item(
        &mut self,
        key: LibraryKey,
        position: Vec2,
        layout: &ui::Layout,
    ) {
        match key {
            LibraryKey::Preset(id) => {
                if let Some(index) = self.state.presets.index_of(id) {
                    self.drop_preset(index, position, layout);
                }
            }
            LibraryKey::Audio(id) => {
                if let Some(index) = self.state.audio_bank.index_of(id) {
                    self.drop_audio(index, position, layout);
                }
            }
            LibraryKey::Sound(_) | LibraryKey::Music(_) => {
                self.state.toast("only audio and presets can be dragged");
            }
        }
    }

    /// Complete a library drag. Timeline drops copy the preset's layers into
    /// the current patch; left-panel drops copy the preset into the project.
    pub(super) fn drop_preset(&mut self, index: usize, position: Vec2, layout: &ui::Layout) {
        let sample_warnings = self.state.presets.hydrate(index);
        let Some(preset) = self.state.presets.get(index).cloned() else {
            return;
        };
        let Some(target) = ui::drop_target(&self.state, layout, position) else {
            self.state
                .toast("drop a preset on the timeline or the left library");
            return;
        };
        let preset_name = preset.name.clone();
        match target {
            ui::DropTarget::Timeline { layer, time } => {
                if self.state.view.tab != Tab::Patches {
                    self.state.toast("switch to the SOUND tab to drop a preset");
                    return;
                }
                let Some(patch_index) = self.state.patch_index() else {
                    self.state.toast("select a project patch first");
                    return;
                };
                let current_layers = self
                    .state
                    .patch()
                    .map(|patch| patch.layers.len())
                    .unwrap_or(0);
                if current_layers + preset.layers.len() > 32 {
                    self.state.toast("layer limit is 32 · delete a layer first");
                    return;
                }
                let time = if self.state.view.modifiers.shift {
                    time
                } else {
                    ui::snap_time(time)
                };
                let layers = preset.instantiate_layers(time);
                if layers.is_empty() {
                    return;
                }
                let count = layers.len();
                let first_has_clips = layers
                    .first()
                    .map(|layer| !layer.clips.is_empty())
                    .unwrap_or(false);
                self.state.begin_edit();
                let at = self
                    .state
                    .patch()
                    .map(|patch| patch.layers.len())
                    .unwrap_or(0)
                    .min(layer);
                if let Some(patch) = self.state.doc.project.patches.get_mut(patch_index) {
                    for (offset, layer) in layers.into_iter().enumerate() {
                        patch.layers.insert(at + offset, layer);
                    }
                }
                self.state.mark_dirty();
                let last = self.state.patch().map(|p| p.layers.len()).unwrap_or(1);
                self.state.select_layer(at.min(last.saturating_sub(1)));
                if first_has_clips {
                    self.state.select_clip(0);
                }
                self.state.commit_edit();
                let warnings = if sample_warnings.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", sample_warnings.join("; "))
                };
                self.state.toast(format!(
                    "inserted {count} layer(s) at {time:.3}s · {preset_name}{warnings}"
                ));
            }
            ui::DropTarget::ProjectLibrary => {
                if self.state.doc.project.patches.len() >= 512 {
                    self.state.toast("project patch limit is 512");
                    return;
                }
                self.state.begin_edit();
                let index = self.state.doc.project.insert_patch_copy(&preset);
                self.state.mark_dirty();
                self.state.select_patch(index);
                self.state.select_layer(0);
                self.state.commit_edit();
                let name = self
                    .state
                    .doc
                    .project
                    .patches
                    .get(index)
                    .map(|patch| patch.name.clone())
                    .unwrap_or(preset_name);
                let warnings = if sample_warnings.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", sample_warnings.join("; "))
                };
                self.state
                    .toast(format!("copied preset '{name}' into the project{warnings}"));
            }
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
        if matches!(
            self.state.view.kind_filter,
            Some(LibraryKind::Audio | LibraryKind::Music)
        ) {
            self.state.view.kind_filter = None;
        }
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
        self.state.view.overlays.close(Overlay::CategoryFilter);
        self.refresh_filter();
    }

    pub(super) fn pick_kind(&mut self, kind: Option<LibraryKind>) {
        self.state.view.kind_filter = kind;
        self.state.view.overlays.close(Overlay::KindFilter);
        self.state.view.overlays.close(Overlay::CategoryFilter);
        self.refresh_filter();
        let selected_visible = self.state.view.selected_library.is_some_and(|key| {
            self.state
                .visible_library_entries()
                .iter()
                .any(|entry| entry.key == key)
        });
        if !selected_visible {
            if let Some(key) = self
                .state
                .visible_library_entries()
                .first()
                .map(|entry| entry.key)
            {
                // Filtering is a view operation: select the first visible row
                // without auditioning/loading anything. Music is loaded only by
                // an explicit click/Enter (see `activate_library_row`).
                self.select_library_row(key);
            } else {
                self.state.view.selected_library = None;
            }
        }
        self.state.toast(match kind {
            Some(kind) => format!("type: {}", kind.label()),
            None => "type: all resources".into(),
        });
    }

    pub(super) fn clear_filters(&mut self) {
        self.state.view.kind_filter = None;
        self.state.view.category_filter = None;
        self.state.view.favorites_only = false;
        self.state.view.search.clear();
        self.state.view.search_focus = false;
        self.state.view.overlays.close(Overlay::KindFilter);
        self.state.view.overlays.close(Overlay::CategoryFilter);
        self.refresh_filter();
        self.state.toast("cleared library filters");
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
        self.state.view.library_scroll = 0;
        let selected = self.state.view.selected_library;
        let visible = self.state.visible_library_entries();
        if selected.is_some_and(|key| !visible.iter().any(|entry| entry.key == key)) {
            self.state.view.selected_library = None;
        }
        if !self.state.preset_focus()
            && (self.state.view.kind_filter.is_none()
                || self.state.view.kind_filter == Some(LibraryKind::Sound))
        {
            self.keep_selection_visible();
        }
        self.state.ensure_library_visible();
        self.state.bump_view();
    }

    fn keep_selection_visible(&mut self) {
        if self
            .state
            .patch_index()
            .is_some_and(|index| self.state.patch_visible(index))
        {
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
            self.state.view.kind_filter = None;
            self.state.view.category_filter = None;
            self.state.view.favorites_only = false;
            self.state.view.library_scroll = 0;
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
            ModalAction::ConfirmNewProject(folder) => self.create_project_now(folder),
            ModalAction::NewPatch => {
                self.push_undo();
                self.state
                    .doc
                    .project
                    .patches
                    .push(Patch::new_layer(name.clone(), Wave::Sine));
                self.state.mark_dirty();
                let last = self.state.doc.project.patches.len() - 1;
                self.state.select_patch(last);
                self.state.select_layer(0);
            }
            ModalAction::RenamePatch => {
                self.push_undo();
                if let Some(patch) = self.state.patch_mut() {
                    patch.name = name;
                    self.state.mark_dirty();
                }
            }
            ModalAction::RenamePreset => {
                let Some((index, old_name)) = self.selected_preset() else {
                    self.state.toast("select a preset first");
                    return;
                };
                if old_name == name {
                    return;
                }
                match self.state.presets.rename(index, &name) {
                    Ok(()) => {
                        let root = self.state.doc.root.clone();
                        let warnings = root
                            .as_deref()
                            .map(|root| self.state.presets.load_project(root))
                            .unwrap_or_default();
                        if let Some(index) = self
                            .state
                            .presets
                            .presets
                            .iter()
                            .position(|preset| preset.name == name)
                        {
                            self.state.select_preset(index);
                        }
                        self.state.bump_view();
                        self.state.toast(if warnings.is_empty() {
                            format!("renamed preset to '{name}'")
                        } else {
                            format!("renamed preset to '{name}' · {}", warnings.join("; "))
                        });
                    }
                    Err(error) => self.state.toast(format!("rename preset failed: {error}")),
                }
            }
            ModalAction::RenameAudio => {
                let Some(root) = self.state.doc.root.clone() else {
                    self.state.toast("open a project first");
                    return;
                };
                let Some(index) = self.state.selected_audio_index() else {
                    self.state.toast("select an audio file first");
                    return;
                };
                match self.state.audio_bank.rename(&root, index, &name) {
                    Ok(()) => {
                        self.state.bump_view();
                        self.state.toast(format!("renamed audio to '{name}'"));
                    }
                    Err(error) => self.state.toast(format!("rename audio failed: {error}")),
                }
            }
            ModalAction::ConfirmDeleteAudio => {
                let Some(root) = self.state.doc.root.clone() else {
                    self.state.toast("open a project first");
                    return;
                };
                let Some(index) = self.state.selected_audio_index() else {
                    self.state.toast("select an audio file first");
                    return;
                };
                match self.state.audio_bank.delete(&root, index) {
                    Ok(()) => {
                        self.state.view.selected_library = None;
                        self.state.bump_view();
                        self.state.toast("moved audio file to audio/.trash/");
                    }
                    Err(error) => self.state.toast(format!("delete audio failed: {error}")),
                }
            }
            ModalAction::RenameMusic => {
                let Some(index) = self.state.selected_music() else {
                    self.state.toast("select a music file first");
                    return;
                };
                match self.state.music_bank.rename(index, &name) {
                    Ok(()) => {
                        self.state.bump_view();
                        self.state.toast(format!("renamed music to '{name}'"));
                    }
                    Err(error) => self.state.toast(format!("rename music failed: {error}")),
                }
            }
            ModalAction::ConfirmDeleteMusic => {
                let Some(root) = self.state.doc.root.clone() else {
                    self.state.toast("open a project first");
                    return;
                };
                let Some(index) = self.state.selected_music() else {
                    self.state.toast("select a music file first");
                    return;
                };
                match self.state.music_bank.delete(&root, index) {
                    Ok(()) => {
                        self.state.view.selected_library = None;
                        self.state.bump_view();
                        self.state.toast("moved music file to music/.trash/");
                    }
                    Err(error) => self.state.toast(format!("delete music failed: {error}")),
                }
            }
            ModalAction::ConfirmDeletePreset => {
                let Some((index, name)) = self.selected_preset() else {
                    self.state.toast("select a preset first");
                    return;
                };
                match self.state.presets.delete(index) {
                    Ok(()) => {
                        let root = self.state.doc.root.clone();
                        let warnings = root
                            .as_deref()
                            .map(|root| self.state.presets.load_project(root))
                            .unwrap_or_default();
                        self.state.view.selected_library = None;
                        self.state.bump_view();
                        self.state.toast(if warnings.is_empty() {
                            format!("moved preset '{name}' to presets/.trash/")
                        } else {
                            format!(
                                "moved preset '{name}' to presets/.trash/ · {}",
                                warnings.join("; ")
                            )
                        });
                    }
                    Err(error) => self.state.toast(format!("delete preset failed: {error}")),
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

fn resample_linear(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if samples.is_empty() || from == 0 || to == 0 || from == to {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len.max(1));
    for index in 0..out_len {
        let pos = index as f64 * ratio;
        let base = pos.floor() as usize;
        let frac = (pos - base as f64) as f32;
        let a = samples.get(base).copied().unwrap_or(0.0);
        let b = samples.get(base + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}
