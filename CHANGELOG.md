<!-- SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script) -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Changelog

All notable changes to CigarScript. The format follows Keep a Changelog; the
project follows semantic versioning.

## Unreleased

- **`cigar fmt`, the tree-sitter grammar and the VS Code extension (#16, part 2).**
  `cigar fmt <path>...` rewrites `.cigar` and `.cig` files in one style with no
  options (two-space indent, spacing and blank lines normalized, every comment
  kept); `--check` changes nothing, names what would change and exits `2`, for
  CI. It reads files with the lexer `cigar check` uses (no second parser), is
  idempotent, and refuses a file whose tokens would change: a formatted file
  has the same diagnostics and the same lowered program. `cigar-lsp` formats
  the document too (`textDocument/formatting`, the same formatter).
  `editors/tree-sitter-cigarscript/` is a grammar with highlight queries and a
  test corpus; `editors/vscode/` is an extension (a TextMate grammar and a
  client that starts `cigar-lsp` from `PATH` or a setting; no telemetry, no
  network; `vscode-languageclient` is its one dependency, built with `npm ci`
  from a committed lockfile). `.github/workflows/editors.yml` builds both and
  runs `cigar fmt --check` on the repository's own files. Style rules:
  [commands](docs/commands.md#cigar-fmt); setup: [editors](docs/editors.md).
  Tests: `tests/fmt.rs` (idempotent over the corpus and CigScript's files,
  comments kept, the same check results, `--check` exits non-zero) and
  a formatting test in `tests/lsp.rs`.

- **A crash in cigar joins the family's one queue (cig-installer#10).** A panic is written
  into cig's ashtray as a report from `cigar-script`, handed over the way `cigo` does it (`cig
  crash submit --origin cigar-script --repo otmof-ops/cigar-script`), or, with no `cig` on the
  machine, written in the same schema (built with cig's own crash code, so the redaction is cig's:
  every path a shape, the command line only the words cigar knows). `ciginstaller crash list`
  shows it and `ciginstaller crash send` files it here. Nothing is sent by the crash itself, and
  `reports = never` (the installer's setting, `CIG_REPORTS`, `CIGI_REPORTS`, cig's own config, or a
  machine lock the installer reports) saves nothing. `src/crash.rs` has the tests: a deliberate
  panic in a child of the test binary leaves one report with `origin` `cigar-script`, and every way
  of saying `never` leaves none.

- **`cigar-lsp`, the language server (#16, part 1).** A second program, built
  from the same crates as the checker (no second parser): the Language Server
  Protocol over stdio. Diagnostics on every change with their codes and
  severities (errors, `W12xx` warnings, a `.cig` file's hints); hover with a
  name's type, an error code's kind of no, and the layers of an `into`,
  `measure` or `within` (the same reading as `cigar explain --layers`);
  go to definition and find references for functions, `stick`/`roll` names,
  parameters, records, enums, fields and the functions of `use`d modules;
  completion of the keywords a strength allows, names in scope, record fields
  after `.` and a module's `pub` names. Framed JSON-RPC is hand-written on
  `serde_json`, so there is no new dependency. Not in this part: `cigar fmt`,
  the VS Code extension and the tree-sitter grammar. Tests: `tests/lsp.rs`
  drives the binary over stdio under a hard timeout.
- **A number out of range inside a list or map is found when the file is
  checked, and a boundary failure names where (#22 L7 and L8).**
  `stick xs: List<u8> = [1, 2, 300]` is `E1510` at the `300` at check time,
  at any depth and in an argument, a `snuff` or a record field, and a
  number the checker can fold (`[b + 10]`) counts. When the values only
  arrive at run time, `E1210` for a list or map now says which one:
  `E1210 xs should be List<u8> and is List: xs[2] is 300, outside u8 (0 to 255)`,
  `value.users[0].age is Text "old", not u8` for `json.parse<T>`. The path is
  worked out only after the value is known not to fit, so the happy path is
  unchanged. Tests: `literals_out_of_range_in_containers_are_found_at_check_time`
  and the `e1210-*-path` cases in `tests/corpus/runtime`.
- **The strictness gaps are found when the file is checked (#22 L1 to L6).**
  At medium and full, `cigar check` now warns of a parameter of a top-level
  `pull` that has no type the body settles (`W1230`, an error at full and
  under `--deny any`), code that cannot run after `snuff`, `cough`, `break`,
  `continue` or a `match` whose arms all end (`W1231`), a `stick`, `roll`,
  parameter or `use` nothing reads (`W1232`, `_` silences it) and a dropped
  result of a pure method or a `-> T` function (`W1233`). `_ = f()` drops a
  result on purpose. `--deny warnings` makes every warning an error. The
  checker also folds whole numbers through `stick`: a sized number out of
  range is `E1510` and a divisor that is `0` is `E502`, both before the
  program runs; what it cannot see is still checked when it runs. A `.cig`
  file is never held to any of it. Tests: `any_parameters_warn_and_full_refuses_them`,
  `unreachable_code_warns`, `unused_names_warn`, `dropped_results_warn`,
  `constant_overflow_is_found_at_check_time`,
  `constant_division_by_zero_is_found_at_check_time`,
  `deny_makes_warnings_errors`, `a_cig_file_is_never_held_to_a_warning`.
- **Shared writes in a parallel loop are refused, and the turns' values come
  back safely (#22 S8).** The turns of a `parallel(threads: N) for` loop will
  run side by side, so a turn writes only what it declares itself: a `roll`
  reassignment, `push`, `pop`, `insert`, `remove`, `set` or a field or index
  write to a name, list, map or record from outside the loop is `E1521`
  (`` `total` is written by every turn of a parallel loop: use a channel, or
  `parallel … reduce` ``), and so is a write through a name the turn made for
  one (`` `grid` (as `row`) ``). Reading is fine, and a channel takes a send
  from any turn. Two forms give the values back:
  `stick squares = parallel(threads: 4) for n in xs collect { n * n }` keeps
  every turn's last line in loop order, and `reduce(+)` (also `*`, `min` and
  `max`, whose order does not change the result) combines them; an empty loop
  reduces to `null`. A body whose last line is a statement, or another
  operator, is `E1522`. Test: `parallel_body_cannot_write_shared`; the corpus
  case that pushed from every turn now collects.
- **The note for a run that cannot ask about `strength full` is said once per
  project**, not on every run, so a CI job at full is not told each time.
- **CigScript 1.20.0** is the version checked against (was 1.18.0): passes
  (`pass.memo`) and the rest of 1.19 and 1.20 check as they run.

- **Mixing and the experience warning (#7).** A mild file may `use` a
  medium or full module (`use` is the one medium word a mild file may write;
  `pub`, `record` and the rest are still `E1101`). The module is checked at
  its own strength, and what the mild file passes is checked when it runs:
  a wrong-typed value stops the run with `E1210`, naming the parameter.
  `strength full` now asks once per project, on `cigar run`, whether the
  person has worked with an ownership checker before; the answer is
  remembered in the person's state folder keyed by the project's root,
  never in the project. A run with no terminal never asks and says so once;
  `strength_full_warning = off` in the person's config turns it off. New
  `cigarscript::experience` is what cig-compile calls for the same notice
  on `cigcompile build`. `docs/LANGUAGE.md` has both.
- **Contextual keywords and raw identifiers (#22 D1 and L9).** The words
  CigarScript adds are keywords only where their syntax begins (`match x {`,
  `record Name`, `use path`, `layer name`, `move x`, `parallel(...) for`...).
  After a `.` and as a map or record key they are names, so a `.cigar` file
  can read `r.match` from parsed JSON or the `.layer` of a box: cig-orchestrator's
  `orchestrate.cig`, read as `.cigar`, no longer gets 63 refusals (45 `layer`, 18
  `use`). As a variable name, a reserved word is `E1105`, "`match` is a reserved
  word in `.cigar` files; rename it, or write `r#match`", and `r#match` works at
  medium and full. Below the strength that reserves it, a name that shadows a
  keyword is allowed with the new warning `W1234`. `E1101` now fires on the
  syntax (`match x {`), not on a variable called `match`. `strength` after a
  `.` or as a key is a name too. `docs/LANGUAGE.md`'s "Reserved words" has the
  rules; `tests/strict/` and `tests/corpus` have the cases.
- **Held to Rust's standard, moves (#22 S5, S6, S7).** At `full`, binding a
  list, map, record or channel moves it (`stick alias = data`, storing it in a
  container, sending it): using the old name is `E1501`; `.copy()` copies. A
  `pack` that only reads a name borrows it, and giving the name away while the
  function is used later is the new `E1503`; `move pack(...) => ...`, a stored
  or sent function, or one that gives a name away owns what it captures. A
  name given away on some paths is moved after they join, also `E1503`, naming
  the branch; paths that end in `snuff` or `cough` do not join, and a `break`
  joins after the loop. A name the check cannot type is treated as a copy.
  `tests/strict/` has each program with its code (`binding_moves_at_full`,
  `closure_capture_borrows_or_moves`, `possibly_moved_is_moved`);
  `docs/LANGUAGE.md` has the three rules.
- **`Result<T, E>` and `?` (#2).** `Result` is built in, with the variants
  `Ok(value)` and `Err(error)` written bare, `match`ed exhaustively and
  checked like any generic enum. `e?` on a `Result` in a `pull` that returns a
  `Result` with a fitting error gives the `Ok` value or returns the `Err`; any
  other `?` is the new `E1219`. A `cough` in a function that returns a
  `Result` is its `Err` (`E1201` when it is not the error type), and
  `attempt(call)` turns what a call coughs into an `Err`. A file's own enum
  named `Result` keeps its meaning. `docs/LANGUAGE.md`'s "Types, values and
  checks" is final, with the rules written, and `tests/corpus` has the
  `Result`, `?` and `cough` cases (listed in its README) with their expected
  errors.
- **Held to Rust's standard, first four (#22 S1-S4).** `tests/strict/` has each
  program from the issue with its error, run by `tests/strict.rs`.
  A `T?` used as a `T` (method, field, index, arithmetic) is `E1209` until it
  is narrowed; a function with a result type must `snuff` on every path
  (`E1217` at the closing `}`); `List`, `Map` and `Channel` are invariant, with
  a read-only covariant `ListView<T>` for the case that needs it (`E1201` says
  why and points at `.copy()`; writing through a view is `E1215`); function
  parameters are contravariant, and `Any` no longer stands in for a type in a
  parameter. `docs/LANGUAGE.md` stops documenting covariance.
- **A latent error in a `.cig` file is a hint (#1).** `cigar check` and `cigar run`
  check a `.cig` file again as medium and say each error that finds, which
  CigScript only hits when the line runs, as `hint[E1216 check]: this would be
  an error at strength medium: ...` on stderr. The file is accepted as `cig`
  accepts it and runs as `cig run` runs it, with the same output and exit; no
  `.cig` file gets a new error. `Checked` carries them as `hints`. Written
  into `docs/LANGUAGE.md` as final, with the leftover wording for #1 and #4
  gone from it.
- **The rules for calling Rust and C (#5).** `docs/LANGUAGE.md` has them:
  `foreign "c"` signatures with linking in `cigar.toml`, `#[cigar::export]`, a
  foreign function effectful unless `pure`, called only inside a burn,
  journaled as irreversible unless it comes with a snapshot and an undo, the
  doctor's check of `pure`, and the guard still watching. The implementation
  is not built; `foreign` is still `E1102`.
- **The memory decision (#3).** `docs/decisions/0002-memory.md`, accepted:
  automatic by default, reference counting with reuse, a cycle collector,
  ownership at full with borrows scoped to a block, and copy or wrap at the
  full-to-medium boundary. It carries a measurement of plain ownership,
  reference counting and a tracing collector on three programs
  (`bench/memory-options`), beside today's runtime (the `memory` groups of
  `bench/bench.toml`).
- **`cigar explain --layers file:line:col` (#13).** It peels the `into`,
  `measure` or `within` at a place back to CigScript's core: which layer was
  chosen, for which type, where the meaning came from, the enclosing `within`
  blocks outermost first, and one line per level (full, medium, mild, core).
  `--json` prints one object. It reads the choices the checker made, so it
  cannot disagree with a run.
- **`into` a channel** sends the value (`rows into workers`), checked against
  the channel's element type like `into` a list.
- **`within timeout(d)` (#13).** The design named it and did not say what it
  means; the rule is written in the language reference first: a deadline taken
  when the block starts, checked at the top of every `for` and `while` pass
  in the block and once at its end, and past it a cough, `E1404`, so the
  burns roll back or a `within retry` around it tries again. No limit is
  `E1405`. A context that is not built is still `E1102`.
- **The hidden-effect note.** `rows into "report.csv"` outside a `burn` now
  says why: "`into` means *write file* here, because `"report.csv"` is a Path".
- Tests for the design document's examples 1, 2 and 4 (compile, run, and the
  `--layers` reduction checked); example 3 waits on #14.
- **License: FSL-1.1-ALv2 from the next release.** The code moves from Apache-2.0 to
  the Functional Source License 1.1 with an Apache-2.0 future license: free to
  use, study, change and share, not to sell or to offer in a competing
  product or service, and each version becomes Apache-2.0 two years after it
  is made available. Every commit before this change stays Apache-2.0 (there has been no release) and can't be taken back; the last Apache-2.0
  commit is the tag `apache-2.0-last`, and [LICENSING.md](LICENSING.md) holds
  the table. `LICENSE` is the publisher's text, `LICENSES/FSL-1.1-ALv2.txt`
  carries it beside the others, and `NOTICE`, `REUSE.toml`, the Cargo
  `license` fields, every SPDX header, the README and the trademark policy
  say the same. Documentation stays CC BY 4.0. Contributions now come in
  under the contributor agreement (`.github/CLA`, signed by a comment and
  recorded by the `cla` workflow) instead of the DCO. Decision record:
  `docs/decisions/0001-licence.md` (cig-installer#27).
