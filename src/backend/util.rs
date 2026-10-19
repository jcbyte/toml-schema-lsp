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
    // A component can contain array indices, such as `group[0]`
    let mut item = doc.as_item();
    'path: for component in components {
        let (name, indices) = component.split_once('[').unwrap_or((component, ""));

        match item.get(name) {
            Some(next) => item = next,
            None => break,
        };

        for index in indices.split('[') {
            let Some(index) = index
                .strip_suffix(']')
                .and_then(|i| i.parse::<usize>().ok())
            else {
                continue;
            };
            match item.get(index) {
                Some(next) => item = next,
                None => break 'path,
            };
        }
    }

    item.span()
        .map(|s| (s.start as u32)..(s.end as u32))
        // Implicit tables (such as `a` in `[a.b]`) have no span of their own
        .unwrap_or(0..0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_items_inside_arrays() {
        let text = "
            [[layout]]

            [[layout.group]]
            scale = 1

            [[layout.group]]
            scale = 2
        ";
        let doc = toml_edit::ImDocument::parse(text).unwrap();

        let span = get_closest_item_span(&doc, "$.layout[0].group[1].scale");
        assert_eq!(&text[span.start as usize..span.end as usize], "2");

        // Missing indices resolve to the closest existing item
        let span = get_closest_item_span(&doc, "$.layout[0].group[5].scale");
        assert_eq!(span, get_closest_item_span(&doc, "$.layout[0].group"));
    }
}
