//! Playback start / seek / stop bridge between the UI and the audio thread.

use super::*;

impl SfxLogic {
    pub(super) fn play(&mut self) {
        let rate = self.state.preview_rate();
        self.state.ensure_cache(rate);
        self.state.defer_render = false;
        let Some(audio) = &self.audio else {
            self.state.toast("no audio device");
            return;
        };
        match self.state.tab {
            Tab::Patches => {
                let samples = self.state.cache.clone();
                let offset = if self.state.seeked {
                    (self.state.playhead * rate as f32) as usize
                } else {
                    0
                };
                if samples.is_empty() || offset >= samples.len() {
                    self.state.toast("nothing to play");
                    return;
                }
                audio.play(samples, offset, self.state.loop_play);
                if !self.state.seeked {
                    self.state.playhead = 0.0;
                }
            }
            Tab::Music => {
                let mut samples = self.state.music_cache.clone();
                let gain = 10f32.powf(self.state.project.music.volume_db / 20.0);
                for sample in &mut samples {
                    *sample *= gain;
                }
                audio.play(samples, 0, true);
                self.state.playhead = 0.0;
            }
        }
        self.state.seeked = false;
        self.state.was_playing = true;
    }

    /// Seek while paused. Stopping the stream avoids the UI and the audio
    /// thread disagreeing about the current position.
    pub(super) fn seek(&mut self, time: f32) {
        if let Some(audio) = &self.audio {
            if audio.is_playing() {
                audio.stop();
            }
        }
        self.state.was_playing = false;
        self.state.playhead = time.max(0.0);
        self.state.seeked = true;
        self.state.bump_view();
    }

    pub(super) fn stop(&mut self) {
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        self.state.playhead = 0.0;
        self.state.seeked = false;
        self.state.was_playing = false;
    }
}
