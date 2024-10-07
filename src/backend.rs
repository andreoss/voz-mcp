use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::tts::{Timeout, TimeoutError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeuralPaths {
    pub bin: PathBuf,
    pub talker: PathBuf,
    pub codec: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiperPaths {
    pub bin: PathBuf,
    pub voices: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendPick {
    Neural {
        bin: PathBuf,
        talker: PathBuf,
        codec: PathBuf,
    },
    Piper {
        bin: PathBuf,
        voices: PathBuf,
    },
    Null,
}

pub const BACKEND_CHOICES: &str = "auto|neural|fallback|null";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    Auto,
    Neural,
    Fallback,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoiceError {
    Unknown,
}

impl BackendChoice {
    pub fn parse(s: &str) -> Result<BackendChoice, BackendChoiceError> {
        match s {
            "auto" => Ok(BackendChoice::Auto),
            "neural" => Ok(BackendChoice::Neural),
            "fallback" => Ok(BackendChoice::Fallback),
            "null" => Ok(BackendChoice::Null),
            _ => Err(BackendChoiceError::Unknown),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            BackendChoice::Auto => "auto",
            BackendChoice::Neural => "neural",
            BackendChoice::Fallback => "fallback",
            BackendChoice::Null => "null",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    UnknownChoice(String),
    Unavailable(BackendChoice),
    UnavailableAt {
        choice: BackendChoice,
        searched: String,
    },
    BadTimeout(String),
}

impl SelectionError {
    pub fn exit_code(&self) -> i32 {
        match self {
            SelectionError::UnknownChoice(_) => 2,
            SelectionError::BadTimeout(_) => 2,
            SelectionError::Unavailable(_) => 1,
            SelectionError::UnavailableAt { .. } => 1,
        }
    }
}

impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectionError::UnknownChoice(value) => {
                write!(f, "unsupported backend {value}, expected {BACKEND_CHOICES}")
            }
            SelectionError::Unavailable(choice) => {
                write!(f, "{} backend not available", choice.code())
            }
            SelectionError::UnavailableAt { choice, searched } => write!(
                f,
                "{} backend not available; searched {}",
                choice.code(),
                searched
            ),
            SelectionError::BadTimeout(value) => write!(
                f,
                "unsupported timeout {value}, expected whole seconds {}..{}",
                Timeout::MIN_SECS,
                Timeout::MAX_SECS
            ),
        }
    }
}

pub fn backend_choice(env_override: Option<&OsStr>) -> Result<BackendChoice, SelectionError> {
    let Some(raw) = env_override else {
        return Ok(BackendChoice::Auto);
    };
    let raw = raw.to_string_lossy();
    BackendChoice::parse(&raw).map_err(|_| SelectionError::UnknownChoice(raw.into_owned()))
}

pub fn pick_backend(
    choice: BackendChoice,
    neural_discovered: Option<&NeuralPaths>,
    piper_discovered: Option<&PiperPaths>,
) -> Result<BackendPick, SelectionError> {
    match choice {
        BackendChoice::Null => Ok(BackendPick::Null),
        BackendChoice::Neural => neural_discovered
            .map(neural_pick)
            .ok_or(SelectionError::Unavailable(choice)),
        BackendChoice::Fallback => piper_discovered
            .map(piper_pick)
            .ok_or(SelectionError::Unavailable(choice)),
        BackendChoice::Auto => Ok(match (neural_discovered, piper_discovered) {
            (Some(n), _) => neural_pick(n),
            (None, Some(p)) => piper_pick(p),
            (None, None) => BackendPick::Null,
        }),
    }
}

fn neural_pick(n: &NeuralPaths) -> BackendPick {
    BackendPick::Neural {
        bin: n.bin.clone(),
        talker: n.talker.clone(),
        codec: n.codec.clone(),
    }
}

fn piper_pick(p: &PiperPaths) -> BackendPick {
    BackendPick::Piper {
        bin: p.bin.clone(),
        voices: p.voices.clone(),
    }
}

pub fn read_backend_choice_override() -> Option<OsString> {
    std::env::var_os("VOZ_BACKEND")
}

pub fn read_timeout_override() -> Option<OsString> {
    std::env::var_os("VOZ_TIMEOUT_SECS")
}

pub fn select_timeout(env_override: Option<&OsStr>) -> Result<Timeout, SelectionError> {
    let Some(raw) = env_override else {
        return Ok(Timeout::default_timeout());
    };
    let raw = raw.to_string_lossy();
    match Timeout::parse(&raw) {
        Ok(t) => Ok(t),
        Err(TimeoutError::NotANumber) | Err(TimeoutError::OutOfRange) => {
            Err(SelectionError::BadTimeout(raw.into_owned()))
        }
    }
}

pub fn read_neural_root_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_NEURAL_ROOT").map(PathBuf::from)
}

pub fn read_piper_bin_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_PIPER_BIN").map(PathBuf::from)
}

pub fn read_neural_bin_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_NEURAL_BIN").map(PathBuf::from)
}

pub fn read_piper_voices_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_PIPER_VOICES").map(PathBuf::from)
}

pub fn default_root(xdg_data_home: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf {
    if let Some(xdg) = xdg_data_home
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("voz");
    }
    if let Some(home) = home
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".local").join("share").join("voz");
    }
    PathBuf::from("/usr/local/share/voz")
}

pub fn neural_root(env_override: Option<&Path>) -> PathBuf {
    match env_override {
        Some(p) if p.exists() => p.to_path_buf(),
        _ => default_root(
            std::env::var_os("XDG_DATA_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neural() -> NeuralPaths {
        NeuralPaths {
            bin: PathBuf::from("/made-up/modelz/qwentts/build/qwen-tts"),
            talker: PathBuf::from("/made-up/modelz/gguf/talker.gguf"),
            codec: PathBuf::from("/made-up/modelz/gguf/tokenizer.gguf"),
        }
    }

    fn piper() -> PiperPaths {
        PiperPaths {
            bin: PathBuf::from("/made-up/modelz/piper/bin/piper"),
            voices: PathBuf::from("/made-up/modelz/piper/voices"),
        }
    }

    #[test]
    fn auto_yields_neural_when_discovered() {
        let n = neural();
        let pick = pick_backend(BackendChoice::Auto, Some(&n), None);
        assert_eq!(
            pick,
            Ok(BackendPick::Neural {
                bin: n.bin,
                talker: n.talker,
                codec: n.codec,
            })
        );
    }

    #[test]
    fn auto_prefers_neural_over_fallback() {
        let n = neural();
        let p = piper();
        assert!(matches!(
            pick_backend(BackendChoice::Auto, Some(&n), Some(&p)),
            Ok(BackendPick::Neural { .. })
        ));
    }

    #[test]
    fn auto_falls_back_when_neural_absent() {
        let p = piper();
        assert_eq!(
            pick_backend(BackendChoice::Auto, None, Some(&p)),
            Ok(BackendPick::Piper {
                bin: p.bin,
                voices: p.voices,
            })
        );
    }

    #[test]
    fn auto_yields_null_when_nothing_discovered() {
        assert_eq!(pick_backend(BackendChoice::Auto, None, None), Ok(BackendPick::Null));
    }

    #[test]
    fn read_neural_root_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_NEURAL_ROOT").map(PathBuf::from);
        assert_eq!(read_neural_root_override(), expected);
    }

    #[test]
    fn read_piper_bin_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_PIPER_BIN").map(PathBuf::from);
        assert_eq!(read_piper_bin_override(), expected);
    }

    #[test]
    fn neural_root_uses_existing_override() {
        let dir = std::env::temp_dir().join(format!("voz-neural-root-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let root = neural_root(Some(&dir));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(root, dir);
    }

    fn env_default_root() -> PathBuf {
        default_root(
            std::env::var_os("XDG_DATA_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }

    #[test]
    fn neural_root_ignores_missing_override() {
        let missing = PathBuf::from("/definitely/not/a/real/modelz-root");
        assert_eq!(neural_root(Some(&missing)), env_default_root());
    }

    #[test]
    fn neural_root_defaults_without_override() {
        assert_eq!(neural_root(None), env_default_root());
    }

    fn choice(s: &str) -> BackendChoice {
        BackendChoice::parse(s).expect("choice")
    }

    #[test]
    fn parses_every_backend_choice() {
        assert_eq!(choice("auto"), BackendChoice::Auto);
        assert_eq!(choice("neural"), BackendChoice::Neural);
        assert_eq!(choice("fallback"), BackendChoice::Fallback);
        assert_eq!(choice("null"), BackendChoice::Null);
    }

    #[test]
    fn rejects_unknown_backend_choice() {
        assert_eq!(BackendChoice::parse("espeak"), Err(BackendChoiceError::Unknown));
        assert_eq!(BackendChoice::parse(""), Err(BackendChoiceError::Unknown));
    }

    #[test]
    fn backend_choice_code_round_trips() {
        for c in [
            BackendChoice::Auto,
            BackendChoice::Neural,
            BackendChoice::Fallback,
            BackendChoice::Null,
        ] {
            assert_eq!(choice(c.code()), c);
        }
    }

    #[test]
    fn forced_fallback_wins_over_discovered_neural() {
        let n = neural();
        let p = piper();
        assert_eq!(
            pick_backend(BackendChoice::Fallback, Some(&n), Some(&p)),
            Ok(BackendPick::Piper {
                bin: p.bin,
                voices: p.voices,
            })
        );
    }

    #[test]
    fn forced_neural_wins_over_discovered_fallback() {
        let n = neural();
        let p = piper();
        assert!(matches!(
            pick_backend(BackendChoice::Neural, Some(&n), Some(&p)),
            Ok(BackendPick::Neural { .. })
        ));
    }

    #[test]
    fn forced_neural_errors_when_undiscovered() {
        let p = piper();
        assert_eq!(
            pick_backend(BackendChoice::Neural, None, Some(&p)),
            Err(SelectionError::Unavailable(BackendChoice::Neural))
        );
    }

    #[test]
    fn forced_fallback_errors_when_undiscovered() {
        let n = neural();
        assert_eq!(
            pick_backend(BackendChoice::Fallback, Some(&n), None),
            Err(SelectionError::Unavailable(BackendChoice::Fallback))
        );
    }

    #[test]
    fn forced_null_ignores_discovery() {
        let n = neural();
        let p = piper();
        assert_eq!(
            pick_backend(BackendChoice::Null, Some(&n), Some(&p)),
            Ok(BackendPick::Null)
        );
    }

    #[test]
    fn unknown_choice_error_lists_expected_values() {
        let err = SelectionError::UnknownChoice("espeak".to_string());
        let text = err.to_string();
        assert!(text.contains("espeak"));
        assert!(text.contains(BACKEND_CHOICES));
    }

    #[test]
    fn unavailable_error_names_the_forced_backend() {
        let text = SelectionError::Unavailable(BackendChoice::Fallback).to_string();
        assert!(text.contains("fallback"));
        assert!(text.contains("not available"));
    }

    #[test]
    fn unknown_choice_exits_two_and_unavailable_exits_one() {
        assert_eq!(
            SelectionError::UnknownChoice("x".to_string()).exit_code(),
            2
        );
        assert_eq!(
            SelectionError::Unavailable(BackendChoice::Neural).exit_code(),
            1
        );
    }

    #[test]
    fn backend_choice_defaults_to_auto_without_env() {
        assert_eq!(backend_choice(None), Ok(BackendChoice::Auto));
    }

    #[test]
    fn backend_choice_reads_override() {
        assert_eq!(
            backend_choice(Some(OsStr::new("fallback"))),
            Ok(BackendChoice::Fallback)
        );
    }

    #[test]
    fn backend_choice_rejects_unknown_override() {
        assert_eq!(
            backend_choice(Some(OsStr::new("espeak"))),
            Err(SelectionError::UnknownChoice("espeak".to_string()))
        );
    }

    #[test]
    fn read_backend_choice_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_BACKEND");
        assert_eq!(read_backend_choice_override(), expected);
    }

    #[test]
    fn select_timeout_defaults_without_override() {
        assert_eq!(select_timeout(None), Ok(Timeout::default_timeout()));
    }

    #[test]
    fn select_timeout_reads_override() {
        assert_eq!(
            select_timeout(Some(OsStr::new("30"))).map(Timeout::seconds),
            Ok(30)
        );
    }

    #[test]
    fn select_timeout_rejects_bad_values() {
        let err = select_timeout(Some(OsStr::new("soon"))).unwrap_err();
        assert_eq!(err, SelectionError::BadTimeout("soon".to_string()));
        assert_eq!(err.exit_code(), 2);
        let text = err.to_string();
        assert!(text.contains("soon"));
        assert!(text.contains("86400"));
    }

    #[test]
    fn read_timeout_override_reflects_process_env() {
        assert_eq!(read_timeout_override(), std::env::var_os("VOZ_TIMEOUT_SECS"));
    }

    #[test]
    fn read_neural_bin_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_NEURAL_BIN").map(PathBuf::from);
        assert_eq!(read_neural_bin_override(), expected);
    }

    #[test]
    fn default_root_prefers_xdg_data_home() {
        assert_eq!(
            default_root(Some(OsStr::new("/x/data")), Some(OsStr::new("/home/a"))),
            PathBuf::from("/x/data/voz")
        );
    }

    #[test]
    fn default_root_falls_back_to_home_share() {
        assert_eq!(
            default_root(None, Some(OsStr::new("/home/a"))),
            PathBuf::from("/home/a/.local/share/voz")
        );
    }

    #[test]
    fn default_root_ignores_empty_xdg() {
        assert_eq!(
            default_root(Some(OsStr::new("")), Some(OsStr::new("/home/a"))),
            PathBuf::from("/home/a/.local/share/voz")
        );
    }

    #[test]
    fn default_root_without_home_uses_system_share() {
        assert_eq!(
            default_root(None, None),
            PathBuf::from("/usr/local/share/voz")
        );
    }

    #[test]
    fn default_root_carries_no_personal_path() {
        for r in [
            default_root(Some(OsStr::new("/x")), None),
            default_root(None, Some(OsStr::new("/home/a"))),
            default_root(None, None),
        ] {
            assert!(!r.to_string_lossy().contains("/user/modelz"));
        }
    }
}
