use std::path::{Path, PathBuf};

use crate::backend::{
    neural_root, pick_backend, read_env_override, read_neural_root_override, BackendPick,
};
use crate::readback::fs::FsReadback;
use crate::tool::Server;
use crate::tts::espeak::{scan_store, Espeak};
use crate::tts::null::Null;
use crate::tts::qwen::{scan_modelz, Qwen};
use crate::tts::Tts;

pub fn select_backend_pick() -> BackendPick {
    let espeak = scan_store(Path::new("/nix/store"));
    let neural = scan_modelz(&neural_root(read_neural_root_override().as_deref()));
    pick_backend(read_env_override().as_deref(), espeak.as_deref(), neural.as_ref())
}

pub fn build_backend(pick: BackendPick, out_dir: PathBuf) -> Box<dyn Tts> {
    match pick {
        BackendPick::Espeak(bin) => Box::new(Espeak::new(bin, out_dir)),
        BackendPick::Neural { bin, talker, codec } => {
            Box::new(Qwen::new(bin, talker, codec, out_dir))
        }
        BackendPick::Null => Box::new(Null),
    }
}

pub fn build_mcp_server(pick: BackendPick, out_dir: PathBuf) -> Server {
    let backend = build_backend(pick, out_dir.clone());
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
        assert!(matches!(pick, BackendPick::Neural { .. }));
    }

    #[test]
    fn build_backend_null_reports_dev_null() {
        let out = std::env::temp_dir().join(format!("voz-server-null-{}", std::process::id()));
        let backend = build_backend(BackendPick::Null, out.clone());
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
    fn build_backend_espeak_creates_out_dir() {
        let out = std::env::temp_dir().join(format!("voz-server-espeak-{}", std::process::id()));
        let _backend = build_backend(BackendPick::Espeak(PathBuf::from("/nonexistent")), out.clone());
        assert!(out.exists());
        std::fs::remove_dir_all(&out).ok();
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
        );
        assert!(out.exists());
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn build_mcp_server_constructs_with_null_pick() {
        let out = std::env::temp_dir().join(format!("voz-server-mcp-null-{}", std::process::id()));
        let _server = build_mcp_server(BackendPick::Null, out.clone());
        std::fs::remove_dir_all(&out).ok();
    }
}
