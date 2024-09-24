use std::path::{Path, PathBuf};

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

pub fn pick_backend(
    neural_discovered: Option<&NeuralPaths>,
    piper_discovered: Option<&PiperPaths>,
) -> BackendPick {
    match (neural_discovered, piper_discovered) {
        (Some(n), _) => BackendPick::Neural {
            bin: n.bin.clone(),
            talker: n.talker.clone(),
            codec: n.codec.clone(),
        },
        (None, Some(p)) => BackendPick::Piper {
            bin: p.bin.clone(),
            voices: p.voices.clone(),
        },
        (None, None) => BackendPick::Null,
    }
}

pub fn read_neural_root_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_NEURAL_ROOT").map(PathBuf::from)
}

pub fn read_piper_bin_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_PIPER_BIN").map(PathBuf::from)
}

pub fn neural_root(env_override: Option<&Path>) -> PathBuf {
    match env_override {
        Some(p) if p.exists() => p.to_path_buf(),
        _ => PathBuf::from("/user/modelz"),
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
    fn discovered_neural_yields_neural_pick() {
        let n = neural();
        let pick = pick_backend(Some(&n), None);
        assert_eq!(
            pick,
            BackendPick::Neural {
                bin: n.bin,
                talker: n.talker,
                codec: n.codec,
            }
        );
    }

    #[test]
    fn neural_wins_over_piper() {
        let n = neural();
        let p = piper();
        assert!(matches!(
            pick_backend(Some(&n), Some(&p)),
            BackendPick::Neural { .. }
        ));
    }

    #[test]
    fn piper_picked_when_neural_absent() {
        let p = piper();
        assert_eq!(
            pick_backend(None, Some(&p)),
            BackendPick::Piper {
                bin: p.bin,
                voices: p.voices,
            }
        );
    }

    #[test]
    fn nothing_discovered_yields_null() {
        assert_eq!(pick_backend(None, None), BackendPick::Null);
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

    #[test]
    fn neural_root_ignores_missing_override() {
        let missing = PathBuf::from("/definitely/not/a/real/modelz-root");
        assert_eq!(neural_root(Some(&missing)), PathBuf::from("/user/modelz"));
    }

    #[test]
    fn neural_root_defaults_without_override() {
        assert_eq!(neural_root(None), PathBuf::from("/user/modelz"));
    }
}