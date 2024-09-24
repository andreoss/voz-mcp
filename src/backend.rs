use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeuralPaths {
    pub bin: PathBuf,
    pub talker: PathBuf,
    pub codec: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendPick {
    Espeak(PathBuf),
    Neural {
        bin: PathBuf,
        talker: PathBuf,
        codec: PathBuf,
    },
    Null,
}

pub fn pick_backend(
    espeak_override: Option<&Path>,
    espeak_discovered: Option<&Path>,
    neural_discovered: Option<&NeuralPaths>,
) -> BackendPick {
    if let Some(p) = espeak_override
        && p.exists()
    {
        return BackendPick::Espeak(p.to_path_buf());
    }
    if let Some(n) = neural_discovered {
        return BackendPick::Neural {
            bin: n.bin.clone(),
            talker: n.talker.clone(),
            codec: n.codec.clone(),
        };
    }
    match espeak_discovered {
        Some(p) => BackendPick::Espeak(p.to_path_buf()),
        None => BackendPick::Null,
    }
}

pub fn read_env_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_ESPEAK_BIN").map(PathBuf::from)
}

pub fn read_neural_root_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_NEURAL_ROOT").map(PathBuf::from)
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

    #[test]
    fn env_override_wins_even_when_store_also_matches() {
        let dir = std::env::temp_dir().join(format!("voz-backend-override-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let override_bin = dir.join("override-espeak");
        std::fs::write(&override_bin, b"x").expect("write");
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let pick = pick_backend(Some(&override_bin), Some(&discovered), None);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(pick, BackendPick::Espeak(override_bin));
    }

    #[test]
    fn env_override_wins_even_when_neural_is_discovered() {
        let dir = std::env::temp_dir().join(format!("voz-backend-override2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let override_bin = dir.join("override-espeak");
        std::fs::write(&override_bin, b"x").expect("write");
        let n = neural();
        let pick = pick_backend(Some(&override_bin), None, Some(&n));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(pick, BackendPick::Espeak(override_bin));
    }

    #[test]
    fn neural_discovery_wins_over_discovered_espeak() {
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let n = neural();
        let pick = pick_backend(None, Some(&discovered), Some(&n));
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
    fn missing_override_falls_through_to_neural() {
        let missing = PathBuf::from("/definitely/not/a/real/espeak-ng-override");
        let n = neural();
        let pick = pick_backend(Some(&missing), None, Some(&n));
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
    fn missing_override_falls_through_to_discovered() {
        let missing = PathBuf::from("/definitely/not/a/real/espeak-ng-override");
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let pick = pick_backend(Some(&missing), Some(&discovered), None);
        assert_eq!(pick, BackendPick::Espeak(discovered));
    }

    #[test]
    fn no_override_uses_discovered_store_bin() {
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let pick = pick_backend(None, Some(&discovered), None);
        assert_eq!(pick, BackendPick::Espeak(discovered));
    }

    #[test]
    fn neither_override_nor_discovered_yields_null() {
        let pick = pick_backend(None, None, None);
        assert_eq!(pick, BackendPick::Null);
    }

    #[test]
    fn null_selected_when_scanning_finds_nothing() {
        let missing = PathBuf::from("/definitely/not/a/real/espeak-ng-override");
        let pick = pick_backend(Some(&missing), None, None);
        assert_eq!(pick, BackendPick::Null);
    }

    #[test]
    fn read_env_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_ESPEAK_BIN").map(PathBuf::from);
        assert_eq!(read_env_override(), expected);
    }

    #[test]
    fn read_neural_root_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_NEURAL_ROOT").map(PathBuf::from);
        assert_eq!(read_neural_root_override(), expected);
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
