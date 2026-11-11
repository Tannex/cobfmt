use cobfmt::format_source;

fn layout(input: &str, expected: &str) {
    let result = format_source(input).unwrap();
    assert_eq!(result, expected);
    assert_eq!(
        format_source(&result).unwrap(),
        result,
        "second pass changed output"
    );
    assert!(result.lines().all(|line| line.chars().count() <= 80));
}

fn area_b(snippet: &str) -> String {
    snippet
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("           {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn card(sequence: &str, indicator: char, area: &str, id: &str) -> String {
    assert!(area.chars().count() <= 65);
    format!("{sequence:6}{indicator}{area:65}{id:8}")
}

#[test]
fn aligns_numbered_and_unnumbered_statements_in_area_b() {
    layout(
        concat!(
            "000100     IF active\n\n\n000200     MOVE id IN src TO id OF dest\n",
            "           move customer-name of src to customer-name in dest\n",
            "000400     SET ready TO TRUE\n\n\n000500     END-IF"
        ),
        concat!(
            "000100     IF active\n\n",
            "000200        MOVE id            IN src TO id            OF dest\n",
            "              move customer-name of src to customer-name in dest\n",
            "000400        SET  ready                TO TRUE\n\n000500     END-IF"
        ),
    );
}

#[test]
fn preserves_comment_debug_and_page_eject_cards() {
    let opener = "000100     IF ok   *>  keep \"this\"  \r\n";
    let comments = "000300* > a comment '   \n000400/  page   \r\n      D     DISPLAY x  \r\n";
    layout(
        &format!("{opener}           MOVE x TO y\r\n{comments}           END-IF"),
        &format!("{opener}\r\n              MOVE x TO y\r\n\r\n{comments}           END-IF"),
    );
}

#[test]
fn keeps_verbose_literal_continuation_chains_including_intervening_comments() {
    let chain = concat!(
        "000100     MOVE 'literal with spaces   \r\n",
        "000200*  between physical lines   \r\n",
        "      -    'and more spaces\r\n",
        "000400-    ' and the end' TO result.\r\n"
    );
    layout(chain, chain);
    let word = "000100      DISPLAY some-id\n      -    entifier.\n";
    layout(word, word);
}

#[test]
fn wraps_immediately_before_a_keyword_using_column_72() {
    for n in [60, 61, 62] {
        let prefix = format!("MOVE {}", "x".repeat(n - "MOVE  TO y".len()));
        let input = area_b(&format!("{prefix}\nTO y"));
        let expected = if n <= 61 {
            area_b(&format!("{prefix} TO y"))
        } else {
            area_b(&format!("{prefix}\n   TO y"))
        };
        layout(&input, &expected);
    }
}

#[test]
fn retains_metadata_and_record_width_when_wrap_adds_an_unnumbered_card() {
    let if_card = card("000100", ' ', "    IF ok", "FIRST");
    let moved = card(
        "00A200",
        ' ',
        &format!("    MOVE {} TO result", "x".repeat(45)),
        "TO IN",
    );
    let short = card("000300", ' ', "    MOVE x TO y", "LASTMOVE");
    let end = card("000400", ' ', "    END-IF.", "END");
    let input = [if_card.clone(), moved, short, end.clone()].join("\n");
    let output = format_source(&input).unwrap();
    assert!(output.lines().all(|s| s.len() == 80));
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines[0], if_card);
    assert_eq!(lines[1], " ".repeat(80));
    assert_eq!(&lines[2][..6], "00A200");
    assert_eq!(&lines[2][72..], "TO IN   ");
    assert_eq!(lines[3].trim(), "TO result");
    assert_eq!(&lines[3][..7], "       ");
    assert_eq!(&lines[3][72..], "        ");
    assert_eq!(&lines[4][72..], "LASTMOVE");
    assert_eq!(lines.last().unwrap(), &end);
    layout(&input, &output);
}

#[test]
fn preserves_distinct_metadata_on_existing_keyword_continuations() {
    let src = [
        card("000100", ' ', "    MOVE id", "SRC"),
        card("000200", ' ', "       TO out-id", "DEST"),
        card("000300", ' ', "       IN group-id.", "QUAL"),
    ]
    .join("\r\n");
    layout(&src, &src);
}

#[test]
fn numbered_blank_cards_survive_group_spacing() {
    let numbered = card("000200", ' ', "", "VERSION");
    let input = format!("000100     DISPLAY x.\n\n{numbered}\n\n000300     MOVE a TO b.");
    layout(
        &input,
        &format!("000100     DISPLAY x.\n{numbered}\n000300     MOVE a TO b."),
    );
}

#[test]
fn utf8_counts_source_columns_and_keeps_right_hand_text_at_73() {
    let prefix = format!("    DISPLAY '{}'", "é".repeat(51));
    let input = card("00Å100", ' ', &prefix, "ÉD1");
    assert!(input.len() > 80);
    assert_eq!(input.chars().count(), 80);
    layout(&input, &input);
}

#[test]
fn keeps_area_a_data_and_procedure_headers_and_embedded_sql() {
    let prefix = concat!(
        "CBL APOST\n000010 IDENTIFICATION DIVISION.\n000020 PROGRAM-ID. T.\n",
        "000030 DATA DIVISION.\n000040 01  var pic 9.  \n",
        "000050 PROCEDURE DIVISION.\n000060 READ-IT SECTION.\n000070 P1.\n"
    );
    let sql = "000080     EXEC SQL\n               SELECT  x  FROM  tab\n           END-EXEC\n";
    layout(
        &format!(
            "{prefix}           IF active\n{sql}           DISPLAY 1\n           END-IF.\n       END PROGRAM T.\n"
        ),
        &format!(
            "{prefix}           IF active\n{sql}              DISPLAY 1\n           END-IF.\n       END PROGRAM T.\n"
        ),
    );
    let copybook = "000010 01  MY-GROUP.\n000020     05   X PIC X.\n";
    layout(copybook, copybook);
}

#[test]
fn never_breaks_inside_literals_or_qualified_identifiers() {
    let quoted = area_b("MOVE 'TO  IN *> ''quoted''' TO result.\n");
    layout(&quoted, &quoted);
    let long = area_b(&format!("DISPLAY '{}'", "a".repeat(51)));
    layout(&long, &long);
}

#[test]
fn aligns_only_set_to_and_indents_explicit_and_implicit_scopes() {
    let expected = concat!(
        "PERFORM setup\nPERFORM UNTIL done\n   IF found\n      EVALUATE tag\n",
        "         WHEN 1\n            DISPLAY tag\n         WHEN OTHER\n            CONTINUE\n",
        "      END-EVALUATE\n   ELSE\n      DISPLAY 'missing'\n   END-IF\nEND-PERFORM.\n",
        "IF ok\n   IF ready\n      DISPLAY 1.\nSET ix UP BY 1\n\nSET ready TO TRUE\n\nSTOP RUN."
    );
    let messy = expected
        .lines()
        .filter(|s| !s.is_empty())
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    layout(&area_b(&messy), &area_b(expected));
}

#[test]
fn rejects_overwidth_bad_indicator_and_free_directive_without_truncating() {
    for text in [
        "x".repeat(81),
        "123456!    MOVE id TO out".into(),
        "       >>SOURCE FORMAT FREE".into(),
        "           MOVE 'unfinished".into(),
        area_b("COPY thing REPLACING ==a== BY ==b==."),
    ] {
        assert!(
            format_source(&text).unwrap_err().contains("line 1"),
            "{text:?}"
        );
    }
}

#[test]
fn preserves_empty_inputs_and_mixed_comment_endings() {
    for raw in [
        "",
        "\n",
        "  \r\n",
        "000100* hi  ",
        "      *>héj\r\n       *>there  ",
    ] {
        layout(raw, raw);
    }
}

#[test]
fn runnable_example_is_canonical() {
    let example = include_str!("../examples/accounts.cob");
    layout(example, example);
}

#[test]
fn end_prefixed_paragraphs_stay_in_area_a() {
    let program = "000100 PROCEDURE DIVISION.\n000200 END-PROCESSING.\n000300     GOBACK.\n";
    layout(program, program);
}

#[test]
fn a_period_on_a_literal_continuation_ends_the_enclosing_sentence() {
    let program = concat!(
        "           IF ok\n",
        "000200         MOVE 'first\n",
        "000300-    'last' TO dest.\n",
        "000400     DISPLAY dest.\n"
    );
    layout(program, program);
}
