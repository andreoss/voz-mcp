use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    schemars, serde, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use crate::readback::Readback;
use crate::tts::{Language, Pitch, Rate, SpeakRequest, Tts};

pub struct Server {
    backend: Box<dyn Tts>,
    readback: Box<dyn Readback>,
    synthesis: std::sync::Mutex<()>,
}

impl Server {
    pub fn new(backend: Box<dyn Tts>, readback: Box<dyn Readback>) -> Self {
        Self {
            backend,
            readback,
            synthesis: std::sync::Mutex::new(()),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SpeakInput {
    pub text: String,
    pub lang: String,
    pub rate: Option<u32>,
    pub pitch: Option<u32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SpeakOutput {
    pub path: String,
    pub lang: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ReadbackItem {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ReadbackOutput {
    pub items: Vec<ReadbackItem>,
}

#[tool_router]
impl Server {
    #[tool(
        name = "speak",
        description = "Produce speech from the given text in the requested language"
    )]
    fn speak(
        &self,
        Parameters(input): Parameters<SpeakInput>,
    ) -> Result<Json<SpeakOutput>, rmcp::ErrorData> {
        let lang = Language::parse(&input.lang).map_err(|_| {
            rmcp::ErrorData::invalid_params(
                format!(
                    "unsupported language '{}', expected ru|en|es|de|fr|it|pt|zh|ja|ko",
                    input.lang
                ),
                None,
            )
        })?;
        let rate = input
            .rate
            .map(Rate::parse)
            .transpose()
            .map_err(|_| {
                rmcp::ErrorData::invalid_params(
                    format!(
                        "rate must be an integer between {} and {} wpm",
                        Rate::MIN,
                        Rate::MAX
                    ),
                    None,
                )
            })?;
        let pitch = input
            .pitch
            .map(Pitch::parse)
            .transpose()
            .map_err(|_| {
                rmcp::ErrorData::invalid_params(
                    format!(
                        "pitch must be an integer between {} and {}",
                        Pitch::MIN,
                        Pitch::MAX
                    ),
                    None,
                )
            })?;
        let _one_at_a_time = self
            .synthesis
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let speech = self
            .backend
            .speak(&SpeakRequest {
                text: input.text,
                lang,
                rate,
                pitch,
            })
            .map_err(|e| {
                rmcp::ErrorData::invalid_params(format!("speech synthesis failed: {}", e.reason), None)
            })?;
        Ok(Json(SpeakOutput {
            path: speech.path.to_string_lossy().into_owned(),
            lang: lang.code().to_string(),
        }))
    }

    #[tool(
        name = "readback",
        description = "List previously synthesized speech output files"
    )]
    fn readback(&self) -> Result<Json<ReadbackOutput>, rmcp::ErrorData> {
        let recordings = self.readback.list().map_err(|e| {
            rmcp::ErrorData::internal_error(
                format!("failed to list recordings: {}", e.reason),
                None,
            )
        })?;
        Ok(Json(ReadbackOutput {
            items: recordings
                .into_iter()
                .map(|r| ReadbackItem {
                    name: r.name,
                    path: r.path.to_string_lossy().into_owned(),
                    bytes: r.bytes,
                })
                .collect(),
        }))
    }
}

#[tool_handler(name = "voz-mcp", version = "1.0.0")]
impl ServerHandler for Server {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readback::{Readback, ReadbackError, Recording};
    use crate::tts::{Speech, TtsError};
    use std::path::PathBuf;

    struct StubTts;
    impl Tts for StubTts {
        fn speak(&self, req: &SpeakRequest) -> Result<Speech, TtsError> {
            Ok(Speech {
                path: PathBuf::from(format!("audio/{}.wav", req.lang.code())),
            })
        }
    }

    struct FailingTts;
    impl Tts for FailingTts {
        fn speak(&self, _req: &SpeakRequest) -> Result<Speech, TtsError> {
            Err(TtsError {
                reason: "backend down".to_string(),
            })
        }
    }

    struct StubReadback(Vec<Recording>);
    impl Readback for StubReadback {
        fn list(&self) -> Result<Vec<Recording>, ReadbackError> {
            Ok(self.0.clone())
        }
    }

    struct FailingReadback;
    impl Readback for FailingReadback {
        fn list(&self) -> Result<Vec<Recording>, ReadbackError> {
            Err(ReadbackError {
                reason: "disk error".to_string(),
            })
        }
    }

    fn server(backend: impl Tts + 'static, readback: impl Readback + 'static) -> Server {
        Server::new(Box::new(backend), Box::new(readback))
    }

    #[test]
    fn speak_returns_output_for_known_language() {
        let s = server(StubTts, StubReadback(vec![]));
        let out = s
            .speak(Parameters(SpeakInput {
                text: "hi".to_string(),
                lang: "en".to_string(),
                rate: None,
                pitch: None,
            }))
            .expect("ok");
        assert_eq!(out.0.lang, "en");
        assert_eq!(out.0.path, "audio/en.wav");
    }

    #[test]
    fn speak_rejects_unknown_language() {
        let s = server(StubTts, StubReadback(vec![]));
        let err = match s.speak(Parameters(SpeakInput {
            text: "hi".to_string(),
            lang: "zz".to_string(),
            rate: None,
            pitch: None,
        })) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.message.contains("unsupported language"));
    }

    #[test]
    fn speak_surfaces_backend_error() {
        let s = server(FailingTts, StubReadback(vec![]));
        let err = match s.speak(Parameters(SpeakInput {
            text: "hi".to_string(),
            lang: "en".to_string(),
            rate: None,
            pitch: None,
        })) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.message.contains("speech synthesis failed"));
    }

    #[test]
    fn speak_accepts_valid_rate() {
        let s = server(StubTts, StubReadback(vec![]));
        let out = s
            .speak(Parameters(SpeakInput {
                text: "hi".to_string(),
                lang: "en".to_string(),
                rate: Some(120),
                pitch: None,
            }))
            .expect("ok");
        assert_eq!(out.0.lang, "en");
    }

    #[test]
    fn speak_rejects_out_of_range_rate() {
        let s = server(StubTts, StubReadback(vec![]));
        let err = match s.speak(Parameters(SpeakInput {
            text: "hi".to_string(),
            lang: "en".to_string(),
            rate: Some(9999),
            pitch: None,
        })) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.message.contains("rate"));
    }

    #[test]
    fn non_integer_rate_fails_deserialization() {
        let err = serde_json::from_str::<SpeakInput>(
            r#"{"text":"hi","lang":"en","rate":120.5}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("u32"));
    }

    #[test]
    fn speak_accepts_valid_pitch() {
        let s = server(StubTts, StubReadback(vec![]));
        let out = s
            .speak(Parameters(SpeakInput {
                text: "hi".to_string(),
                lang: "en".to_string(),
                rate: None,
                pitch: Some(60),
            }))
            .expect("ok");
        assert_eq!(out.0.lang, "en");
    }

    #[test]
    fn speak_rejects_out_of_range_pitch() {
        let s = server(StubTts, StubReadback(vec![]));
        let err = match s.speak(Parameters(SpeakInput {
            text: "hi".to_string(),
            lang: "en".to_string(),
            rate: None,
            pitch: Some(100),
        })) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.message.contains("pitch"));
    }

    #[test]
    fn non_integer_pitch_fails_deserialization() {
        let err = serde_json::from_str::<SpeakInput>(
            r#"{"text":"hi","lang":"en","pitch":60.5}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("u32"));
    }

    #[test]
    fn readback_lists_recordings() {
        let recs = vec![Recording {
            name: "a.wav".to_string(),
            path: PathBuf::from("audio/a.wav"),
            bytes: 10,
        }];
        let s = server(StubTts, StubReadback(recs));
        let out = s.readback().expect("ok");
        assert_eq!(out.0.items.len(), 1);
        assert_eq!(out.0.items[0].name, "a.wav");
        assert_eq!(out.0.items[0].bytes, 10);
    }

    #[test]
    fn readback_surfaces_port_error() {
        let s = server(StubTts, FailingReadback);
        let err = match s.readback() {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.message.contains("failed to list recordings"));
    }

    struct ConcurrencyProbe {
        live: std::sync::atomic::AtomicUsize,
        peak: std::sync::atomic::AtomicUsize,
    }

    impl Tts for ConcurrencyProbe {
        fn speak(&self, _req: &SpeakRequest) -> Result<Speech, TtsError> {
            use std::sync::atomic::Ordering;
            let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(40));
            self.live.fetch_sub(1, Ordering::SeqCst);
            Ok(Speech {
                path: PathBuf::from("/dev/null"),
            })
        }
    }

    #[test]
    fn concurrent_speak_calls_run_one_at_a_time() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let probe = std::sync::Arc::new(ConcurrencyProbe {
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        });
        struct Shared(std::sync::Arc<ConcurrencyProbe>);
        impl Tts for Shared {
            fn speak(&self, req: &SpeakRequest) -> Result<Speech, TtsError> {
                self.0.speak(req)
            }
        }
        let server = std::sync::Arc::new(Server::new(
            Box::new(Shared(probe.clone())),
            Box::new(StubReadback(Vec::new())),
        ));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let server = server.clone();
                std::thread::spawn(move || {
                    server
                        .speak(Parameters(SpeakInput {
                            text: "hi".to_string(),
                            lang: "en".to_string(),
                            rate: None,
                            pitch: None,
                        }))
                        .map(|_| ())
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread").expect("speak");
        }
        assert_eq!(
            probe.peak.load(Ordering::SeqCst),
            1,
            "synthesis must not run concurrently"
        );
    }

    #[test]
    fn advertised_version_matches_the_package() {
        use rmcp::ServerHandler;
        let s = server(StubTts, StubReadback(Vec::new()));
        let info = s.get_info();
        assert_eq!(
            info.server_info.version,
            env!("CARGO_PKG_VERSION"),
            "the version in the tool_handler attribute must track Cargo.toml"
        );
        assert_eq!(info.server_info.name, "voz-mcp");
    }
}
