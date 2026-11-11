use std::borrow::Cow;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TokenKind {
    Word,
    Literal,
    Symbol,
    Comment,
    UnclosedLiteral,
}

pub(crate) struct Token<'a> {
    pub text: &'a str,
    pub start: usize,
    pub kind: TokenKind,
}

pub(crate) fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut result = Vec::new();
    let mut chars = text.char_indices().peekable();
    let word_char = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    while let Some((start, ch)) = chars.next() {
        let kind = match ch {
            c if c.is_whitespace() => continue,
            '*' if chars.next_if(|&(_, c)| c == '>').is_some() => {
                result.push(Token {
                    start,
                    text: &text[start..],
                    kind: TokenKind::Comment,
                });
                break;
            }
            '\'' | '"' => {
                let mut kind = TokenKind::UnclosedLiteral;
                while let Some((_, c)) = chars.next() {
                    if c == ch && chars.next_if(|&(_, c)| c == ch).is_none() {
                        kind = TokenKind::Literal;
                        break;
                    }
                }
                kind
            }
            c if word_char(c) => {
                while chars.next_if(|&(_, c)| word_char(c)).is_some() {}
                TokenKind::Word
            }
            _ => TokenKind::Symbol,
        };
        let end = chars.peek().map_or(text.len(), |&(i, _)| i);
        result.push(Token {
            start,
            text: &text[start..end],
            kind,
        });
    }
    result
}

pub(crate) fn is_clause(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "TO" | "IN"
            | "OF"
            | "FROM"
            | "BY"
            | "INTO"
            | "GIVING"
            | "USING"
            | "RETURNING"
            | "WITH"
            | "UNTIL"
            | "VARYING"
            | "AFTER"
            | "BEFORE"
            | "AND"
            | "OR"
            | "THEN"
            | "UPON"
            | "DELIMITED"
            | "ON"
    )
}

fn physical_lines(mut source: &str) -> impl Iterator<Item = (&str, &str)> {
    std::iter::from_fn(move || {
        if source.is_empty() {
            return None;
        }
        let end = source.find(['\r', '\n']).unwrap_or(source.len());
        let (line, tail) = source.split_at(end);
        let size = if tail.starts_with("\r\n") {
            2
        } else {
            usize::from(!tail.is_empty())
        };
        let (ending, rest) = tail.split_at(size);
        source = rest;
        Some((line, ending))
    })
}

fn fixed_record(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 7 {
        return false;
    }
    let sequence = &bytes[..6];
    let indicator = bytes[6];
    (sequence.iter().all(u8::is_ascii_digit) && b" */Dd-".contains(&indicator))
        || (sequence == b"      "
            && (matches!(indicator, b'/' | b'-')
                || (indicator == b'*' && bytes.get(7) != Some(&b'>'))))
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Format,
    Keep,
    // Keep the original line, but use this prefix to track block indentation.
    KeepWithCode(usize),
}

pub(crate) struct Record<'a> {
    pub text: Cow<'a, str>,
    pub ending: &'a str,
    action: Action,
}

impl Record<'_> {
    pub fn verbatim(&self) -> bool {
        self.action != Action::Format
    }

    pub fn code(&self) -> &str {
        match self.action {
            Action::Format => &self.text,
            Action::Keep => "",
            Action::KeepWithCode(end) => &self.text[..end],
        }
    }
}

pub(crate) fn records(source: &str) -> Result<Vec<Record<'_>>, String> {
    let mut result: Vec<Record<'_>> = Vec::new();
    let mut procedure = true; // Also accept standalone procedure snippets.
    let mut explicit_free = false;
    let mut embedded = false;
    for (index, (line, ending)) in physical_lines(source).enumerate() {
        let error = |message: &str| format!("line {}: {message}", index + 1);
        let directive = line.trim_start().strip_prefix(">>");
        if let Some(directive) = directive {
            let mut words = directive.split_whitespace();
            if words
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("SOURCE"))
            {
                let format = words
                    .find(|w| !w.eq_ignore_ascii_case("FORMAT") && !w.eq_ignore_ascii_case("IS"));
                if !format.is_some_and(|w| w.eq_ignore_ascii_case("FREE")) {
                    return Err(error("only free-format COBOL is supported"));
                }
                explicit_free = true;
            }
        }
        let mut lexed = tokens(line);
        let comment = lexed
            .last()
            .filter(|t| t.kind == TokenKind::Comment)
            .map(|t| t.start);
        if comment.is_some() {
            lexed.pop();
        }
        let first = lexed.first().map_or("", |t| t.text);
        let division = lexed
            .get(1)
            .is_some_and(|t| t.text.eq_ignore_ascii_case("DIVISION"))
            && matches!(
                first.to_ascii_uppercase().as_str(),
                "ID" | "IDENTIFICATION" | "ENVIRONMENT" | "DATA" | "PROCEDURE"
            );
        if !explicit_free && (fixed_record(line) || (division && line.starts_with("       "))) {
            return Err(error("input looks like fixed-format COBOL"));
        }
        if division {
            procedure = first.eq_ignore_ascii_case("PROCEDURE");
        }
        if first.eq_ignore_ascii_case("EXEC") {
            embedded = true;
        }
        let action = if !procedure || division || embedded || directive.is_some() {
            Action::Keep
        } else {
            if lexed.iter().any(|t| t.kind == TokenKind::UnclosedLiteral) {
                return Err(error("continued literals are unsupported"));
            }
            if lexed
                .iter()
                .any(|t| t.text == "=" && line[t.start..].starts_with("=="))
            {
                return Err(error("COBOL pseudo-text is unsupported"));
            }
            match comment {
                Some(end) => Action::KeepWithCode(end),
                None => Action::Format,
            }
        };
        if embedded
            && lexed
                .iter()
                .any(|t| t.text.eq_ignore_ascii_case("END-EXEC"))
        {
            embedded = false;
        }
        if action == Action::Format
            && is_clause(first)
            && let Some(previous) = result.last_mut()
            && !previous.verbatim()
            && !previous.text.trim().is_empty()
            && !previous.text.ends_with('.')
        {
            let text = previous.text.to_mut();
            text.push(' ');
            text.push_str(line.trim());
            previous.ending = ending;
            continue;
        }
        let text = if action == Action::Format && !lexed.is_empty() {
            line.trim()
        } else {
            line
        };
        result.push(Record {
            text: Cow::Borrowed(text),
            ending,
            action,
        });
    }
    Ok(result)
}
