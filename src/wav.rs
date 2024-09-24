#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    pub audio_format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub data_size: u32,
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

    let mut fmt: Option<(u16, u16, u32)> = None;
    let mut data_size: Option<u32> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = read_u32(bytes, offset + 4).ok_or(WavError::Truncated)? as usize;
        let body_start = offset + 8;
        if id == b"data" {
            let available = bytes.len() - body_start;
            data_size = Some(size.min(available) as u32);
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
            fmt = Some((audio_format, channels, sample_rate));
        }
        offset = body_end + (size % 2);
    }

    let (audio_format, channels, sample_rate) = fmt.ok_or(WavError::MissingFmtChunk)?;
    if audio_format != 1 && audio_format != 0xfffe {
        return Err(WavError::UnsupportedAudioFormat);
    }
    if channels == 0 {
        return Err(WavError::ZeroChannels);
    }
    if sample_rate == 0 {
        return Err(WavError::ZeroSampleRate);
    }
    let data_size = data_size.ok_or(WavError::MissingDataChunk)?;
    if data_size == 0 {
        return Err(WavError::ZeroDataSize);
    }

    Ok(WavInfo {
        audio_format,
        channels,
        sample_rate,
        data_size,
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
}
