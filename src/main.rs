use line_index::LineIndex;
use tokio::net::TcpListener;
use toml::{Table, Value};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use toml_schema::schema::Schema;

mod util;

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

        // Deserialise the TOML now, to use it for extracting schema and validating
        // Only create a `toml_edit` document later, when we need span locations
        let Ok(doc) = toml::from_str::<Table>(text) else {
            // If the TOML cannot be parsed, then ignore (this is not a TOML syntax extension)
            return;
        };

        // Retrieve the specified schema location
        let Some(schema_loc) = doc
            .get("toml-schema")
            .and_then(Value::as_table)
            .and_then(|metadata| metadata.get("location"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|location| !location.is_empty())
        else {
            // If `toml-schema.location` is not specified, then ignore
            // The user is not using a toml schema on this file
            return;
        };

        let text_index = LineIndex::new(text);

        // Load the specified toml schema
        let Some(schema_path) = uri
            .to_file_path()
            .ok()
            .and_then(|fp| fp.parent()?.to_owned().into())
            .map(|dp| dp.join(schema_loc))
        else {
            // If the schema path cannot be resolved, potentially user is in scratchpad
            // Show a warning and do not continue

            let doc_edit = toml_edit::ImDocument::parse(text)
                .expect("TOML was correctly deserialised earlier");
            let schema_span = util::get_toml_span(&doc_edit, "toml-schema.location")
                .expect("A schema was loaded, but its span cannot be found");
            let range = util::get_file_range(
                &text_index,
                (schema_span.start as u32)..(schema_span.end as u32),
            );

            self.client
                .publish_diagnostics(
                    uri,
                    vec![Diagnostic::new(
                        range,
                        Some(DiagnosticSeverity::WARNING),
                        None,
                        None,
                        format!("TOML Schema location cannot be resolved"),
                        None,
                        None,
                    )],
                    None,
                )
                .await;

            return;
        };

        let schema = match Schema::load(&schema_path) {
            Ok(s) => s,
            Err(e) => {
                // If the specified toml schema cannot be loaded
                // Show a warning, and do not continue, as we do not know the schema to validate against

                let doc_edit = toml_edit::ImDocument::parse(text)
                    .expect("TOML was correctly deserialised earlier");
                let schema_span = util::get_toml_span(&doc_edit, "toml-schema.location")
                    .expect("A schema was loaded, but its span cannot be found");
                let range = util::get_file_range(
                    &text_index,
                    (schema_span.start as u32)..(schema_span.end as u32),
                );

                self.client
                    .publish_diagnostics(
                        uri,
                        vec![Diagnostic::new(
                            range,
                            Some(DiagnosticSeverity::WARNING),
                            None,
                            None,
                            format!("TOML Schema cannot be loaded: {e}"),
                            None,
                            None,
                        )],
                        None,
                    )
                    .await;

                return;
            }
        };

        // Validate against the schema, and generate diagnostics
        let result = schema.validate(&doc);
        if result.valid() {
            // If the results are valid then publish no diagnostics, to clear any previous
            self.client.publish_diagnostics(uri, vec![], None).await;
            return;
        }

        let doc_edit =
            toml_edit::ImDocument::parse(text).expect("TOML was correctly deserialised earlier");

        let diagnostics = result
            .errors
            .iter()
            .map(|error| {
                // todo where do i place errors if they are not found
                let span = util::get_toml_span(&doc_edit, &error.path).unwrap_or(0..10);
                let range =
                    util::get_file_range(&text_index, (span.start as u32)..(span.end as u32));
                Diagnostic::new_simple(range, format!("{}: {}", error.message, error.path))
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
