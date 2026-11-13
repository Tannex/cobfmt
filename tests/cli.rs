use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cobfmt"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_stdout_and_check_exit_codes() {
    let input = b"000100     IF ok\r\n000200     MOVE x TO y\r\n000300     END-IF.\r\n";
    let formatted = run(&[], input);
    assert!(formatted.status.success());
    assert_eq!(
        formatted.stdout,
        b"000100     IF ok\r\n\r\n000200        MOVE x TO y\r\n\r\n000300     END-IF.\r\n"
    );
    let changes = run(&["--check"], input);
    assert_eq!(changes.status.code(), Some(1));
    assert!(changes.stdout.is_empty());
    assert!(run(&["--check"], &formatted.stdout).status.success());
}

#[test]
fn file_input_and_errors() {
    assert!(
        run(&["--check", "examples/accounts.cob"], b"")
            .status
            .success()
    );
    for args in [&["--bad-option"][..], &["missing.cob"], &[]] {
        let failure = run(
            args,
            b"           DISPLAY a.\n000200!    Invalid indicator.\n",
        );
        assert_eq!(failure.status.code(), Some(2));
        assert!(failure.stdout.is_empty());
        assert!(!failure.stderr.is_empty());
    }
}

#[test]
fn procedure_copy_replacing_formats_and_checks_without_the_copybook() {
    let input = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. COPYDEMO.\n",
        "       PROCEDURE DIVISION.\n",
        "           COPY ERROR-HANDLER REPLACING ==OLD-EXIT==\n",
        "                               BY ==NEW-EXIT==.\n",
        "           GOBACK.\n",
    );
    let formatted = run(&[], input.as_bytes());
    assert!(formatted.status.success());
    assert_eq!(formatted.stdout, input.as_bytes());
    assert!(run(&["--check"], input.as_bytes()).status.success());
    let messy = input.replace("GOBACK.", "   GOBACK.");
    assert_eq!(run(&["--check"], messy.as_bytes()).status.code(), Some(1));
    assert_eq!(run(&[], messy.as_bytes()).stdout, input.as_bytes());
}

#[test]
fn repeated_replace_directives_between_copybooks() {
    let input = concat!(
        "           COPY KONM128K.\n",
        "               REPLACE ==:XXXX:== BY ==FI01==.\n",
        "           COPY BDSISEQI.\n",
        "               REPLACE ==:XXXX:== BY ==F001==.\n",
        "           COPY BDSISEQO.\n",
        "               REPLACE ==:XXXX:== BY ==F002==.\n",
        "           COPY BDSISEQO.\n",
        "               REPLACE OFF.\n",
        "           GOBACK.\n",
    );
    for prefix in [
        "",
        "       PROCEDURE DIVISION.\n",
        "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
    ] {
        let body = if prefix.contains("DATA DIVISION") {
            input.replace(
                "           GOBACK.",
                "       PROCEDURE DIVISION.\n           GOBACK.",
            )
        } else {
            input.to_string()
        };
        let src = format!("{prefix}{body}");
        let formatted = run(&[], src.as_bytes());
        assert!(formatted.status.success());
        assert_eq!(formatted.stdout, src.as_bytes());
        assert!(run(&["--check"], src.as_bytes()).status.success());
    }
}
