//! Playback start / seek / stop bridge between the UI and the audio thread.

use super::*;

impl SfxLogic {
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
                audio.play(samples, channels, offset, self.state.playback.loop_play);
                if !self.state.playback.seeked {
                    self.state.playback.playhead = 0.0;
                }
            }
            Tab::Music => {
                let mut samples = self.state.playback.music_cache.clone();
                let gain = 10f32.powf(self.state.doc.project.music.volume_db / 20.0);
                for sample in &mut samples {
                    *sample *= gain;
                }
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
    }
}
