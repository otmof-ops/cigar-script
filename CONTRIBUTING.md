# Contributing

The plan of record is the issues, and the design is
[docs/CIGARSCRIPT-AND-CIGCOMPILE.md](docs/CIGARSCRIPT-AND-CIGCOMPILE.md).
Each issue says why, what lands, and when it is done. Pick one, or open one
for what you have in mind before writing much. The language's decisions are
recorded in [docs/LANGUAGE.md](docs/LANGUAGE.md) as they are made.

## Building and testing

```sh
cargo build                                     # the cigarscript library and cigar
CIGSCRIPT_SRC=../cig-script cargo test           # every test, the superset ones included
```

The superset tests need a checkout of CigScript at the version `Cargo.toml`
pins: every CigScript script must check as mild CigarScript exactly as `cig
check` checks it. CI runs formatting, clippy and the tests.

## Pull requests

One subject per pull request, naming its issue (`Closes #9`), with the test
that proves it. Reviews are about the code and the behavior; expect a reply,
and expect it to be kind.

## Licensing your contribution

Code is Apache-2.0 and documentation is CC BY 4.0; [REUSE.toml](REUSE.toml)
maps every path, and every source file carries SPDX headers (a new one gets
the same two lines; copy them from a neighbor). A contribution is accepted
under the license of the kind of file it changes, which Apache-2.0 says itself
in its section 5. Sign off each commit (`git commit -s`): the `Signed-off-by`
line is your statement, under the
[Developer Certificate of Origin](https://developercertificate.org), that the
work is yours to give. You keep your copyright.
