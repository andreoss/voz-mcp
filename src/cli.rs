use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::tts::{Language, LanguageError, Pitch, PitchError, Rate, RateError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Mcp,
    Audio(AudioArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioArgs {
    pub text: String,
    pub lang: Language,
    pub rate: Option<Rate>,
    pub pitch: Option<Pitch>,
    pub out: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MissingText,
    UnknownFlag(String),
    MissingValue(&'static str),
    Language(LanguageError),
    Rate(RateError),
    Pitch(PitchError),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingText => write!(
                f,
                "text is required: voz <text> [--lang ru|en|es] [--rate {}..{}] [--pitch {}..{}] [--out path]",
                Rate::MIN,
                Rate::MAX,
                Pitch::MIN,
                Pitch::MAX
            ),
            ParseError::UnknownFlag(flag) => write!(f, "unknown flag: {flag}"),
            ParseError::MissingValue(flag) => write!(f, "missing value for {flag}"),
            ParseError::Language(_) => write!(f, "unsupported language, expected ru|en|es"),
            ParseError::Rate(_) => write!(
                f,
                "rate must be an integer between {} and {}",
                Rate::MIN,
                Rate::MAX
            ),
            ParseError::Pitch(_) => write!(
                f,
                "pitch must be an integer between {} and {}",
                Pitch::MIN,
                Pitch::MAX
            ),
        }
    }
}

pub fn parse<I>(args: I) -> Result<Mode, ParseError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err(ParseError::MissingText);
    };
    let first = first.to_string_lossy().into_owned();
    if first == "mcp" {
        return match args.next() {
            None => Ok(Mode::Mcp),
            Some(extra) => Err(ParseError::UnknownFlag(extra.to_string_lossy().into_owned())),
        };
    }
    if first.starts_with("--") {
        return Err(ParseError::MissingText);
    }
    parse_audio(first, args)
}

fn parse_audio(text: String, mut args: impl Iterator<Item = OsString>) -> Result<Mode, ParseError> {
    let mut lang = Language::English;
    let mut rate = None;
    let mut pitch = None;
    let mut out = None;
    while let Some(flag) = args.next() {
        let flag = flag.to_string_lossy().into_owned();
        match flag.as_str() {
            "--lang" => {
                let v = args.next().ok_or(ParseError::MissingValue("--lang"))?;
                lang = Language::parse(&v.to_string_lossy()).map_err(ParseError::Language)?;
            }
            "--rate" => {
                let v = args.next().ok_or(ParseError::MissingValue("--rate"))?;
                let n: u32 = v
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| ParseError::Rate(RateError::OutOfRange))?;
                rate = Some(Rate::parse(n).map_err(ParseError::Rate)?);
            }
            "--pitch" => {
                let v = args.next().ok_or(ParseError::MissingValue("--pitch"))?;
                let n: u32 = v
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| ParseError::Pitch(PitchError::OutOfRange))?;
                pitch = Some(Pitch::parse(n).map_err(ParseError::Pitch)?);
            }
            "--out" => {
                let v = args.next().ok_or(ParseError::MissingValue("--out"))?;
                out = Some(PathBuf::from(v));
            }
            _ => return Err(ParseError::UnknownFlag(flag)),
        }
    }
    Ok(Mode::Audio(AudioArgs {
        text,
        lang,
        rate,
        pitch,
        out,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn mcp_mode_selected() {
        assert_eq!(parse(os(&["mcp"])), Ok(Mode::Mcp));
    }

    #[test]
    fn mcp_mode_rejects_extra_positional() {
        let err = parse(os(&["mcp", "extra"])).unwrap_err();
        assert_eq!(err, ParseError::UnknownFlag("extra".to_string()));
    }

    #[test]
    fn text_mode_uses_defaults() {
        let mode = parse(os(&["hello"])).expect("ok");
        assert_eq!(
            mode,
            Mode::Audio(AudioArgs {
                text: "hello".to_string(),
                lang: Language::English,
                rate: None,
                pitch: None,
                out: None,
            })
        );
    }

    #[test]
    fn lang_flag_is_applied() {
        let mode = parse(os(&["hola", "--lang", "es"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.lang, Language::Spanish),
            Mode::Mcp => panic!("expected audio mode"),
        }
    }

    #[test]
    fn rate_flag_is_applied() {
        let mode = parse(os(&["hi", "--rate", "150"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.rate.map(Rate::value), Some(150)),
            Mode::Mcp => panic!("expected audio mode"),
        }
    }

    #[test]
    fn pitch_flag_is_applied() {
        let mode = parse(os(&["hi", "--pitch", "60"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.pitch.map(Pitch::value), Some(60)),
            Mode::Mcp => panic!("expected audio mode"),
        }
    }

    #[test]
    fn out_flag_is_applied() {
        let mode = parse(os(&["hi", "--out", "/tmp/out.wav"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.out, Some(PathBuf::from("/tmp/out.wav"))),
            Mode::Mcp => panic!("expected audio mode"),
        }
    }

    #[test]
    fn all_flags_combine() {
        let mode = parse(os(&[
            "hi", "--lang", "ru", "--rate", "200", "--pitch", "40", "--out", "/tmp/x.wav",
        ]))
        .expect("ok");
        assert_eq!(
            mode,
            Mode::Audio(AudioArgs {
                text: "hi".to_string(),
                lang: Language::Russian,
                rate: Some(Rate::parse(200).unwrap()),
                pitch: Some(Pitch::parse(40).unwrap()),
                out: Some(PathBuf::from("/tmp/x.wav")),
            })
        );
    }

    #[test]
    fn missing_text_when_no_args() {
        assert_eq!(parse(os(&[])), Err(ParseError::MissingText));
    }

    #[test]
    fn missing_text_when_first_arg_is_a_flag() {
        assert_eq!(parse(os(&["--lang", "en"])), Err(ParseError::MissingText));
    }

    #[test]
    fn unknown_flag_is_rejected() {
        let err = parse(os(&["hi", "--bogus"])).unwrap_err();
        assert_eq!(err, ParseError::UnknownFlag("--bogus".to_string()));
    }

    #[test]
    fn missing_value_for_lang_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--lang"])),
            Err(ParseError::MissingValue("--lang"))
        );
    }

    #[test]
    fn missing_value_for_rate_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--rate"])),
            Err(ParseError::MissingValue("--rate"))
        );
    }

    #[test]
    fn missing_value_for_pitch_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--pitch"])),
            Err(ParseError::MissingValue("--pitch"))
        );
    }

    #[test]
    fn missing_value_for_out_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--out"])),
            Err(ParseError::MissingValue("--out"))
        );
    }

    #[test]
    fn invalid_lang_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--lang", "fr"])),
            Err(ParseError::Language(LanguageError::Unknown))
        );
    }

    #[test]
    fn rate_below_minimum_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--rate", "0"])),
            Err(ParseError::Rate(RateError::OutOfRange))
        );
    }

    #[test]
    fn rate_above_maximum_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--rate", "501"])),
            Err(ParseError::Rate(RateError::OutOfRange))
        );
    }

    #[test]
    fn non_numeric_rate_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--rate", "abc"])),
            Err(ParseError::Rate(RateError::OutOfRange))
        );
    }

    #[test]
    fn pitch_above_maximum_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--pitch", "100"])),
            Err(ParseError::Pitch(PitchError::OutOfRange))
        );
    }

    #[test]
    fn non_numeric_pitch_is_rejected() {
        assert_eq!(
            parse(os(&["hi", "--pitch", "abc"])),
            Err(ParseError::Pitch(PitchError::OutOfRange))
        );
    }

    #[test]
    fn extra_positional_after_text_is_rejected() {
        let err = parse(os(&["hi", "there"])).unwrap_err();
        assert_eq!(err, ParseError::UnknownFlag("there".to_string()));
    }

    #[test]
    fn display_messages_are_human_readable() {
        assert!(ParseError::MissingText.to_string().contains("voz <text>"));
        assert!(ParseError::UnknownFlag("--x".to_string())
            .to_string()
            .contains("--x"));
        assert!(ParseError::MissingValue("--lang")
            .to_string()
            .contains("--lang"));
        assert!(ParseError::Language(LanguageError::Unknown)
            .to_string()
            .contains("ru|en|es"));
        assert!(ParseError::Rate(RateError::OutOfRange)
            .to_string()
            .contains("50"));
        assert!(ParseError::Pitch(PitchError::OutOfRange)
            .to_string()
            .contains("99"));
    }
}
