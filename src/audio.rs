//! Minimal cpal output. The callback reads from a shared mono buffer; the UI
//! plays rendered patches/music and polls the playhead.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

#[derive(Default)]
struct Playback {
    samples: Vec<f32>,
    pos: usize,
    playing: bool,
    looping: bool,
}

pub struct AudioEngine {
    shared: Arc<Mutex<Playback>>,
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
        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                build::<f32>(&device, &supported.config(), channels, shared.clone())
            }
            SampleFormat::I16 => {
                build::<i16>(&device, &supported.config(), channels, shared.clone())
            }
            SampleFormat::U16 => {
                build::<u16>(&device, &supported.config(), channels, shared.clone())
            }
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self {
            shared,
            rate,
            _stream: stream,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    pub fn play(&self, samples: Vec<f32>, looping: bool) {
        if let Ok(mut playback) = self.shared.lock() {
            playback.samples = samples;
            playback.pos = 0;
            playback.playing = true;
            playback.looping = looping;
        }
    }

    pub fn stop(&self) {
        if let Ok(mut playback) = self.shared.lock() {
            playback.playing = false;
            playback.pos = 0;
        }
    }

    pub fn is_playing(&self) -> bool {
        self.shared.lock().map(|p| p.playing).unwrap_or(false)
    }

    pub fn position(&self) -> f32 {
        self.shared
            .lock()
            .map(|p| p.pos as f32 / self.rate as f32)
            .unwrap_or(0.0)
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    shared: Arc<Mutex<Playback>>,
) -> Option<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + 'static,
{
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
                for frame in data.chunks_mut(channels) {
                    let value = if !playback.playing {
                        0.0
                    } else if playback.pos >= playback.samples.len() {
                        if playback.looping && !playback.samples.is_empty() {
                            playback.pos = 0;
                            playback.samples[0]
                        } else {
                            playback.playing = false;
                            0.0
                        }
                    } else {
                        let value = playback.samples[playback.pos];
                        playback.pos += 1;
                        value
                    };
                    for sample in frame {
                        *sample = T::from_sample(value);
                    }
                }
            },
            error,
            None,
        )
        .ok()
}
