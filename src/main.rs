use std::path::PathBuf;

use rmcp::service::serve_server;
use rmcp::transport::stdio;

use voz_mcp::readback::fs::FsReadback;
use voz_mcp::tool::Server;
use voz_mcp::tts::espeak::{Espeak, discover_bin};
use voz_mcp::tts::flite::{Flite, discover_flite};
use voz_mcp::tts::null::Null;

#[tokio::main]
async fn main() {
    let out_dir = std::env::var("VOZ_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("audio"));
    let backend: Box<dyn voz_mcp::tts::Tts> = if let Some(bin) = discover_bin() {
        Box::new(Espeak::new(bin, out_dir.clone()))
    } else if let Some(bin) = discover_flite() {
        Box::new(Flite::new(bin, out_dir.clone()))
    } else {
        Box::new(Null)
    };
    let readback = Box::new(FsReadback::new(out_dir));
    let service = Server::new(backend, readback);
    let server = serve_server(service, stdio()).await.expect("failed to serve");
    let _ = server.waiting().await;
}