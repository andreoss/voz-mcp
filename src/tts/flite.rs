use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{SpeakRequest, Speech, Tts, TtsError};

pub struct Flite {
    bin: PathBuf,
    out_dir: PathBuf,
    counter: AtomicU64,
}

impl Flite {
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

impl Tts for Flite {
    fn speak(&self, req: &SpeakRequest) -> Result<Speech, TtsError> {
        let text = req.text.trim();
        if text.is_empty() {
            return Err(TtsError {
                reason: "text must not be empty".to_string(),
            });
        }
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let stem = format!("speech-{n}-{:04}.wav", std::process::id() % 10000);
        let path = self.out_dir.join(&stem);
        let tmp_path = self.out_dir.join(format!("{stem}.tmp"));
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-voice", req.lang.flite_voice()])
            .arg("-t")
            .arg(text)
            .arg("-o")
            .arg(&tmp_path);
        let output = super::spawn_with_retry(&mut cmd).map_err(|e| TtsError {
            reason: format!("flite spawn failed: {e}"),
        })?;
        if !output.status.success() {
            std::fs::remove_file(&tmp_path).ok();
            return Err(TtsError {
                reason: format!(
                    "flite exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        let produced = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
        if produced == 0 {
            std::fs::remove_file(&tmp_path).ok();
            return Err(TtsError {
                reason: "flite produced no audio".to_string(),
            });
        }
        std::fs::rename(&tmp_path, &path).map_err(|e| TtsError {
            reason: format!("failed to write output file: {e}"),
        })?;
        Ok(Speech { path })
    }
}

pub fn discover_flite() -> Option<PathBuf> {
    if let Ok(env_bin) = std::env::var("VOZ_FLITE_BIN") {
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
        if name.contains("flite") && !name.ends_with(".drv") {
            let candidate = store_path.join("bin").join("flite");
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
    use crate::tts::Language;
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

    const STUB_FLITE: &str = "#!/bin/sh\nprev=\"\"\nout=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\nprintf 'RIFFxxxxWAVEfmt ' > \"$out\"\n";

    #[test]
    fn rejects_empty_text() {
        let tts = Flite::new("/nonexistent", std::env::temp_dir());
        assert!(tts
            .speak(&SpeakRequest {
                text: "   ".to_string(),
                lang: Language::English,
                rate: None,
            })
            .is_err());
    }

    #[test]
    fn produces_real_wav_with_flite_stub() {
        let script = write_script("voz-flite-real-bin", STUB_FLITE);
        let out = std::env::temp_dir().join(format!("voz-flite-real-out-{}", std::process::id()));
        let tts = Flite::new(&script, &out);
        let speech = tts
            .speak(&SpeakRequest {
                text: "hello world".to_string(),
                lang: Language::English,
                rate: None,
            })
            .expect("speak ok");
        assert!(speech.path.exists(), "file missing: {}", speech.path.display());
        assert_eq!(wav_header(&speech.path).as_deref(), Some("RIFF"));
        assert!(!speech.path.to_string_lossy().ends_with(".tmp"));
        std::fs::remove_file(&script).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn spawn_failure_surfaces_error() {
        let out = std::env::temp_dir().join(format!("voz-flite-spawn-fail-{}", std::process::id()));
        let tts = Flite::new("/definitely/not/a/real/binary", &out);
        let err = tts
            .speak(&SpeakRequest {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
            })
            .unwrap_err();
        assert!(err.reason.contains("flite spawn failed"));
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn nonzero_exit_status_surfaces_error() {
        let script = write_script("voz-flite-fail-bin", "#!/bin/sh\necho boom 1>&2\nexit 1\n");
        let out = std::env::temp_dir().join(format!("voz-flite-exit-fail-{}", std::process::id()));
        let tts = Flite::new(&script, &out);
        let err = tts
            .speak(&SpeakRequest {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
            })
            .unwrap_err();
        assert!(err.reason.contains("flite exited with"));
        std::fs::remove_file(&script).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn empty_output_surfaces_error() {
        let script = write_script("voz-flite-empty-bin", "#!/bin/sh\nexit 0\n");
        let out = std::env::temp_dir().join(format!("voz-flite-empty-out-{}", std::process::id()));
        let tts = Flite::new(&script, &out);
        let err = tts
            .speak(&SpeakRequest {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
            })
            .unwrap_err();
        assert_eq!(err.reason, "flite produced no audio");
        std::fs::remove_file(&script).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn scan_store_returns_none_when_nothing_matches() {
        let root = std::env::temp_dir().join(format!("voz-flite-store-empty-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("mkdir");
        let hit = scan_store(&root);
        std::fs::remove_dir_all(&root).ok();
        assert!(hit.is_none());
    }

    #[test]
    fn scan_store_finds_built_flite_and_skips_drv() {
        let root = std::env::temp_dir().join(format!("voz-flite-store-{}", std::process::id()));
        let fake = root.join("fake-flite-9.9.9").join("bin");
        std::fs::create_dir_all(&fake).expect("mkdir");
        std::fs::write(fake.join("flite"), b"x").expect("write");
        std::fs::write(root.join("fake-flite-1.0.drv"), b"y").expect("write drv");
        std::fs::write(root.join("other-flite-lib.drv"), b"z").expect("write drv2");

        let hit = scan_store(&root).expect("found");
        let _ = std::fs::remove_dir_all(&root);
        assert!(hit.ends_with("fake-flite-9.9.9/bin/flite"));
    }
}
