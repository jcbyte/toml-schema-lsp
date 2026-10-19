use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(options) = &params.initialization_options {
            self.update_settings(options);
        }

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
        self.validate_doc(uri, text).await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.update_settings(&params.settings);
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = &params.text_document.text;
        self.validate_doc(uri, text).await;
    }
}
