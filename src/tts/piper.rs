use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Language, Pitch, SpeakRequest, Speech, Timeout, Tts, TtsError};
use crate::backend::PiperPaths;

pub struct Piper {
    bin: PathBuf,
    voices: PathBuf,
    out_dir: PathBuf,
    timeout: Timeout,
    counter: AtomicU64,
}

impl Piper {
    pub fn new(
        bin: impl Into<PathBuf>,
        voices: impl Into<PathBuf>,
        out_dir: impl Into<PathBuf>,
        timeout: Timeout,
    ) -> Self {
        let out_dir = out_dir.into();
        std::fs::create_dir_all(&out_dir).expect("failed to create output directory");
        Self {
            bin: bin.into(),
            voices: voices.into(),
            out_dir,
            timeout,
            counter: AtomicU64::new(0),
        }
    }
}

fn json_sibling(voice: &Path) -> PathBuf {
    let mut os = voice.as_os_str().to_owned();
    os.push(".json");
    PathBuf::from(os)
}

fn voice_for(voices: &Path, lang: Language) -> Option<PathBuf> {
    let prefix = format!("{}_", lang.code());
    let mut hits: Vec<PathBuf> = std::fs::read_dir(voices)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with(&prefix) && n.ends_with(".onnx")
                })
                .unwrap_or(false)
                && json_sibling(p).exists()
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

fn length_scale(rate: Option<super::Rate>, pitch: Option<Pitch>) -> Option<String> {
    if rate.is_none() && pitch.is_none() {
        return None;
    }
    let base = rate.map_or(1.0, |r| 170.0 / f64::from(r.value()));
    let shift = pitch.map_or(1.0, Pitch::factor);
    Some(format!("{:.3}", base * shift))
}

pub fn resample(samples: &[i16], factor: f64) -> Vec<i16> {
    if samples.is_empty() || (factor - 1.0).abs() < f64::EPSILON {
        return samples.to_vec();
    }
    let last = samples.len() - 1;
    let out_len = (samples.len() as f64 / factor).round() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * factor;
            let idx = pos.floor() as usize;
            let frac = pos - idx as f64;
            let a = f64::from(samples[idx.min(last)]);
            let b = f64::from(samples[(idx + 1).min(last)]);
            (a + (b - a) * frac).round() as i16
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewrapError {
    Truncated,
    BadMagic,
    MissingFmtChunk,
    UnsupportedFormat,
    MissingDataChunk,
    EmptyData,
}

pub fn rewrap_f32(bytes: &[u8], pitch: Option<Pitch>) -> Result<Vec<u8>, RewrapError> {
    if bytes.len() < 12 {
        return Err(RewrapError::Truncated);
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(RewrapError::BadMagic);
    }
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let body_start = offset + 8;
        if id == b"data" {
            let available = bytes.len() - body_start;
            data = Some(&bytes[body_start..body_start + size.min(available)]);
            break;
        }
        let body_end = body_start.checked_add(size).ok_or(RewrapError::Truncated)?;
        if body_end > bytes.len() {
            return Err(RewrapError::Truncated);
        }
        if id == b"fmt " {
            if size < 16 {
                return Err(RewrapError::Truncated);
            }
            let format = u16::from_le_bytes([bytes[body_start], bytes[body_start + 1]]);
            let channels = u16::from_le_bytes([bytes[body_start + 2], bytes[body_start + 3]]);
            let sample_rate = u32::from_le_bytes([
                bytes[body_start + 4],
                bytes[body_start + 5],
                bytes[body_start + 6],
                bytes[body_start + 7],
            ]);
            let bits = u16::from_le_bytes([bytes[body_start + 14], bytes[body_start + 15]]);
            fmt = Some((format, channels, sample_rate, bits));
        }
        offset = body_end + (size % 2);
    }
    let (format, channels, sample_rate, bits) = fmt.ok_or(RewrapError::MissingFmtChunk)?;
    if format != 3 || bits != 32 || channels == 0 || sample_rate == 0 {
        return Err(RewrapError::UnsupportedFormat);
    }
    let data = data.ok_or(RewrapError::MissingDataChunk)?;
    let decoded: Vec<i16> = data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .map(|f| (f.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();
    let samples = match pitch {
        Some(p) => resample(&decoded, p.factor()),
        None => decoded,
    };
    if samples.is_empty() {
        return Err(RewrapError::EmptyData);
    }
    let data_size = (samples.len() * 2) as u32;
    let block_align = channels * 2;
    let byte_rate = sample_rate * u32::from(block_align);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    Ok(out)
}

impl Tts for Piper {
    fn speak(&self, req: &SpeakRequest) -> Result<Speech, TtsError> {
        let text = req.text.trim();
        if text.is_empty() {
            return Err(TtsError {
                reason: "text must not be empty".to_string(),
            });
        }
        let voice = voice_for(&self.voices, req.lang).ok_or_else(|| TtsError {
            reason: format!("no vits voice for language {}", req.lang.code()),
        })?;
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let raw = self
            .out_dir
            .join(format!("vits-{n}-{:04}.f32.wav", std::process::id() % 10000));
        let path = self
            .out_dir
            .join(format!("speech-{n}-{:04}.wav", std::process::id() % 10000));
        let mut cmd = Command::new(&self.bin);
        cmd.arg("-m").arg(&voice).arg("-f").arg(&raw);
        if let Some(scale) = length_scale(req.rate, req.pitch) {
            cmd.args(["--length_scale", &scale]);
        }
        let output =
            super::spawn_feed_with_retry(&mut cmd, text.as_bytes(), self.timeout).map_err(|e| TtsError {
                reason: format!("vits spawn failed: {e}"),
            })?;
        if !output.status.success() {
            return Err(TtsError {
                reason: format!(
                    "vits engine exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        let raw_bytes = std::fs::read(&raw).map_err(|e| TtsError {
            reason: format!("vits output missing: {e}"),
        })?;
        std::fs::remove_file(&raw).ok();
        let rewrapped = rewrap_f32(&raw_bytes, req.pitch).map_err(|e| TtsError {
            reason: format!("vits output invalid: {e:?}"),
        })?;
        std::fs::write(&path, &rewrapped).map_err(|e| TtsError {
            reason: format!("vits output not writable: {e}"),
        })?;
        Ok(Speech { path })
    }
}

pub fn scan_piper(root: &Path, bin_override: Option<&Path>) -> Option<PiperPaths> {
    let bin = match bin_override {
        Some(p) if p.exists() => p.to_path_buf(),
        _ => root.join("piper").join("bin").join("piper"),
    };
    if !bin.exists() {
        return None;
    }
    let voices = bin.parent()?.parent()?.join("voices");
    let found = std::fs::read_dir(&voices)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .any(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(".onnx"))
                .unwrap_or(false)
                && json_sibling(&p).exists()
        });
    if found {
        Some(PiperPaths { bin, voices })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::Rate;
    use crate::wav;

    fn f32_wav(samples: &[f32]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0x7fff_f024u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&3u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&22050u32.to_le_bytes());
        v.extend_from_slice(&88200u32.to_le_bytes());
        v.extend_from_slice(&4u16.to_le_bytes());
        v.extend_from_slice(&32u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&0x7fff_f000u32.to_le_bytes());
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    struct Tree {
        dir: PathBuf,
    }

    impl Tree {
        fn new(name: &str, voice_names: &[&str]) -> Self {
            let dir = std::env::temp_dir().join(format!("voz-piper-{name}-{}", std::process::id()));
            let voices = dir.join("voices");
            std::fs::create_dir_all(&voices).expect("mkdir voices");
            for v in voice_names {
                std::fs::write(voices.join(format!("{v}.onnx")), b"onnx").expect("write onnx");
                std::fs::write(voices.join(format!("{v}.onnx.json")), b"{}").expect("write json");
            }
            Self { dir }
        }

        fn with_stub(self, body: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let bin_dir = self.dir.join("bin");
            std::fs::create_dir_all(&bin_dir).expect("mkdir bin");
            let script = bin_dir.join("piper");
            std::fs::write(&script, body).expect("write script");
            let mut perms = std::fs::metadata(&script).expect("meta").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).expect("chmod");
            self
        }

        fn emitting_fixture(self) -> Self {
            std::fs::write(
                self.dir.join("fixture.wav"),
                f32_wav(&[0.0, 0.5, -0.5, 1.5, -1.5]),
            )
            .expect("write fixture");
            let body = format!(
                "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-f\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\necho \"$@\" > {d}/args.txt\ncat > {d}/stdin.txt\ncp {d}/fixture.wav \"$out\"\n",
                d = self.dir.display()
            );
            self.with_stub(&body)
        }

        fn bin(&self) -> PathBuf {
            self.dir.join("bin").join("piper")
        }

        fn voices(&self) -> PathBuf {
            self.dir.join("voices")
        }

        fn tts(&self) -> Piper {
            Piper::new(self.bin(), self.voices(), self.dir.join("out"), Timeout::default_timeout())
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn request(text: &str, lang: Language) -> SpeakRequest {
        SpeakRequest {
            text: text.to_string(),
            lang,
            rate: None,
            pitch: None,
        }
    }

    #[test]
    fn rejects_empty_text() {
        let tree = Tree::new("empty", &["en_US-a-medium"]).emitting_fixture();
        assert!(tree.tts().speak(&request("  ", Language::English)).is_err());
    }

    #[test]
    fn happy_path_rewraps_to_valid_s16_wav_and_feeds_stdin() {
        let tree = Tree::new("happy", &["en_US-a-medium"]).emitting_fixture();
        let speech = tree
            .tts()
            .speak(&request("hello there", Language::English))
            .expect("speak ok");
        let bytes = std::fs::read(&speech.path).expect("read");
        let info = wav::validate(&bytes).expect("valid wav");
        assert_eq!(info.audio_format, 1);
        assert_eq!(info.sample_rate, 22050);
        assert_eq!(info.data_size, 10);
        let stdin = std::fs::read_to_string(tree.dir.join("stdin.txt")).expect("stdin");
        assert_eq!(stdin, "hello there");
        let args = std::fs::read_to_string(tree.dir.join("args.txt")).expect("args");
        assert!(args.contains("en_US-a-medium.onnx"), "args were: {args}");
        assert!(!args.contains("--length_scale"), "args were: {args}");
        let leftovers: Vec<_> = std::fs::read_dir(tree.dir.join("out"))
            .expect("out dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".f32.wav"))
            .collect();
        assert!(leftovers.is_empty(), "raw files left: {leftovers:?}");
    }

    #[test]
    fn rate_maps_to_length_scale() {
        let tree = Tree::new("rate", &["en_US-a-medium"]).emitting_fixture();
        let mut req = request("hi", Language::English);
        req.rate = Some(Rate::parse(340).unwrap());
        tree.tts().speak(&req).expect("speak ok");
        let args = std::fs::read_to_string(tree.dir.join("args.txt")).expect("args");
        assert!(args.contains("--length_scale 0.500"), "args were: {args}");
    }

    #[test]
    fn picks_first_voice_by_language_prefix() {
        let tree = Tree::new(
            "pick",
            &["ru_RU-b-medium", "en_US-b-medium", "en_US-a-medium"],
        )
        .emitting_fixture();
        tree.tts()
            .speak(&request("hello", Language::English))
            .expect("speak ok");
        let args = std::fs::read_to_string(tree.dir.join("args.txt")).expect("args");
        assert!(args.contains("en_US-a-medium.onnx"), "args were: {args}");
        assert!(!args.contains("ru_RU"), "args were: {args}");
    }

    #[test]
    fn missing_voice_for_language_surfaces_error() {
        let tree = Tree::new("novoice", &["en_US-a-medium"]).emitting_fixture();
        let err = tree
            .tts()
            .speak(&request("hello", Language::Japanese))
            .unwrap_err();
        assert!(err.reason.contains("no vits voice for language ja"));
    }

    #[test]
    fn voice_without_json_sibling_is_ignored() {
        let tree = Tree::new("nojson", &["en_US-a-medium"]).emitting_fixture();
        std::fs::remove_file(tree.voices().join("en_US-a-medium.onnx.json")).expect("rm json");
        let err = tree
            .tts()
            .speak(&request("hello", Language::English))
            .unwrap_err();
        assert!(err.reason.contains("no vits voice for language en"));
    }

    #[test]
    fn spawn_failure_surfaces_error() {
        let tree = Tree::new("spawn", &["en_US-a-medium"]);
        let err = tree
            .tts()
            .speak(&request("hello", Language::English))
            .unwrap_err();
        assert!(err.reason.contains("vits spawn failed"));
    }

    #[test]
    fn nonzero_exit_surfaces_stderr() {
        let tree =
            Tree::new("fail", &["en_US-a-medium"]).with_stub("#!/bin/sh\necho boom 1>&2\nexit 1\n");
        let err = tree
            .tts()
            .speak(&request("hello", Language::English))
            .unwrap_err();
        assert!(err.reason.contains("vits engine exited with"));
        assert!(err.reason.contains("boom"));
    }

    #[test]
    fn missing_output_file_surfaces_error() {
        let tree = Tree::new("noout", &["en_US-a-medium"])
            .with_stub("#!/bin/sh\ncat > /dev/null\nexit 0\n");
        let err = tree
            .tts()
            .speak(&request("hello", Language::English))
            .unwrap_err();
        assert!(err.reason.contains("vits output missing"));
    }

    #[test]
    fn invalid_output_file_surfaces_error() {
        let tree = Tree::new("badout", &["en_US-a-medium"]).with_stub(
            "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-f\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\ncat > /dev/null\nprintf 'not audio' > \"$out\"\n",
        );
        let err = tree
            .tts()
            .speak(&request("hello", Language::English))
            .unwrap_err();
        assert!(err.reason.contains("vits output invalid"));
    }

    #[test]
    fn blocked_final_path_surfaces_error() {
        let tree = Tree::new("blocked", &["en_US-a-medium"]).emitting_fixture();
        let blocker = tree.dir.join("out").join(format!(
            "speech-0-{:04}.wav",
            std::process::id() % 10000
        ));
        std::fs::create_dir_all(&blocker).expect("mkdir blocker");
        let err = tree
            .tts()
            .speak(&request("hello", Language::English))
            .unwrap_err();
        assert!(err.reason.contains("vits output not writable"));
    }

    #[test]
    fn rewrap_clamps_and_scales_samples() {
        let out = rewrap_f32(&f32_wav(&[0.0, 1.0, -1.0, 2.0, -2.0]), None).expect("rewrap");
        let info = wav::validate(&out).expect("valid");
        assert_eq!(info.audio_format, 1);
        assert_eq!(info.channels, 1);
        assert_eq!(info.sample_rate, 22050);
        let s: Vec<i16> = out[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(s, vec![0, 32767, -32767, 32767, -32767]);
    }

    #[test]
    fn rewrap_rejects_truncated_input() {
        assert_eq!(rewrap_f32(b"RIFF", None), Err(RewrapError::Truncated));
        let mut wav = f32_wav(&[0.0]);
        wav[16] = 200;
        assert_eq!(rewrap_f32(&wav, None), Err(RewrapError::Truncated));
    }

    #[test]
    fn rewrap_rejects_bad_magic() {
        assert_eq!(
            rewrap_f32(b"RIFXaaaaWAVEbbbb", None),
            Err(RewrapError::BadMagic)
        );
        assert_eq!(
            rewrap_f32(b"RIFFaaaaWAVXbbbb", None),
            Err(RewrapError::BadMagic)
        );
    }

    #[test]
    fn rewrap_rejects_missing_fmt_chunk() {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&36u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"data");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(rewrap_f32(&v, None), Err(RewrapError::MissingFmtChunk));
    }

    #[test]
    fn rewrap_rejects_undersized_fmt_chunk() {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&20u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&8u32.to_le_bytes());
        v.extend_from_slice(&[0; 8]);
        assert_eq!(rewrap_f32(&v, None), Err(RewrapError::Truncated));
    }

    #[test]
    fn rewrap_rejects_non_float_format() {
        let mut wav = f32_wav(&[0.0]);
        wav[20] = 1;
        assert_eq!(rewrap_f32(&wav, None), Err(RewrapError::UnsupportedFormat));
    }

    #[test]
    fn rewrap_rejects_missing_data_chunk() {
        let wav = f32_wav(&[]);
        let truncated = &wav[..36];
        assert_eq!(rewrap_f32(truncated, None), Err(RewrapError::MissingDataChunk));
    }

    #[test]
    fn rewrap_rejects_empty_data() {
        assert_eq!(rewrap_f32(&f32_wav(&[]), None), Err(RewrapError::EmptyData));
    }

    #[test]
    fn scan_piper_finds_default_layout_under_root() {
        let tree = Tree::new("scanroot", &["en_US-a-medium"]).emitting_fixture();
        let root = tree.dir.join("root");
        let bin_dir = root.join("piper").join("bin");
        std::fs::create_dir_all(&bin_dir).expect("mkdir");
        std::fs::rename(tree.bin(), bin_dir.join("piper")).expect("mv bin");
        let voices_dst = root.join("piper").join("voices");
        std::fs::rename(tree.voices(), &voices_dst).expect("mv voices");
        let hit = scan_piper(&root, None).expect("found");
        assert_eq!(hit.bin, bin_dir.join("piper"));
        assert_eq!(hit.voices, voices_dst);
    }

    #[test]
    fn scan_piper_uses_existing_bin_override() {
        let tree = Tree::new("scanover", &["en_US-a-medium"]).emitting_fixture();
        let root = tree.dir.join("empty-root");
        std::fs::create_dir_all(&root).expect("mkdir");
        let hit = scan_piper(&root, Some(&tree.bin())).expect("found");
        assert_eq!(hit.bin, tree.bin());
        assert_eq!(hit.voices, tree.voices());
    }

    #[test]
    fn scan_piper_ignores_missing_bin_override() {
        let root = std::env::temp_dir().join(format!("voz-piper-noroot-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("mkdir");
        let missing = PathBuf::from("/definitely/not/a/vits/bin");
        let hit = scan_piper(&root, Some(&missing));
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_piper_returns_none_without_voices() {
        let tree = Tree::new("scannovoice", &[]).emitting_fixture();
        assert!(scan_piper(&tree.dir.join("nowhere"), Some(&tree.bin())).is_none());
    }

    #[test]
    fn scan_piper_returns_none_when_voice_lacks_json() {
        let tree = Tree::new("scannojson", &["en_US-a-medium"]).emitting_fixture();
        std::fs::remove_file(tree.voices().join("en_US-a-medium.onnx.json")).expect("rm");
        assert!(scan_piper(&tree.dir.join("nowhere"), Some(&tree.bin())).is_none());
    }

    fn sine(n: usize, cycles: f64) -> Vec<i16> {
        (0..n)
            .map(|i| {
                let t = i as f64 / n as f64;
                ((t * cycles * std::f64::consts::TAU).sin() * 10000.0) as i16
            })
            .collect()
    }

    fn zero_crossings(s: &[i16]) -> usize {
        s.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count()
    }

    #[test]
    fn resample_at_unit_factor_returns_the_same_samples() {
        let input = sine(1000, 50.0);
        assert_eq!(resample(&input, 1.0), input);
    }

    #[test]
    fn resample_above_one_shortens_and_raises_pitch() {
        let input = sine(8000, 100.0);
        let out = resample(&input, 2.0);
        assert!(
            (out.len() as f64 - 4000.0).abs() < 4.0,
            "len {}",
            out.len()
        );
        let before = zero_crossings(&input);
        let after = zero_crossings(&out);
        assert!(
            (after as f64 - before as f64).abs() < before as f64 * 0.05,
            "cycles must survive: {before} -> {after}"
        );
    }

    #[test]
    fn resample_below_one_lengthens_and_lowers_pitch() {
        let input = sine(4000, 50.0);
        let out = resample(&input, 0.5);
        assert!((out.len() as f64 - 8000.0).abs() < 4.0, "len {}", out.len());
        let before = zero_crossings(&input);
        let after = zero_crossings(&out);
        assert!(
            (after as f64 - before as f64).abs() < before as f64 * 0.05,
            "cycles must survive: {before} -> {after}"
        );
    }

    #[test]
    fn resample_of_empty_input_is_empty() {
        assert!(resample(&[], 1.5).is_empty());
    }

    #[test]
    fn length_scale_compensates_pitch_so_duration_holds() {
        let pitch = Pitch::parse(99).expect("p");
        let f = pitch.factor();
        let only_pitch = length_scale(None, Some(pitch)).expect("scale");
        assert_eq!(only_pitch, format!("{f:.3}"));
        let with_rate = length_scale(super::super::Rate::parse(340).ok(), Some(pitch))
            .expect("scale");
        assert_eq!(with_rate, format!("{:.3}", 0.5 * f));
    }

    #[test]
    fn length_scale_is_absent_without_rate_or_pitch() {
        assert!(length_scale(None, None).is_none());
    }

    #[test]
    fn rewrap_applies_pitch_when_given() {
        let plain = rewrap_f32(&f32_wav(&[0.0; 800]), None).expect("rewrap");
        let shifted = rewrap_f32(
            &f32_wav(&[0.0; 800]),
            Some(Pitch::parse(99).expect("pitch")),
        )
        .expect("rewrap");
        assert!(
            shifted.len() < plain.len(),
            "raising pitch shortens the payload: {} vs {}",
            shifted.len(),
            plain.len()
        );
    }
}
