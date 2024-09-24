pub mod null;
pub mod espeak;
pub mod flite;

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

    pub fn voice(self) -> &'static str {
        match self {
            Language::Russian => "ru",
            Language::English => "en-us",
            Language::Spanish => "es",
        }
    }

    pub fn flite_voice(self) -> &'static str {
        match self {
            Language::Russian => "kal16",
            Language::English => "kal16",
            Language::Spanish => "kal16",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateError {
    OutOfRange,
}

impl Rate {
    pub const MIN: u16 = 50;
    pub const MAX: u16 = 500;

    pub fn parse(n: u32) -> Result<Rate, RateError> {
        if (Self::MIN as u32..=Self::MAX as u32).contains(&n) {
            Ok(Rate(n as u16))
        } else {
            Err(RateError::OutOfRange)
        }
    }

    pub fn value(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakRequest {
    pub text: String,
    pub lang: Language,
    pub rate: Option<Rate>,
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

pub(crate) fn spawn_with_retry(
    cmd: &mut std::process::Command,
) -> std::io::Result<std::process::Output> {
    for _ in 0..19 {
        match cmd.output() {
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            result => return result,
        }
    }
    cmd.output()
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

    #[test]
    fn voice_maps_language() {
        assert_eq!(Language::Russian.voice(), "ru");
        assert_eq!(Language::English.voice(), "en-us");
        assert_eq!(Language::Spanish.voice(), "es");
    }

    #[test]
    fn flite_voice_falls_back_to_the_only_bundled_voice() {
        assert_eq!(Language::Russian.flite_voice(), "kal16");
        assert_eq!(Language::English.flite_voice(), "kal16");
        assert_eq!(Language::Spanish.flite_voice(), "kal16");
    }

    #[test]
    fn parses_rate_within_bounds() {
        assert_eq!(Rate::parse(50).map(Rate::value), Ok(50));
        assert_eq!(Rate::parse(500).map(Rate::value), Ok(500));
        assert_eq!(Rate::parse(120).map(Rate::value), Ok(120));
    }

    #[test]
    fn rejects_rate_out_of_bounds() {
        assert_eq!(Rate::parse(49), Err(RateError::OutOfRange));
        assert_eq!(Rate::parse(501), Err(RateError::OutOfRange));
        assert_eq!(Rate::parse(9999), Err(RateError::OutOfRange));
        assert_eq!(Rate::parse(0), Err(RateError::OutOfRange));
    }
}
