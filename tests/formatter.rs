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
            "000100     IF active\n\n\n",
            "000200        MOVE id            IN src TO id            OF dest\n",
            "              move customer-name of src to customer-name in dest\n",
            "000400        SET  ready                TO TRUE\n\n\n000500     END-IF"
        ),
    );
}

#[test]
fn preserves_comment_debug_and_page_eject_cards() {
    let opener = "000100     IF ok   *>  keep \"this\"  \r\n";
    let comments = "000300* > a comment '   \n000400/  page   \r\n      D     DISPLAY x  \r\n";
    layout(
        &format!("{opener}           MOVE x TO y\r\n{comments}           END-IF"),
        &format!("{opener}              MOVE x TO y\r\n{comments}           END-IF"),
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
    // Inside the IF, a statement of 58 columns ends in column 72.
    for n in [57, 58, 59] {
        for (verb, keyword, hang) in [("DISPLAY", "UPON", "      "), ("MOVE", "TO", "     ")] {
            let prefix = format!("{verb} {}", "x".repeat(n - verb.len() - keyword.len() - 4));
            let input = area_b(&format!("IF ok\n{prefix} {keyword} y\nEND-IF."));
            let expected = if n <= 58 {
                area_b(&format!("IF ok\n   {prefix} {keyword} y\nEND-IF."))
            } else {
                area_b(&format!("IF ok\n   {prefix}\n{hang}{keyword} y\nEND-IF."))
            };
            layout(&input, &expected);
        }
    }
}

#[test]
fn keeps_a_statement_split_before_a_keyword_on_separate_records() {
    layout(
        &area_b("MOVE a\nTO b\nDISPLAY c\nUPON d\nMOVE e TO f"),
        &area_b("MOVE a\n  TO b\nDISPLAY c\n   UPON d\nMOVE e TO f"),
    );
}

#[test]
fn puts_a_subscript_record_under_the_identifier_it_belongs_to() {
    layout(
        &area_b("MOVE amt TO total IN entry\n    (idx)\nMOVE a TO b"),
        &area_b("MOVE amt TO total IN entry\n            (idx)\nMOVE a   TO b"),
    );
}

#[test]
fn lays_out_an_assignment_group_as_a_table_with_blank_cells() {
    layout(
        &area_b("MOVE long-source-name TO x\nMOVE a IN grp TO b IN out\nSET done TO TRUE"),
        &area_b(concat!(
            "MOVE long-source-name        TO x\n",
            "MOVE a                IN grp TO b    IN out\n",
            "SET  done                    TO TRUE"
        )),
    );
}

#[test]
fn a_row_too_wide_for_the_table_continues_at_to_and_the_rest_stay_aligned() {
    // The first row fits in column 12 but not inside the IF.
    let source = "s".repeat(22);
    let target = "t".repeat(22);
    layout(
        &area_b(&format!(
            "IF ok\nMOVE {source} IN grp TO {target}\nMOVE a TO b\nSET done TO TRUE\nEND-IF."
        )),
        &area_b(&format!(
            "IF ok\n   MOVE {source} IN grp\n     TO {target}\n   MOVE a    TO b\n   SET  done TO TRUE\nEND-IF."
        )),
    );
}

#[test]
fn aligns_conditions_and_operators_when_and_or_start_their_own_records() {
    layout(
        &area_b(concat!(
            "IF WS-TYPE = 'A'\nAND WS-STATUS-CODE = '00'\nOR WS-OVERRIDE-FLAG = 'Y'\n",
            "PERFORM 1000-WRITE\nEND-IF\n",
            "EVALUATE WS-TYPE = 'A'\nAND WS-STATUS-CODE = '00'\nOR WS-READY\n",
            "WHEN TRUE\nCONTINUE\nEND-EVALUATE\n",
            "IF a = 1 AND b = 2\nCONTINUE\nEND-IF."
        )),
        &area_b(concat!(
            "IF     WS-TYPE          = 'A'\n",
            "   AND WS-STATUS-CODE   = '00'\n",
            "    OR WS-OVERRIDE-FLAG = 'Y'\n",
            "   PERFORM 1000-WRITE\nEND-IF\n",
            "EVALUATE WS-TYPE        = 'A'\n",
            "     AND WS-STATUS-CODE = '00'\n",
            "      OR WS-READY\n",
            "   WHEN TRUE\n      CONTINUE\nEND-EVALUATE\n",
            "IF a = 1 AND b = 2\n   CONTINUE\nEND-IF."
        )),
    );
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
    assert_eq!(&lines[1][..6], "00A200");
    assert_eq!(&lines[1][72..], "TO IN   ");
    assert_eq!(lines[2].trim(), "TO result");
    assert_eq!(
        lines[1].find("MOVE").unwrap() + 3,
        lines[2].find("TO result").unwrap() + 1
    );
    assert_eq!(&lines[2][..7], "       ");
    assert_eq!(&lines[2][72..], "        ");
    assert_eq!(&lines[3][72..], "LASTMOVE");
    assert_eq!(lines.last().unwrap(), &end);
    layout(&input, &output);
}

#[test]
fn preserves_distinct_metadata_on_existing_keyword_continuations() {
    let src = [
        card("000100", ' ', "    MOVE id", "SRC"),
        card("000200", ' ', "      TO out-id", "DEST"),
        card("000300", ' ', "       IN group-id.", "QUAL"),
    ]
    .join("\r\n");
    layout(&src, &src);
}

#[test]
fn aligns_to_with_the_actual_move_column_when_either_clause_is_long() {
    for (source, destination) in [
        (format!("'{}'", "x".repeat(52)), "target".to_owned()),
        (
            "source".to_owned(),
            format!("{} {}", "a".repeat(27), "b".repeat(28)),
        ),
    ] {
        let input = area_b(&format!("IF ok\nmove {source}\nto {destination}\nEND-IF."));
        let expected = area_b(&format!(
            "IF ok\nmove {source}\n  to {destination}\nEND-IF."
        ));
        layout(&input, &expected);
    }
}

#[test]
fn wrapping_does_not_depend_on_extra_spaces_before_a_clause_keyword() {
    // With its 15-space gap the indented ADD crosses column 72, but the wrapped
    // form rejoins with one space and fits. Both passes must agree.
    let input = area_b(concat!(
        "IF retry-count > 0\nIF status = '00'\n",
        "ADD WS-LINE-AMOUNT               TO WS-INVOICE-TOTAL-AMOUNT\n",
        "END-IF\nEND-IF."
    ));
    let expected = area_b(concat!(
        "IF retry-count > 0\n   IF status = '00'\n",
        "      ADD WS-LINE-AMOUNT TO WS-INVOICE-TOTAL-AMOUNT\n",
        "   END-IF\nEND-IF."
    ));
    layout(&input, &expected);
}

#[test]
fn set_continuation_uses_the_normal_hanging_indent() {
    let input = "000100     SET my-flag\n000200     TO TRUE";
    layout(input, "000100     SET my-flag\n000200        TO TRUE");
}

#[test]
fn reports_a_to_clause_that_cannot_fit_even_with_move_in_column_12() {
    let targets = format!("{} {}", "a".repeat(28), "b".repeat(28));
    let input = area_b(&format!("MOVE src\nTO {targets}"));
    assert!(
        format_source(&input)
            .unwrap_err()
            .contains("aligned TO clause cannot fit")
    );
}

#[test]
fn aligns_across_comments_using_the_printed_move_position() {
    let input = concat!(
        "000100     IF ok\n",
        "000200     MOVE src\n",
        "000210* Keep this text here.\n",
        "000300     TO dst\n",
        "000400         Move x   *> keep these spaces\n",
        "000500     to y\n",
        "000600     END-IF."
    );
    let expected = concat!(
        "000100     IF ok\n",
        "000200        MOVE src\n",
        "000210* Keep this text here.\n",
        "000300          TO dst\n",
        "000400         Move x   *> keep these spaces\n",
        "000500           to y\n",
        "000600     END-IF."
    );
    layout(input, expected);
}

#[test]
fn keeps_blank_cards_as_written_and_adds_none() {
    let numbered = card("000200", ' ', "", "VERSION");
    let input = format!(
        "000100     DISPLAY x.\n\n{numbered}\n\n000300     MOVE a TO b.\n000400     DISPLAY y."
    );
    layout(&input, &input);
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
        "IF ok\n   IF ready\n      DISPLAY 1.\nSET ix UP BY 1\nSET ready TO TRUE\nSTOP RUN."
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
        area_b("COPY thing REPLACING ==a== BY ==b."),
    ] {
        assert!(
            format_source(&text).unwrap_err().contains("line 1"),
            "{text:?}"
        );
    }
}

#[test]
fn preserves_copy_cards_without_interpreting_replacement_text_as_code() {
    let copy = [
        card(
            "000100",
            ' ',
            "    cOpY my-book replacing ==old== BY ==",
            "ID.==",
        ),
        card("000200", ' ', "    END-IF.   DISPLAY '==.'", "ID2"),
        card("000300", '*', " . == in a comment", "ID3"),
        card("000400", ' ', "    ==   *> . == in another comment", "ID4"),
        card("000500", ' ', "    ==other== BY ====.", "ID5"),
    ]
    .join("\r\n");
    let input = format!("           IF ok\r\n{copy}\r\n           DISPLAY x\r\n           END-IF.");
    let expected = input.replace("           DISPLAY x", "              DISPLAY x");
    layout(&input, &expected);
}

#[test]
fn copy_terminator_after_a_literal_continuation_resumes_normal_formatting() {
    let copy = concat!(
        "000100     COPY my-book REPLACING ==old== BY =='long\n",
        "000200-    'quoted ==. text'==."
    );
    let input = format!("{copy}\n             DISPLAY 'COPY'.");
    layout(&input, &format!("{copy}\n           DISPLAY 'COPY'."));
}

#[test]
fn preserves_copy_with_surrounding_code_on_the_same_record() {
    let input = area_b("IF ok COPY a REPLACING ==x== BY ==y==. COPY b.\nDISPLAY x\nEND-IF.");
    layout(
        &input,
        &input.replace("           DISPLAY", "              DISPLAY"),
    );
}

#[test]
fn preserves_multiline_replace_then_formats_following_statements() {
    let directive = [
        card("000100", ' ', "      rEpLaCe ==:XXXX:== BY ==", "TEST0001"),
        card("000200", '*', " Comment with == and a period.", "TEST0002"),
        card(
            "000300",
            ' ',
            "          'a.==b'==  *> keep this comment",
            "TEST0003",
        ),
        card("000400", ' ', "          ==END-IF.== BY ====.", "TEST0004"),
        card("000500", ' ', "      replace off.", "TEST0005"),
    ]
    .join("\r\n");
    let src = format!(
        "           DISPLAY 'REPLACE'.\r\n{directive}\r\n             IF ok\r\n           DISPLAY x\r\n           END-IF."
    );
    let expected = src
        .replace("             IF ok", "           IF ok")
        .replace("           DISPLAY x", "              DISPLAY x");
    layout(&src, &expected);
}

#[test]
fn reports_the_start_of_an_unterminated_replace() {
    let input = area_b("COPY prefix.\nREPLACE ==:XXXX:== BY ==open.\n");
    assert_eq!(
        format_source(&input).unwrap_err(),
        "line 2: unterminated REPLACE statement"
    );
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
