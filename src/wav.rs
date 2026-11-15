//! Minimal RIFF/WAVE reader used by `Wave::Sample` layers.
//!
//! Supports PCM 16-bit and IEEE float 32-bit, mono or stereo. Other formats are
//! rejected with a readable error instead of producing silence.

use std::path::Path;

/// Hard cap for one decoded sample: 60 seconds at 48 kHz mono (~11.5 MB).
/// This keeps undo snapshots, previews and preset loads bounded.
pub const MAX_FRAMES: usize = 48_000 * 60;
/// Refuse absurd source files before `fs::read` allocates the whole buffer.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Decoded mono sample data at its native rate.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleData {
    pub rate: u32,
    /// Mono, normalized to roughly -1..1.
    pub samples: Vec<f32>,
}

pub fn load(path: &Path) -> Result<SampleData, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{}: sample file is too large ({} bytes; limit {MAX_FILE_BYTES})",
            path.display(),
            metadata.len()
        ));
    }
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
    let frame_bytes = ((bits as usize).div_ceil(8)).saturating_mul(channels);
    check_frame_limit(data.len() / frame_bytes.max(1))?;
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

fn check_frame_limit(frames: usize) -> Result<(), String> {
    if frames > MAX_FRAMES {
        return Err(format!(
            "sample is too long ({} frames; limit {MAX_FRAMES})",
            frames
        ));
    }
    Ok(())
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

    #[test]
    fn rejects_samples_over_the_frame_limit() {
        assert!(check_frame_limit(MAX_FRAMES).is_ok());
        assert!(check_frame_limit(MAX_FRAMES + 1).is_err());
    }

    #[test]
    fn rejects_oversized_files_before_reading() {
        let root = std::env::temp_dir().join(format!(
            "sfx-studio-wav-limit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("huge.wav");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
