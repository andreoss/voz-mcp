use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::tts::{Language, LanguageError, Pitch, PitchError, Rate, RateError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Mcp,
    Help,
    Version,
    Voices(VoicesArgs),
    Audio(AudioArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoicesArgs {
    pub cmd: VoicesCmd,
    pub lang: Option<String>,
    pub all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoicesCmd {
    List,
    Fetch,
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
    UnknownVoicesCommand(String),
    MissingVoicesScope,
}

pub fn usage() -> String {
    format!(
        "voz <text> [--lang {}] [--rate {}..{}] [--pitch {}..{}] [--out path]\n\
         voz mcp                stdio MCP server (speak, readback)\n\
         voz voices list        catalogued voices and which are installed\n\
         voz voices fetch --lang es | --all\n\
         \n\
         voz --help | -h        this message\n\
         voz --version | -V     version\n\
         \n\
         env: VOZ_OUT_DIR output dir; VOZ_BACKEND {}; VOZ_NEURAL_ROOT data\n\
         root; VOZ_NEURAL_BIN, VOZ_PIPER_BIN engine paths; VOZ_PIPER_VOICES\n\
         voice dir; VOZ_CONFIG config file; VOZ_AUTO_FETCH 0|1; VOZ_TIMEOUT_SECS\n\
         synthesis budget in seconds",
        LANGUAGES,
        Rate::MIN,
        Rate::MAX,
        Pitch::MIN,
        Pitch::MAX,
        crate::backend::BACKEND_CHOICES
    )
}

pub fn version_line() -> String {
    format!("voz {}", env!("CARGO_PKG_VERSION"))
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingText => write!(f, "text is required\n{}", usage()),
            ParseError::UnknownFlag(flag) => write!(f, "unknown flag: {flag}"),
            ParseError::MissingValue(flag) => write!(f, "missing value for {flag}"),
            ParseError::Language(_) => {
                write!(f, "unsupported language, expected {LANGUAGES}")
            }
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
            ParseError::UnknownVoicesCommand(cmd) => {
                write!(f, "unknown voices command: {cmd}, expected list|fetch")
            }
            ParseError::MissingVoicesScope => {
                write!(f, "voices fetch needs --lang <code> or --all")
            }
        }
    }
}

pub const LANGUAGES: &str = "ru|en|es|de|fr|it|pt|zh|ja|ko";

pub fn parse<I>(args: I) -> Result<Mode, ParseError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err(ParseError::MissingText);
    };
    let first = first.to_string_lossy().into_owned();
    if first == "--help" || first == "-h" {
        return Ok(Mode::Help);
    }
    if first == "--version" || first == "-V" {
        return Ok(Mode::Version);
    }
    if first == "mcp" {
        return match args.next() {
            None => Ok(Mode::Mcp),
            Some(extra) => Err(ParseError::UnknownFlag(extra.to_string_lossy().into_owned())),
        };
    }
    if first == "voices" {
        return parse_voices(args);
    }
    if first.starts_with("--") {
        return Err(ParseError::MissingText);
    }
    parse_audio(first, args)
}

fn parse_voices(mut args: impl Iterator<Item = OsString>) -> Result<Mode, ParseError> {
    let cmd = match args.next() {
        None => VoicesCmd::List,
        Some(word) => match word.to_string_lossy().as_ref() {
            "list" => VoicesCmd::List,
            "fetch" => VoicesCmd::Fetch,
            other => return Err(ParseError::UnknownVoicesCommand(other.to_string())),
        },
    };
    let mut lang = None;
    let mut all = false;
    while let Some(flag) = args.next() {
        let flag = flag.to_string_lossy().into_owned();
        match flag.as_str() {
            "--lang" => {
                let v = args.next().ok_or(ParseError::MissingValue("--lang"))?;
                lang = Some(v.to_string_lossy().into_owned());
            }
            "--all" => all = true,
            _ => return Err(ParseError::UnknownFlag(flag)),
        }
    }
    if lang.is_none() && !all && cmd == VoicesCmd::Fetch {
        return Err(ParseError::MissingVoicesScope);
    }
    Ok(Mode::Voices(VoicesArgs { cmd, lang, all }))
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
    fn voices_list_parses_without_a_command() {
        assert_eq!(
            parse(os(&["voices"])),
            Ok(Mode::Voices(VoicesArgs {
                cmd: VoicesCmd::List,
                lang: None,
                all: false
            }))
        );
        assert_eq!(
            parse(os(&["voices", "list"])),
            Ok(Mode::Voices(VoicesArgs {
                cmd: VoicesCmd::List,
                lang: None,
                all: false
            }))
        );
    }

    #[test]
    fn voices_fetch_requires_a_scope() {
        assert_eq!(
            parse(os(&["voices", "fetch"])),
            Err(ParseError::MissingVoicesScope)
        );
    }

    #[test]
    fn voices_fetch_accepts_a_language_or_all() {
        assert_eq!(
            parse(os(&["voices", "fetch", "--lang", "es"])),
            Ok(Mode::Voices(VoicesArgs {
                cmd: VoicesCmd::Fetch,
                lang: Some("es".to_string()),
                all: false
            }))
        );
        assert_eq!(
            parse(os(&["voices", "fetch", "--all"])),
            Ok(Mode::Voices(VoicesArgs {
                cmd: VoicesCmd::Fetch,
                lang: None,
                all: true
            }))
        );
    }

    #[test]
    fn voices_rejects_an_unknown_command() {
        assert_eq!(
            parse(os(&["voices", "nuke"])),
            Err(ParseError::UnknownVoicesCommand("nuke".to_string()))
        );
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
            other => panic!("expected audio mode, got {other:?}"),
        }
    }

    #[test]
    fn every_language_flag_is_applied() {
        for (flag, lang) in [
            ("ru", Language::Russian),
            ("en", Language::English),
            ("es", Language::Spanish),
            ("de", Language::German),
            ("fr", Language::French),
            ("it", Language::Italian),
            ("pt", Language::Portuguese),
            ("zh", Language::Chinese),
            ("ja", Language::Japanese),
            ("ko", Language::Korean),
        ] {
            let mode = parse(os(&["x", "--lang", flag])).expect("ok");
            match mode {
                Mode::Audio(a) => assert_eq!(a.lang, lang, "flag {flag}"),
                other => panic!("expected audio mode, got {other:?}"),
            }
        }
    }

    #[test]
    fn rate_flag_is_applied() {
        let mode = parse(os(&["hi", "--rate", "150"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.rate.map(Rate::value), Some(150)),
            other => panic!("expected audio mode, got {other:?}"),
        }
    }

    #[test]
    fn pitch_flag_is_applied() {
        let mode = parse(os(&["hi", "--pitch", "60"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.pitch.map(Pitch::value), Some(60)),
            other => panic!("expected audio mode, got {other:?}"),
        }
    }

    #[test]
    fn out_flag_is_applied() {
        let mode = parse(os(&["hi", "--out", "/tmp/out.wav"])).expect("ok");
        match mode {
            Mode::Audio(a) => assert_eq!(a.out, Some(PathBuf::from("/tmp/out.wav"))),
            other => panic!("expected audio mode, got {other:?}"),
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
            parse(os(&["hi", "--lang", "zz"])),
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
            .contains("ru|en|es|de|fr|it|pt|zh|ja|ko"));
        assert!(ParseError::Rate(RateError::OutOfRange)
            .to_string()
            .contains("50"));
        assert!(ParseError::Pitch(PitchError::OutOfRange)
            .to_string()
            .contains("99"));
    }

    #[test]
    fn parses_help_flags() {
        assert_eq!(parse(os(&["--help"])), Ok(Mode::Help));
        assert_eq!(parse(os(&["-h"])), Ok(Mode::Help));
    }

    #[test]
    fn parses_version_flags() {
        assert_eq!(parse(os(&["--version"])), Ok(Mode::Version));
        assert_eq!(parse(os(&["-V"])), Ok(Mode::Version));
    }

    #[test]
    fn help_wins_over_trailing_arguments() {
        assert_eq!(parse(os(&["--help", "--lang", "en"])), Ok(Mode::Help));
    }

    #[test]
    fn usage_names_every_flag_and_language() {
        let u = usage();
        for token in ["--lang", "--rate", "--pitch", "--out", "mcp", "--help", "--version"] {
            assert!(u.contains(token), "usage missing {token}: {u}");
        }
        assert!(u.contains("ru|en|es|de|fr|it|pt|zh|ja|ko"));
    }

    #[test]
    fn usage_lists_the_environment_surface() {
        let u = usage();
        for var in [
            "VOZ_OUT_DIR",
            "VOZ_BACKEND",
            "VOZ_NEURAL_ROOT",
            "VOZ_NEURAL_BIN",
            "VOZ_PIPER_BIN",
            "VOZ_TIMEOUT_SECS",
        ] {
            assert!(u.contains(var), "usage missing {var}");
        }
    }

    #[test]
    fn version_line_carries_the_package_version() {
        let v = version_line();
        assert!(v.starts_with("voz "));
        assert!(v.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn missing_text_error_shows_usage() {
        let err = parse(os(&[])).unwrap_err();
        assert_eq!(err, ParseError::MissingText);
        assert!(err.to_string().contains("--lang"));
    }
}
