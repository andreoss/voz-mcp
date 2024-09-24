pub mod fs;

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadbackError {
    pub reason: String,
}

pub trait Readback: Send + Sync {
    fn list(&self) -> Result<Vec<Recording>, ReadbackError>;
}

#[cfg(test)]
mod tests {
    use super::fs::FsReadback;
    use super::*;

    #[test]
    fn lists_nothing_for_empty_dir() {
        let dir = std::env::temp_dir().join(format!("voz-readback-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let rb = FsReadback::new(&dir);
        let items = rb.list().expect("list");
        std::fs::remove_dir_all(&dir).ok();
        assert!(items.is_empty());
    }

    #[test]
    fn lists_wav_files_and_skips_other_files() {
        let dir =
            std::env::temp_dir().join(format!("voz-readback-populated-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("a.wav"), b"RIFF....").expect("write a");
        std::fs::write(dir.join("b.wav"), b"RIFF..").expect("write b");
        std::fs::write(dir.join("notes.txt"), b"ignore me").expect("write txt");
        let rb = FsReadback::new(&dir);
        let mut items = rb.list().expect("list");
        items.sort_by(|a, b| a.name.cmp(&b.name));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "a.wav");
        assert_eq!(items[0].bytes, 8);
        assert_eq!(items[1].name, "b.wav");
        assert_eq!(items[1].bytes, 6);
    }

    #[test]
    fn missing_dir_yields_empty_list() {
        let dir =
            std::env::temp_dir().join(format!("voz-readback-missing-{}", std::process::id()));
        let rb = FsReadback::new(&dir);
        let items = rb.list().expect("list");
        assert!(items.is_empty());
    }

    #[test]
    fn out_dir_pointing_at_a_file_is_an_error() {
        let path = std::env::temp_dir().join(format!("voz-readback-notdir-{}", std::process::id()));
        std::fs::write(&path, b"not a directory").expect("write");
        let rb = FsReadback::new(&path);
        let err = rb.list().unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(err.reason.contains("failed to read output directory"));
    }
}
