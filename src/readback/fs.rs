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
        let mut items = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| ReadbackError {
                reason: format!("failed to read directory entry: {e}"),
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("wav") {
                continue;
            }
            let meta = entry.metadata().map_err(|e| ReadbackError {
                reason: format!("failed to read metadata: {e}"),
            })?;
            if !meta.is_file() {
                continue;
            }
            items.push(Recording {
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
                bytes: meta.len(),
            });
        }
        Ok(items)
    }
}
