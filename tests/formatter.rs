use cobfmt::format_source;

fn layout(input: &str, expected: &str) {
    assert_eq!(format_source(input).unwrap(), expected);
    assert_eq!(
        format_source(expected).unwrap(),
        expected,
        "second pass changed output"
    );
}

#[test]
fn aligns_verbs_qualifiers_and_to_in_one_group() {
    layout(
        "IF active\n\n\nMOVE id IN src TO id OF dest\nmove customer-name of src to customer-name in dest\nSET ready TO TRUE\n\n\nEND-IF",
        concat!(
            "IF active\n\n",
            "   MOVE id            IN src TO id            OF dest\n",
            "   move customer-name of src to customer-name in dest\n",
            "   SET  ready                TO TRUE\n\nEND-IF"
        ),
    );
}

#[test]
fn protects_entire_comment_records_and_their_endings() {
    let opener = "  IF ok   *>  keep \"this\",  spacing  \r\n";
    let comment = "\t*>   MOVE this TO here.  \n";
    layout(
        &format!("{opener}MOVE x TO y\r\n{comment}SET z TO TRUE\r\nEND-IF"),
        &format!("{opener}\r\n   MOVE x TO y\r\n\r\n{comment}\r\n   SET z TO TRUE\r\n\r\nEND-IF"),
    );
}

#[test]
fn wraps_before_to_at_eighty_columns() {
    layout(
        "MOVE customer-reference-number IN customer-record TO destination-reference-number IN destination-record.\n",
        "MOVE customer-reference-number IN customer-record\n   TO destination-reference-number IN destination-record.\n",
    );
    for width in [79, 80, 81] {
        let prefix = format!("MOVE {}", "a".repeat(width - "MOVE  TO x".len()));
        let source = format!("{prefix} TO x");
        let expected = if width > 80 {
            format!("{prefix}\n   TO x")
        } else {
            source.clone()
        };
        layout(&source, &expected);
    }
}

#[test]
fn reassembles_and_wraps_non_assignment_clauses() {
    let condition = format!("IF {} = {}", "a".repeat(32), "b".repeat(32));
    layout(
        &format!("{condition}\nAND counter > threshold\nDISPLAY counter\nEND-IF"),
        &format!("{condition}\n   AND counter > threshold\n   DISPLAY counter\nEND-IF"),
    );
}

#[test]
fn never_splits_a_literal_or_a_keyword_inside_an_identifier() {
    let literal = format!("\"TO  IN *> \"\"quoted\"\" {}\"", "x".repeat(64));
    layout(
        &format!("MOVE {literal} TO result.\n"),
        &format!("MOVE {literal}\n   TO result.\n"),
    );
    let name = "some-IN-field-".repeat(8);
    layout(&format!("DISPLAY {name}"), &format!("DISPLAY {name}"));
}

#[test]
fn separates_groups_from_non_to_set_statements() {
    layout(
        "SET ix UP BY 1\nSET ready TO TRUE\nSET ix DOWN BY 1\n",
        "SET ix UP BY 1\n\nSET ready TO TRUE\n\nSET ix DOWN BY 1\n",
    );
}

#[test]
fn indents_nested_scopes_and_out_of_line_perform() {
    let expected = concat!(
        "PERFORM setup\nPERFORM UNTIL done\n   IF found\n      EVALUATE tag\n",
        "         WHEN 1\n            DISPLAY tag\n         WHEN OTHER\n            CONTINUE\n",
        "      END-EVALUATE\n   ELSE\n      DISPLAY 'missing'\n   END-IF\nEND-PERFORM.\nSTOP RUN."
    );
    let unindented = expected
        .lines()
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    layout(&unindented, expected);
}

#[test]
fn sentence_period_closes_implicit_scopes() {
    layout(
        "IF ok\nIF ready\nDISPLAY 1.\nDISPLAY 2.\n",
        "IF ok\n   IF ready\n      DISPLAY 1.\nDISPLAY 2.\n",
    );
}

#[test]
fn declarations_and_embedded_sql_stay_verbatim() {
    let prelude = concat!(
        "identification division.\nprogram-id. demo.\ndata division.\n",
        "  01  var pic 9.  \nprocedure division.\n"
    );
    let sql = "  EXEC SQL\n    SELECT  x  FROM   tab\n  END-EXEC\n";
    layout(
        &format!("{prelude}IF active\n{sql}DISPLAY 1\nEND-IF.\n"),
        &format!("{prelude}IF active\n{sql}   DISPLAY 1\nEND-IF.\n"),
    );
}

#[test]
fn rejects_detectable_fixed_layout_and_unsupported_tokens() {
    for input in [
        "       identification division.\n",
        "000100 MOVE x TO y\n",
        "      *fixed comment\n",
        ">>SOURCE FORMAT IS FIXED\n",
        "MOVE 'unfinished\nTO x",
        "COPY thing REPLACING ==a== BY ==b==.",
    ] {
        assert!(
            format_source(input).unwrap_err().contains("line 1"),
            "{input:?}"
        );
    }
}

#[test]
fn preserves_blank_and_comment_only_files() {
    for text in ["", "\n", "  \r\n", "  *> hi  ", "*>héj\r\n*>there  "] {
        layout(text, text);
    }
}

#[test]
fn runnable_example_is_canonical() {
    let example = include_str!("../examples/accounts.cob");
    layout(example, example);
}
