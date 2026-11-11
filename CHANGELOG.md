<!-- SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script) -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Changelog

All notable changes to CigarScript. The format follows Keep a Changelog; the
project follows semantic versioning.

## Unreleased

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
