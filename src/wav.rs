#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    pub audio_format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub data_size: u32,
    pub data_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavError {
    Truncated,
    BadMagic,
    MissingFmtChunk,
    UnsupportedAudioFormat,
    ZeroChannels,
    ZeroSampleRate,
    MissingDataChunk,
    ZeroDataSize,
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn validate(bytes: &[u8]) -> Result<WavInfo, WavError> {
    if bytes.len() < 12 {
        return Err(WavError::Truncated);
    }
    if &bytes[0..4] != b"RIFF" {
        return Err(WavError::BadMagic);
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WavError::BadMagic);
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<(u32, usize)> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = read_u32(bytes, offset + 4).ok_or(WavError::Truncated)? as usize;
        let body_start = offset + 8;
        if id == b"data" {
            let available = bytes.len() - body_start;
            data = Some((size.min(available) as u32, body_start));
            break;
        }
        let body_end = body_start.checked_add(size).ok_or(WavError::Truncated)?;
        if body_end > bytes.len() {
            return Err(WavError::Truncated);
        }
        if id == b"fmt " {
            let audio_format = read_u16(bytes, body_start).ok_or(WavError::Truncated)?;
            let channels = read_u16(bytes, body_start + 2).ok_or(WavError::Truncated)?;
            let sample_rate = read_u32(bytes, body_start + 4).ok_or(WavError::Truncated)?;
            let bits = read_u16(bytes, body_start + 14).ok_or(WavError::Truncated)?;
            fmt = Some((audio_format, channels, sample_rate, bits));
        }
        offset = body_end + (size % 2);
    }

    let (audio_format, channels, sample_rate, bits_per_sample) =
        fmt.ok_or(WavError::MissingFmtChunk)?;
    if audio_format != 1 && audio_format != 0xfffe {
        return Err(WavError::UnsupportedAudioFormat);
    }
    if channels == 0 {
        return Err(WavError::ZeroChannels);
    }
    if sample_rate == 0 {
        return Err(WavError::ZeroSampleRate);
    }
    let (data_size, data_offset) = data.ok_or(WavError::MissingDataChunk)?;
    if data_size == 0 {
        return Err(WavError::ZeroDataSize);
    }

    Ok(WavInfo {
        audio_format,
        channels,
        sample_rate,
        bits_per_sample,
        data_size,
        data_offset,
    })
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Signal {
    pub frames: u64,
    pub seconds: f64,
    pub peak: i16,
    pub rms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalError {
    Wav(WavError),
    NotS16,
    OddSampleData,
}

impl Signal {
    pub const SILENCE_PEAK: i16 = 64;
    pub const SILENCE_RMS: f64 = 16.0;

    pub fn is_silent(&self) -> bool {
        self.peak < Self::SILENCE_PEAK || self.rms < Self::SILENCE_RMS
    }
}

pub fn measure(bytes: &[u8]) -> Result<Signal, SignalError> {
    let info = validate(bytes).map_err(SignalError::Wav)?;
    if info.bits_per_sample != 16 {
        return Err(SignalError::NotS16);
    }
    let data = &bytes[info.data_offset..info.data_offset + info.data_size as usize];
    if data.len() % 2 != 0 {
        return Err(SignalError::OddSampleData);
    }
    let samples: Vec<i16> = data
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    let peak = samples.iter().map(|s| s.saturating_abs()).max().unwrap_or(0);
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    let rms = (sum / samples.len() as f64).sqrt();
    let frames = samples.len() as u64 / u64::from(info.channels);
    let seconds = frames as f64 / f64::from(info.sample_rate);
    Ok(Signal {
        frames,
        seconds,
        peak,
        rms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_wav(audio_format: u16, channels: u16, sample_rate: u32, data: &[u8]) -> Vec<u8> {
        let mut fmt_chunk = Vec::new();
        fmt_chunk.extend_from_slice(&audio_format.to_le_bytes());
        fmt_chunk.extend_from_slice(&channels.to_le_bytes());
        fmt_chunk.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate.wrapping_mul(channels as u32).wrapping_mul(2);
        fmt_chunk.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align: u16 = channels.wrapping_mul(2);
        fmt_chunk.extend_from_slice(&block_align.to_le_bytes());
        fmt_chunk.extend_from_slice(&16u16.to_le_bytes());

        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt_chunk.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt_chunk);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);

        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(body.len() as u32).to_le_bytes());
        wav.extend_from_slice(&body);
        wav
    }

    fn wav_without_fmt_chunk(data: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(body.len() as u32).to_le_bytes());
        wav.extend_from_slice(&body);
        wav
    }

    fn wav_without_data_chunk(audio_format: u16, channels: u16, sample_rate: u32) -> Vec<u8> {
        let mut fmt_chunk = Vec::new();
        fmt_chunk.extend_from_slice(&audio_format.to_le_bytes());
        fmt_chunk.extend_from_slice(&channels.to_le_bytes());
        fmt_chunk.extend_from_slice(&sample_rate.to_le_bytes());
        fmt_chunk.extend_from_slice(&0u32.to_le_bytes());
        fmt_chunk.extend_from_slice(&0u16.to_le_bytes());
        fmt_chunk.extend_from_slice(&16u16.to_le_bytes());

        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt_chunk.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt_chunk);
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(body.len() as u32).to_le_bytes());
        wav.extend_from_slice(&body);
        wav
    }

    #[test]
    fn accepts_minimal_valid_pcm_wav() {
        let wav = minimal_wav(1, 1, 22050, &[0, 0, 0, 0]);
        let info = validate(&wav).expect("valid wav");
        assert_eq!(info.audio_format, 1);
        assert_eq!(info.channels, 1);
        assert_eq!(info.sample_rate, 22050);
        assert_eq!(info.data_size, 4);
    }

    #[test]
    fn accepts_extensible_audio_format() {
        let wav = minimal_wav(0xfffe, 2, 44100, &[0, 0, 0, 0]);
        let info = validate(&wav).expect("valid wav");
        assert_eq!(info.audio_format, 0xfffe);
    }

    #[test]
    fn rejects_truncated_header() {
        let wav = minimal_wav(1, 1, 22050, &[0, 0]);
        assert_eq!(validate(&wav[..10]), Err(WavError::Truncated));
    }

    #[test]
    fn rejects_truncated_fmt_chunk() {
        let wav = minimal_wav(1, 1, 22050, &[0, 0, 0, 0]);
        assert_eq!(validate(&wav[..30]), Err(WavError::Truncated));
    }

    #[test]
    fn clamps_oversized_streaming_data_size_to_available_bytes() {
        let mut wav = minimal_wav(1, 1, 22050, &[0, 0, 0, 0]);
        let len = wav.len();
        wav[len - 8..len - 4].copy_from_slice(&0x7ffff000u32.to_le_bytes());
        let info = validate(&wav).expect("valid wav despite placeholder size");
        assert_eq!(info.data_size, 4);
    }

    #[test]
    fn rejects_bad_riff_magic() {
        let mut wav = minimal_wav(1, 1, 22050, &[0, 0]);
        wav[0..4].copy_from_slice(b"JUNK");
        assert_eq!(validate(&wav), Err(WavError::BadMagic));
    }

    #[test]
    fn rejects_bad_wave_magic() {
        let mut wav = minimal_wav(1, 1, 22050, &[0, 0]);
        wav[8..12].copy_from_slice(b"JUNK");
        assert_eq!(validate(&wav), Err(WavError::BadMagic));
    }

    #[test]
    fn rejects_missing_fmt_chunk() {
        let wav = wav_without_fmt_chunk(&[0, 0]);
        assert_eq!(validate(&wav), Err(WavError::MissingFmtChunk));
    }

    #[test]
    fn rejects_unsupported_audio_format() {
        let wav = minimal_wav(3, 1, 22050, &[0, 0]);
        assert_eq!(validate(&wav), Err(WavError::UnsupportedAudioFormat));
    }

    #[test]
    fn rejects_zero_channels() {
        let wav = minimal_wav(1, 0, 22050, &[0, 0]);
        assert_eq!(validate(&wav), Err(WavError::ZeroChannels));
    }

    #[test]
    fn rejects_zero_sample_rate() {
        let wav = minimal_wav(1, 1, 0, &[0, 0]);
        assert_eq!(validate(&wav), Err(WavError::ZeroSampleRate));
    }

    #[test]
    fn rejects_missing_data_chunk() {
        let wav = wav_without_data_chunk(1, 1, 22050);
        assert_eq!(validate(&wav), Err(WavError::MissingDataChunk));
    }

    #[test]
    fn rejects_zero_data_size() {
        let wav = minimal_wav(1, 1, 22050, &[]);
        assert_eq!(validate(&wav), Err(WavError::ZeroDataSize));
    }

    fn s16(samples: &[i16]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&22050u32.to_le_bytes());
        v.extend_from_slice(&44100u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    fn tone(n: usize, amp: i16) -> Vec<i16> {
        (0..n)
            .map(|i| if i % 2 == 0 { amp } else { -amp })
            .collect()
    }

    #[test]
    fn measures_duration_peak_and_rms_of_speech_like_audio() {
        let bytes = s16(&tone(22050, 8000));
        let m = measure(&bytes).expect("measured");
        assert_eq!(m.frames, 22050);
        assert!((m.seconds - 1.0).abs() < 0.01, "{}", m.seconds);
        assert_eq!(m.peak, 8000);
        assert!((m.rms - 8000.0).abs() < 1.0, "{}", m.rms);
        assert!(!m.is_silent());
    }

    #[test]
    fn flags_all_zero_audio_as_silent() {
        let m = measure(&s16(&vec![0i16; 22050])).expect("measured");
        assert_eq!(m.peak, 0);
        assert_eq!(m.rms, 0.0);
        assert!(m.is_silent());
    }

    #[test]
    fn flags_near_silence_below_the_floor_as_silent() {
        let m = measure(&s16(&tone(22050, 2))).expect("measured");
        assert!(m.peak > 0);
        assert!(m.is_silent(), "peak {} rms {}", m.peak, m.rms);
    }

    #[test]
    fn rejects_audio_that_is_not_valid_wav() {
        assert!(measure(b"not a wav at all").is_err());
    }

    #[test]
    fn rejects_non_s16_payload() {
        let mut bytes = s16(&tone(64, 1000));
        bytes[34] = 8;
        assert_eq!(measure(&bytes), Err(SignalError::NotS16));
    }

    #[test]
    fn rejects_odd_length_sample_data() {
        let mut bytes = s16(&tone(64, 1000));
        bytes.pop();
        assert!(measure(&bytes).is_err());
    }
}
