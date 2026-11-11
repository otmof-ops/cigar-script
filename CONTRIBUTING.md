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

Building from a checkout is the contributor's route. People who only want
`cigar` install it with `ciginstaller` once it is released (see the
[README](README.md#install)). `cigar __doctor [--json]` is the hidden
self-check `ciginstaller doctor --tools` reads: it lowers a known sample,
compares the CigScript `cigar` carries with an installed `cig`, and checks
the run cache. It is read-only, exits 0, and never starts `ciginstaller`.

## Pull requests

One subject per pull request, naming its issue (`Closes #9`), with the test
that proves it. Reviews are about the code and the behavior; expect a reply,
and expect it to be kind.

## Licensing your contribution

Code is FSL-1.1-ALv2 and documentation is CC BY 4.0; [REUSE.toml](REUSE.toml)
maps every path, and source files carry SPDX headers. A new source file gets
the same two lines as its neighbors, with the code identifier:

```text
SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
SPDX-License-Identifier: FSL-1.1-ALv2
```

Documentation files use `CC-BY-4.0` in the second line. See [LICENSING.md](LICENSING.md)
for which versions carry which license.

## The contributor agreement

CigarScript uses a Contributor License Agreement, not a DCO. Before a pull
request from you can be merged, you sign the agreement once:

- individuals: [`.github/CLA/ICLA.md`](.github/CLA/ICLA.md)
- companies whose employees contribute: [`.github/CLA/CCLA.md`](.github/CLA/CCLA.md)

You keep your copyright. The agreement gives the project a license to use,
relicense and distribute your contribution, a patent license for what you
contribute, and your confirmation that the work is yours to give. It exists so
the project can, for example, offer a commercially licensed edition or move to
a newer license later without tracking down every past contributor. Signing is
a comment on your first pull request:

> I have read the CLA Document and I hereby sign the CLA

A bot records it; you will not be asked again.
