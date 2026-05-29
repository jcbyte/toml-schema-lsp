use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use std::sync::Arc;
use toml_schema::schema::Schema;

struct Backend {
    client: Client,
    schema: Arc<Option<Schema>>,
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
            .log_message(MessageType::INFO, "TOML Validator Server Initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // Triggers every time the user opens or edits a TOML file
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = &params.content_changes[0].text;

        // Parse the TOML text from memory
        let parsed_doc = match toml::from_str::<toml::Table>(text) {
            Ok(doc) => doc,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("TOML Parse Error: {}", e))
                    .await;
                return;
            }
        };

        // If schema is available, validate the document
        if let Some(schema) = self.schema.as_ref() {
            let result = schema.validate(&parsed_doc);

            // Map validation errors to LSP Diagnostics
            let diagnostics: Vec<Diagnostic> = result
                .errors
                .iter()
                .map(|error| {
                    // Parse the JSONPath-like error.path to estimate line/column
                    // For now, report all errors at line 0
                    Diagnostic {
                        range: Range::new(Position::new(0, 0), Position::new(0, 10)),
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: None,
                        code_description: None,
                        source: Some("TOML Schema".to_string()),
                        message: format!("{}: {}", error.path, error.message),
                        related_information: None,
                        tags: None,
                        data: None,
                    }
                })
                .collect();

            // Push diagnostics back to VS Code
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
    }
}

#[tokio::main]
async fn main() {
    // Load schema once at startup (you may want to load from a specific path or URI)
    // For now, we'll leave it uninitialized - you can set this based on your LSP configuration
    let schema = Arc::new(Schema::load("eg.tosd").ok()); // Try to load schema, but don't fail if missing

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        schema: schema.clone(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
