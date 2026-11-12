# Roadmap

CigarScript is built a strength at a time, and the language is designed
before its features are built. This page is the plan, in the open, with the
issue behind each step. It changes as the work does.

## Promises that don't change

- **Nothing touches the world outside a burn**, at every strength. Full
  changes how a burn does its work, never whether an effect needs one.
- **A `.cig` file stays valid forever**, and never gets a new error. A
  `.cigar` file is one that uses the new features.
- **Turning a strength up is additive.** Changing `strength mild` to
  `strength medium` never breaks a file.
- **Nothing is claimed that is not built.** A feature that is designed but
  not built is `E1102`, which names its issue, until it is.

## The order of work

| step | what | issue | state |
|---|---|---|---|
| **1. mild** | `strength` declarations, the feature table, the gate (`E1101` to `E1103`), `--strength` | [#7](https://github.com/otmof-ops/cigar-script/issues/7) | **built** in 0.1.0, not released |
| **2. the docs** | the language reference and grammar, the lexicon, the three guides, tested samples | [#6](https://github.com/otmof-ops/cigar-script/issues/6) | **built**: every sample is checked by the docs test. The grammar is provisional where its design question is open |
| **3. the decisions** | how far the superset goes, types, memory, which feature at which strength, calling Rust and C | [#1](https://github.com/otmof-ops/cigar-script/issues/1), [#2](https://github.com/otmof-ops/cigar-script/issues/2), [#3](https://github.com/otmof-ops/cigar-script/issues/3), [#4](https://github.com/otmof-ops/cigar-script/issues/4), [#5](https://github.com/otmof-ops/cigar-script/issues/5) | open. The answers in [the language](LANGUAGE.md) are provisional, and what is built follows them |
| **4. medium: types** | annotations, local inference, `T?`, typed coughs, checked boundaries, `E1200` to `E1399` | [#8](https://github.com/otmof-ops/cigar-script/issues/8) | **built**, provisional (#2) |
| **5. medium: records** | records, enums, `match` with exhaustiveness | [#9](https://github.com/otmof-ops/cigar-script/issues/9) | **built**, provisional |
| **6. medium: modules** | modules, generics, interfaces | [#10](https://github.com/otmof-ops/cigar-script/issues/10) | **built**, provisional. A module's run-time errors are reported past the end of the main file |
| **7. the library** | CigScript's standard library, typed at medium | [#11](https://github.com/otmof-ops/cigar-script/issues/11) | **built** |
| **8. full: ownership** | `owned`, `borrow`, `move`, concurrency, raw buffers, custom journaling | [#12](https://github.com/otmof-ops/cigar-script/issues/12) | **built**: ownership, `parallel` loops (in loop order on one thread until cig has per-thread journal segments), channels, sized numbers, `Bytes` (checked as `Any`). **Not built:** `unchecked` blocks and custom journaling adapters |
| **9. full: layers** | `into`, `measure`, `within`, `cig explain --layers` | [#13](https://github.com/otmof-ops/cigar-script/issues/13) | **built**: `into`, `measure`, `layer`, `within retry` and `within parallel`. **Not built:** other `within` contexts and `cig explain --layers` |
| **10. full: context reads** | scripts reading each other's results by context (`stats.total`) | [#14](https://github.com/otmof-ops/cigar-script/issues/14) | not built |
| **11. calling Rust and C** | `foreign` blocks and how the burn rule applies | [#5](https://github.com/otmof-ops/cigar-script/issues/5) | not built |
| **12. running** | `cigar run`: check, then run as a cig program, with plans, journals and rollback | | **built** |
| **13. carton** | packages, with no install scripts by design | [#15](https://github.com/otmof-ops/cigar-script/issues/15) | not built |
| **14. the language server** | `cigar-lsp`, `cigar fmt`, editor extensions | [#16](https://github.com/otmof-ops/cigar-script/issues/16) | **built**: `cigar fmt` (idempotent, over cigscript's lexer), `cigar-lsp` (LSP over stdio, from the checker's own crates), the tree-sitter grammar and the VS Code extension are in `editors/`. Not published to a marketplace yet |
| **15. benchmarks** | each strength's checking cost, the superset no slower than CigScript, what types buy | [#17](https://github.com/otmof-ops/cigar-script/issues/17) | **built**: `bench/bench.toml` measures `cigar check` at every strength, the superset against `cig run`, generics-heavy inference and exhaustive `match`, the examples corpus, and today's runtime against the memory decision's three programs. The nightly posts to `bench-history` |

## What is next

- The parts of full that wait on a decision or on cig: `foreign` (#5),
  `unchecked` and custom journaling (#12), more `within` contexts and
  `cig explain --layers` (#13), and cross-script reads (#14).
- Real threads for `parallel`, with the kernel's per-thread journal
  segments. A program written now does not change when they arrive.
- Run-time errors inside modules and the lowering's helpers reported where
  they happened.
- Compiling a `.cigar` file, and the decisions #1 to #5 in final form.

## Compiling

`cigar` checks and runs; it does not compile. CigCompile compiles CigScript
today, as [its roadmap](https://github.com/otmof-ops/cig-compile/blob/main/docs/roadmap.md)
says, and compiling a `.cigar` file through it comes later. Until then
`cigar run` runs it.

## How a request gets here

Open an issue in [otmof-ops/cigar-script](https://github.com/otmof-ops/cigar-script/issues).
A feature request names the strength it belongs at and what a person writing
at that strength would otherwise have to do; the lowest strength where it
cannot surprise someone who does not know it exists is where it lands (#4).
