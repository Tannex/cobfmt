# cobfmt

`cobfmt` formats free-format COBOL procedure code with a single style and no
configuration file. It accepts a full program or a procedure snippet.

Given this input,

```cobol
IF active
 MOVE id IN src TO id IN dest
   MOVE customer-name IN src TO customer-name IN dest
 SET ready-flag TO TRUE
END-IF
```

`cobfmt` prints

```cobol
IF active

   MOVE id            IN src TO id            IN dest
   MOVE customer-name IN src TO customer-name IN dest
   SET  ready-flag           TO TRUE

END-IF
```

## Run it

Build and install the command from a checkout with Rust 1.88 or later.

```sh
cargo install --path . --locked
cobfmt examples/accounts.cob
```

The command reads a file or standard input and writes formatted COBOL to
standard output. Use `--check` in a build to get exit status 0 for formatted
input, 1 for a formatting difference, or 2 for an input error.

```sh
cobfmt < program.cob > formatted.cob
cobfmt --check program.cob
```

## Initial rules

- Preserve every line containing a `*>` comment byte for byte, including its
  indentation, trailing spaces, and line ending. A quoted `*>` is literal text.
- Treat adjacent `MOVE` and `SET … TO` statements at the same indentation as one
  group. Put exactly one blank line between the group and surrounding code.
  A comment or existing blank line separates groups.
- Line up the verbs, their operands, `TO`, and corresponding `IN` or `OF`
  qualifiers on either side of `TO`. Use spaces and the longest operand that
  precedes each shared column.
- Indent each nested block by three spaces. The first version handles `IF` and
  `ELSE`, inline `PERFORM`, and `EVALUATE` or `SEARCH` with `WHEN` branches.
- Aim for 80 columns. Wrap before a recognized clause keyword such as `TO`,
  `IN`, `OF`, `AND`, or `GIVING`. Indent continuations three more spaces.
  An indivisible clause or literal can exceed 80 columns.

## Source support

The starter expects UTF-8 free-format source with each procedure statement
starting on its own line. It preserves keyword case, division headers, data
declarations in full programs, and embedded `EXEC … END-EXEC` blocks.
Fixed-column programs are outside this version's scope. An explicit
`>>SOURCE FORMAT FREE` directive identifies free format when the layout is
ambiguous. Continued procedure literals and procedure pseudo-text produce a
diagnostic.

The [example program](examples/accounts.cob) is runnable with GnuCOBOL.

```sh
cobc -free -x examples/accounts.cob -o /tmp/cobfmt-accounts
/tmp/cobfmt-accounts
# 42 Ada
```

Run the formatter and CLI tests with `cargo test --locked`.
