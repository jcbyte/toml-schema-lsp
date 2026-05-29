use tokio::net::TcpListener;
use toml::{Table, Value};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use toml_schema::schema::Schema;

// todo should the schema be stored in a cache? What if it changes?

struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Tell LSP we want to manage full text documents
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "TOML Schema Language Server Initialised!",
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // Triggers every time the user edits a TOML file
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = &params.content_changes[0].text;

        let Ok(document) = toml::from_str::<Table>(text) else {
            // If the TOML cannot be parsed, then ignore (this is not a TOML syntax extension)
            return;
        };

        // Retrieve the specified schema location
        let Some(schema_loc) = document
            .get("toml-schema")
            .and_then(Value::as_table)
            .and_then(|metadata| metadata.get("location"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|location| !location.is_empty())
        else {
            // If `toml-schema.location` is not specified, then ignore.
            // The user is not using a toml schema on this file
            return;
        };

        // Load the specified toml schema
        // todo remove dangerously unwrapping
        let doc_dir = uri.to_file_path().unwrap().parent().unwrap().to_path_buf();
        let schema_path = doc_dir.join(schema_loc);
        let schema = match Schema::load(&schema_path) {
            Ok(s) => s,
            Err(e) => {
                // todo show this warning on the toml-schema.location
                return;
            }
        };

        // Validate against the schema, and generate diagnostics
        let result = schema.validate(&document);
        let diagnostics = result
            .errors
            .iter()
            .map(|error| {
                // todo work out locations
                Diagnostic::new_simple(
                    Range::new(Position::new(0, 0), Position::new(0, 10)),
                    format!("{}:{}", error.path, error.message),
                )
            })
            .collect::<Vec<_>>();

        // Push the diagnostics back to the client
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(|client| Backend { client });

    if cfg!(debug_assertions) {
        // In development, run the LSP completely separately with TCP
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
