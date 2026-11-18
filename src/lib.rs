//! A single style for IBM z/OS fixed-format COBOL procedure code.

mod layout;
mod source;

use layout::{
    Cells, Place, Scopes, align, assignment, clauses, collapse_gaps, condition, fits,
    move_to_indent, operand_column, source_width, statement_column, subscript, subscript_column,
    to_token, width_before, wrap,
};
use source::{Action, Card, Record, cards, is_clause, records};

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
            // A continuation cut off from its statement still hangs under it.
            let depth = self.depth + usize::from(record.orphan);
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
                // A continuation that starts with an operand goes under the
                // operand it follows on from.
                let column = card_parts.first().filter(|_| index > 0).and_then(|p| {
                    if p.first == "(" && subscript(code, p.token) {
                        subscript_column(code, p.token, &positions)
                    } else if is_clause(p.first) {
                        None
                    } else {
                        operand_column(code, p.token, &positions)
                    }
                });
                let number = card.map_or(record.first().number, |c| c.number);
                let wrapped = wrap(card_parts, depth, indent, column, &mut positions)
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

/// Move an embedded EXEC block as a whole so that EXEC starts where a
/// statement at `depth` does. Every record shifts by the same number of
/// columns. `None` when the block is in place already or cannot move: a record
/// would leave Area B, or a hyphen continues a literal.
fn shift_block(rows: &[Row<'_>], depth: usize) -> Option<Vec<String>> {
    let leading = |area: &str| area.chars().take_while(|&c| c == ' ').count();
    let first = leading(rows.first()?.record.first().area);
    let target = statement_column(depth);
    if first == target {
        return None;
    }
    let mut lines = Vec::new();
    for row in rows {
        let card = row.record.first();
        if row.record.action == Action::Blank || matches!(card.indicator, '*' | '/' | 'D' | 'd') {
            lines.push(card.raw.to_owned());
            continue;
        }
        if card.indicator != ' ' || card.raw.contains('\t') {
            return None;
        }
        let text = card.area.trim();
        let column = (leading(card.area) + target).checked_sub(first)?;
        let width = column + text.chars().count();
        if column < statement_column(0) || width > 65 {
            return None;
        }
        let mut line = format!("{}{}{:column$}{text}", card.sequence, card.indicator, "");
        // Keep the identification field, or the record's padding, in place.
        let area = card.area.chars().count();
        let trailing = area - card.area.trim_end().chars().count();
        let padding = if area == 65 { 65 - width } else { trailing };
        line.extend(std::iter::repeat_n(' ', padding));
        line.push_str(card.identification);
        lines.push(line);
    }
    Some(lines)
}

/// A MOVE or SET that ends a record with TO continues at TO instead, so the
/// keyword stays with its targets.
fn lead_with_to(record: &mut Record<'_>) {
    let lexed = source::tokens(&record.code);
    let assignment = lexed
        .first()
        .is_some_and(|t| t.text.eq_ignore_ascii_case("MOVE") || t.text.eq_ignore_ascii_case("SET"));
    let Some(to) = to_token(&record.code).filter(|_| assignment) else {
        return;
    };
    if record.anchors.iter().any(|&(token, _)| token == to) {
        return;
    }
    for (token, _) in record.anchors.iter_mut().skip(1) {
        if *token == to + 1 {
            *token = to;
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
        .map(|mut record| {
            let depth = scopes.indent(&record.code);
            if record.action == Action::Format {
                lead_with_to(&mut record);
            }
            let cells = if record.action == Action::Format {
                let anchors: Vec<_> = record.anchors.iter().map(|&(token, _)| token).collect();
                assignment(&record.code, &anchors)
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
        if row.record.exec != 0 {
            let block = remaining
                .iter()
                .take_while(|r| r.record.exec == row.record.exec)
                .count();
            let (block, rest) = remaining.split_at(block);
            let shifted = shift_block(block, row.depth);
            for (i, member) in block.iter().enumerate() {
                let raw = member.record.first().raw;
                out.push_str(shifted.as_ref().map_or(raw, |lines| &lines[i]));
                out.push_str(member.record.ending);
            }
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
        let width = |lines: &[String], i: usize| {
            let next = members[i].record.anchors.get(1).map(|&(token, _)| token);
            width_before(&lines[i], next)
        };
        let overflow = |places: &[Place]| -> Vec<usize> {
            let lines = align(&cells, places);
            (0..members.len())
                .filter(|&i| places[i] != Place::Split && !fits(row.depth, width(&lines, i)))
                .collect()
        };
        // A row too wide for one record continues at TO.
        let mut forced = vec![false; members.len()];
        for i in overflow(&vec![Place::Alone; members.len()]) {
            forced[i] = !members[i].split_before_to();
        }
        let split: Vec<_> = (0..members.len())
            .map(|i| forced[i] || members[i].split_before_to())
            .collect();
        // The table takes the rows whose source is no wider than a limit, less
        // the rows that are then too wide for it. The limit that keeps the
        // most rows wins, the narrower on a tie, and each other row goes on a
        // record of its own.
        let mut limits: Vec<_> = (0..members.len())
            .filter(|&i| !split[i])
            .map(|i| source_width(cells[i]))
            .collect();
        limits.sort_unstable();
        limits.dedup();
        let mut places: Vec<_> = split
            .iter()
            .map(|&split| if split { Place::Split } else { Place::Alone })
            .collect();
        let mut most = 0;
        for &limit in &limits {
            let mut trial: Vec<_> = (0..members.len())
                .map(|i| match (split[i], source_width(cells[i]) <= limit) {
                    (true, _) => Place::Split,
                    (false, true) => Place::Table,
                    (false, false) => Place::Alone,
                })
                .collect();
            loop {
                let lines = align(&cells, &trial);
                let widest = overflow(&trial)
                    .into_iter()
                    .max_by_key(|&i| (width(&lines, i), std::cmp::Reverse(i)));
                let Some(i) = widest else {
                    break;
                };
                trial[i] = Place::Alone;
            }
            let count = trial.iter().filter(|&&p| p == Place::Table).count();
            if count > most {
                most = count;
                places = trial;
            }
        }
        let lines = align(&cells, &places);
        for ((member, line), &forced) in members.iter().zip(&lines).zip(&forced) {
            member.write(&mut out, line, forced, newline, padded, &mut pending_to)?;
        }
        remaining = &remaining[members.len()..];
    }
    Ok(out)
}
