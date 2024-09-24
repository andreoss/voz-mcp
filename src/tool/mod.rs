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
