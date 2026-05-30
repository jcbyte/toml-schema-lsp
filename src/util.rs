use std::ops::Range;

use line_index::LineIndex;
use tower_lsp::lsp_types::{self, Position};

pub fn get_toml_span<T>(doc: toml_edit::Document<T>, key: &str) -> Option<Range<usize>> {
    let trimmed_key = key.strip_prefix("$.").unwrap_or(key);
    let components = trimmed_key.split(".");

    let mut item = doc.as_item();
    for component in components {
        match item.get(component) {
            Some(next) => item = next,
            None => return None,
        };
    }

    item.span()
}

pub fn get_file_range(index: &LineIndex, range: Range<u32>) -> lsp_types::Range {
    let start = index.line_col(line_index::TextSize::new(range.start as u32));
    let end = index.line_col(line_index::TextSize::new(range.end as u32));

    lsp_types::Range::new(
        Position::new(start.line, start.col),
        Position::new(end.line, end.col),
    )
}
