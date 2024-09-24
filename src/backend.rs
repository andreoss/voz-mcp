use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendPick {
    Espeak(PathBuf),
    Null,
}

pub fn pick_backend(env_override: Option<&Path>, discovered: Option<&Path>) -> BackendPick {
    if let Some(p) = env_override
        && p.exists()
    {
        return BackendPick::Espeak(p.to_path_buf());
    }
    match discovered {
        Some(p) => BackendPick::Espeak(p.to_path_buf()),
        None => BackendPick::Null,
    }
}

pub fn read_env_override() -> Option<PathBuf> {
    std::env::var_os("VOZ_ESPEAK_BIN").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_wins_even_when_store_also_matches() {
        let dir = std::env::temp_dir().join(format!("voz-backend-override-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let override_bin = dir.join("override-espeak");
        std::fs::write(&override_bin, b"x").expect("write");
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let pick = pick_backend(Some(&override_bin), Some(&discovered));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(pick, BackendPick::Espeak(override_bin));
    }

    #[test]
    fn missing_override_falls_through_to_discovered() {
        let missing = PathBuf::from("/definitely/not/a/real/espeak-ng-override");
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let pick = pick_backend(Some(&missing), Some(&discovered));
        assert_eq!(pick, BackendPick::Espeak(discovered));
    }

    #[test]
    fn no_override_uses_discovered_store_bin() {
        let discovered = PathBuf::from("/made-up/store/espeak-ng-9.9.9/bin/espeak-ng");
        let pick = pick_backend(None, Some(&discovered));
        assert_eq!(pick, BackendPick::Espeak(discovered));
    }

    #[test]
    fn neither_override_nor_discovered_yields_null() {
        let pick = pick_backend(None, None);
        assert_eq!(pick, BackendPick::Null);
    }

    #[test]
    fn null_selected_when_scanning_finds_nothing() {
        let missing = PathBuf::from("/definitely/not/a/real/espeak-ng-override");
        let pick = pick_backend(Some(&missing), None);
        assert_eq!(pick, BackendPick::Null);
    }

    #[test]
    fn read_env_override_reflects_process_env() {
        let expected = std::env::var_os("VOZ_ESPEAK_BIN").map(PathBuf::from);
        assert_eq!(read_env_override(), expected);
    }
}
