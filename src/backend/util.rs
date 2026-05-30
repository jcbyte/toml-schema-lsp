use std::ops::Range;

use line_index::LineIndex;
use tower_lsp::lsp_types::{self, Position};

/// Find the closest element to the key formatted as `$.outer.inner.value`.
/// Returns the location within the original document for that element.
pub fn get_closest_item_span<S>(doc: &toml_edit::ImDocument<S>, key: &str) -> Range<u32> {
    // Remove the "$" (main) component if it is added.
    let trimmed_key = key.strip_prefix("$.").unwrap_or(key);
    let components = trimmed_key.split(".");

    // Keep retrieving the item further down the chain,
    // until the end component is reached, or a component does not exist
    let mut item = doc.as_item();
    for component in components {
        match item.get(component) {
            Some(next) => item = next,
            None => break,
        };
    }

    item.span()
        .map(|s| (s.start as u32)..(s.end as u32))
        .expect("Every item in `ImDocument` should have a span")
}

// Use a line index for a file to convert exact character range to a `tower_lsp` valid range.
pub fn get_file_range(index: &LineIndex, range: Range<u32>) -> lsp_types::Range {
    let start = index.line_col(line_index::TextSize::new(range.start));
    let end = index.line_col(line_index::TextSize::new(range.end));

    lsp_types::Range::new(
        Position::new(start.line, start.col),
        Position::new(end.line, end.col),
    )
}
