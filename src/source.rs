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
            '=' if chars.next_if(|&(_, c)| c == '=').is_some() => TokenKind::Symbol,
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

const PROGRAM_COLUMNS: usize = 65; // Columns 8 through 72.

fn split_columns(text: &str, count: usize) -> (&str, &str) {
    let end = text
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(i, _)| i);
    text.split_at(end)
}

#[derive(Clone, Copy)]
pub(crate) struct Card<'a> {
    pub raw: &'a str,
    pub ending: &'a str,
    pub number: usize,
    pub sequence: &'a str,
    pub area: &'a str,
    pub identification: &'a str,
    pub width: usize,
    indicator: char,
    control: bool,
    protected: bool,
}

impl Card<'_> {
    pub fn marked(&self) -> bool {
        !self.sequence.trim().is_empty() || !self.identification.trim().is_empty()
    }

    fn blank(&self) -> bool {
        self.indicator == ' ' && self.area.trim().is_empty()
    }
}

pub(crate) fn cards(source: &str) -> Result<Vec<Card<'_>>, String> {
    let mut cards = Vec::new();
    for (i, (raw, ending)) in physical_lines(source).enumerate() {
        let width = raw.chars().count();
        if width > 80 {
            return Err(format!("line {}: record exceeds 80 columns", i + 1));
        }
        let (sequence, rest) = split_columns(raw, 6);
        let (indicator, rest) = split_columns(rest, 1);
        let indicator = indicator.chars().next().unwrap_or(' ');
        let (area, identification) = split_columns(rest, PROGRAM_COLUMNS);
        let control_word = |s: &str| {
            s.split_whitespace()
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("CBL") || w.eq_ignore_ascii_case("PROCESS"))
        };
        let control = control_word(raw) || (indicator == ' ' && control_word(area));
        if !control && !matches!(indicator, ' ' | '*' | '/' | 'D' | 'd' | '-') {
            return Err(format!(
                "line {}: invalid fixed-format indicator in column 7",
                i + 1
            ));
        }
        cards.push(Card {
            raw,
            ending,
            number: i + 1,
            sequence,
            indicator,
            area,
            identification,
            width,
            control,
            protected: false,
        });
    }

    // A hyphen can continue a word or literal. Spacing on its predecessor,
    // including implicit spaces through column 72, is part of the source.
    let mut followed_by_hyphen = false;
    for card in cards.iter_mut().rev() {
        if matches!(card.indicator, '*' | '/' | 'D' | 'd')
            || card.blank()
            || card.area.trim_start().starts_with("*>")
        {
            continue;
        }
        card.protected = followed_by_hyphen || card.indicator == '-';
        followed_by_hyphen = card.indicator == '-';
    }
    Ok(cards)
}

#[derive(PartialEq)]
pub(crate) enum Action {
    Format,
    Keep,
    Blank,
}

pub(crate) struct Record<'a> {
    // A numbered or identified continuation keeps its original keyword boundary.
    // Indices count COBOL tokens, so alignment spaces cannot move a boundary.
    pub anchors: Vec<(usize, Card<'a>)>,
    pub code: Cow<'a, str>,
    pub ending: &'a str,
    pub action: Action,
    token_count: usize,
}

impl Record<'_> {
    pub fn first(&self) -> &Card<'_> {
        &self.anchors[0].1
    }
}

fn area_a_header(lexed: &[Token<'_>], area: &str) -> bool {
    let first = lexed.first().map_or("", |t| t.text).to_ascii_uppercase();
    let second = lexed.get(1).map_or("", |t| t.text).to_ascii_uppercase();
    let statement = matches!(
        first.as_str(),
        "GOBACK"
            | "CONTINUE"
            | "EXIT"
            | "END-IF"
            | "END-EVALUATE"
            | "END-SEARCH"
            | "END-PERFORM"
            | "END-READ"
            | "END-WRITE"
            | "END-CALL"
            | "END-ADD"
            | "END-COMPUTE"
            | "END-DELETE"
            | "END-DIVIDE"
            | "END-MULTIPLY"
            | "END-RECEIVE"
            | "END-RETURN"
            | "END-REWRITE"
            | "END-START"
            | "END-STRING"
            | "END-SUBTRACT"
            | "END-UNSTRING"
            | "END-ACCEPT"
            | "END-DISPLAY"
            | "END-INVOKE"
            | "END-JSON"
            | "END-XML"
    );
    let paragraph = lexed.len() == 2 && second == "." && !area.starts_with("    ") && !statement;
    paragraph
        || second == "SECTION"
        || first == "DECLARATIVES"
        || (first == "END"
            && matches!(
                second.as_str(),
                "DECLARATIVES" | "PROGRAM" | "CLASS" | "METHOD"
            ))
}

struct PreprocessorStatement {
    keyword: &'static str,
    line: usize,
    in_pseudo_text: bool,
}

// Keep COPY and REPLACE records verbatim. Pass only the surrounding COBOL
// to the layout engine, so periods and keywords inside directives cannot close scopes.
fn code_outside_preprocessor(
    code: &str,
    lexed: &[Token<'_>],
    line: usize,
    statement: &mut Option<PreprocessorStatement>,
) -> Option<String> {
    let mut found = statement.is_some();
    let mut outside = String::new();
    let mut from = 0;
    for (index, t) in lexed.iter().enumerate() {
        if let Some(state) = statement {
            match t.text {
                "==" => state.in_pseudo_text = !state.in_pseudo_text,
                "." if !state.in_pseudo_text => {
                    let tail = &code[t.start + 1..];
                    if tail.is_empty()
                        || tail.starts_with(char::is_whitespace)
                        || tail.starts_with("*>")
                    {
                        *statement = None;
                        from = t.start + 1;
                    }
                }
                _ => {}
            }
        } else {
            let keyword = if t.text.eq_ignore_ascii_case("COPY") {
                "COPY"
            } else if t.text.eq_ignore_ascii_case("REPLACE")
                && (index == 0 || lexed[index - 1].text == ".")
            {
                "REPLACE"
            } else {
                continue;
            };
            found = true;
            outside.push_str(&code[from..t.start]);
            outside.push(' ');
            *statement = Some(PreprocessorStatement {
                keyword,
                line,
                in_pseudo_text: false,
            });
        }
    }
    if !found {
        return None;
    }
    if statement.is_none() {
        outside.push_str(&code[from..]);
    }
    Some(outside.trim().to_owned())
}

pub(crate) fn records(cards: Vec<Card<'_>>) -> Result<Vec<Record<'_>>, String> {
    let mut result: Vec<Record<'_>> = Vec::new();
    let mut procedure = true; // Also accept fixed-format procedure snippets.
    let mut embedded = false;
    let mut preprocessor = None;
    let mut first_code = true;
    for card in cards {
        let mut code = card.area.trim();
        let mut action = Action::Keep;
        let mut count = 0;
        if card.control {
            code = "";
        } else if card.blank() {
            action = Action::Blank;
        } else if card.indicator != ' ' && !(card.indicator == '-' && preprocessor.is_some()) {
            let end_sentence = card.indicator == '-'
                && tokens(code)
                    .into_iter()
                    .rfind(|t| t.kind != TokenKind::Comment)
                    .is_some_and(|t| t.text == ".");
            code = if end_sentence { "." } else { "" };
        } else {
            let mut lexed = tokens(code);
            let comment = lexed.last().is_some_and(|t| t.kind == TokenKind::Comment);
            if comment {
                let start = lexed.pop().unwrap().start;
                code = code[..start].trim_end();
            }
            count = lexed.len();
            let first = lexed.first().map_or("", |t| t.text);
            let second = lexed.get(1).map_or("", |t| t.text);
            if (preprocessor.is_some()
                || (!embedded && !first.eq_ignore_ascii_case("EXEC") && !code.starts_with(">>")))
                && let Some(outside) =
                    code_outside_preprocessor(code, &lexed, card.number, &mut preprocessor)
            {
                result.push(Record {
                    anchors: vec![(0, card)],
                    code: Cow::Owned(outside),
                    ending: card.ending,
                    action: Action::Keep,
                    token_count: 0,
                });
                continue;
            }
            let division = !embedded
                && second.eq_ignore_ascii_case("DIVISION")
                && matches!(
                    first.to_ascii_uppercase().as_str(),
                    "ID" | "IDENTIFICATION" | "ENVIRONMENT" | "DATA" | "PROCEDURE"
                );
            if division {
                procedure = first.eq_ignore_ascii_case("PROCEDURE");
            }
            // A source member can be a data copybook without a division header.
            if first_code
                && (matches!(first.to_ascii_uppercase().as_str(), "FD" | "SD")
                    || (first.len() == 2
                        && matches!(first.parse::<u8>(), Ok(1..=49 | 66 | 77 | 78 | 88))
                        && !second.is_empty()
                        && second != "."))
            {
                procedure = false;
            }
            if first.eq_ignore_ascii_case("EXEC") {
                embedded = true;
            }

            let directive = code.strip_prefix(">>");
            if directive.is_none() && !first.is_empty() {
                first_code = false;
            }
            if let Some(text) = directive {
                let mut words = text.split_whitespace();
                if words
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case("SOURCE"))
                    && !words
                        .find(|w| {
                            !w.eq_ignore_ascii_case("FORMAT") && !w.eq_ignore_ascii_case("IS")
                        })
                        .is_some_and(|w| w.eq_ignore_ascii_case("FIXED"))
                {
                    return Err(format!(
                        "line {}: expected fixed reference format",
                        card.number
                    ));
                }
            }
            if !procedure
                || division
                || embedded
                || directive.is_some()
                || area_a_header(&lexed, card.area)
            {
                code = "";
            } else if !comment && !card.protected {
                action = Action::Format;
                if lexed.iter().any(|t| t.kind == TokenKind::UnclosedLiteral) {
                    return Err(format!(
                        "line {}: literal requires a continuation indicator",
                        card.number
                    ));
                }
                if lexed.iter().any(|t| t.text == "==") {
                    return Err(format!(
                        "line {}: procedure pseudo-text is unsupported",
                        card.number
                    ));
                }
                if card.raw.contains('\t') {
                    return Err(format!(
                        "line {}: use spaces for fixed source columns",
                        card.number
                    ));
                }
            }
            if embedded
                && lexed
                    .iter()
                    .any(|t| t.text.eq_ignore_ascii_case("END-EXEC"))
            {
                embedded = false;
                if lexed.last().is_some_and(|t| t.text == ".") {
                    code = ".";
                }
            }
            if action == Action::Format
                && is_clause(first)
                && let Some(previous) = result.last_mut()
                && previous.action == Action::Format
                && !previous.code.ends_with('.')
            {
                if card.marked() {
                    previous.anchors.push((previous.token_count, card));
                }
                previous.code.to_mut().push(' ');
                previous.code.to_mut().push_str(code);
                previous.token_count += count;
                previous.ending = card.ending;
                continue;
            }
        }
        result.push(Record {
            anchors: vec![(0, card)],
            code: Cow::Borrowed(code),
            ending: card.ending,
            action,
            token_count: count,
        });
    }
    if let Some(state) = preprocessor {
        return Err(format!(
            "line {}: unterminated {} statement",
            state.line, state.keyword
        ));
    }
    Ok(result)
}
