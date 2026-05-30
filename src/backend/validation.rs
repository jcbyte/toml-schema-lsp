use line_index::LineIndex;
use toml::{Table, Value};
use toml_schema::Schema;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use super::Backend;
use super::util;

impl Backend {
    pub(super) async fn validate_doc(&self, uri: Url, text: &str) {
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
            let schema_span = util::get_closest_item_span(&doc_edit, "toml-schema.location");
            let doc_index = LineIndex::new(text);
            let range = util::get_file_range(&doc_index, schema_span);

            self.client
                .publish_diagnostics(
                    uri,
                    vec![Diagnostic::new(
                        range,
                        Some(DiagnosticSeverity::WARNING),
                        None,
                        None,
                        "TOML Schema location cannot be resolved".to_string(),
                        None,
                        None,
                    )],
                    None,
                )
                .await;

            return;
        };

        // todo the schema should be stored in a cache instead, what if it changes?
        let schema = match Schema::load(&schema_path) {
            Ok(s) => s,
            Err(e) => {
                // If the specified toml schema cannot be loaded
                // Show a warning, and do not continue, as we do not know the schema to validate against

                let doc_edit = toml_edit::ImDocument::parse(text)
                    .expect("TOML was correctly deserialised earlier");
                let schema_span = util::get_closest_item_span(&doc_edit, "toml-schema.location");
                let doc_index = LineIndex::new(text);
                let range = util::get_file_range(&doc_index, schema_span);

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
        let doc_index = LineIndex::new(text);

        let diagnostics = result
            .errors
            .iter()
            .map(|error| {
                // This function will return the closest item to the element, when inner keys are incorrect (span is correct)
                let span = util::get_closest_item_span(&doc_edit, &error.path);
                let range = util::get_file_range(&doc_index, span);
                Diagnostic::new(
                    range,
                    Some(DiagnosticSeverity::ERROR),
                    None,
                    None,
                    format!("{}: {}", error.message, error.path),
                    None,
                    None,
                )
            })
            .collect::<Vec<_>>();

        // Push the diagnostics back to the client
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}
