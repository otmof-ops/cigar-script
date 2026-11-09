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
| **1. mild** | `strength` declarations, the feature table, the gate (`E1101` to `E1103`), `--strength` | [#7](https://github.com/otmof-ops/cigar-script/issues/7) | **built** in 0.1.0, not released. Mixing strengths and the experience warning wait on #10 and #12 |
| **2. the docs** | the language reference and grammar, the lexicon, the three guides, tested samples | [#6](https://github.com/otmof-ops/cigar-script/issues/6) | the grammar covers what is built; each feature's grammar is written as it is built |
| **3. the decisions** | how far the superset goes, types, memory, which feature at which strength, calling Rust and C | [#1](https://github.com/otmof-ops/cigar-script/issues/1), [#2](https://github.com/otmof-ops/cigar-script/issues/2), [#3](https://github.com/otmof-ops/cigar-script/issues/3), [#4](https://github.com/otmof-ops/cigar-script/issues/4), [#5](https://github.com/otmof-ops/cigar-script/issues/5) | open. The answers in [the language](LANGUAGE.md) are provisional |
| **4. medium: types** | annotations, local inference, `T?`, typed coughs, checked boundaries, `E1200` to `E1399` | [#8](https://github.com/otmof-ops/cigar-script/issues/8) | not built |
| **5. medium: records** | records, enums, `match` with exhaustiveness | [#9](https://github.com/otmof-ops/cigar-script/issues/9) | not built |
| **6. medium: modules** | modules, generics, interfaces | [#10](https://github.com/otmof-ops/cigar-script/issues/10) | not built |
| **7. the library** | CigScript's standard library, typed at medium | [#11](https://github.com/otmof-ops/cigar-script/issues/11) | not built |
| **8. full: ownership** | `owned`, `borrow`, `move`, concurrency, raw buffers, custom journaling | [#12](https://github.com/otmof-ops/cigar-script/issues/12) | not built |
| **9. full: layers** | `into`, `measure`, `within`, `cig explain --layers` | [#13](https://github.com/otmof-ops/cigar-script/issues/13) | not built |
| **10. full: context reads** | scripts reading each other's results by context (`stats.total`) | [#14](https://github.com/otmof-ops/cigar-script/issues/14) | not built |
| **11. calling Rust and C** | `foreign` blocks and how the burn rule applies | [#5](https://github.com/otmof-ops/cigar-script/issues/5) | not built |
| **12. carton** | packages, with no install scripts by design | [#15](https://github.com/otmof-ops/cigar-script/issues/15) | not built |
| **13. the language server** | `cigar-lsp`, a formatter, editor extensions | [#16](https://github.com/otmof-ops/cigar-script/issues/16) | not built |
| **14. benchmarks** | each strength's checking cost, the superset no slower than CigScript, what types buy | [#17](https://github.com/otmof-ops/cigar-script/issues/17) | started: `bench/bench.toml` measures the checker; the other groups come with the features they measure |

Steps 4 to 11 are the order the features land in; each is useful on its
own, and a feature is built after the decision it depends on.

## Compiling

`cigar` checks; it does not compile or run. CigCompile compiles CigScript
today, as [its roadmap](https://github.com/otmof-ops/cig-compile/blob/main/docs/roadmap.md)
says, and compiling a `.cigar` file through it waits on the types of step 4
(its stage D). Until then a mild program runs under `cig` as a `.cig` file.

## How a request gets here

Open an issue in [otmof-ops/cigar-script](https://github.com/otmof-ops/cigar-script/issues).
A feature request names the strength it belongs at and what a person writing
at that strength would otherwise have to do; the lowest strength where it
cannot surprise someone who does not know it exists is where it lands (#4).
