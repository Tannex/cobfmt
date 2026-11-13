//! A single style for IBM z/OS fixed-format COBOL procedure code.

mod layout;
mod source;

use layout::{
    Cells, Scopes, align, assignment, clauses, collapse_gaps, condition, fits, move_to_indent,
    subscript_column, to_token, width_before, wrap,
};
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
    /// Whether the source already continues this assignment at or before TO.
    fn split_before_to(&self) -> bool {
        let to = to_token(&self.record.code);
        let mut later = self.record.anchors.iter().skip(1);
        later.any(|&(token, _)| to.is_some_and(|to| token <= to))
    }

    fn write(
        &self,
        out: &mut String,
        code: &str,
        break_at_to: bool,
        newline: &str,
        padded: bool,
        pending_to: &mut Option<usize>,
    ) -> Result<(), String> {
        let record = &self.record;
        let mut segments: Vec<(usize, Option<&Card<'_>>)> = record
            .anchors
            .iter()
            .map(|(token, card)| (*token, Some(card)))
            .collect();
        if break_at_to && let Some(to) = to_token(code) {
            let at = segments.partition_point(|&(token, _)| token < to);
            segments.insert(at, (to, None));
        }
        let breaks: Vec<_> = segments.iter().skip(1).map(|&(token, _)| token).collect();
        let mut parts = clauses(code, &breaks);
        if self.cells.is_none() {
            collapse_gaps(&mut parts);
        }
        let indent = move_to_indent(&parts, self.depth, record, pending_to)
            .map_err(|e| format!("line {}: {e}", record.first().number))?;
        if record.action == Action::Keep {
            out.push_str(record.first().raw);
            out.push_str(record.ending);
            return Ok(());
        }
        // Each line carries the source record whose sequence and identification it keeps.
        // A wrap adds a record boundary. Lay the statement out again with that
        // boundary, as the next run will see it, until no new one appears.
        let lines = loop {
            let breaks: Vec<_> = segments.iter().skip(1).map(|&(token, _)| token).collect();
            let conditions = if self.cells.is_none() {
                condition(code, &breaks, self.depth)
            } else {
                None
            };
            if let Some(conditions) = conditions {
                let line = |(source, line): (Option<usize>, String)| {
                    (line, source.and_then(|i| segments[i].1))
                };
                break conditions.into_iter().map(line).collect::<Vec<_>>();
            }
            let mut lines: Vec<(String, Option<&Card<'_>>)> = Vec::new();
            let mut added = Vec::new();
            let mut positions = Vec::new();
            let mut remaining = parts.as_slice();
            for (index, &(_, card)) in segments.iter().enumerate() {
                let end = segments
                    .get(index + 1)
                    .map_or(remaining.len(), |&(token, _)| {
                        remaining.partition_point(|p| p.token < token)
                    });
                let (card_parts, rest) = remaining.split_at(end);
                remaining = rest;
                let subscript = card_parts
                    .first()
                    .filter(|p| p.first == "(")
                    .and_then(|p| subscript_column(code, p.token, &positions));
                let number = card.map_or(record.first().number, |c| c.number);
                let wrapped = wrap(card_parts, self.depth, indent, subscript, &mut positions)
                    .map_err(|e| format!("line {number}: {e}"))?;
                for (i, (token, line)) in wrapped.into_iter().enumerate() {
                    if i > 0 {
                        added.push(token);
                    }
                    lines.push((line, if i == 0 { card } else { None }));
                }
            }
            if added.is_empty() || self.cells.is_some() {
                break lines;
            }
            for token in added {
                let at = segments.partition_point(|&(t, _)| t < token);
                segments.insert(at, (token, None));
            }
        };
        let mut ending = newline;
        let count = lines.len();
        for (i, (line, card)) in lines.iter().enumerate() {
            emit_body(out, line, *card, padded);
            if let Some(card) = card
                && !card.ending.is_empty()
            {
                ending = card.ending;
            }
            out.push_str(if i + 1 == count {
                record.ending
            } else {
                ending
            });
        }
        Ok(())
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
    let mut pending_to = None;
    let mut remaining = rows.as_slice();
    while let Some((row, rest)) = remaining.split_first() {
        if row.record.action == Action::Blank {
            out.push_str(row.record.first().raw);
            out.push_str(row.record.ending);
            remaining = rest;
            continue;
        }
        if row.cells.is_none() {
            let code = &row.record.code;
            row.write(&mut out, code, false, newline, padded, &mut pending_to)?;
            remaining = rest;
            continue;
        }
        let members: Vec<_> = remaining
            .iter()
            .take_while(|r| r.depth == row.depth && r.cells.is_some())
            .collect();
        let cells: Vec<_> = members.iter().filter_map(|r| r.cells.as_ref()).collect();
        let mut split: Vec<_> = members.iter().map(|r| r.split_before_to()).collect();
        let mut forced = vec![false; members.len()];
        // While a row is too wide for the table, the widest one continues at TO
        // and the table is laid out again.
        let lines = loop {
            let lines = align(&cells, &split);
            let width = |i: usize| {
                let next = members[i].record.anchors.get(1).map(|&(token, _)| token);
                width_before(&lines[i], next)
            };
            let widest = (0..members.len())
                .filter(|&i| !split[i] && !fits(row.depth, width(i)))
                .max_by_key(|&i| (width(i), std::cmp::Reverse(i)));
            let Some(i) = widest else {
                break lines;
            };
            split[i] = true;
            forced[i] = true;
        };
        for ((member, line), &forced) in members.iter().zip(&lines).zip(&forced) {
            member.write(&mut out, line, forced, newline, padded, &mut pending_to)?;
        }
        remaining = &remaining[members.len()..];
    }
    Ok(out)
}
