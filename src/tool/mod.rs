use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    schemars, serde, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use crate::readback::Readback;
use crate::tts::{Language, SpeakRequest, Tts};

pub struct Server {
    backend: Box<dyn Tts>,
    readback: Box<dyn Readback>,
}

impl Server {
    pub fn new(backend: Box<dyn Tts>, readback: Box<dyn Readback>) -> Self {
        Self { backend, readback }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SpeakInput {
    pub text: String,
    pub lang: String,
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
                format!("unsupported language '{}', expected ru|en|es", input.lang),
                None,
            )
        })?;
        let speech = self
            .backend
            .speak(&SpeakRequest {
                text: input.text,
                lang,
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

#[tool_handler(name = "voz-mcp", version = "0.1.0")]
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
            lang: "fr".to_string(),
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
        })) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.message.contains("speech synthesis failed"));
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
}
