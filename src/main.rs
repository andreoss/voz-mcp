use std::path::PathBuf;

use rmcp::service::serve_server;
use rmcp::transport::stdio;

use voz_mcp::audio::synthesize;
use voz_mcp::backend::{
    neural_root, read_neural_root_override, read_piper_bin_override, read_piper_voices_override,
    read_timeout_override, select_timeout, BackendPick,
};
use voz_mcp::tts::Timeout;
use voz_mcp::cli::{parse, usage, version_line, AudioArgs, Mode, VoicesArgs, VoicesCmd};
use voz_mcp::server::{build_backend, build_mcp_server, select_backend_pick};
use voz_mcp::tts::piper::piper_search;
use voz_mcp::voices;

fn out_dir() -> PathBuf {
    std::env::var("VOZ_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("audio"))
}

#[tokio::main]
async fn main() {
    let args = std::env::args_os().skip(1);
    match parse(args) {
        Ok(Mode::Mcp) => run_mcp().await,
        Ok(Mode::Help) => println!("{}", usage()),
        Ok(Mode::Version) => println!("{}", version_line()),
        Ok(Mode::Voices(voices_args)) => run_voices(voices_args),
        Ok(Mode::Audio(audio)) => run_audio(audio),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}

fn timeout_or_exit() -> Timeout {
    match select_timeout(read_timeout_override().as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(e.exit_code());
        }
    }
}

fn pick_or_exit() -> BackendPick {
    match select_backend_pick() {
        Ok(pick) => pick,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(e.exit_code());
        }
    }
}

async fn run_mcp() {
    let service = build_mcp_server(pick_or_exit(), out_dir(), timeout_or_exit());
    let server = serve_server(service, stdio()).await.expect("failed to serve");
    let _ = server.waiting().await;
}

fn data_root() -> PathBuf {
    neural_root(read_neural_root_override().as_deref())
}

fn voices_dir() -> PathBuf {
    let root = data_root();
    let config = voz_mcp::config::load(&voz_mcp::config::config_path(&root));
    let (_, voices) = piper_search(
        &root,
        read_piper_bin_override().as_deref(),
        read_piper_voices_override()
            .as_deref()
            .or(config.voices.as_deref()),
    );
    voices
}

fn run_voices(args: VoicesArgs) {
    let root = data_root();
    let dir = voices_dir();
    let catalog = voices::catalog(&root);
    match args.cmd {
        VoicesCmd::List => {
            for spec in &catalog {
                let state = if dir.join(format!("{}.onnx", spec.id)).is_file() {
                    "installed"
                } else {
                    "missing"
                };
                println!("{}\t{}\t{}", spec.lang, spec.id, state);
            }
        }
        VoicesCmd::Fetch => {
            let langs: Vec<String> = if args.all {
                catalog.iter().map(|s| s.lang.clone()).collect()
            } else {
                vec![args.lang.clone().expect("scope checked by parser")]
            };
            let mut failed = false;
            for lang in langs {
                match voices::install(&root, &dir, &lang) {
                    Ok(path) => println!("{lang}\t{}", path.display()),
                    Err(e) => {
                        eprintln!("{lang}: {e}");
                        failed = true;
                    }
                }
            }
            if failed {
                std::process::exit(1);
            }
        }
    }
}

fn run_audio(args: AudioArgs) {
    let pick = pick_or_exit();
    if pick == BackendPick::Null {
        eprintln!("no speech backend available");
        std::process::exit(1);
    }
    let backend = build_backend(pick, out_dir(), timeout_or_exit());
    match synthesize(args, backend.as_ref()) {
        Ok(path) => println!("{}", path.display()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
