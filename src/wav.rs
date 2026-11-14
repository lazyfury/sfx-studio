//! Minimal RIFF/WAVE reader used by `Wave::Sample` layers.
//!
//! Supports PCM 16-bit and IEEE float 32-bit, mono or stereo. Other formats are
//! rejected with a readable error instead of producing silence.

use std::path::Path;

/// Decoded mono sample data at its native rate.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleData {
    pub rate: u32,
    /// Mono, normalized to roughly -1..1.
    pub samples: Vec<f32>,
}

pub fn load(path: &Path) -> Result<SampleData, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    decode(&bytes)
}

pub fn decode(bytes: &[u8]) -> Result<SampleData, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let mut offset = 12usize;
    let mut format = None;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| "bad chunk size")?,
        ) as usize;
        let start = offset + 8;
        let end = start.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " => format = Some(parse_format(&bytes[start..end])?),
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        offset = start + size + (size & 1);
    }
    let (audio_format, channels, rate, bits) = format.ok_or("missing fmt chunk")?;
    let data = data.ok_or("missing data chunk")?;
    let mono = match (audio_format, bits) {
        (1, 16) => decode_i16(data, channels),
        (3, 32) => decode_f32(data, channels),
        _ => {
            return Err(format!(
                "unsupported WAV format {audio_format} / {bits} bit (need PCM16 or F32)"
            ))
        }
    };
    if mono.is_empty() {
        return Err("empty sample data".into());
    }
    Ok(SampleData {
        rate,
        samples: mono,
    })
}

fn parse_format(chunk: &[u8]) -> Result<(u16, usize, u32, u16), String> {
    if chunk.len() < 16 {
        return Err("short fmt chunk".into());
    }
    let audio_format = u16::from_le_bytes([chunk[0], chunk[1]]);
    let channels = u16::from_le_bytes([chunk[2], chunk[3]]) as usize;
    let rate = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
    let bits = u16::from_le_bytes([chunk[14], chunk[15]]);
    if channels == 0 || channels > 2 {
        return Err(format!("unsupported channel count {channels}"));
    }
    if rate == 0 {
        return Err("sample rate is 0".into());
    }
    Ok((audio_format, channels, rate, bits))
}

fn decode_i16(data: &[u8], channels: usize) -> Vec<f32> {
    let frames = data.len() / (2 * channels);
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut sum = 0.0f32;
        for channel in 0..channels {
            let at = (frame * channels + channel) * 2;
            let value = i16::from_le_bytes([data[at], data[at + 1]]);
            sum += value as f32 / 32767.0;
        }
        out.push((sum / channels as f32).clamp(-1.0, 1.0));
    }
    out
}

fn decode_f32(data: &[u8], channels: usize) -> Vec<f32> {
    let frames = data.len() / (4 * channels);
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut sum = 0.0f32;
        for channel in 0..channels {
            let at = (frame * channels + channel) * 4;
            let value = f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
            sum += value;
        }
        out.push((sum / channels as f32).clamp(-1.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_the_wav_writer_output() {
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-wav-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("test.wav");
        let samples = vec![0.0f32, 0.5, -0.5, 1.0];
        crate::synth::write_wav(&path, &samples, 48_000).unwrap();
        let decoded = load(&path).unwrap();
        assert_eq!(decoded.rate, 48_000);
        assert_eq!(decoded.samples.len(), samples.len());
        for (a, b) in decoded.samples.iter().zip(samples.iter()) {
            assert!((a - b).abs() < 0.001, "{a} vs {b}");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_non_wav_bytes() {
        assert!(decode(b"not a wave file").is_err());
    }
}
