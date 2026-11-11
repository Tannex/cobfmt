use std::collections::{BTreeMap, BTreeSet};

use crate::source::{TokenKind, is_clause, tokens};

const INDENT: usize = 3;

#[derive(Default)]
pub(crate) struct Scopes(Vec<&'static str>);

impl Scopes {
    fn close(&mut self, terminator: &str) {
        if let Some(name) = terminator.strip_prefix("END-")
            && let Some(position) = self.0.iter().rposition(|&scope| scope == name)
        {
            self.0.truncate(position);
        }
    }

    pub fn indent(&mut self, code: &str) -> usize {
        let lexed = tokens(code);
        let Some(first) = lexed.first() else {
            return self.0.len();
        };
        let head = first.text.to_ascii_uppercase();
        self.close(&head);
        if head == "WHEN" && self.0.last() == Some(&"WHEN") {
            self.0.pop();
        }
        let mut depth = self.0.len();
        let second = lexed.get(1).map_or("", |t| t.text).to_ascii_uppercase();
        match head.as_str() {
            "IF" => self.0.push("IF"),
            "ELSE" if self.0.last() == Some(&"IF") => {
                depth -= 1;
                if second == "IF" {
                    self.0.push("IF");
                }
            }
            "EVALUATE" => self.0.push("EVALUATE"),
            "SEARCH" => self.0.push("SEARCH"),
            "WHEN" => self.0.push("WHEN"),
            "PERFORM"
                if matches!(
                    second.as_str(),
                    "" | "UNTIL" | "VARYING" | "WITH" | "FOREVER"
                ) || lexed
                    .get(2)
                    .is_some_and(|t| t.text.eq_ignore_ascii_case("TIMES")) =>
            {
                self.0.push("PERFORM");
            }
            _ => {}
        }
        for token in &lexed[1..] {
            if token.kind == TokenKind::Word {
                self.close(&token.text.to_ascii_uppercase());
            }
        }
        if lexed.last().is_some_and(|t| t.text == ".") {
            self.0.clear();
        }
        depth
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Column {
    Verb,
    Source(usize),
    Destination(usize),
}

pub(crate) type Cells = BTreeMap<Column, String>;

pub(crate) fn assignment(code: &str) -> Option<Cells> {
    let lexed = tokens(code);
    let verb = lexed.first()?.text;
    if !(verb.eq_ignore_ascii_case("MOVE") || verb.eq_ignore_ascii_case("SET")) {
        return None;
    }
    let mut starts = vec![(Column::Verb, 0), (Column::Source(0), verb.len())];
    let mut destination = false;
    let mut qualifier = 0;
    for token in &lexed[1..] {
        if token.text.eq_ignore_ascii_case("TO") {
            if destination {
                return None;
            }
            destination = true;
            qualifier = 0;
        } else if token.text.eq_ignore_ascii_case("IN") || token.text.eq_ignore_ascii_case("OF") {
            qualifier += 1;
        } else {
            continue;
        }
        let key = if destination {
            Column::Destination(qualifier)
        } else {
            Column::Source(qualifier)
        };
        starts.push((key, token.start));
    }
    if !destination {
        return None;
    }
    let ends = starts.iter().skip(1).map(|&(_, i)| i).chain([code.len()]);
    let mut cells = Cells::new();
    for (&(column, start), end) in starts.iter().zip(ends) {
        let text = code[start..end].trim();
        // Canonicalize the gap after a column keyword while keeping its operand intact.
        let value = match (column, text.split_once(char::is_whitespace)) {
            (Column::Verb | Column::Source(0), _) | (_, None) => text.to_owned(),
            (_, Some((keyword, operand))) => format!("{keyword} {}", operand.trim_start()),
        };
        cells.insert(column, value);
    }
    Some(cells)
}

pub(crate) fn align(group: &[&Cells]) -> Vec<String> {
    let mut lines = vec![String::new(); group.len()];
    let columns: BTreeSet<_> = group.iter().flat_map(|cells| cells.keys()).collect();
    for col in columns {
        // Only rows sharing this role participate in its width calculation.
        let members: Vec<_> = group
            .iter()
            .enumerate()
            .filter_map(|(i, cells)| cells.get(col).map(|value| (i, value)))
            .collect();
        let width = members
            .iter()
            .map(|&(i, _)| lines[i].chars().count())
            .max()
            .unwrap_or(0);
        let start = width + usize::from(width > 0);
        for (i, value) in members {
            let pad = start - lines[i].chars().count();
            lines[i].extend(std::iter::repeat_n(' ', pad));
            lines[i].push_str(value);
        }
    }
    lines
}

/// Lay out Area B text. Each returned string starts at physical column 12.
pub(crate) fn wrap(
    code: &str,
    depth: usize,
    continuation: bool,
) -> Result<Vec<String>, &'static str> {
    const AREA_B_WIDTH: usize = 72 - 11;
    let mut starts = vec![0];
    for t in tokens(code) {
        if t.start > 0
            && t.kind == TokenKind::Word
            && is_clause(t.text)
            && code[..t.start].ends_with(char::is_whitespace)
        {
            starts.push(t.start);
        }
    }
    starts.push(code.len());
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut column = 0;
    let mut previous_end = 0;
    for window in starts.windows(2) {
        let start = window[0];
        let piece = code[start..window[1]].trim_end();
        let length = piece.chars().count();
        let gap = code[previous_end..start].chars().count();
        if start > 0 && column + gap + length > AREA_B_WIDTH {
            lines.push(std::mem::take(&mut line));
        }
        if line.is_empty() {
            if length > AREA_B_WIDTH {
                return Err("clause cannot fit before column 73 at a keyword boundary");
            }
            let nesting = depth + usize::from(continuation || start != 0);
            // Give long indivisible clauses the space they need, in whole indent levels.
            column = INDENT * nesting.min((AREA_B_WIDTH - length) / INDENT);
            line.extend(std::iter::repeat_n(' ', column));
        } else {
            line.extend(std::iter::repeat_n(' ', gap));
            column += gap;
        }
        line.push_str(piece);
        column += length;
        previous_end = start + piece.len();
    }
    lines.push(line);
    Ok(lines)
}
