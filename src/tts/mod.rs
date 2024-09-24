pub mod null;

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Russian,
    English,
    Spanish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageError {
    Unknown,
}

impl Language {
    pub fn parse(s: &str) -> Result<Language, LanguageError> {
        match s {
            "ru" => Ok(Language::Russian),
            "en" => Ok(Language::English),
            "es" => Ok(Language::Spanish),
            _ => Err(LanguageError::Unknown),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::Russian => "ru",
            Language::English => "en",
            Language::Spanish => "es",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakRequest {
    pub text: String,
    pub lang: Language,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speech {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsError {
    pub reason: String,
}

pub trait Tts: Send + Sync {
    fn speak(&self, req: &SpeakRequest) -> Result<Speech, TtsError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_languages() {
        assert_eq!(Language::parse("ru"), Ok(Language::Russian));
        assert_eq!(Language::parse("en"), Ok(Language::English));
        assert_eq!(Language::parse("es"), Ok(Language::Spanish));
    }

    #[test]
    fn rejects_unknown_language() {
        assert_eq!(Language::parse("fr"), Err(LanguageError::Unknown));
        assert_eq!(Language::parse(""), Err(LanguageError::Unknown));
    }

    #[test]
    fn code_roundtrips() {
        assert_eq!(Language::Russian.code(), "ru");
        assert_eq!(Language::English.code(), "en");
        assert_eq!(Language::Spanish.code(), "es");
    }
}
