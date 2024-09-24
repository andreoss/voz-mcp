use std::path::{Path, PathBuf};

use rmcp::service::serve_server;
use rmcp::transport::stdio;

use voz_mcp::backend::{pick_backend, read_env_override, BackendPick};
use voz_mcp::readback::fs::FsReadback;
use voz_mcp::tool::Server;
use voz_mcp::tts::espeak::{scan_store, Espeak};
use voz_mcp::tts::null::Null;

#[tokio::main]
async fn main() {
    let out_dir = std::env::var("VOZ_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("audio"));
    let discovered = scan_store(Path::new("/nix/store"));
    let pick = pick_backend(read_env_override().as_deref(), discovered.as_deref());
    let backend: Box<dyn voz_mcp::tts::Tts> = match pick {
        BackendPick::Espeak(bin) => Box::new(Espeak::new(bin, out_dir.clone())),
        BackendPick::Null => Box::new(Null),
    };
    let readback = Box::new(FsReadback::new(out_dir));
    let service = Server::new(backend, readback);
    let server = serve_server(service, stdio()).await.expect("failed to serve");
    let _ = server.waiting().await;
}