# cobfmt

`cobfmt` is an opinionated Rust formatter for IBM Enterprise COBOL on z/OS.
It reads fixed-format source members and procedure snippets from a UTF-8
checkout. There is one style and no configuration file.

## Source columns

| Columns | Field | Formatter behavior |
| --- | --- | --- |
| 1–6 | Optional sequence field | Preserve existing numbers or labels. Give new cards a blank sequence field. |
| 7 | Indicator | Preserve comment, page-eject, debugging, and hyphen-continuation records. |
| 8–11 | Area A | Keep headers, paragraph names, and declarations in place. |
| 12–72 | Area B | Lay out procedure statements here. Start at column 12 and indent nested blocks by three spaces. |
| 73–80 | Identification | Keep the original text at column 73 on its corresponding record. |

The formatter accepts space-padded 80-column records and text exports with
trailing blanks omitted. For an 80-column deck it pads generated records to 80
columns, including blank separators. Each decoded character occupies one source
column for single-byte host source. Checkout and check-in handle EBCDIC/UTF-8
conversion. [IBM reference format](https://www.ibm.com/docs/en/cobol-zos/6.5.0?topic=structure-reference-format)
and [identification area](https://www.ibm.com/docs/SSBDYH_3.2/com.ibm.zexpl.ug.rseforzos.doc/topics/t_compare_lang_pref.html).

## Example

Input may mix numbered and unnumbered records.

```cobol
000100     IF active
000200     MOVE id IN src TO id IN dest
           MOVE customer-name IN src TO customer-name IN dest
000400     SET ready-flag TO TRUE
000500     END-IF
```

Formatted output:

```cobol
000100     IF active

000200        MOVE id            IN src TO id            IN dest
              MOVE customer-name IN src TO customer-name IN dest
000400        SET  ready-flag           TO TRUE

000500     END-IF
```

## Formatting rules

- Surround adjacent `MOVE` and `SET … TO` statements at the same depth with one
  blank record. Align the verbs, operands, `TO`, and corresponding `IN` or `OF`
  qualifiers with spaces. Preserve numbered or identified blank records.
- Wrap before a clause keyword such as `TO`, `IN`, `AND`, or `GIVING` before
  code would cross column 72. New continuation records start in Area B with a
  blank indicator. Existing numbered or identified keyword continuations keep
  their record boundary.
- Indent `IF`/`ELSE`, inline `PERFORM`, and `EVALUATE` or `SEARCH` with `WHEN`
  branches by three spaces per level. For an indivisible clause, use the
  deepest whole indent that fits. Report an error if it cannot fit in Area B.
- Preserve entire comment records byte for byte, including `*` and `/` in
  column 7 and floating `*>` comments. Preserve the source record and all
  hyphen continuation records of a split word or literal.
- Preserve keyword case, declarations, and embedded `EXEC … END-EXEC` blocks.
  This version expects each new procedure statement on its own record and
  reports unsupported procedure pseudo-text with a line number.

## Run it

Build and install from a checkout with Rust 1.88 or later.

```sh
cargo install --path . --locked
cobfmt examples/accounts.cob
cobfmt < program.cob > formatted.cob
cobfmt --check program.cob
```

The command writes to standard output. With `--check` it exits with 0 for
formatted input, 1 for a formatting difference, or 2 for an input error.

Run the formatter and CLI tests with `cargo test --locked`. The numbered
[example program](examples/accounts.cob) prints `42 Ada` and can also be
compiled locally with GnuCOBOL in fixed reference format.

```sh
cobc -fixed -x examples/accounts.cob -o /tmp/cobfmt-accounts
/tmp/cobfmt-accounts
```
