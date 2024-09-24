use std::fmt;
use std::path::{Path, PathBuf};

use crate::cli::AudioArgs;
use crate::tts::{SpeakRequest, Tts};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCliError {
    Synthesis(String),
    Output(String),
}

impl fmt::Display for AudioCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioCliError::Synthesis(reason) => write!(f, "speech synthesis failed: {reason}"),
            AudioCliError::Output(reason) => write!(f, "failed to write output: {reason}"),
        }
    }
}

pub fn synthesize(args: AudioArgs, backend: &dyn Tts) -> Result<PathBuf, AudioCliError> {
    let speech = backend
        .speak(&SpeakRequest {
            text: args.text,
            lang: args.lang,
            rate: args.rate,
            pitch: args.pitch,
        })
        .map_err(|e| AudioCliError::Synthesis(e.reason))?;
    match args.out {
        Some(dest) => {
            move_file(&speech.path, &dest).map_err(|e| AudioCliError::Output(e.to_string()))?;
            Ok(dest)
        }
        None => Ok(speech.path),
    }
}

fn move_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    copy_and_remove(src, dest)
}

fn copy_and_remove(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::copy(src, dest)?;
    std::fs::remove_file(src)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::{Language, Speech, TtsError};

    struct StubTts(PathBuf);
    impl Tts for StubTts {
        fn speak(&self, _req: &SpeakRequest) -> Result<Speech, TtsError> {
            Ok(Speech {
                path: self.0.clone(),
            })
        }
    }

    struct FailingTts;
    impl Tts for FailingTts {
        fn speak(&self, _req: &SpeakRequest) -> Result<Speech, TtsError> {
            Err(TtsError {
                reason: "backend down".to_string(),
            })
        }
    }

    fn args(out: Option<PathBuf>) -> AudioArgs {
        AudioArgs {
            text: "hi".to_string(),
            lang: Language::English,
            rate: None,
            pitch: None,
            out,
        }
    }

    #[test]
    fn returns_backend_path_when_no_out_given() {
        let src = std::env::temp_dir().join(format!("voz-audio-noout-{}", std::process::id()));
        std::fs::write(&src, b"data").expect("write");
        let backend = StubTts(src.clone());
        let path = synthesize(args(None), &backend).expect("ok");
        assert_eq!(path, src);
        std::fs::remove_file(&src).ok();
    }

    #[test]
    fn moves_output_to_requested_path() {
        let src = std::env::temp_dir().join(format!("voz-audio-src-{}", std::process::id()));
        std::fs::write(&src, b"data").expect("write");
        let dest = std::env::temp_dir().join(format!("voz-audio-dest-{}", std::process::id()));
        let backend = StubTts(src.clone());
        let path = synthesize(args(Some(dest.clone())), &backend).expect("ok");
        assert_eq!(path, dest);
        assert!(dest.exists());
        assert!(!src.exists());
        std::fs::remove_file(&dest).ok();
    }

    #[test]
    fn creates_missing_out_parent_directory() {
        let src = std::env::temp_dir().join(format!("voz-audio-mkdir-src-{}", std::process::id()));
        std::fs::write(&src, b"data").expect("write");
        let parent = std::env::temp_dir().join(format!("voz-audio-mkdir-dir-{}", std::process::id()));
        let dest = parent.join("out.wav");
        let backend = StubTts(src.clone());
        let path = synthesize(args(Some(dest.clone())), &backend).expect("ok");
        assert_eq!(path, dest);
        assert!(dest.exists());
        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn surfaces_synthesis_failure() {
        let err = synthesize(args(None), &FailingTts).unwrap_err();
        assert_eq!(err, AudioCliError::Synthesis("backend down".to_string()));
        assert!(err.to_string().contains("backend down"));
    }

    #[test]
    fn output_error_display_includes_reason() {
        let err = AudioCliError::Output("disk full".to_string());
        assert!(err.to_string().contains("disk full"));
    }

    #[test]
    fn falls_back_to_copy_when_rename_target_is_a_directory() {
        let src = std::env::temp_dir().join(format!("voz-audio-renamefail-src-{}", std::process::id()));
        std::fs::write(&src, b"data").expect("write");
        let dest = std::env::temp_dir().join(format!("voz-audio-renamefail-dest-{}", std::process::id()));
        std::fs::create_dir_all(&dest).expect("mkdir dest");
        let backend = StubTts(src.clone());
        let err = synthesize(args(Some(dest.clone())), &backend).unwrap_err();
        assert!(matches!(err, AudioCliError::Output(_)));
        std::fs::remove_file(&src).ok();
        std::fs::remove_dir_all(&dest).ok();
    }

    #[test]
    fn copy_and_remove_moves_file_across_directories() {
        let src = std::env::temp_dir().join(format!("voz-audio-copyrm-src-{}", std::process::id()));
        std::fs::write(&src, b"data").expect("write");
        let dest = std::env::temp_dir().join(format!("voz-audio-copyrm-dest-{}", std::process::id()));
        copy_and_remove(&src, &dest).expect("ok");
        assert!(dest.exists());
        assert!(!src.exists());
        std::fs::remove_file(&dest).ok();
    }

    #[test]
    fn surfaces_output_write_failure() {
        let src = std::env::temp_dir().join(format!("voz-audio-blocked-src-{}", std::process::id()));
        std::fs::write(&src, b"data").expect("write");
        let blocker = std::env::temp_dir().join(format!("voz-audio-blocker-{}", std::process::id()));
        std::fs::write(&blocker, b"x").expect("write blocker");
        let dest = blocker.join("sub").join("out.wav");
        let backend = StubTts(src.clone());
        let err = synthesize(args(Some(dest)), &backend).unwrap_err();
        assert!(matches!(err, AudioCliError::Output(_)));
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&blocker).ok();
    }
}
