use std::ffi::OsString;
use std::path::PathBuf;

use super::{Readback, ReadbackError, Recording};

pub struct FsReadback {
    out_dir: PathBuf,
}

impl FsReadback {
    pub fn new(out_dir: impl Into<PathBuf>) -> Self {
        Self {
            out_dir: out_dir.into(),
        }
    }
}

trait DirEntryLike {
    fn path(&self) -> PathBuf;
    fn file_name(&self) -> OsString;
    fn metadata(&self) -> std::io::Result<std::fs::Metadata>;
}

impl DirEntryLike for std::fs::DirEntry {
    fn path(&self) -> PathBuf {
        std::fs::DirEntry::path(self)
    }

    fn file_name(&self) -> OsString {
        std::fs::DirEntry::file_name(self)
    }

    fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        std::fs::DirEntry::metadata(self)
    }
}

fn collect_recordings<I, E>(entries: I) -> Result<Vec<Recording>, ReadbackError>
where
    I: Iterator<Item = std::io::Result<E>>,
    E: DirEntryLike,
{
    let mut items = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| ReadbackError {
            reason: format!("failed to read directory entry: {e}"),
        })?;
        let path = entry.path();
        let is_wav = path.extension().and_then(|e| e.to_str()) == Some("wav");
        if !is_wav { continue; }
        let meta = entry.metadata().map_err(|e| ReadbackError {
            reason: format!("failed to read metadata: {e}"),
        })?;
        let is_file = meta.is_file();
        if !is_file { continue; }
        items.push(Recording {
            name: entry.file_name().to_string_lossy().into_owned(),
            path,
            bytes: meta.len(),
        });
    }
    Ok(items)
}

impl Readback for FsReadback {
    fn list(&self) -> Result<Vec<Recording>, ReadbackError> {
        let entries = match std::fs::read_dir(&self.out_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(ReadbackError {
                    reason: format!("failed to read output directory: {e}"),
                });
            }
        };
        collect_recordings(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct FakeEntry {
        path: PathBuf,
        metadata: io::Result<std::fs::Metadata>,
    }

    impl DirEntryLike for FakeEntry {
        fn path(&self) -> PathBuf {
            self.path.clone()
        }

        fn file_name(&self) -> OsString {
            self.path.file_name().expect("file name").to_owned()
        }

        fn metadata(&self) -> io::Result<std::fs::Metadata> {
            match &self.metadata {
                Ok(m) => Ok(m.clone()),
                Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
            }
        }
    }

    #[test]
    fn entry_read_error_surfaces_as_readback_error() {
        let entries: Vec<io::Result<FakeEntry>> = vec![Err(io::Error::other("boom"))];
        let err = collect_recordings(entries.into_iter()).unwrap_err();
        assert!(err.reason.contains("failed to read directory entry"));
    }

    #[test]
    fn entry_metadata_error_surfaces_as_readback_error() {
        let entries: Vec<io::Result<FakeEntry>> = vec![Ok(FakeEntry {
            path: PathBuf::from("speech.wav"),
            metadata: Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
        })];
        let err = collect_recordings(entries.into_iter()).unwrap_err();
        assert!(err.reason.contains("failed to read metadata"));
    }

    #[test]
    fn non_wav_entry_is_skipped() {
        let dir = std::env::temp_dir().join(format!("voz-fs-nonwav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let meta = std::fs::metadata(&dir).expect("meta");
        let entries: Vec<io::Result<FakeEntry>> = vec![Ok(FakeEntry {
            path: PathBuf::from("notes.txt"),
            metadata: Ok(meta),
        })];
        let items = collect_recordings(entries.into_iter()).expect("list");
        std::fs::remove_dir_all(&dir).ok();
        assert!(items.is_empty());
    }

    #[test]
    fn non_file_wav_entry_is_skipped() {
        let dir = std::env::temp_dir().join(format!("voz-fs-nonfile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let meta = std::fs::metadata(&dir).expect("meta");
        let entries: Vec<io::Result<FakeEntry>> = vec![Ok(FakeEntry {
            path: PathBuf::from("speech.wav"),
            metadata: Ok(meta),
        })];
        let items = collect_recordings(entries.into_iter()).expect("list");
        std::fs::remove_dir_all(&dir).ok();
        assert!(items.is_empty());
    }
}
