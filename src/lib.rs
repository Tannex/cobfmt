//! A single style for free-format COBOL procedure code.

mod layout;
mod source;

use layout::{Cells, Scopes, align, assignment, wrap};
use source::{Record, records};

struct Row<'a> {
    record: Record<'a>,
    depth: usize,
    cells: Option<Cells>,
}

/// Format a full program or a procedure snippet. Diagnostics include the source line.
pub fn format_source(source: &str) -> Result<String, String> {
    let mut scopes = Scopes::default();
    let rows: Vec<_> = records(source)?
        .into_iter()
        .map(|record| {
            let depth = scopes.indent(record.code());
            let cells = if record.verbatim() {
                None
            } else {
                assignment(&record.text)
            };
            Row {
                record,
                depth,
                cells,
            }
        })
        .collect();
    let newline = rows
        .iter()
        .map(|r| r.record.ending)
        .find(|s| !s.is_empty())
        .unwrap_or("\n");
    let mut output = String::with_capacity(source.len());
    let mut blanks = String::new();
    let mut after_group = false;
    let mut remaining = rows.as_slice();
    while let Some((row, rest)) = remaining.split_first() {
        let record = &row.record;
        if record.text.trim().is_empty() {
            blanks.push_str(&record.text);
            blanks.push_str(record.ending);
            remaining = rest;
            continue;
        }
        let group = row.cells.is_some();
        if after_group || group {
            if !output.is_empty() {
                output.push_str(newline);
            }
        } else {
            output.push_str(&blanks);
        }
        blanks.clear();

        if group {
            let cells: Vec<_> = remaining
                .iter()
                .take_while(|r| r.depth == row.depth && r.cells.is_some())
                .filter_map(|r| r.cells.as_ref())
                .collect();
            for (line, r) in align(&cells).iter().zip(remaining) {
                output.push_str(&wrap(line, row.depth, newline));
                output.push_str(r.record.ending);
            }
            remaining = &remaining[cells.len()..];
        } else {
            if record.verbatim() {
                output.push_str(&record.text);
            } else {
                output.push_str(&wrap(&record.text, row.depth, newline));
            }
            output.push_str(record.ending);
            remaining = rest;
        }
        after_group = group;
    }
    if !after_group {
        output.push_str(&blanks);
    }
    Ok(output)
}
