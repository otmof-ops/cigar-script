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
  <a href="docs/roadmap.md"><img alt="medium: built" src="https://img.shields.io/badge/medium-built-2ea44f"></a>
  <a href="docs/roadmap.md"><img alt="full: mostly built" src="https://img.shields.io/badge/full-mostly%20built-f59e0b"></a>
  <a href="LICENSE"><img alt="license Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-6b7280"></a>
</p>

CigScript is small on purpose: it automates, and it keeps one rule,
**nothing touches the world outside a burn**. Its own design says heavy
data work belongs in a program the script hops to. CigarScript is where
that program gets written, without leaving the family: the same words
(`stick`, `roll`, `pull`, `burn`, `cough`, `exhale`), the same rule, and
three **strengths** you choose per file. **Mild** is CigScript as it is.
**Medium** adds types, records, enums, `match`, modules and interfaces.
**Full** adds ownership, parallel loops, channels, sized numbers and
layered meanings (`within`, `layer`); calls into Rust and C are designed and
not built. Every `.cig` file is already valid CigarScript, forever.

**What works today is all three strengths, with four exceptions.**
`cigar check` reads a `.cigar` file's `strength` declaration, refuses every
feature above the strength (`E1101`), type-checks medium and full, checks
ownership at full, and then checks the result as `cig check` would.
`cigar run` runs it, as a cig program: every burn journaled, a plan with
`--dry-run`, rollback if the script fails. `foreign`, `unchecked` blocks,
`within` contexts other than `retry` and `parallel`, and custom journaling
adapters are not built; a file that uses the first three gets `E1102`, which
says so and names the issue. The syntax is **provisional** where the design
questions (#1 to #5) are open. CigarScript is 0.1.0, not released, and not on
crates.io.

<p align="center"><img src="docs/assets/strengths.svg" alt="Three strengths, each including the one below: mild is CigScript as it is, medium adds types, records, enums, match, modules and interfaces, and full adds ownership, parallel loops, channels, sized numbers and layered meanings. The burn rule holds at every strength. Mild and medium are built; full is built except calls into Rust and C, unchecked blocks and some within contexts." width="820"></p>

## Thirty seconds

```sh
$ printf 'strength mild\n\nrecord Move { from: Path, to: Path }\n' > moves.cigar
$ cigar check moves.cigar
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> moves.cigar:3:1
  |
3 | record Move { from: Path, to: Path }
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
$ sed -i 's/mild/medium/' moves.cigar
$ cigar check moves.cigar
checked moves.cigar
$ printf 'exhale Move { from: "a", to: "b" }\n' >> moves.cigar
$ cigar run moves.cigar
Move { from: "a", to: "b" }
$ cigar check --strength mild moves.cigar
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> moves.cigar:3:1
  |
3 | record Move { from: Path, to: Path }
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
$ cigar --json check --strength mild moves.cigar
{"code":"E1101","kind":"check","message":"using records needs strength medium; this file is mild","hint":"to use it, put `strength medium` at the top of the file","line":3,"col":1,"file":"moves.cigar"}
```

The first check is told which strength allows a record, and the fix; the
second passes; `cigar run` runs the file; turning it back down lists the
medium feature it uses with its place; `--json` gives one object per error, and
exit `2` when a check fails.

[Getting started](docs/getting-started.md) walks through it in ten
minutes, install included.

## See it run

A medium file with a record and typed functions passes the check, and
`cigar run --dry-run` prints the plan for its burn and changes nothing. One
wrong type, and the checker says where the value is and which signature it
broke.

<p align="center"><img src="docs/assets/terminal.svg" alt="A terminal: cat shows a medium CigarScript file with a record, a typed function and a burn that moves two files; cigar check prints checked; cigar run --dry-run prints the moves and a plan of three reversible operations and says nothing was changed; after one field is changed to the number 3, cigar check gives E1201, expected Path, found Int, with the line and the field's type as the reason." width="860"></p>

## What works, and what does not yet

| | state |
|---|---|
| `cigar check` on a `.cig` file | **built.** Exactly what `cig check` does, of the cig version `cigar` is built on; a `.cig` file never gets a new error |
| `cigar check` on a `.cigar` file | **built.** The `strength` declaration, the feature gate, lowering, the type and ownership checks, and then CigScript's own check |
| `cigar run` | **built.** Checks, then runs as a cig program: plans, journals, rollback and run records are cig's; errors name your file and line |
| `E1101` to `E1104`, `E1201` to `E1218`, `E1301` to `E1304`, `E1311` to `E1313`, `E1401` to `E1403`, `E1501`, `E1502`, `E1510`, `E1520` | **built.** [The language](docs/LANGUAGE.md#errors) says when each comes |
| `--strength`, `--json`, `--plain`, `--no-color`, folders | **built.** Exit `0` checked, `2` a check failed, `3` could not check |
| mild: everything CigScript has | **built** |
| medium: types, records, enums, `match`, generics, interfaces, modules, `into`, `measure`, the typed standard library | **built**, provisional ([#8](https://github.com/otmof-ops/cigar-script/issues/8), [#9](https://github.com/otmof-ops/cigar-script/issues/9), [#10](https://github.com/otmof-ops/cigar-script/issues/10), [#11](https://github.com/otmof-ops/cigar-script/issues/11), [#13](https://github.com/otmof-ops/cigar-script/issues/13)) |
| full: `owned`, `borrow`, `move`, `parallel`, channels, `within retry` and `within parallel`, sized numbers, `Bytes`, `layer` | **built**, provisional ([#12](https://github.com/otmof-ops/cigar-script/issues/12), [#13](https://github.com/otmof-ops/cigar-script/issues/13)). `parallel` runs in loop order on one thread today, so the journal is the sequential run's |
| full: `foreign` | **not built** ([#5](https://github.com/otmof-ops/cigar-script/issues/5)). Every use is `E1102` |
| full: `unchecked` blocks, other `within` contexts | **not built** ([#12](https://github.com/otmof-ops/cigar-script/issues/12), [#13](https://github.com/otmof-ops/cigar-script/issues/13)). Every use is `E1102` |
| full: custom journaling adapters, scripts reading each other's results | **not built** ([#12](https://github.com/otmof-ops/cigar-script/issues/12), [#14](https://github.com/otmof-ops/cigar-script/issues/14)). No syntax yet |
| the experience warning, `cig explain --layers` | **not built** |
| compiling a `.cigar` file | **not built.** CigCompile builds CigScript today; CigarScript comes through it later. `cigar run` runs it |
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
| [Medium](docs/guides/medium.md) | write types, records, enums, `match`, modules and interfaces, and run them |
| [Full](docs/guides/full.md) | use ownership, parallel loops, channels, sized numbers and layers, and see what is not built |
| [Commands](docs/commands.md) | look up `cigar check` and `cigar run`, every flag, the exit codes and the JSON shape |
| [The language](docs/LANGUAGE.md) | read the strengths, the feature table, the grammar and the open decisions |
| [Lexicon](docs/LEXICON.md) | find a word (`strength`, `carton`, `record`, `within`, `pub`) and the manual's name for it |
| [Roadmap](docs/roadmap.md) | see what is built, what is next, and the issue behind each |
| [The design](docs/CIGARSCRIPT-AND-CIGCOMPILE.md) | read where CigarScript and CigCompile are going, and why |

Every `cigar` sample in these pages is checked by the docs test: a block that
should pass does, and one marked to fail (`# expect: E1101`, or any other
code) gives that error first. Multi-file samples write each module as its own
block, with its path after the language.

## Build from source

`cigar` is not released and is not on crates.io. With Rust 1.82 or newer:

```sh
cargo install --git https://github.com/otmof-ops/cigar-script --locked
cigar --version
```

From a checkout, `cargo build --release` writes `target/release/cigar`, and
`cargo test` runs the checker's tests and the docs test. `cigar run` is part of the same binary.

## Part of CigScript

CigarScript is built on CigScript, the language at
[otmof-ops/cig-script](https://github.com/otmof-ops/cig-script): `cigar`
reads, lowers and checks a file through the `cigscript` library of the version it
pins, so what mild accepts is what `cig check` accepts, and `cigar run` runs the
lowered script with cig's own runtime: its plans, journals and rollback. The lexicon, the
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
