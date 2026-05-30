use tokio::net::TcpListener;
use tower_lsp::{LspService, Server};

mod backend;

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(backend::Backend::new);

    if cfg!(debug_assertions) {
        // In development, run the LSP completely separately with TCP
        // Use magic port number 9260, this is synced in `extension.ts`
        let listener = TcpListener::bind(("localhost", 9260)).await.unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let (read, write) = tokio::io::split(stream);
        Server::new(read, write, socket).serve(service).await;
    } else {
        // In production, the server communicates with stdin/stdout
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        Server::new(stdin, stdout, socket).serve(service).await;
    }
}
