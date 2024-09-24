use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{SpeakRequest, Speech, Tts, TtsError};

pub struct Espeak {
    bin: PathBuf,
    out_dir: PathBuf,
    counter: AtomicU64,
}

impl Espeak {
    pub fn new(bin: impl Into<PathBuf>, out_dir: impl Into<PathBuf>) -> Self {
        let out_dir = out_dir.into();
        std::fs::create_dir_all(&out_dir)
            .expect("failed to create output directory");
        Self {
            bin: bin.into(),
            out_dir,
            counter: AtomicU64::new(0),
        }
    }
}

impl Tts for Espeak {
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
        cmd.args(["-v", req.lang.voice()]);
        if let Some(rate) = req.rate {
            cmd.args(["-s", &rate.value().to_string()]);
        }
        if let Some(pitch) = req.pitch {
            cmd.args(["-p", &pitch.value().to_string()]);
        }
        let output = super::spawn_with_retry(cmd.args(["--stdout"]).arg(text)).map_err(|e| {
            TtsError {
                reason: format!("espeak spawn failed: {e}"),
            }
        })?;
        if !output.status.success() {
            return Err(TtsError {
                reason: format!(
                    "espeak exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        if output.stdout.is_empty() {
            return Err(TtsError {
                reason: "espeak produced no audio".to_string(),
            });
        }
        let mut f = std::fs::File::create(&path).map_err(|e| TtsError {
            reason: format!("failed to create output file: {e}"),
        })?;
        f.write_all(&output.stdout).map_err(|e| TtsError {
            reason: format!("failed to write output file: {e}"),
        })?;
        Ok(Speech { path })
    }
}

pub fn discover_bin() -> Option<PathBuf> {
    if let Ok(env_bin) = std::env::var("VOZ_ESPEAK_BIN") {
        let p = PathBuf::from(env_bin);
        if p.exists() {
            return Some(p);
        }
    }
    scan_store(Path::new("/nix/store"))
}

fn scan_store(root: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let store_path = entry.path();
        if name.contains("espeak-ng") && !name.ends_with(".drv") {
            let candidate = store_path.join("bin").join("espeak-ng");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::{Language, Pitch, Rate};
    use std::io::Read;

    fn wav_header(file: &Path) -> Option<String> {
        let mut buf = [0u8; 4];
        let mut f = std::fs::File::open(file).ok()?;
        f.read_exact(&mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    fn write_script(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        std::fs::write(&script, body).expect("write script");
        let mut perms = std::fs::metadata(&script).expect("meta").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("chmod");
        script
    }

    #[test]
    fn rejects_empty_text() {
        let tts = Espeak::new("/nonexistent", std::env::temp_dir());
        assert!(tts
            .speak(&SpeakRequest {
                text: "   ".to_string(),
                lang: Language::English,
                rate: None,
                pitch: None,
            })
            .is_err());
    }

    #[test]
    fn rejects_empty_text_even_with_rate_set() {
        let tts = Espeak::new("/nonexistent", std::env::temp_dir());
        assert!(tts
            .speak(&SpeakRequest {
                text: "   ".to_string(),
                lang: Language::English,
                rate: Some(Rate::parse(120).unwrap()),
                pitch: None,
            })
            .is_err());
    }

    #[test]
    fn produces_real_wav_with_espeak() {
        let bin = std::env::var("VOZ_ESPEAK_BIN").unwrap_or_else(|_| {
            "/nix/store/156gf924ld4apvl77h22z8b38jqd9wi2-espeak-ng-1.52.0.1-unstable-2025-09-09/bin/espeak-ng".to_string()
        });
        if !Path::new(&bin).exists() {
            eprintln!("skipping: espeak-ng not present at {bin}");
            return;
        }
        let out = std::env::temp_dir().join("voz-test");
        let tts = Espeak::new(&bin, &out);
        for lang in [Language::Russian, Language::English, Language::Spanish] {
            let speech = tts
                .speak(&SpeakRequest {
                    text: "hello world".to_string(),
                    lang,
                    rate: None,
                    pitch: None,
                })
                .expect("speak ok");
            assert!(speech.path.exists(), "file missing: {}", speech.path.display());
            assert_eq!(wav_header(&speech.path).as_deref(), Some("RIFF"));
            let meta = std::fs::metadata(&speech.path).expect("meta");
            assert!(meta.len() > 44, "wav too small");
        }
    }

    #[test]
    fn produces_real_wav_with_rate_via_espeak() {
        let bin = std::env::var("VOZ_ESPEAK_BIN").unwrap_or_else(|_| {
            "/nix/store/156gf924ld4apvl77h22z8b38jqd9wi2-espeak-ng-1.52.0.1-unstable-2025-09-09/bin/espeak-ng".to_string()
        });
        if !Path::new(&bin).exists() {
            eprintln!("skipping: espeak-ng not present at {bin}");
            return;
        }
        let out = std::env::temp_dir().join("voz-test-rate");
        let tts = Espeak::new(&bin, &out);
        let speech = tts
            .speak(&SpeakRequest {
                text: "hello world".to_string(),
                lang: Language::English,
                rate: Some(Rate::parse(120).unwrap()),
                pitch: None,
            })
            .expect("speak ok");
        assert!(speech.path.exists(), "file missing: {}", speech.path.display());
        assert_eq!(wav_header(&speech.path).as_deref(), Some("RIFF"));
    }

    #[test]
    fn passes_speed_flag_when_rate_present() {
        let record = std::env::temp_dir().join(format!("voz-rate-args-{}", std::process::id()));
        let script = write_script(
            "voz-rate-bin",
            &format!(
                "#!/bin/sh\necho \"$@\" > {}\nprintf 'RIFFxxxxWAVEfmt '\n",
                record.display()
            ),
        );
        let out = std::env::temp_dir().join(format!("voz-rate-out-{}", std::process::id()));
        let tts = Espeak::new(&script, &out);
        tts.speak(&SpeakRequest {
            text: "hello".to_string(),
            lang: Language::English,
            rate: Some(Rate::parse(120).unwrap()),
            pitch: None,
        })
        .expect("speak ok");
        let recorded = std::fs::read_to_string(&record).expect("read args");
        assert!(recorded.contains("-s 120"), "args were: {recorded}");
        std::fs::remove_file(&script).ok();
        std::fs::remove_file(&record).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn omits_speed_flag_when_rate_absent() {
        let record = std::env::temp_dir().join(format!("voz-norate-args-{}", std::process::id()));
        let script = write_script(
            "voz-norate-bin",
            &format!(
                "#!/bin/sh\necho \"$@\" > {}\nprintf 'RIFFxxxxWAVEfmt '\n",
                record.display()
            ),
        );
        let out = std::env::temp_dir().join(format!("voz-norate-out-{}", std::process::id()));
        let tts = Espeak::new(&script, &out);
        tts.speak(&SpeakRequest {
            text: "hello".to_string(),
            lang: Language::English,
            rate: None,
            pitch: None,
        })
        .expect("speak ok");
        let recorded = std::fs::read_to_string(&record).expect("read args");
        assert!(!recorded.contains("-s "), "args were: {recorded}");
        std::fs::remove_file(&script).ok();
        std::fs::remove_file(&record).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn passes_pitch_flag_when_pitch_present() {
        let record = std::env::temp_dir().join(format!("voz-pitch-args-{}", std::process::id()));
        let script = write_script(
            "voz-pitch-bin",
            &format!(
                "#!/bin/sh\necho \"$@\" > {}\nprintf 'RIFFxxxxWAVEfmt '\n",
                record.display()
            ),
        );
        let out = std::env::temp_dir().join(format!("voz-pitch-out-{}", std::process::id()));
        let tts = Espeak::new(&script, &out);
        tts.speak(&SpeakRequest {
            text: "hello".to_string(),
            lang: Language::English,
            rate: None,
            pitch: Some(Pitch::parse(60).unwrap()),
        })
        .expect("speak ok");
        let recorded = std::fs::read_to_string(&record).expect("read args");
        assert!(recorded.contains("-p 60"), "args were: {recorded}");
        std::fs::remove_file(&script).ok();
        std::fs::remove_file(&record).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn omits_pitch_flag_when_pitch_absent() {
        let record = std::env::temp_dir().join(format!("voz-nopitch-args-{}", std::process::id()));
        let script = write_script(
            "voz-nopitch-bin",
            &format!(
                "#!/bin/sh\necho \"$@\" > {}\nprintf 'RIFFxxxxWAVEfmt '\n",
                record.display()
            ),
        );
        let out = std::env::temp_dir().join(format!("voz-nopitch-out-{}", std::process::id()));
        let tts = Espeak::new(&script, &out);
        tts.speak(&SpeakRequest {
            text: "hello".to_string(),
            lang: Language::English,
            rate: None,
            pitch: None,
        })
        .expect("speak ok");
        let recorded = std::fs::read_to_string(&record).expect("read args");
        assert!(!recorded.contains("-p "), "args were: {recorded}");
        std::fs::remove_file(&script).ok();
        std::fs::remove_file(&record).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn produces_real_wav_with_pitch_via_espeak() {
        let bin = std::env::var("VOZ_ESPEAK_BIN").unwrap_or_else(|_| {
            "/nix/store/156gf924ld4apvl77h22z8b38jqd9wi2-espeak-ng-1.52.0.1-unstable-2025-09-09/bin/espeak-ng".to_string()
        });
        if !Path::new(&bin).exists() {
            eprintln!("skipping: espeak-ng not present at {bin}");
            return;
        }
        let out = std::env::temp_dir().join("voz-test-pitch");
        let tts = Espeak::new(&bin, &out);
        let speech = tts
            .speak(&SpeakRequest {
                text: "hello world".to_string(),
                lang: Language::English,
                rate: None,
                pitch: Some(Pitch::parse(60).unwrap()),
            })
            .expect("speak ok");
        assert!(speech.path.exists(), "file missing: {}", speech.path.display());
        assert_eq!(wav_header(&speech.path).as_deref(), Some("RIFF"));
    }

    #[test]
    fn rejects_empty_text_even_with_pitch_set() {
        let tts = Espeak::new("/nonexistent", std::env::temp_dir());
        assert!(tts
            .speak(&SpeakRequest {
                text: "   ".to_string(),
                lang: Language::English,
                rate: None,
                pitch: Some(Pitch::parse(60).unwrap()),
            })
            .is_err());
    }

    #[test]
    fn spawn_failure_surfaces_error() {
        let out = std::env::temp_dir().join(format!("voz-spawn-fail-{}", std::process::id()));
        let tts = Espeak::new("/definitely/not/a/real/binary", &out);
        let err = tts
            .speak(&SpeakRequest {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
                pitch: None,
            })
            .unwrap_err();
        assert!(err.reason.contains("espeak spawn failed"));
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn nonzero_exit_status_surfaces_error() {
        let script = write_script("voz-fail-bin", "#!/bin/sh\necho boom 1>&2\nexit 1\n");
        let out = std::env::temp_dir().join(format!("voz-exit-fail-{}", std::process::id()));
        let tts = Espeak::new(&script, &out);
        let err = tts
            .speak(&SpeakRequest {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
                pitch: None,
            })
            .unwrap_err();
        assert!(err.reason.contains("espeak exited with"));
        std::fs::remove_file(&script).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn empty_stdout_surfaces_error() {
        let script = write_script("voz-empty-bin", "#!/bin/sh\nexit 0\n");
        let out = std::env::temp_dir().join(format!("voz-empty-out-{}", std::process::id()));
        let tts = Espeak::new(&script, &out);
        let err = tts
            .speak(&SpeakRequest {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
                pitch: None,
            })
            .unwrap_err();
        assert_eq!(err.reason, "espeak produced no audio");
        std::fs::remove_file(&script).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn scan_store_returns_none_when_nothing_matches() {
        let root = std::env::temp_dir().join(format!("voz-store-empty-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("mkdir");
        let hit = scan_store(&root);
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_store_finds_built_espeak_and_skips_drv() {
        let root = std::env::temp_dir().join(format!("voz-store-{}", std::process::id()));
        let fake = root.join("fake-espeak-ng-9.9.9").join("bin");
        std::fs::create_dir_all(&fake).expect("mkdir");
        std::fs::write(fake.join("espeak-ng"), b"x").expect("write");
        std::fs::write(root.join("fake-espeak-ng-1.0.drv"), b"y").expect("write drv");
        std::fs::write(root.join("other-espeak-ng-data.drv"), b"z").expect("write drv2");

        let hit = scan_store(&root).expect("found");
        let _ = std::fs::remove_dir_all(&root);
        assert!(hit.ends_with("fake-espeak-ng-9.9.9/bin/espeak-ng"));
    }
}