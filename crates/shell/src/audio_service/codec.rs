//! Bounded, device-free codec and resampling helpers. These run on a worker.
use makepad_audio_decode::{decode_audio_limited, sniff, DecodedAudio, Limits};

pub const RATE: usize = 16_000;
pub const MAX_RECORD_MS: u64 = 30_000;
pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_PLAY_FRAMES: usize = 48_000 * 60;

pub fn decode(bytes: &[u8]) -> Result<DecodedAudio, String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err("resource_limit: Audio input must contain at most 1 MiB".into());
    }
    let pcm = if bytes.starts_with(b"RIFF") {
        decode_wav(bytes)?
    } else {
        let format =
            sniff(bytes).ok_or("unsupported_format: Use PCM WAV, MP3, FLAC or Ogg Vorbis")?;
        decode_audio_limited(
            bytes,
            format,
            Limits {
                max_frames: MAX_PLAY_FRAMES,
                max_channels: 2,
            },
        )
        .map_err(|_| "invalid_audio: Cannot decode the bounded audio input")?
    };
    if !(8_000..=192_000).contains(&pcm.rate)
        || !(1..=2).contains(&pcm.channels)
        || pcm.frames() == 0
        || pcm.frames() > MAX_PLAY_FRAMES
        || pcm.duration_secs() > 60.0
        || pcm
            .pcm_interleaved_f32
            .iter()
            .any(|sample| !sample.is_finite())
    {
        return Err("resource_limit: Playback requires finite mono/stereo audio, at most 60 seconds and 2,880,000 frames".into());
    }
    Ok(pcm)
}

fn decode_wav(bytes: &[u8]) -> Result<DecodedAudio, String> {
    let invalid = || "invalid_audio: Invalid PCM WAV".to_string();
    if bytes.len() < 44 || &bytes[8..12] != b"WAVE" {
        return Err(invalid());
    }
    let declared = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if declared.checked_add(8) != Some(bytes.len()) {
        return Err(invalid());
    }
    let mut format = None;
    let mut data = None;
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(invalid)?;
        match &bytes[at..at + 4] {
            b"fmt " if size >= 16 => {
                let encoding = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
                let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
                let rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
                let block = u16::from_le_bytes(bytes[start + 12..start + 14].try_into().unwrap());
                let bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
                if !matches!((encoding, bits), (1, 16) | (3, 32))
                    || !(1..=2).contains(&channels)
                    || block != channels * (bits / 8)
                    || format.is_some()
                {
                    return Err(invalid());
                }
                format = Some((encoding, channels, rate, bits));
            }
            b"data" => {
                if data.is_some() {
                    return Err(invalid());
                }
                data = Some(&bytes[start..end]);
            }
            _ => {}
        }
        at = end.checked_add(size & 1).ok_or_else(invalid)?;
    }
    let (encoding, channels, rate, bits) = format.ok_or_else(invalid)?;
    let data = data.ok_or_else(invalid)?;
    let width = (bits / 8) as usize;
    if data.len() % (width * channels as usize) != 0 {
        return Err(invalid());
    }
    let samples = data
        .chunks_exact(width)
        .map(|chunk| {
            if encoding == 1 {
                i16::from_le_bytes(chunk.try_into().unwrap()) as f32 / 32768.0
            } else {
                f32::from_le_bytes(chunk.try_into().unwrap())
            }
        })
        .collect();
    Ok(DecodedAudio {
        rate,
        channels,
        pcm_interleaved_f32: samples,
    })
}

pub fn wav(samples: &[i16]) -> Result<Vec<u8>, String> {
    let data = samples
        .len()
        .checked_mul(2)
        .ok_or("resource_limit: Recording is too large")?;
    if samples.is_empty()
        || data + 44 > MAX_BYTES
        || samples.len() > RATE * MAX_RECORD_MS as usize / 1000
    {
        return Err("resource_limit: Recording is empty or exceeds 30 seconds".into());
    }
    let mut bytes = Vec::with_capacity(data + 44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(data as u32 + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(RATE as u32).to_le_bytes());
    bytes.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data as u32).to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(bytes)
}

/// Streaming area resampling to mono 16 kHz, bounded before every append.
#[derive(Default)]
pub struct Recorder {
    rate: u32,
    phase: u64,
    sum: f64,
    count: u64,
    samples: Vec<i16>,
}
impl Recorder {
    pub fn push(&mut self, rate: u32, input: &[f32], limit: usize) -> Result<bool, String> {
        if !(16_000..=192_000).contains(&rate) || (self.rate != 0 && self.rate != rate) {
            return Err("device_changed: Recording sample rate changed or is unsupported".into());
        }
        self.rate = rate;
        for &value in input {
            if !value.is_finite() {
                return Err("invalid_audio: Device returned non-finite samples".into());
            }
            self.sum += value.clamp(-1.0, 1.0) as f64;
            self.count += 1;
            self.phase += RATE as u64;
            if self.phase >= rate as u64 {
                if self.samples.len() >= limit {
                    return Ok(true);
                }
                self.samples
                    .push((self.sum / self.count as f64 * 32767.0).round() as i16);
                self.phase -= rate as u64;
                self.sum = 0.0;
                self.count = 0;
            }
        }
        Ok(self.samples.len() >= limit)
    }
    pub fn finish(self) -> Result<Vec<u8>, String> {
        wav(&self.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_capture_roundtrips_and_obeys_exact_budget() {
        let mut recorder = Recorder::default();
        assert!(!recorder.push(48_000, &[0.25; 300], RATE).unwrap());
        let bytes = recorder.finish().unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.frames(), 100);
        assert_eq!(decoded.rate, 16_000);
        let maximum = wav(&vec![0; RATE * 30]).unwrap();
        assert_eq!(maximum.len(), 960044);
        assert!(wav(&vec![0; RATE * 30 + 1]).is_err());
    }
    #[test]
    fn malformed_oversized_and_changed_rate_inputs_are_refused() {
        for bytes in [vec![], vec![0; MAX_BYTES + 1], b"RIFFbad".to_vec()] {
            assert!(decode(&bytes).is_err());
        }
        let mut recorder = Recorder::default();
        recorder.push(48_000, &[0.0; 3], 100).unwrap();
        assert!(recorder.push(44_100, &[0.0], 100).is_err());
        assert!(Recorder::default().push(48_000, &[f32::NAN], 100).is_err());
        let mut wav = wav(&[1, 2]).unwrap();
        wav[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&wav).is_err());
    }
}
