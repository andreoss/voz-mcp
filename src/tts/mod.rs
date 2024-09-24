pub mod null;
pub mod qwen;

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Russian,
    English,
    Spanish,
    German,
    French,
    Italian,
    Portuguese,
    Chinese,
    Japanese,
    Korean,
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
            "de" => Ok(Language::German),
            "fr" => Ok(Language::French),
            "it" => Ok(Language::Italian),
            "pt" => Ok(Language::Portuguese),
            "zh" => Ok(Language::Chinese),
            "ja" => Ok(Language::Japanese),
            "ko" => Ok(Language::Korean),
            _ => Err(LanguageError::Unknown),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::Russian => "ru",
            Language::English => "en",
            Language::Spanish => "es",
            Language::German => "de",
            Language::French => "fr",
            Language::Italian => "it",
            Language::Portuguese => "pt",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Korean => "ko",
        }
    }

    pub fn voice(self) -> &'static str {
        match self {
            Language::English => "en-us",
            _ => self.code(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pitch(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchError {
    OutOfRange,
}

impl Pitch {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 99;

    pub fn parse(n: u32) -> Result<Pitch, PitchError> {
        if (Self::MIN as u32..=Self::MAX as u32).contains(&n) {
            Ok(Pitch(n as u8))
        } else {
            Err(PitchError::OutOfRange)
        }
    }

    pub fn value(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakRequest {
    pub text: String,
    pub lang: Language,
    pub rate: Option<Rate>,
    pub pitch: Option<Pitch>,
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

pub(crate) fn spawn_feed_with_retry(
    cmd: &mut std::process::Command,
    input: &[u8],
) -> std::io::Result<std::process::Output> {
    retry_on_etxtbsy(|| feed_child(cmd, input), std::thread::sleep)
}

fn feed_child(
    cmd: &mut std::process::Command,
    input: &[u8],
) -> std::io::Result<std::process::Output> {
    use std::io::Write;
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    child.wait_with_output()
}

fn retry_on_etxtbsy<F>(
    mut attempt: F,
    mut sleep: impl FnMut(std::time::Duration),
) -> std::io::Result<std::process::Output>
where
    F: FnMut() -> std::io::Result<std::process::Output>,
{
    for _ in 0..19 {
        match attempt() {
            Err(e) if e.raw_os_error() == Some(26) => {
                sleep(std::time::Duration::from_millis(5));
            }
            result => return result,
        }
    }
    attempt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_languages() {
        assert_eq!(Language::parse("ru"), Ok(Language::Russian));
        assert_eq!(Language::parse("en"), Ok(Language::English));
        assert_eq!(Language::parse("es"), Ok(Language::Spanish));
        assert_eq!(Language::parse("de"), Ok(Language::German));
        assert_eq!(Language::parse("fr"), Ok(Language::French));
        assert_eq!(Language::parse("it"), Ok(Language::Italian));
        assert_eq!(Language::parse("pt"), Ok(Language::Portuguese));
        assert_eq!(Language::parse("zh"), Ok(Language::Chinese));
        assert_eq!(Language::parse("ja"), Ok(Language::Japanese));
        assert_eq!(Language::parse("ko"), Ok(Language::Korean));
    }

    #[test]
    fn rejects_unknown_language() {
        assert_eq!(Language::parse("zz"), Err(LanguageError::Unknown));
        assert_eq!(Language::parse(""), Err(LanguageError::Unknown));
        assert_eq!(Language::parse("EN"), Err(LanguageError::Unknown));
    }

    #[test]
    fn code_roundtrips_all_languages() {
        for code in ["ru", "en", "es", "de", "fr", "it", "pt", "zh", "ja", "ko"] {
            assert_eq!(Language::parse(code).map(Language::code), Ok(code));
        }
    }

    #[test]
    fn voice_maps_language() {
        assert_eq!(Language::Russian.voice(), "ru");
        assert_eq!(Language::English.voice(), "en-us");
        assert_eq!(Language::Spanish.voice(), "es");
        assert_eq!(Language::German.voice(), "de");
        assert_eq!(Language::French.voice(), "fr");
        assert_eq!(Language::Italian.voice(), "it");
        assert_eq!(Language::Portuguese.voice(), "pt");
        assert_eq!(Language::Chinese.voice(), "zh");
        assert_eq!(Language::Japanese.voice(), "ja");
        assert_eq!(Language::Korean.voice(), "ko");
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

    #[test]
    fn parses_pitch_within_bounds() {
        assert_eq!(Pitch::parse(0).map(Pitch::value), Ok(0));
        assert_eq!(Pitch::parse(99).map(Pitch::value), Ok(99));
        assert_eq!(Pitch::parse(60).map(Pitch::value), Ok(60));
    }

    #[test]
    fn rejects_pitch_out_of_bounds() {
        assert_eq!(Pitch::parse(100), Err(PitchError::OutOfRange));
        assert_eq!(Pitch::parse(9999), Err(PitchError::OutOfRange));
    }

    fn etxtbsy() -> std::io::Error {
        std::io::Error::from_raw_os_error(26)
    }

    fn output_ok() -> std::io::Result<std::process::Output> {
        Ok(std::process::Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }

    #[test]
    fn retries_and_sleeps_on_etxtbsy_then_succeeds() {
        let mut attempts = 0;
        let mut sleeps = 0;
        let result = retry_on_etxtbsy(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(etxtbsy())
                } else {
                    output_ok()
                }
            },
            |_| sleeps += 1,
        );
        assert!(result.is_ok());
        assert_eq!(attempts, 3);
        assert_eq!(sleeps, 2);
    }

    #[test]
    fn gives_up_after_exhausting_retries() {
        let mut attempts = 0;
        let mut sleeps = 0;
        let result = retry_on_etxtbsy(
            || {
                attempts += 1;
                Err(etxtbsy())
            },
            |_| sleeps += 1,
        );
        assert_eq!(result.unwrap_err().raw_os_error(), Some(26));
        assert_eq!(attempts, 20);
        assert_eq!(sleeps, 19);
    }
}
