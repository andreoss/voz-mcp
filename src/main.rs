use std::path::PathBuf;

use rmcp::service::serve_server;
use rmcp::transport::stdio;

use voz_mcp::tool::Server;
use voz_mcp::tts::espeak::Espeak;
use voz_mcp::tts::null::Null;

fn espeak_bin() -> PathBuf {
    std::env::var("VOZ_ESPEAK_BIN").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(
            "/nix/store/156gf924ld4apvl77h22z8b38jqd9wi2-espeak-ng-1.52.0.1-unstable-2025-09-09/bin/espeak-ng",
        )
    })
}

#[tokio::main]
async fn main() {
    let out_dir = std::env::var("VOZ_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("audio"));
    let bin = espeak_bin();
    let backend: Box<dyn voz_mcp::tts::Tts> = if bin.exists() {
        Box::new(Espeak::new(bin, out_dir))
    } else {
        Box::new(Null)
    };
    let service = Server::new(backend);
    let server = serve_server(service, stdio()).await.expect("failed to serve");
    let _ = server.waiting().await;
}