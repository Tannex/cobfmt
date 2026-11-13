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

/// Lay the rows out as a table. Each keyword role is one column, as wide as its
/// widest cell. A row without that keyword leaves the cell blank.
fn tabulate(rows: &[&Cells], verb: usize, wanted: impl Fn(&Column) -> bool) -> Vec<String> {
    let mut lines = vec![String::new(); rows.len()];
    let columns: BTreeSet<_> = rows
        .iter()
        .flat_map(|cells| cells.keys())
        .filter(|&column| wanted(column))
        .collect();
    let mut start = 0;
    for column in columns {
        // Every row of the group shares the verb column, wherever its TO is.
        let mut width = if *column == Column::Verb { verb } else { 0 };
        for (line, cells) in lines.iter_mut().zip(rows) {
            if let Some(value) = cells.get(column) {
                let pad = start - line.chars().count();
                line.extend(std::iter::repeat_n(' ', pad));
                line.push_str(value);
                width = width.max(value.chars().count());
            }
        }
        start += width + 1;
    }
    lines
}

/// Align a group of assignments. A row that keeps TO on its verb's record joins
/// one table. A row with TO on a later record aligns its source and its
/// destination with the other such rows.
pub(crate) fn align(group: &[&Cells], split: &[bool]) -> Vec<String> {
    let pick =
        |wanted: bool| -> Vec<usize> { (0..group.len()).filter(|&i| split[i] == wanted).collect() };
    let mut lines = vec![String::new(); group.len()];
    let verb = group
        .iter()
        .filter_map(|cells| cells.get(&Column::Verb))
        .map(|v| v.chars().count())
        .max()
        .unwrap_or(0);
    let whole = pick(false);
    let rows: Vec<_> = whole.iter().map(|&i| group[i]).collect();
    for (&i, line) in whole.iter().zip(tabulate(&rows, verb, |_| true)) {
        lines[i] = line;
    }
    let broken = pick(true);
    let rows: Vec<_> = broken.iter().map(|&i| group[i]).collect();
    let destination = |column: &Column| matches!(column, Column::Destination(_));
    let sources = tabulate(&rows, verb, |column| !destination(column));
    let destinations = tabulate(&rows, verb, destination);
    for ((&i, source), target) in broken.iter().zip(sources).zip(destinations) {
        lines[i] = format!("{source} {target}");
    }
    lines
}

/// Index of the TO keyword of an assignment.
pub(crate) fn to_token(code: &str) -> Option<usize> {
    tokens(code)
        .iter()
        .position(|t| t.kind == TokenKind::Word && t.text.eq_ignore_ascii_case("TO"))
}

/// Width of the text before `token`, or of the whole line.
pub(crate) fn width_before(line: &str, token: Option<usize>) -> usize {
    let lexed = tokens(line);
    let end = token
        .and_then(|i| lexed.get(i))
        .map_or(line.len(), |t| t.start);
    line[..end].trim_end().chars().count()
}

pub(crate) fn fits(depth: usize, width: usize) -> bool {
    INDENT * depth + width <= AREA_B_WIDTH
}

pub(crate) struct Clause<'a> {
    pub token: usize,
    pub start: usize,
    pub first: &'a str,
    text: &'a str,
    gap: usize,
}

/// Split code before each clause keyword and before each token in `breaks`.
pub(crate) fn clauses<'a>(code: &'a str, breaks: &[usize]) -> Vec<Clause<'a>> {
    let lexed = tokens(code);
    let mut starts = lexed
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            *i == 0
                || breaks.contains(i)
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
            start: first.start,
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

/// Column for a subscript on its own record: under the first character of the
/// identifier it belongs to. `positions` holds the printed column of each clause.
pub(crate) fn subscript_column(
    code: &str,
    token: usize,
    positions: &[(usize, usize)],
) -> Option<usize> {
    let lexed = tokens(code);
    let mut operand = token.checked_sub(1)?;
    while operand >= 2
        && ["IN", "OF"]
            .iter()
            .any(|k| lexed[operand - 1].text.eq_ignore_ascii_case(k))
    {
        operand -= 2;
    }
    if operand == 0 {
        return None;
    }
    let at = lexed[operand].start;
    let &(start, column) = positions.iter().rev().find(|&&(start, _)| start <= at)?;
    Some(column + code[start..at].chars().count())
}

/// Lay out Area B text. Each returned string starts at physical column 12 and
/// is paired with the token that starts it. `first_column` places the first clause. `positions` records where each
/// clause was printed, as its offset in the code and its column.
pub(crate) fn wrap(
    parts: &[Clause<'_>],
    depth: usize,
    to_indent: Option<usize>,
    first_column: Option<usize>,
    positions: &mut Vec<(usize, usize)>,
) -> Result<Vec<(usize, String)>, &'static str> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut first = 0;
    let mut column = 0;
    for (index, part) in parts.iter().enumerate() {
        let length = part.text.chars().count();
        if !line.is_empty() && column + part.gap + length > AREA_B_WIDTH {
            lines.push((first, std::mem::take(&mut line)));
        }
        if line.is_empty() {
            first = part.token;
            if length > AREA_B_WIDTH {
                return Err("clause cannot fit before column 73 at a keyword boundary");
            }
            column = match (first_column.filter(|_| index == 0), to_indent) {
                (Some(column), _) => column.min(AREA_B_WIDTH - length),
                (_, Some(indent)) if part.first.eq_ignore_ascii_case("MOVE") && part.token == 0 => {
                    indent - MOVE_TO_OFFSET
                }
                (_, Some(indent)) if part.first.eq_ignore_ascii_case("TO") => {
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
        positions.push((part.start, column));
        line.push_str(part.text);
        column += length;
    }
    lines.push((first, line));
    Ok(lines)
}

fn comparison(text: &str) -> bool {
    matches!(
        text.to_ascii_uppercase().as_str(),
        "=" | "<" | ">" | "NOT" | "IS" | "EQUAL" | "EQUALS" | "GREATER" | "LESS"
    )
}

// Source record, connective token, first operand token, end token.
type Segment = (Option<usize>, Option<usize>, usize, usize);

/// Lay out a condition whose AND and OR start their own records. The
/// connectives end in one column, the conditions start in the next but one,
/// and the comparison operators share a column when that fits. Each line is
/// paired with the index of its source record, or `None` for a new record.
#[allow(clippy::needless_range_loop)] // Token indices also mark the cuts.
pub(crate) fn condition(
    code: &str,
    breaks: &[usize],
    depth: usize,
) -> Option<Vec<(Option<usize>, String)>> {
    let lexed = tokens(code);
    let connective = |i: usize| {
        lexed.get(i).is_some_and(|t| {
            t.kind == TokenKind::Word
                && (t.text.eq_ignore_ascii_case("AND") || t.text.eq_ignore_ascii_case("OR"))
        })
    };
    if breaks.is_empty() || !breaks.iter().all(|&b| connective(b)) {
        return None;
    }
    let word = |i: usize| {
        lexed
            .get(i)
            .map_or(String::new(), |t| t.text.to_ascii_uppercase())
    };
    let head_tokens = match word(0).as_str() {
        "IF" | "EVALUATE" | "WHEN" | "UNTIL" => 1,
        "ELSE" if word(1) == "IF" => 2,
        "PERFORM" => {
            lexed
                .iter()
                .position(|t| t.text.eq_ignore_ascii_case("UNTIL"))?
                + 1
        }
        _ => return None,
    };
    let end = |i: usize| lexed[i].start + lexed[i].text.len();
    // Tokens with one space wherever the source has any, so that the layout
    // does not depend on the spacing it replaces.
    let squeeze = |from: usize, to: usize| {
        let mut text = String::new();
        for i in from..to {
            if i > from && lexed[i].start > end(i - 1) {
                text.push(' ');
            }
            text.push_str(lexed[i].text);
        }
        text
    };
    let head = squeeze(0, head_tokens);
    let operand_column = (head.chars().count() + 1).max(INDENT + 4);

    let mut segments: Vec<Segment> = vec![(Some(0), None, head_tokens, breaks[0])];
    for (k, &b) in breaks.iter().enumerate() {
        let next = breaks.get(k + 1).copied().unwrap_or(lexed.len());
        segments.push((Some(k + 1), Some(b), b + 1, next));
    }
    loop {
        if segments.iter().any(|&(_, _, from, to)| from >= to) {
            return None;
        }
        let inner = |&(_, _, from, to): &Segment| {
            (from + 1..to)
                .filter(|&i| connective(i) && code[..lexed[i].start].ends_with(char::is_whitespace))
                .collect::<Vec<_>>()
        };
        let pieces: Vec<(String, Option<String>)> = segments
            .iter()
            .map(|&(_, _, from, to)| {
                let mut nesting = 0usize;
                for i in from..to {
                    let t = &lexed[i];
                    match t.text {
                        "(" => nesting += 1,
                        ")" => nesting = nesting.saturating_sub(1),
                        _ if nesting == 0
                            && i > from
                            && t.kind != TokenKind::Literal
                            && comparison(t.text) =>
                        {
                            return (squeeze(from, i), Some(squeeze(i, to)));
                        }
                        _ => {}
                    }
                }
                (squeeze(from, to), None)
            })
            .collect();
        let mut overflow = Vec::new();
        for aligned in [true, false] {
            let width = pieces
                .iter()
                .filter(|p| aligned && p.1.is_some())
                .map(|p| p.0.chars().count())
                .max()
                .unwrap_or(0);
            let lines: Vec<String> = segments
                .iter()
                .zip(&pieces)
                .map(|(&(_, keyword, _, _), (left, rest))| {
                    let mut line = " ".repeat(INDENT * depth);
                    match keyword {
                        None => line.push_str(&format!("{head:<operand_column$}")),
                        Some(k) => {
                            let keyword = lexed[k].text;
                            line.push_str(&format!("{keyword:>w$} ", w = operand_column - 1));
                        }
                    }
                    match rest {
                        Some(rest) => line.push_str(&format!("{left:<width$} {rest}")),
                        None => line.push_str(left),
                    }
                    line
                })
                .collect();
            overflow = (0..lines.len())
                .filter(|&i| lines[i].chars().count() > AREA_B_WIDTH)
                .collect();
            if overflow.is_empty() {
                let records = segments.iter().map(|s| s.0);
                return Some(records.zip(lines).collect());
            }
            if overflow.iter().any(|&i| !inner(&segments[i]).is_empty()) {
                break;
            }
        }
        // A line that is too long continues on new records at its inner AND and OR.
        let index = overflow
            .into_iter()
            .find(|&i| !inner(&segments[i]).is_empty())?;
        let (record, keyword, from, to) = segments[index];
        let cuts = inner(&segments[index]);
        let mut replacement = vec![(record, keyword, from, cuts[0])];
        for (n, &cut) in cuts.iter().enumerate() {
            let next = cuts.get(n + 1).copied().unwrap_or(to);
            replacement.push((None, Some(cut), cut + 1, next));
        }
        segments.splice(index..=index, replacement);
    }
}
