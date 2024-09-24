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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::Language;

    #[test]
    fn always_reports_dev_null() {
        let speech = Null
            .speak(&SpeakRequest {
                text: "hi".to_string(),
                lang: Language::English,
                rate: None,
            })
            .expect("ok");
        assert_eq!(speech.path, PathBuf::from("/dev/null"));
    }
}
