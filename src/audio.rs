//! Minimal cpal output. The callback owns its playback cursor and receives
//! commands through a single-slot handoff; the UI never shares the ring state.
//!
//! Real-time rules followed here:
//! - the callback never blocks: command pickup uses `try_lock` and keeps a
//!   locally owned `Playback` (no long-lived mutex around the render loop);
//! - `is_playing` / `position` are atomics so per-frame UI polling is cheap;
//! - previews render at [`synth::PREVIEW_RATE`] and the callback linearly
//!   resamples them to the device rate;
//! - retired sample buffers are handed back to the UI for deallocation;
//! - play / stop / seek ramp over ~5 ms to avoid clicks;
//! - looping wraps the frame cursor without re-reading frame 0.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::synth::PREVIEW_RATE;

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
    /// Fractional source-frame cursor (48 kHz); advanced by the resample ratio.
    pos: f64,
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
                    self.pos = 0.0;
                    return;
                }
                self.samples = samples;
                self.pos = start.min(self.samples.len().saturating_sub(1)) as f64;
                self.looping = looping;
                self.fade_target = 1.0;
            }
            Command::Stop => {
                self.fade_target = 0.0;
            }
        }
    }

    /// Advance one output frame. `ratio` is source frames per output frame.
    /// Used by the callback and by regression tests.
    fn render_frame(&mut self, step: f32, ratio: f64, audible: bool, playing: &AtomicBool) -> f32 {
        let mut value = 0.0f32;
        if (audible || self.fade > 0.0) && !self.samples.is_empty() {
            let len = self.samples.len();
            if self.pos >= len as f64 {
                if self.looping {
                    // Wrap without replaying the last frame.
                    self.pos %= len as f64;
                } else {
                    self.fade_target = 0.0;
                    self.pos = len as f64;
                }
            }
            if self.pos < len as f64 {
                let index = self.pos.floor() as usize;
                let frac = (self.pos - index as f64) as f32;
                let a = self.samples[index];
                let b = if index + 1 < len {
                    self.samples[index + 1]
                } else if self.looping {
                    self.samples[0]
                } else {
                    a
                };
                value = a + (b - a) * frac;
                self.pos += ratio;
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
            pos: 0.0,
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

/// Hand a buffer that is no longer playing back to the UI for deallocation.
fn retire(slot: &Mutex<Option<Vec<f32>>>, old: Vec<f32>) {
    if old.capacity() == 0 {
        return;
    }
    if let Ok(mut slot) = slot.try_lock() {
        if slot.is_none() {
            *slot = Some(old);
        }
    }
}

pub struct AudioEngine {
    stream: Option<cpal::Stream>,
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: SampleFormat,
    channels: usize,
    rate: u32,
    command: Arc<Mutex<Option<Command>>>,
    playing: Arc<AtomicBool>,
    /// Source-frame cursor published by the callback once per buffer.
    position: Arc<AtomicU64>,
    retired: Arc<Mutex<Option<Vec<f32>>>>,
    device_errors: Arc<AtomicU64>,
    underruns: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    retry_after: Instant,
}

impl AudioEngine {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let mut engine = Self {
            stream: None,
            device,
            config: supported.config(),
            sample_format: supported.sample_format(),
            channels,
            rate,
            command: Arc::new(Mutex::new(None)),
            playing: Arc::new(AtomicBool::new(false)),
            position: Arc::new(AtomicU64::new(0)),
            retired: Arc::new(Mutex::new(None)),
            device_errors: Arc::new(AtomicU64::new(0)),
            underruns: Arc::new(AtomicU64::new(0)),
            alive: Arc::new(AtomicBool::new(false)),
            retry_after: Instant::now(),
        };
        if engine.connect() {
            Some(engine)
        } else {
            None
        }
    }

    fn spawn_stream(&self) -> Option<cpal::Stream> {
        match self.sample_format {
            SampleFormat::F32 => build::<f32>(self),
            SampleFormat::I16 => build::<i16>(self),
            SampleFormat::U16 => build::<u16>(self),
            _ => None,
        }
    }

    fn connect(&mut self) -> bool {
        let Some(stream) = self.spawn_stream() else {
            self.retry_after = Instant::now() + Duration::from_secs(2);
            return false;
        };
        self.stream = Some(stream);
        self.playing.store(false, Ordering::Release);
        self.alive.store(true, Ordering::Release);
        if self
            .stream
            .as_ref()
            .is_none_or(|stream| stream.play().is_err())
        {
            self.alive.store(false, Ordering::Release);
            self.stream = None;
            self.retry_after = Instant::now() + Duration::from_secs(2);
            return false;
        }
        true
    }

    /// Re-open the output stream after a device error. Throttled to 2 s.
    pub fn try_reconnect(&mut self) -> bool {
        if self.alive.load(Ordering::Acquire) {
            return true;
        }
        if Instant::now() < self.retry_after {
            return false;
        }
        self.retry_after = Instant::now() + Duration::from_secs(2);
        self.connect()
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Start playing `samples` (rendered at [`PREVIEW_RATE`]) at `start_frame`.
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
        self.position.load(Ordering::Acquire) as f32 / PREVIEW_RATE as f32
    }

    /// Number of stream errors seen since the last call (device loss etc.).
    pub fn take_device_errors(&self) -> u64 {
        self.device_errors.swap(0, Ordering::AcqRel)
    }

    /// Callback buffers that found the transport running without samples.
    pub fn take_underruns(&self) -> u64 {
        self.underruns.swap(0, Ordering::AcqRel)
    }

    /// Drop a retired sample buffer on the UI thread.
    pub fn reclaim(&self) {
        if let Ok(mut slot) = self.retired.lock() {
            let _ = slot.take();
        }
    }
}

fn build<T>(engine: &AudioEngine) -> Option<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + 'static,
{
    let rate = engine.rate;
    let step = 1.0 / (FADE_SECONDS * rate as f32).max(1.0);
    let ratio = PREVIEW_RATE as f64 / rate.max(1) as f64;
    let channels = engine.channels;
    let command = engine.command.clone();
    let playing = engine.playing.clone();
    let position = engine.position.clone();
    let retired = engine.retired.clone();
    let device_errors = engine.device_errors.clone();
    let underruns = engine.underruns.clone();
    let alive = engine.alive.clone();
    let error_playing = playing.clone();
    let error = move |err| {
        eprintln!("audio stream error: {err}");
        device_errors.fetch_add(1, Ordering::Relaxed);
        error_playing.store(false, Ordering::Release);
        alive.store(false, Ordering::Release);
    };
    let mut playback = Playback::default();
    engine
        .device
        .build_output_stream(
            &engine.config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                // The playback state is owned by this closure; only the command
                // slot is shared and it is read with try_lock.
                if let Some(command) = take_command(&command) {
                    if let Command::Play {
                        samples,
                        start,
                        looping,
                    } = command
                    {
                        let old = std::mem::take(&mut playback.samples);
                        retire(&retired, old);
                        playback.apply(Command::Play {
                            samples,
                            start,
                            looping,
                        });
                    } else {
                        playback.apply(command);
                    }
                }
                let audible = playing.load(Ordering::Acquire);
                if audible && playback.fade > 0.0 && playback.samples.is_empty() {
                    underruns.fetch_add(1, Ordering::Relaxed);
                }
                for frame in data.chunks_mut(channels) {
                    let value = playback.render_frame(step, ratio, audible, &playing);
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
            out.push(playback.render_frame(0.0, 1.0, true, &playing));
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
        assert_eq!(playback.render_frame(0.5, 1.0, true, &playing), 1.0);
        assert_eq!(playback.render_frame(0.5, 1.0, true, &playing), 0.0);
        assert_eq!(playback.render_frame(0.5, 1.0, true, &playing), 0.0);
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
        assert_eq!(playback.render_frame(1.0, 1.0, true, &playing), 1.0);
        assert_eq!(playback.render_frame(0.0, 1.0, true, &playing), 2.0);
        assert_eq!(playback.render_frame(0.0, 1.0, true, &playing), 0.0);
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
        assert_eq!(playback.render_frame(0.5, 1.0, true, &playing), 0.5);
        playback.apply(Command::Stop);
        assert_eq!(playback.render_frame(0.5, 1.0, true, &playing), 0.0);
        assert_eq!(playback.render_frame(0.5, 1.0, true, &playing), 0.0);
    }

    #[test]
    fn command_slot_takes_commands_without_blocking() {
        let slot = Mutex::new(Some(Command::Stop));
        assert!(matches!(take_command(&slot), Some(Command::Stop)));
        assert!(take_command(&slot).is_none());
    }

    #[test]
    fn resamples_48k_frames_to_the_device_ratio() {
        let playing = AtomicBool::new(true);
        let mut playback = Playback {
            samples: vec![0.0, 1.0],
            fade: 1.0,
            fade_target: 1.0,
            ..Playback::default()
        };
        // Device at 48k -> ratio 1.0; at 24k -> ratio 2.0 (two source frames
        // per output frame). Half ratio covers interpolation.
        assert_eq!(playback.render_frame(0.0, 0.5, true, &playing), 0.0);
        assert_eq!(playback.render_frame(0.0, 0.5, true, &playing), 0.5);
        assert_eq!(playback.render_frame(0.0, 0.5, true, &playing), 1.0);
    }

    #[test]
    fn retired_buffers_are_handed_back_to_the_ui() {
        let slot = Mutex::new(None);
        retire(&slot, vec![0.0; 16]);
        assert!(slot.lock().unwrap().take().is_some());
        retire(&slot, Vec::new());
        assert!(slot.lock().unwrap().is_none());
    }
}
