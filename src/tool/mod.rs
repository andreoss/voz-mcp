use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    schemars, serde, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use crate::tts::{Language, SpeakRequest, Tts};

pub struct Server {
    backend: Box<dyn Tts>,
}

impl Server {
    pub fn new(backend: Box<dyn Tts>) -> Self {
        Self { backend }
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
}

#[tool_handler(name = "voz-mcp", version = "0.1.0")]
impl ServerHandler for Server {}
