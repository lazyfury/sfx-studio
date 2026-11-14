//! Minimal cpal output. The callback reads from a shared mono buffer; the UI
//! plays rendered patches/music and polls the playhead.
//!
//! Real-time rules followed here:
//! - the callback never allocates, logs or blocks on a UI-owned lock for long;
//! - `is_playing` is lock-free so per-frame UI polling stays cheap;
//! - play / stop / seek ramp over ~5 ms to avoid clicks;
//! - looping wraps the frame cursor without re-reading frame 0.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

const FADE_SECONDS: f32 = 0.005;

struct Playback {
    samples: Vec<f32>,
    /// Frame index for the next output sample.
    pos: usize,
    looping: bool,
    /// Current fade gain (0..1) and its target.
    fade: f32,
    fade_target: f32,
}

impl Playback {
    /// Advance one mono frame. Used by the callback and by regression tests
    /// (loop wrap must not replay sample 0 twice).
    fn render_frame(&mut self, step: f32, audible: bool, playing: &AtomicBool) -> f32 {
        let mut value = 0.0f32;
        if (audible || self.fade > 0.0) && !self.samples.is_empty() {
            let len = self.samples.len();
            if self.pos >= len {
                if self.looping {
                    // Wrap without replaying the last frame.
                    self.pos %= len;
                } else {
                    self.fade_target = 0.0;
                    self.pos = len;
                }
            }
            if self.pos < len {
                value = self.samples[self.pos];
                self.pos += 1;
            }
        }

        if self.fade < self.fade_target {
            self.fade = (self.fade + step).min(self.fade_target);
        } else if self.fade > self.fade_target {
            self.fade = (self.fade - step).max(self.fade_target);
        }
        if self.fade <= 0.0 && self.fade_target <= 0.0 {
            playing.store(false, Ordering::Release);
        }
        value * self.fade
    }
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            pos: 0,
            looping: false,
            fade: 0.0,
            fade_target: 0.0,
        }
    }
}

pub struct AudioEngine {
    shared: Arc<Mutex<Playback>>,
    playing: Arc<AtomicBool>,
    /// Frame cursor published by the callback once per buffer; the UI reads it
    /// lock-free so polling the playhead cannot block behind the audio thread.
    position: Arc<AtomicU64>,
    rate: u32,
    _stream: cpal::Stream,
}

impl AudioEngine {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let shared = Arc::new(Mutex::new(Playback::default()));
        let playing = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(0));
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(
                &device,
                &supported.config(),
                channels,
                rate,
                shared.clone(),
                playing.clone(),
                position.clone(),
            ),
            SampleFormat::I16 => build::<i16>(
                &device,
                &supported.config(),
                channels,
                rate,
                shared.clone(),
                playing.clone(),
                position.clone(),
            ),
            SampleFormat::U16 => build::<u16>(
                &device,
                &supported.config(),
                channels,
                rate,
                shared.clone(),
                playing.clone(),
                position.clone(),
            ),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self {
            shared,
            playing,
            position,
            rate,
            _stream: stream,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Start playing `samples` at `start_frame`. `start_frame` is clamped so a
    /// stale seek cannot index past the buffer.
    pub fn play(&self, samples: Vec<f32>, start_frame: usize, looping: bool) {
        if samples.is_empty() {
            self.stop();
            return;
        }
        let start = start_frame.min(samples.len().saturating_sub(1));
        if let Ok(mut playback) = self.shared.lock() {
            playback.samples = samples;
            playback.pos = start;
            playback.looping = looping;
            playback.fade_target = 1.0;
            if playback.fade <= 0.0 {
                playback.fade = 0.0;
            }
        }
        self.position.store(start as u64, Ordering::Release);
        self.playing.store(true, Ordering::Release);
    }

    /// Fade out over 5 ms; output is silent afterwards.
    pub fn stop(&self) {
        if let Ok(mut playback) = self.shared.lock() {
            playback.fade_target = 0.0;
        }
        self.playing.store(false, Ordering::Release);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    pub fn position(&self) -> f32 {
        self.position.load(Ordering::Acquire) as f32 / self.rate.max(1) as f32
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    rate: u32,
    shared: Arc<Mutex<Playback>>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
) -> Option<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + 'static,
{
    let step = 1.0 / (FADE_SECONDS * rate as f32).max(1.0);
    let error = |err| eprintln!("audio stream error: {err}");
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut playback = match shared.lock() {
                    Ok(value) => value,
                    Err(_) => {
                        for sample in data.iter_mut() {
                            *sample = T::from_sample(0.0);
                        }
                        return;
                    }
                };
                let audible = playing.load(Ordering::Acquire);
                for frame in data.chunks_mut(channels) {
                    let value = playback.render_frame(step, audible, &playing);
                    for sample in frame {
                        *sample = T::from_sample(value);
                    }
                }
                position.store(playback.pos as u64, Ordering::Release);
            },
            error,
            None,
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looping_wraps_without_duplicating_the_first_sample() {
        let playing = AtomicBool::new(true);
        let mut playback = Playback {
            samples: vec![0.0, 1.0, 2.0],
            looping: true,
            fade: 1.0,
            fade_target: 1.0,
            ..Playback::default()
        };
        let mut out = Vec::new();
        for _ in 0..6 {
            out.push(playback.render_frame(0.0, true, &playing));
        }
        assert_eq!(out, vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0]);
        assert!(playing.load(Ordering::Acquire));
    }

    #[test]
    fn non_looping_fades_out_and_stops() {
        let playing = AtomicBool::new(true);
        let mut playback = Playback {
            samples: vec![1.0],
            fade: 1.0,
            fade_target: 1.0,
            ..Playback::default()
        };
        assert_eq!(playback.render_frame(0.5, true, &playing), 1.0);
        assert_eq!(playback.render_frame(0.5, true, &playing), 0.0);
        assert_eq!(playback.render_frame(0.5, true, &playing), 0.0);
        assert!(!playing.load(Ordering::Acquire));
    }
}
