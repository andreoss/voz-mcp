use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Language, SpeakRequest, Speech, Tts, TtsError};
use crate::backend::NeuralPaths;
use crate::wav;

pub struct Qwen {
    bin: PathBuf,
    talker: PathBuf,
    codec: PathBuf,
    out_dir: PathBuf,
    counter: AtomicU64,
}

impl Qwen {
    pub fn new(
        bin: impl Into<PathBuf>,
        talker: impl Into<PathBuf>,
        codec: impl Into<PathBuf>,
        out_dir: impl Into<PathBuf>,
    ) -> Self {
        let out_dir = out_dir.into();
        std::fs::create_dir_all(&out_dir).expect("failed to create output directory");
        Self {
            bin: bin.into(),
            talker: talker.into(),
            codec: codec.into(),
            out_dir,
            counter: AtomicU64::new(0),
        }
    }
}

fn lang_flag(lang: Language) -> &'static str {
    match lang {
        Language::Russian => "Russian",
        Language::English => "English",
        Language::Spanish => "Spanish",
    }
}

impl Tts for Qwen {
    fn speak(&self, req: &SpeakRequest) -> Result<Speech, TtsError> {
        let text = req.text.trim();
        if text.is_empty() {
            return Err(TtsError {
                reason: "text must not be empty".to_string(),
            });
        }
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let path = self
            .out_dir
            .join(format!("speech-{n}-{:04}.wav", std::process::id() % 10000));
        let mut cmd = Command::new(&self.bin);
        cmd.arg("--model")
            .arg(&self.talker)
            .arg("--codec")
            .arg(&self.codec)
            .args(["--lang", lang_flag(req.lang)])
            .arg("-o")
            .arg(&path)
            .args(["--format", "wav16"]);
        let output = super::spawn_feed_with_retry(&mut cmd, text.as_bytes()).map_err(|e| {
            TtsError {
                reason: format!("neural spawn failed: {e}"),
            }
        })?;
        if !output.status.success() {
            return Err(TtsError {
                reason: format!(
                    "neural engine exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        let bytes = std::fs::read(&path).map_err(|e| TtsError {
            reason: format!("neural output missing: {e}"),
        })?;
        wav::validate(&bytes).map_err(|e| TtsError {
            reason: format!("neural output invalid: {e:?}"),
        })?;
        Ok(Speech { path })
    }
}

pub fn scan_modelz(root: &Path) -> Option<NeuralPaths> {
    let bin = root.join("qwentts").join("build").join("qwen-tts");
    if !bin.exists() {
        return None;
    }
    let mut talker = None;
    let mut codec = None;
    for entry in std::fs::read_dir(root.join("gguf")).ok()? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".gguf") {
            continue;
        }
        if name.contains("talker") {
            talker = Some(entry.path());
        }
        if name.contains("tokenizer") {
            codec = Some(entry.path());
        }
    }
    Some(NeuralPaths {
        bin,
        talker: talker?,
        codec: codec?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::Language;

    fn minimal_wav() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&38u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&24000u32.to_le_bytes());
        v.extend_from_slice(&48000u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&2u32.to_le_bytes());
        v.extend_from_slice(&[0, 0]);
        v
    }

    struct Stub {
        dir: PathBuf,
        script: PathBuf,
    }

    impl Stub {
        fn new(name: &str, body: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("voz-qwen-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("mkdir");
            Self::new_in(dir, body)
        }

        fn emitting_fixture(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("voz-qwen-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(dir.join("fixture.wav"), minimal_wav()).expect("write fixture");
            let body = format!(
                "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\necho \"$@\" > {d}/args.txt\ncat > {d}/stdin.txt\ncp {d}/fixture.wav \"$out\"\n",
                d = dir.display()
            );
            Self::new_in(dir, &body)
        }

        fn new_in(dir: PathBuf, body: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let script = dir.join("qwen-tts");
            std::fs::write(&script, body).expect("write script");
            let mut perms = std::fs::metadata(&script).expect("meta").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).expect("chmod");
            Self { dir, script }
        }
    }

    impl Drop for Stub {
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
        let tts = Qwen::new("/nonexistent", "/t.gguf", "/c.gguf", std::env::temp_dir());
        assert!(tts.speak(&request("   ", Language::English)).is_err());
    }

    #[test]
    fn happy_path_writes_wav_and_feeds_text_via_stdin() {
        let stub = Stub::emitting_fixture("happy");
        let out = stub.dir.join("out");
        let tts = Qwen::new(&stub.script, "/t.gguf", "/c.gguf", &out);
        let speech = tts
            .speak(&request("hello there", Language::English))
            .expect("speak ok");
        assert!(speech.path.exists());
        assert!(wav::validate(&std::fs::read(&speech.path).unwrap()).is_ok());
        let stdin = std::fs::read_to_string(stub.dir.join("stdin.txt")).expect("stdin");
        assert_eq!(stdin, "hello there");
        let args = std::fs::read_to_string(stub.dir.join("args.txt")).expect("args");
        assert!(args.contains("--model /t.gguf"), "args were: {args}");
        assert!(args.contains("--codec /c.gguf"), "args were: {args}");
        assert!(args.contains("--lang English"), "args were: {args}");
        assert!(args.contains("--format wav16"), "args were: {args}");
    }

    #[test]
    fn maps_each_language_to_engine_lang_flag() {
        for (lang, flag) in [
            (Language::Russian, "--lang Russian"),
            (Language::English, "--lang English"),
            (Language::Spanish, "--lang Spanish"),
        ] {
            let stub = Stub::emitting_fixture("langs");
            let out = stub.dir.join("out");
            let tts = Qwen::new(&stub.script, "/t.gguf", "/c.gguf", &out);
            tts.speak(&request("hola", lang)).expect("speak ok");
            let args = std::fs::read_to_string(stub.dir.join("args.txt")).expect("args");
            assert!(args.contains(flag), "args were: {args}");
        }
    }

    #[test]
    fn spawn_failure_surfaces_error() {
        let out = std::env::temp_dir().join(format!("voz-qwen-spawn-{}", std::process::id()));
        let tts = Qwen::new("/definitely/not/a/real/binary", "/t.gguf", "/c.gguf", &out);
        let err = tts.speak(&request("hello", Language::English)).unwrap_err();
        assert!(err.reason.contains("neural spawn failed"));
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn nonzero_exit_surfaces_stderr() {
        let stub = Stub::new("fail", "#!/bin/sh\necho boom 1>&2\nexit 1\n");
        let out = stub.dir.join("out");
        let tts = Qwen::new(&stub.script, "/t.gguf", "/c.gguf", &out);
        let err = tts.speak(&request("hello", Language::English)).unwrap_err();
        assert!(err.reason.contains("neural engine exited with"));
        assert!(err.reason.contains("boom"));
    }

    #[test]
    fn missing_output_file_surfaces_error() {
        let stub = Stub::new("noout", "#!/bin/sh\ncat > /dev/null\nexit 0\n");
        let out = stub.dir.join("out");
        let tts = Qwen::new(&stub.script, "/t.gguf", "/c.gguf", &out);
        let err = tts.speak(&request("hello", Language::English)).unwrap_err();
        assert!(err.reason.contains("neural output missing"));
    }

    #[test]
    fn invalid_output_file_surfaces_error() {
        let stub = Stub::new(
            "badout",
            "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\ncat > /dev/null\nprintf 'not audio' > \"$out\"\n",
        );
        let out = stub.dir.join("out");
        let tts = Qwen::new(&stub.script, "/t.gguf", "/c.gguf", &out);
        let err = tts.speak(&request("hello", Language::English)).unwrap_err();
        assert!(err.reason.contains("neural output invalid"));
    }

    fn fake_modelz(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("voz-modelz-{name}-{}", std::process::id()));
        std::fs::create_dir_all(root.join("qwentts").join("build")).expect("mkdir");
        std::fs::create_dir_all(root.join("gguf")).expect("mkdir gguf");
        root
    }

    #[test]
    fn scan_modelz_returns_none_without_runtime_bin() {
        let root = fake_modelz("nobin");
        std::fs::write(root.join("gguf").join("x-talker-y.gguf"), b"t").expect("write");
        let hit = scan_modelz(&root);
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_modelz_returns_none_without_talker_weights() {
        let root = fake_modelz("notalker");
        std::fs::write(root.join("qwentts").join("build").join("qwen-tts"), b"x").expect("write");
        std::fs::write(root.join("gguf").join("x-tokenizer-y.gguf"), b"c").expect("write");
        let hit = scan_modelz(&root);
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_modelz_returns_none_without_tokenizer_weights() {
        let root = fake_modelz("nocodec");
        std::fs::write(root.join("qwentts").join("build").join("qwen-tts"), b"x").expect("write");
        std::fs::write(root.join("gguf").join("x-talker-y.gguf"), b"t").expect("write");
        let hit = scan_modelz(&root);
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_modelz_returns_none_without_gguf_dir() {
        let root = std::env::temp_dir().join(format!("voz-modelz-noggufdir-{}", std::process::id()));
        std::fs::create_dir_all(root.join("qwentts").join("build")).expect("mkdir");
        std::fs::write(root.join("qwentts").join("build").join("qwen-tts"), b"x").expect("write");
        let hit = scan_modelz(&root);
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_modelz_finds_runtime_and_weights_skipping_foreign_files() {
        let root = fake_modelz("full");
        std::fs::write(root.join("qwentts").join("build").join("qwen-tts"), b"x").expect("write");
        std::fs::write(root.join("gguf").join("x-talker-y.gguf"), b"t").expect("write");
        std::fs::write(root.join("gguf").join("x-tokenizer-y.gguf"), b"c").expect("write");
        std::fs::write(root.join("gguf").join("talker-notes.txt"), b"n").expect("write");
        let hit = scan_modelz(&root).expect("found");
        let ok = hit.bin.ends_with("qwentts/build/qwen-tts")
            && hit.talker.ends_with("gguf/x-talker-y.gguf")
            && hit.codec.ends_with("gguf/x-tokenizer-y.gguf");
        std::fs::remove_dir_all(&root).ok();
        assert!(ok);
    }
}
