//! A single style for IBM z/OS fixed-format COBOL procedure code.

mod layout;
mod source;

use layout::{Cells, Scopes, align, assignment, clauses, move_to_indent, wrap};
use source::{Action, Card, Record, cards, records};

struct Row<'a> {
    record: Record<'a>,
    depth: usize,
    cells: Option<Cells>,
}

fn emit_body(out: &mut String, code: &str, card: Option<&Card<'_>>, padded: bool) {
    let sequence = card.map_or("      ", |c| c.sequence);
    out.push_str(sequence);
    out.push_str("     "); // Blank indicator and Area A. First code column is 12.
    out.push_str(code);
    let width = 11 + code.chars().count();
    let id = card.map_or("", |c| c.identification);
    if !id.is_empty() {
        out.extend(std::iter::repeat_n(' ', 72 - width));
        out.push_str(id);
    } else if padded {
        out.extend(std::iter::repeat_n(' ', 80 - width));
    }
}

impl Row<'_> {
    fn write(
        &self,
        out: &mut String,
        code: &str,
        newline: &str,
        padded: bool,
        pending_to: &mut Option<usize>,
    ) -> Result<(), String> {
        let record = &self.record;
        let parts = clauses(code);
        let indent = move_to_indent(&parts, self.depth, record, pending_to)
            .map_err(|e| format!("line {}: {e}", record.first().number))?;
        if record.action == Action::Keep {
            out.push_str(record.first().raw);
            out.push_str(record.ending);
            return Ok(());
        }
        let mut remaining = parts.as_slice();
        for (index, &(_, card)) in record.anchors.iter().enumerate() {
            let next = record.anchors.get(index + 1);
            let end = next.map_or(remaining.len(), |&(token, _)| {
                remaining.partition_point(|p| p.token < token)
            });
            let (card_parts, rest) = remaining.split_at(end);
            remaining = rest;
            let lines = wrap(card_parts, self.depth, indent)
                .map_err(|e| format!("line {}: {e}", card.number))?;
            for (i, line) in lines.iter().enumerate() {
                let original = (i == 0).then_some(&card);
                emit_body(out, line, original, padded);
                if i + 1 == lines.len() && next.is_none() {
                    out.push_str(record.ending);
                } else {
                    out.push_str(if card.ending.is_empty() {
                        newline
                    } else {
                        card.ending
                    });
                }
            }
        }
        Ok(())
    }
}

fn write_blanks(
    out: &mut String,
    blanks: &[Card<'_>],
    beside_group: bool,
    between: bool,
    newline: &str,
    padded: bool,
) {
    // Several numbered blank cards may carry distinct metadata. Retain each one.
    let marked = blanks.iter().any(Card::marked);
    if beside_group && between && blanks.is_empty() {
        if padded {
            out.push_str(&" ".repeat(80));
        }
        out.push_str(newline);
    }
    for (i, card) in blanks.iter().enumerate() {
        if !beside_group || card.marked() || (between && !marked && i == 0) {
            out.push_str(card.raw);
            out.push_str(card.ending);
        }
    }
}

/// Format newline-delimited, UTF-8 fixed-format records from a checkout.
pub fn format_source(source: &str) -> Result<String, String> {
    let cards = cards(source)?;
    let padded = cards
        .iter()
        .filter(|c| !c.raw.is_empty())
        .all(|c| c.width == 80);
    let newline = cards
        .iter()
        .map(|c| c.ending)
        .find(|e| !e.is_empty())
        .unwrap_or("\n");
    let mut scopes = Scopes::default();
    let rows: Vec<_> = records(cards)?
        .into_iter()
        .map(|record| {
            let depth = scopes.indent(&record.code);
            let cells = if record.action == Action::Format {
                assignment(&record.code)
            } else {
                None
            };
            Row {
                record,
                depth,
                cells,
            }
        })
        .collect();
    let mut out = String::with_capacity(source.len());
    let mut blanks = Vec::new();
    let mut after_group = false;
    let mut pending_to = None;
    let mut remaining = rows.as_slice();
    while let Some((row, rest)) = remaining.split_first() {
        if row.record.action == Action::Blank {
            blanks.push(*row.record.first());
            remaining = rest;
            continue;
        }
        let group = row.cells.is_some();
        let between = !out.is_empty();
        write_blanks(
            &mut out,
            &blanks,
            after_group || group,
            between,
            newline,
            padded,
        );
        blanks.clear();
        if group {
            let cells: Vec<_> = remaining
                .iter()
                .take_while(|r| r.depth == row.depth && r.cells.is_some())
                .filter_map(|r| r.cells.as_ref())
                .collect();
            for (line, r) in align(&cells).iter().zip(remaining) {
                r.write(&mut out, line, newline, padded, &mut pending_to)?;
            }
            remaining = &remaining[cells.len()..];
        } else {
            row.write(&mut out, &row.record.code, newline, padded, &mut pending_to)?;
            remaining = rest;
        }
        after_group = group;
    }
    write_blanks(&mut out, &blanks, after_group, false, newline, padded);
    Ok(out)
}
