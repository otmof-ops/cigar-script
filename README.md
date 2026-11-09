# CigarScript

The language grown from CigScript, one strength at a time: **mild** (CigScript
as it is), **medium** (types, records, modules) and **full** (ownership,
concurrency, calls into Rust and C), chosen per file. The rule does not change:
nothing touches the world outside a burn.

The design is [docs/CIGARSCRIPT-AND-CIGCOMPILE.md](docs/CIGARSCRIPT-AND-CIGCOMPILE.md),
and the work is in the issues.

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
