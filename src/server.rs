use std::path::PathBuf;

use crate::backend::{
    backend_choice, neural_root, pick_backend, read_backend_choice_override,
    read_neural_bin_override, read_neural_root_override, read_piper_bin_override, BackendChoice,
    BackendPick, SelectionError,
};
use crate::readback::fs::FsReadback;
use crate::tool::Server;
use crate::tts::null::Null;
use crate::tts::piper::{scan_piper, Piper};
use crate::tts::qwen::{scan_modelz, Qwen};
use crate::tts::{Timeout, Tts};

pub fn discover_backend_pick(choice: BackendChoice) -> Result<BackendPick, SelectionError> {
    let root = neural_root(read_neural_root_override().as_deref());
    let neural = scan_modelz(&root, read_neural_bin_override().as_deref());
    let piper = scan_piper(&root, read_piper_bin_override().as_deref());
    pick_backend(choice, neural.as_ref(), piper.as_ref())
}

pub fn select_backend_pick() -> Result<BackendPick, SelectionError> {
    discover_backend_pick(backend_choice(read_backend_choice_override().as_deref())?)
}

pub fn build_backend(pick: BackendPick, out_dir: PathBuf, timeout: Timeout) -> Box<dyn Tts> {
    match pick {
        BackendPick::Neural { bin, talker, codec } => {
            Box::new(Qwen::new(bin, talker, codec, out_dir, timeout))
        }
        BackendPick::Piper { bin, voices } => Box::new(Piper::new(bin, voices, out_dir, timeout)),
        BackendPick::Null => Box::new(Null),
    }
}

pub fn build_mcp_server(pick: BackendPick, out_dir: PathBuf, timeout: Timeout) -> Server {
    let backend = build_backend(pick, out_dir.clone(), timeout);
    let readback = Box::new(FsReadback::new(out_dir));
    Server::new(backend, readback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::{Language, SpeakRequest};

    #[test]
    fn select_backend_pick_finds_neural_stack_on_this_host() {
        let pick = select_backend_pick();
        assert!(matches!(pick, Ok(BackendPick::Neural { .. })));
    }

    #[test]
    fn forced_fallback_discovers_the_fallback_runtime_on_this_host() {
        let pick = discover_backend_pick(BackendChoice::Fallback);
        assert!(matches!(pick, Ok(BackendPick::Piper { .. })));
    }

    #[test]
    fn forced_null_needs_no_discovery() {
        assert_eq!(discover_backend_pick(BackendChoice::Null), Ok(BackendPick::Null));
    }

    #[test]
    fn build_backend_null_reports_dev_null() {
        let out = std::env::temp_dir().join(format!("voz-server-null-{}", std::process::id()));
        let backend = build_backend(BackendPick::Null, out.clone(), Timeout::default_timeout());
        let speech = backend
            .speak(&SpeakRequest {
                text: "hi".to_string(),
                lang: Language::English,
                rate: None,
                pitch: None,
            })
            .expect("ok");
        assert_eq!(speech.path, PathBuf::from("/dev/null"));
    }

    #[test]
    fn build_backend_neural_creates_out_dir() {
        let out = std::env::temp_dir().join(format!("voz-server-neural-{}", std::process::id()));
        let _backend = build_backend(
            BackendPick::Neural {
                bin: PathBuf::from("/nonexistent"),
                talker: PathBuf::from("/nonexistent.gguf"),
                codec: PathBuf::from("/nonexistent-codec.gguf"),
            },
            out.clone(),
            Timeout::default_timeout(),
        );
        assert!(out.exists());
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn build_backend_piper_creates_out_dir() {
        let out = std::env::temp_dir().join(format!("voz-server-piper-{}", std::process::id()));
        let _backend = build_backend(
            BackendPick::Piper {
                bin: PathBuf::from("/nonexistent"),
                voices: PathBuf::from("/nonexistent-voices"),
            },
            out.clone(),
            Timeout::default_timeout(),
        );
        assert!(out.exists());
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn build_mcp_server_constructs_with_null_pick() {
        let out = std::env::temp_dir().join(format!("voz-server-mcp-null-{}", std::process::id()));
        let _server = build_mcp_server(BackendPick::Null, out.clone(), Timeout::default_timeout());
        std::fs::remove_dir_all(&out).ok();
    }
}
