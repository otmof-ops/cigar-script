# CigarScript medium and full corpus

Written from the build contract and docs/LANGUAGE.md before the implementation
existed, and run by `tests/corpus.rs` on every `cargo test`. A failing file is a bug in
the implementation or in the corpus, and is settled against the language reference,
never by loosening the file.

## Layout

| folder | files | the test |
|---|---|---|
| `run/` | `NAME.cigar` + `NAME.out` | `cigar run` exits 0 and stdout equals `NAME.out` byte for byte |
| `check/` | `NAME.cigar` + `NAME.err` | `cigar check` fails; the first diagnostic has the code on line 1 of `.err` and a message containing line 2 |
| `runtime/` | `NAME.cigar` + `NAME.err` | `cigar check` passes; `cigar run` fails with the code on line 1 and a message containing line 2 |

Multi-file cases are a folder: `NAME/main.cigar` (the file that is run or checked)
with `NAME/main.out` or `NAME/main.err`, and its modules under `NAME/tools/`.
Modules resolve relative to the folder of `main.cigar`. A runner finds cases as
`*/*.cigar` and `*/*/main.cigar`; `tools/*.cigar` are never cases on their own.

## Rules

- Every file starts with `strength medium` or `strength full` on line 1.
- One cause per check file. A check file has exactly one thing wrong, so the first
  diagnostic is the only one expected. Each file is named by its code.
- Outputs are exact: stdout only, one trailing newline, no effects, no network.
- A `.err` is two lines: the code, then a substring of the message. Substrings are
  deliberately short (the name at fault, or the phrase the spec gives).
- Run programs use no reserved word as a name (`move`, `use`, `into`, `measure`...).

## Where the spec left a choice

Each was chosen when the corpus was written; the implementation follows the choice,
and the language reference records it. Two were settled differently: `E1201` names
types in backticks (``expected `Int` ``), as every message names code; and a
positional enum field is shown as its value (`Shape.circle(2.0)`).

1. **Printing positional enum fields** was unspecified (`_0`), so no run file prints a
   positional variant; `enum-positional` prints only a named one. Settled: a positional
   field shows as its value.
2. **`E1215`** (assign to a `stick`): cig's own check also refuses it. Settled: the type
   checker reports `E1215`, and cig's refusal at the same place is not repeated.
3. **`E1218`** needs a typed cough; I used `cough IoError { message: "x" }` with a
   record, and in `run/typed-cough` read `err.message` off a record cough.
4. **`E1311`/`E1312`/`E1313`** message substrings (`id`, `size`, `Int`) are guesses;
   `E1313` assumes a method on an `Int` with no impl is `E1313`, not "no method".
5. **`E1401`** is built as a user `measure for List` clashing with the built-in one.
6. **`E1203`, `E1208`, `E1206`** and the other message substrings are the names
   involved; only `E1201` (`expected Int`) and `E1211` (missing variants) are from the spec.
7. **`E1211`** for an `Int` match without `_`: the substring is `_` (the catch-all
   that is needed).
8. **`E1304`, `E1212`, `E1214`** had no stated cause in the spec: no file. Settled:
   `E1304` is a name a module does not have, `E1212` a `match` that cannot be read, and
   `E1214` a field or variant named twice.
9. **`E1102`** files are extras (`foreign`, a context other than `retry`/`parallel`).
10. **A literal `Int` into a sized type** (`stick x: u8 = 200`) is assumed statically fine;
    `Int` to `u8` is range-checked at run time. `Int` into `Float` printing is avoided.
11. **`into`** on a List is tested with scalars only. Settled: `rows into seen` appends
    `rows` as one item.
12. **`within retry(n)`**: only the "succeeds on the second try" case is tested.
    Settled: `n` counts tries, so `retry(3)` runs the block at most three times.
13. **`channel<Int>()`** methods are `ch.send(x)`, `ch.receive()`, `ch.close()`
    (as written in the spec); receive-after-close is not tested.
14. **`impl ... for Int`** is used instead of `Path`/`Text` (which are assignable both
    ways and could look ambiguous); `impl` for `Move` and `Int` only.
15. **`E1210` on a record field** is not tested: the spec does not say which code
    `_cg_record` raises on a bad field. `runtime/e1210-untyped-into-list` only expects
    the code and `should be` (the descriptor text for a list is not given).
16. **`measure for Move = move_cost`** is written with a named `pull`, not a lambda.
17. **A match in the head of a block arm** (`ok => { ... }`) is used as a statement only.
