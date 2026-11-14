//! Minimal cpal output. The callback owns its playback cursor and receives
//! commands through a single-slot handoff; the UI never shares the ring state.
//!
//! Real-time rules followed here:
//! - the callback never blocks: command pickup uses `try_lock` and keeps a
//!   locally owned `Playback` (no long-lived mutex around the render loop);
//! - `is_playing` / `position` are atomics so per-frame UI polling is cheap;
//! - play / stop / seek ramp over ~5 ms to avoid clicks;
//! - looping wraps the frame cursor without re-reading frame 0;
//! - stream errors are counted for the UI and stop the transport.
//!
//! Remaining RT debt: replacing the sample `Vec` drops the previous buffer on
//! the audio thread, and a device error currently needs an app restart.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

const FADE_SECONDS: f32 = 0.005;

enum Command {
    Play {
        samples: Vec<f32>,
        start: usize,
        looping: bool,
    },
    Stop,
}

/// Owned by the audio callback after the stream is built.
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
    fn apply(&mut self, command: Command) {
        match command {
            Command::Play {
                samples,
                start,
                looping,
            } => {
                if samples.is_empty() {
                    self.samples.clear();
                    self.fade_target = 0.0;
                    self.pos = 0;
                    return;
                }
                self.samples = samples;
                self.pos = start.min(self.samples.len().saturating_sub(1));
                self.looping = looping;
                self.fade_target = 1.0;
            }
            Command::Stop => {
                self.fade_target = 0.0;
            }
        }
    }

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

/// Take the newest command without ever blocking the audio thread.
fn take_command(slot: &Mutex<Option<Command>>) -> Option<Command> {
    slot.try_lock().ok().and_then(|mut slot| slot.take())
}

pub struct AudioEngine {
    command: Arc<Mutex<Option<Command>>>,
    playing: Arc<AtomicBool>,
    /// Frame cursor published by the callback once per buffer.
    position: Arc<AtomicU64>,
    device_errors: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
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
        let command = Arc::new(Mutex::new(None));
        let playing = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(0));
        let device_errors = Arc::new(AtomicU64::new(0));
        let alive = Arc::new(AtomicBool::new(true));
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(
                &device,
                &supported.config(),
                channels,
                rate,
                command.clone(),
                playing.clone(),
                position.clone(),
                device_errors.clone(),
                alive.clone(),
            ),
            SampleFormat::I16 => build::<i16>(
                &device,
                &supported.config(),
                channels,
                rate,
                command.clone(),
                playing.clone(),
                position.clone(),
                device_errors.clone(),
                alive.clone(),
            ),
            SampleFormat::U16 => build::<u16>(
                &device,
                &supported.config(),
                channels,
                rate,
                command.clone(),
                playing.clone(),
                position.clone(),
                device_errors.clone(),
                alive.clone(),
            ),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self {
            command,
            playing,
            position,
            device_errors,
            alive,
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
        if !self.alive.load(Ordering::Acquire) {
            self.playing.store(false, Ordering::Release);
            return;
        }
        let start = start_frame.min(samples.len().saturating_sub(1));
        if let Ok(mut slot) = self.command.lock() {
            *slot = Some(Command::Play {
                samples,
                start,
                looping,
            });
        }
        self.position.store(start as u64, Ordering::Release);
        self.playing.store(true, Ordering::Release);
    }

    /// Fade out over 5 ms; output is silent afterwards.
    pub fn stop(&self) {
        if let Ok(mut slot) = self.command.lock() {
            *slot = Some(Command::Stop);
        }
        self.playing.store(false, Ordering::Release);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    pub fn position(&self) -> f32 {
        self.position.load(Ordering::Acquire) as f32 / self.rate.max(1) as f32
    }

    /// Number of stream errors seen since the last call (device loss etc.).
    pub fn take_device_errors(&self) -> u64 {
        self.device_errors.swap(0, Ordering::AcqRel)
    }
}

#[allow(clippy::too_many_arguments)]
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    rate: u32,
    command: Arc<Mutex<Option<Command>>>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
    device_errors: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
) -> Option<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + 'static,
{
    let step = 1.0 / (FADE_SECONDS * rate as f32).max(1.0);
    let error_playing = playing.clone();
    let error = move |err| {
        eprintln!("audio stream error: {err}");
        device_errors.fetch_add(1, Ordering::Relaxed);
        error_playing.store(false, Ordering::Release);
        alive.store(false, Ordering::Release);
    };
    let mut playback = Playback::default();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                // The playback state is owned by this closure; only the command
                // slot is shared and it is read with try_lock.
                if let Some(command) = take_command(&command) {
                    playback.apply(command);
                }
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

    #[test]
    fn play_command_restarts_at_the_requested_frame() {
        let playing = AtomicBool::new(true);
        let mut playback = Playback::default();
        playback.apply(Command::Play {
            samples: vec![0.0, 1.0, 2.0],
            start: 1,
            looping: true,
        });
        assert_eq!(playback.render_frame(1.0, true, &playing), 1.0);
        assert_eq!(playback.render_frame(0.0, true, &playing), 2.0);
        assert_eq!(playback.render_frame(0.0, true, &playing), 0.0);
    }

    #[test]
    fn stop_command_ramps_down_instead_of_cutting() {
        let playing = AtomicBool::new(true);
        let mut playback = Playback::default();
        playback.apply(Command::Play {
            samples: vec![1.0; 8],
            start: 0,
            looping: false,
        });
        assert_eq!(playback.render_frame(0.5, true, &playing), 0.5);
        playback.apply(Command::Stop);
        assert_eq!(playback.render_frame(0.5, true, &playing), 0.0);
        assert_eq!(playback.render_frame(0.5, true, &playing), 0.0);
    }

    #[test]
    fn command_slot_takes_commands_without_blocking() {
        let slot = Mutex::new(Some(Command::Stop));
        assert!(matches!(take_command(&slot), Some(Command::Stop)));
        assert!(take_command(&slot).is_none());
    }
}
