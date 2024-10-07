use std::path::PathBuf;

use crate::backend::{
    backend_choice, neural_root, pick_backend, read_backend_choice_override,
    read_neural_bin_override, read_neural_root_override, read_piper_bin_override,
    read_piper_voices_override, BackendChoice, BackendPick, PiperPaths, SelectionError,
};
use crate::readback::fs::FsReadback;
use crate::tool::Server;
use crate::tts::null::Null;
use crate::tts::piper::{piper_search, scan_piper, Piper};
use crate::tts::qwen::{neural_search, scan_modelz, Qwen};
use crate::tts::{Timeout, Tts};

pub fn discover_backend_pick(choice: BackendChoice) -> Result<BackendPick, SelectionError> {
    let root = neural_root(read_neural_root_override().as_deref());
    let config = crate::config::load(&crate::config::config_path(&root));
    let neural_bin = read_neural_bin_override();
    let piper_bin = read_piper_bin_override().or_else(|| config.bin.clone());
    let piper_voices = read_piper_voices_override().or_else(|| config.voices.clone());
    let neural = scan_modelz(&root, neural_bin.as_deref());
    let mut piper = scan_piper(&root, piper_bin.as_deref(), piper_voices.as_deref());
    if piper.is_none() && crate::config::auto_fetch(&config) {
        let (bin, voices) = piper_search(&root, piper_bin.as_deref(), piper_voices.as_deref());
        if bin.exists() {
            piper = Some(PiperPaths { bin, voices });
        }
    }
    pick_backend(choice, neural.as_ref(), piper.as_ref()).map_err(|e| match e {
        SelectionError::Unavailable(c) => SelectionError::UnavailableAt {
            choice: c,
            searched: searched_paths(c, &root, neural_bin.as_deref(), piper_bin.as_deref(), piper_voices.as_deref()),
        },
        other => other,
    })
}

fn searched_paths(
    choice: BackendChoice,
    root: &std::path::Path,
    neural_bin: Option<&std::path::Path>,
    piper_bin: Option<&std::path::Path>,
    piper_voices: Option<&std::path::Path>,
) -> String {
    match choice {
        BackendChoice::Fallback => {
            let (bin, voices) = piper_search(root, piper_bin, piper_voices);
            format!(
                "engine {} ({}), voices {} ({}); set VOZ_PIPER_BIN and VOZ_PIPER_VOICES to override",
                bin.display(),
                if bin.exists() { "found" } else { "missing" },
                voices.display(),
                if voices.is_dir() {
                    "no <lang>_*.onnx with a matching .onnx.json"
                } else {
                    "missing"
                }
            )
        }
        _ => {
            let (bin, gguf) = neural_search(root, neural_bin);
            format!(
                "engine {} ({}), weights {}; set VOZ_NEURAL_BIN or VOZ_NEURAL_ROOT to override",
                bin.display(),
                if bin.exists() { "found" } else { "missing" },
                gguf.display()
            )
        }
    }
}

pub fn select_backend_pick() -> Result<BackendPick, SelectionError> {
    discover_backend_pick(backend_choice(read_backend_choice_override().as_deref())?)
}

pub fn build_backend(pick: BackendPick, out_dir: PathBuf, timeout: Timeout) -> Box<dyn Tts> {
    match pick {
        BackendPick::Neural { bin, talker, codec } => {
            Box::new(Qwen::new(bin, talker, codec, out_dir, timeout))
        }
        BackendPick::Piper { bin, voices } => Box::new(build_piper(bin, voices, out_dir, timeout)),
        BackendPick::Null => Box::new(Null),
    }
}

fn build_piper(bin: PathBuf, voices: PathBuf, out_dir: PathBuf, timeout: Timeout) -> Piper {
    let root = neural_root(read_neural_root_override().as_deref());
    let config = crate::config::load(&crate::config::config_path(&root));
    let catalog = crate::voices::catalog(&root);
    let manifest = crate::voices::read_manifest(&voices);
    let preferred = crate::config::preferred_voices(&config, &manifest);
    let piper_config = crate::tts::piper::PiperConfig {
        root,
        catalog,
        preferred,
        auto_fetch: crate::config::auto_fetch(&config),
    };
    Piper::configured(bin, voices, out_dir, timeout, piper_config)
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
