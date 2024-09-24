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
        let output = Command::new(&self.bin)
            .args(["-v", req.lang.voice(), "--stdout"])
            .arg(text)
            .output()
            .map_err(|e| TtsError {
                reason: format!("espeak spawn failed: {e}"),
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
    use crate::tts::Language;
    use std::io::Read;

    fn wav_header(file: &Path) -> Option<String> {
        let mut buf = [0u8; 4];
        let mut f = std::fs::File::open(file).ok()?;
        f.read_exact(&mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    #[test]
    fn rejects_empty_text() {
        let tts = Espeak::new("/nonexistent", std::env::temp_dir());
        assert!(tts
            .speak(&SpeakRequest {
                text: "   ".to_string(),
                lang: Language::English,
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
                })
                .expect("speak ok");
            assert!(speech.path.exists(), "file missing: {}", speech.path.display());
            assert_eq!(wav_header(&speech.path).as_deref(), Some("RIFF"));
            let meta = std::fs::metadata(&speech.path).expect("meta");
            assert!(meta.len() > 44, "wav too small");
        }
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