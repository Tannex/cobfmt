use std::collections::{BTreeMap, BTreeSet};

use crate::source::{Action, Record, TokenKind, is_clause, tokens};

const INDENT: usize = 3;
const AREA_B_WIDTH: usize = 72 - 11;
const MOVE_TO_OFFSET: usize = "MOVE".len() - "TO".len();

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

pub(crate) struct Clause<'a> {
    pub token: usize,
    first: &'a str,
    text: &'a str,
    gap: usize,
}

pub(crate) fn clauses(code: &str) -> Vec<Clause<'_>> {
    let lexed = tokens(code);
    let mut starts = lexed
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            *i == 0
                || (t.kind == TokenKind::Word
                    && is_clause(t.text)
                    && code[..t.start].ends_with(char::is_whitespace))
        })
        .peekable();
    let mut parts = Vec::new();
    let mut previous_end = 0;
    while let Some((token, first)) = starts.next() {
        let end = starts.peek().map_or(code.len(), |(_, t)| t.start);
        let text = code[first.start..end].trim_end();
        parts.push(Clause {
            token,
            first: first.text,
            text,
            gap: code[previous_end..first.start].chars().count(),
        });
        previous_end = first.start + text.len();
    }
    parts
}

/// Rows outside an alignment group use one space before each clause keyword,
/// so a wrapped statement and its rejoined form measure the same.
pub(crate) fn collapse_gaps(parts: &mut [Clause<'_>]) {
    for part in parts {
        part.gap = part.gap.min(1);
    }
}

/// Choose the TO column once for a MOVE, even across numbered or comment cards.
pub(crate) fn move_to_indent(
    parts: &[Clause<'_>],
    depth: usize,
    record: &Record<'_>,
    pending: &mut Option<usize>,
) -> Result<Option<usize>, &'static str> {
    let Some(head) = parts.first() else {
        return Ok(None);
    };
    let to = parts.iter().find(|p| p.first.eq_ignore_ascii_case("TO"));
    if head.first.eq_ignore_ascii_case("MOVE") {
        *pending = if record.action == Action::Keep {
            // The comment or literal continuation fixes MOVE's physical position.
            let area = record.first().area;
            let leading = area.chars().take_while(|c| c.is_whitespace()).count();
            (leading + MOVE_TO_OFFSET).checked_sub(4) // Area A is four columns wide.
        } else {
            let mut room = AREA_B_WIDTH.saturating_sub(head.text.chars().count());
            if let Some(to) = to {
                let dest_room = AREA_B_WIDTH
                    .checked_sub(MOVE_TO_OFFSET + to.text.chars().count())
                    .ok_or("aligned TO clause cannot fit before column 73 at a keyword boundary")?;
                room = room.min(dest_room);
            }
            Some(INDENT * depth.min(room / INDENT) + MOVE_TO_OFFSET)
        };
    } else if !is_clause(head.first) {
        *pending = None;
    }
    let indent = *pending;
    if to.is_some() || record.code.ends_with('.') {
        *pending = None;
    }
    Ok(indent)
}

/// Lay out Area B text. Each returned string starts at physical column 12.
pub(crate) fn wrap(
    parts: &[Clause<'_>],
    depth: usize,
    to_indent: Option<usize>,
) -> Result<Vec<String>, &'static str> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut column = 0;
    for part in parts {
        let length = part.text.chars().count();
        if !line.is_empty() && column + part.gap + length > AREA_B_WIDTH {
            lines.push(std::mem::take(&mut line));
        }
        if line.is_empty() {
            if length > AREA_B_WIDTH {
                return Err("clause cannot fit before column 73 at a keyword boundary");
            }
            column = match to_indent {
                Some(indent) if part.first.eq_ignore_ascii_case("MOVE") && part.token == 0 => {
                    indent - MOVE_TO_OFFSET
                }
                Some(indent) if part.first.eq_ignore_ascii_case("TO") => {
                    if indent + length > AREA_B_WIDTH {
                        return Err(
                            "aligned TO clause cannot fit before column 73 at a keyword boundary",
                        );
                    }
                    indent
                }
                _ => {
                    let nesting = depth + usize::from(part.token != 0);
                    INDENT * nesting.min((AREA_B_WIDTH - length) / INDENT)
                }
            };
            line.extend(std::iter::repeat_n(' ', column));
        } else {
            line.extend(std::iter::repeat_n(' ', part.gap));
            column += part.gap;
        }
        line.push_str(part.text);
        column += length;
    }
    lines.push(line);
    Ok(lines)
}
