<!-- SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script) -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Changelog

All notable changes to CigarScript. The format follows Keep a Changelog; the
project follows semantic versioning.

## Unreleased

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
