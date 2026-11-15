//! Playback start / seek / stop bridge between the UI and the audio thread.

use super::*;

impl SfxLogic {
    /// Space toggles whichever library is active: the project patch in
    /// PROJECT mode, or a read-only audition of the selected preset in
    /// PRESETS mode.
    pub(super) fn toggle_play(&mut self) {
        let audio_playing = self.audio.as_ref().is_some_and(|audio| audio.is_playing());
        let transport_active = self.state.playback.source.is_some()
            && (self.state.playback.was_playing || audio_playing);
        if transport_active {
            self.stop();
        } else {
            self.play_focused();
        }
    }

    /// Start the transport for the focused object. Asset focus auditions the
    /// asset; otherwise the current tab's document is played. Space and the
    /// header PLAY button both use this entry point.
    pub(super) fn play_focused(&mut self) {
        if let Some(LibraryKey::Audio(id)) = self.state.view.selected_library {
            match self.state.audio_bank.index_of(id) {
                Some(index) => self.audition_audio(index),
                None => self.state.toast("audio file is no longer available"),
            }
            return;
        }
        if self.state.preset_library_focus() {
            let Some(index) = self.state.selected_preset_index() else {
                self.state.toast("select a preset to audition");
                return;
            };
            self.audition_preset(index);
            return;
        }
        if self.state.selected_music().is_some() {
            self.state.view.tab = Tab::Music;
        }
        self.play();
    }

    pub(super) fn play(&mut self) {
        let rate = self.state.preview_rate();
        self.cancel_preview();
        self.state.ensure_cache(rate);
        self.state.playback.defer_render = false;
        self.state.playback.preset_playing = false;
        let Some(audio) = &self.audio else {
            self.state.toast("no audio device");
            return;
        };
        match self.state.view.tab {
            Tab::Patches => {
                let samples = self.state.playback.cache.clone();
                let channels = self.state.playback.cache_channels.max(1);
                let offset = if self.state.playback.seeked {
                    (self.state.playback.playhead * rate as f32) as usize
                } else {
                    0
                };
                if samples.is_empty() || offset >= samples.len() / channels {
                    self.state.toast("nothing to play");
                    return;
                }
                self.state.playback.source = Some(PlaybackSource::Patch);
                audio.play_shared(samples, channels, offset, self.state.playback.loop_play);
                if !self.state.playback.seeked {
                    self.state.playback.playhead = 0.0;
                }
            }
            Tab::Music => {
                // Music volume is applied at play time; this is the only buffer
                // that still needs a copy (the patch/asset caches are shared).
                let mut samples = (*self.state.playback.music_cache).clone();
                let gain = 10f32.powf(self.state.doc.project.music.volume_db / 20.0);
                for sample in &mut samples {
                    *sample *= gain;
                }
                self.state.playback.source = Some(PlaybackSource::Music);
                audio.play(samples, 1, 0, true);
                self.state.playback.playhead = 0.0;
            }
        }
        self.state.playback.seeked = false;
        self.state.playback.was_playing = true;
    }

    /// Seek while paused. Stopping the stream avoids the UI and the audio
    /// thread disagreeing about the current position.
    pub(super) fn seek(&mut self, time: f32) {
        if let Some(audio) = &self.audio {
            if audio.is_playing() {
                audio.stop();
            }
        }
        self.state.playback.was_playing = false;
        self.state.playback.preset_playing = false;
        self.state.playback.source = None;
        self.state.playback.playhead = time.max(0.0);
        self.state.playback.seeked = true;
        self.state.bump_view();
    }

    pub(super) fn stop(&mut self) {
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        self.state.playback.playhead = 0.0;
        self.state.playback.seeked = false;
        self.state.playback.was_playing = false;
        self.state.playback.preset_playing = false;
        self.state.playback.source = None;
    }
}
