<p align="center">
  <img src="docs/assets/logo.svg" alt="CigarScript" width="520">
</p>

<p align="center">
  <b>CigScript, grown deeper: three strengths, chosen per file.</b><br>
  <i>A language from the maker of <a href="https://github.com/otmof-ops/cig-script">CigScript</a>: the same words and the same rule, with types, ownership and more when you ask for them.</i>
</p>

<p align="center">
  <a href="https://github.com/otmof-ops/cig-script"><img alt="a CigScript language" src="https://img.shields.io/badge/a-CigScript%20language-f97316"></a>
  <a href="docs/LANGUAGE.md"><img alt="strengths 3" src="https://img.shields.io/badge/strengths-3-6b7280"></a>
  <a href="docs/roadmap.md"><img alt="mild: built" src="https://img.shields.io/badge/mild-built-2ea44f"></a>
  <a href="LICENSE"><img alt="license Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-6b7280"></a>
</p>

CigScript is small on purpose: it automates, and it keeps one rule,
**nothing touches the world outside a burn**. Its own design says heavy
data work belongs in a program the script hops to. CigarScript is where
that program gets written, without leaving the family: the same words
(`stick`, `roll`, `pull`, `burn`, `cough`, `exhale`), the same rule, and
three **strengths** you choose per file. **Mild** is CigScript as it is.
**Medium** adds types, records and modules. **Full** adds ownership,
concurrency, and calls into Rust and C. Every `.cig` file is already valid
CigarScript, forever.

**What works today is mild, and the gate that holds a file to its
strength.** `cigar check` reads a `.cigar` file's `strength` declaration,
checks the file as `cig check` would, and refuses every feature above the
strength (`E1101`) with the line to add. Medium and full are designed, not
built: a file that uses them gets `E1102`, which says so and names the
issue. CigarScript is 0.1.0, not released, and not on crates.io.

<p align="center"><img src="docs/assets/strengths.svg" alt="Three strengths, each including the one below: mild is CigScript as it is, medium adds types, records and modules, and full adds ownership, concurrency and calls into Rust and C. The burn rule holds at every strength. Mild is built; medium and full are designed and not built." width="820"></p>

## Thirty seconds

```sh
printf 'strength mild\n\nrecord Move { from: Path, to: Path }\n' > moves.cigar
cigar check moves.cigar                    # E1101: records need strength medium; this file is mild
sed -i 's/mild/medium/' moves.cigar        # do what the hint says
cigar check moves.cigar                    # E1102: not built yet in this version (cigar-script#9)
cigar check --strength mild moves.cigar    # turn it down: E1101 again, with its place
cigar --json check moves.cigar             # one JSON object per error; exit 2 when a check fails
```

[Getting started](docs/getting-started.md) walks through it in ten
minutes, install included.

## See it run

A file at mild passes. The same file with one record in it is told which
strength allows that, and when it gets it, is told the truth: the strength
allows it, and this version has not built it.

<p align="center"><img src="docs/assets/terminal.svg" alt="A terminal: a mild CigarScript file that uses a record gives E1101 and the line to add; with strength medium declared, the same file gives E1102, not built yet, naming issue 9; a mild file with no medium feature prints checked." width="860"></p>

## What works, and what does not yet

| | state |
|---|---|
| `cigar check` on a `.cig` file | **built.** Exactly what `cig check` does, of the cig version `cigar` is built on; a `.cig` file never gets a new error |
| `cigar check` on a `.cigar` file | **built.** The `strength` declaration, the feature gate, and then CigScript's own check |
| `E1101`, `E1102`, `E1103` | **built.** Above the strength; at the strength but not built; a bad declaration |
| `--strength`, `--json`, `--plain`, `--no-color`, folders | **built.** Exit `0` checked, `2` a check failed, `3` could not check |
| mild: everything CigScript has | **built** in what the checker accepts. `cigar` does not run a program: run it with `cig` as a `.cig` file |
| medium: types, records, enums, `match`, modules, interfaces | **not built** ([#8](https://github.com/otmof-ops/cigar-script/issues/8), [#9](https://github.com/otmof-ops/cigar-script/issues/9), [#10](https://github.com/otmof-ops/cigar-script/issues/10), [#11](https://github.com/otmof-ops/cigar-script/issues/11)). Every use is `E1102` |
| full: ownership, `parallel`, `foreign`, `within`, `into`, `measure` | **not built** ([#12](https://github.com/otmof-ops/cigar-script/issues/12), [#13](https://github.com/otmof-ops/cigar-script/issues/13), [#5](https://github.com/otmof-ops/cigar-script/issues/5)). Every use is `E1102` |
| compiling a `.cigar` file | **not built.** CigCompile builds CigScript today; CigarScript comes through it later |
| carton, the language server | **not built** ([#15](https://github.com/otmof-ops/cigar-script/issues/15), [#16](https://github.com/otmof-ops/cigar-script/issues/16)) |
| `cig explain E1101` | no page yet: the codes are registered in cig when CigarScript ships; the message says all of it |

The design questions (#1 to #5) are open, and the answers in
[the language](docs/LANGUAGE.md) are provisional until they are decided.
Each item above is on the [roadmap](docs/roadmap.md).

## Guides

| guide | read it when you want to |
|---|---|
| [Getting started](docs/getting-started.md) | build `cigar`, check your first file, and put the check in CI |
| [Mild](docs/guides/mild.md) | you know CigScript: you already know it |
| [Medium](docs/guides/medium.md) | see types and records as designed, and what the checker says about them today |
| [Full](docs/guides/full.md) | see ownership, concurrency and layered depth as designed, with the experience warning |
| [Commands](docs/commands.md) | look up `cigar check`, every flag, the exit codes and the JSON shape |
| [The language](docs/LANGUAGE.md) | read the strengths, the feature table, the grammar and the open decisions |
| [Lexicon](docs/LEXICON.md) | find a word (`strength`, `carton`, `record`, `within`) and the manual's name for it |
| [Roadmap](docs/roadmap.md) | see what is built, what is next, and the issue behind each |
| [The design](docs/CIGARSCRIPT-AND-CIGCOMPILE.md) | read where CigarScript and CigCompile are going, and why |

Every `cigar` sample in these pages is checked by the docs test: a block that
should pass does, and one marked to fail (`# expect: E1101`, or `E1102`,
`E1103`) gives that error first.

## Build from source

`cigar` is not released and is not on crates.io. With Rust 1.82 or newer:

```sh
cargo install --git https://github.com/otmof-ops/cigar-script --locked
cigar --version
```

From a checkout, `cargo build --release` writes `target/release/cigar`, and
`cargo test` runs the checker's tests and the docs test.

## Part of CigScript

CigarScript is built on CigScript, the language at
[otmof-ops/cig-script](https://github.com/otmof-ops/cig-script): `cigar`
reads and checks a file through the `cigscript` library of the version it
pins, so what mild accepts is what `cig check` accepts. The lexicon, the
burn rule and the kernel are there. [CigCompile](https://github.com/otmof-ops/cig-compile)
compiles CigScript today, and will compile CigarScript later. The design is
[docs/CIGARSCRIPT-AND-CIGCOMPILE.md](docs/CIGARSCRIPT-AND-CIGCOMPILE.md), and
the work is in the [issues](https://github.com/otmof-ops/cigar-script/issues).

## Licensing

- **Code** (`src/`, `tests/`, `bench/`): [Apache-2.0](LICENSE), the same
  license as CigScript. In plain words: use, copy, change, sell and embed
  CigarScript freely, including in closed products; keep the copyright notice
  and the [NOTICE](NOTICE) file with any copy; you get a patent license from
  every contributor; and you can't use the name to endorse your derivative.
- **Documentation** (`docs/`, this README and the other guides at the root):
  [CC BY 4.0](LICENSES/CC-BY-4.0.txt). Reuse and adapt with credit.
- **The name.** CigarScript is a trademark of Jay Taylor, as CigScript is.
  Another implementation or a fork needs a name of its own;
  [TRADEMARKS.md](TRADEMARKS.md) says what is free to do and what to ask about
  first.

[REUSE.toml](REUSE.toml) maps every path to its license. Contributions are
accepted under the same terms, signed off under the DCO; see
[CONTRIBUTING.md](CONTRIBUTING.md). Security reports:
[SECURITY.md](SECURITY.md).
