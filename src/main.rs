use std::path::PathBuf;

use rmcp::service::serve_server;
use rmcp::transport::stdio;

use voz_mcp::audio::synthesize;
use voz_mcp::backend::BackendPick;
use voz_mcp::cli::{parse, AudioArgs, Mode};
use voz_mcp::server::{build_backend, build_mcp_server, select_backend_pick};

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
        Ok(Mode::Audio(audio)) => run_audio(audio),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
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
    let service = build_mcp_server(pick_or_exit(), out_dir());
    let server = serve_server(service, stdio()).await.expect("failed to serve");
    let _ = server.waiting().await;
}

fn run_audio(args: AudioArgs) {
    let pick = pick_or_exit();
    if pick == BackendPick::Null {
        eprintln!("no speech backend available");
        std::process::exit(1);
    }
    let backend = build_backend(pick, out_dir());
    match synthesize(args, backend.as_ref()) {
        Ok(path) => println!("{}", path.display()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
