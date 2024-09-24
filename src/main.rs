use rmcp::service::serve_server;
use rmcp::transport::stdio;

use voz_mcp::tool::Server;
use voz_mcp::tts::null::Null;

#[tokio::main]
async fn main() {
    let service = Server::new(Box::new(Null));
    let server = serve_server(service, stdio()).await.expect("failed to serve");
    let _ = server.waiting().await;
}
