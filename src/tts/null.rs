use super::{SpeakRequest, Speech, Tts, TtsError};
use std::path::PathBuf;

pub struct Null;

impl Tts for Null {
    fn speak(&self, _req: &SpeakRequest) -> Result<Speech, TtsError> {
        Ok(Speech {
            path: PathBuf::from("/dev/null"),
        })
    }
}
