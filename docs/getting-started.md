# Getting started

Ten minutes: build `cigar` from source, check your first `.cigar` file,
declare a strength, read the three errors the checker adds, turn a file
down with `--strength`, and put the check in CI.

What this covers is what is built: `cigar check`. `cigar` does not run or
compile a program yet; the [roadmap](roadmap.md) says when each comes.

## 1. Install cigar

`cigar` is not released yet and is not on crates.io. Build it from source
with Rust 1.82 or newer:

```sh
cargo install --git https://github.com/otmof-ops/cigar-script --locked
cigar --version
```

```
cigar 0.1.0
```

From a checkout, the same binary is `cargo build --release`, then
`target/release/cigar`.

## 2. Check your first file

A `.cigar` file is CigScript with, optionally, a first line that says how
deep it goes. Make one:

```cigar
strength mild

exhale "hello from CigarScript"
```

Save that as `hello.cigar`, then:

```sh
cigar check hello.cigar
```

```
checked hello.cigar
```

Exit `0`: the file is what it says it is. `cigar check` reads the file the
way `cig check` does, so a misspelled name or a missing `burn` fails here
with the error `cig` would give.

## 3. Declare a strength

CigarScript comes in three strengths: **mild** (CigScript as it is),
**medium** (types, records, modules) and **full** (ownership, concurrency,
calls into Rust and C). A `.cigar` file declares its own as its first
statement:

```
strength mild
strength medium
strength full
```

Comments and blank lines may come before it. With no declaration the file
is mild. A `.cig` file takes no declaration: it is always mild, and
`cigar check` checks it exactly as `cig check` does.

The checker's job is to hold a file to its strength. Medium and full
features are designed but **not built**, so what a declaration buys you
today is the check, not the feature. The [language](LANGUAGE.md) has the
full table of what belongs where.

## 4. Read the three errors

`cigar` adds three codes to cig's. Each names what is wrong, where, and
what to do.

### E1101: above the file's strength

A feature the file's strength does not allow. This file is mild and uses a
record:

<!-- expect: E1101 -->
```cigar
strength mild

record Move {
  from: Path
  to: Path
}
```

```sh
$ cigar check moves.cigar
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> moves.cigar:3:1
  |
3 | record Move {
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

The first line of an error is cig's: *Don't see any cigarettes* means
something you asked for isn't allowed here. The hint is the fix.

### E1102: at the file's strength, not built yet

Do what the hint said and the next error is the honest one:

<!-- expect: E1102 -->
```cigar
strength medium

record Move {
  from: Path
  to: Path
}
```

```sh
$ cigar check typed.cigar
Don't see any cigarettes.
error[E1102 check]: using records is medium-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#9)
  --> typed.cigar:3:1
  |
3 | record Move {
  | ^^^^^^
  = explain: cig explain E1102
```

The file's strength allows records; this version of `cigar` has not built
them. The message names the issue that tracks them. Until it is built, a
file at medium or full that uses a medium or full feature stops here, and
`cigar check` does not run CigScript's check on it.

### E1103: a bad declaration

A `strength` line that is not the first statement of the file:

<!-- expect: E1103 -->
```cigar
stick a = 1
stick b = 2
strength medium
```

```sh
$ cigar check late.cigar
Don't see any cigarettes.
error[E1103 check]: `strength` declarations must be the first statement of the file
  --> late.cigar:3:1
  |
3 | strength medium
  | ^^^^^^^^
  = explain: cig explain E1103
```

Or a word that is not one of the three:

<!-- expect: E1103 -->
```cigar
strength spicy
```

```sh
$ cigar check spicy.cigar
Don't see any cigarettes.
error[E1103 check]: `strength` names no strength: `spicy` is not `mild`, `medium` or `full`
  --> spicy.cigar:1:10
  |
1 | strength spicy
  |          ^^^^^
  = explain: cig explain E1103
```

`strength` is reserved in a `.cigar` file, so using it as a name is E1103
too. A `.cig` file is not read for declarations at all: `strength medium`
in one is cig's own `E203`.

`cig explain E1101` does not know these codes yet; they are registered in
cig when CigarScript ships. The message carries the whole explanation until
then.

## 5. Turn a file down with `--strength`

`--strength` checks a file as if it declared another strength. It does not
change the file. Turning a file down is how you find what it uses above a
strength:

```sh
$ cigar check --strength mild typed.cigar
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> typed.cigar:3:1
  |
3 | record Move {
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

`typed.cigar` declares medium, and the check was at mild, so the record is
E1101 instead of E1102. A file with several features above the strength
lists every one, each with its place. `--strength` is ignored for `.cig`
files, which are always mild.

## 6. Use it in CI

`cigar check` exits `0` when every file checked, `2` when a check failed,
and `3` when it could not check (no path, a path that does not exist, a
file that is not `.cig` or `.cigar`, a bad `--strength`: each `E800` or
`E801`). Any nonzero exit
fails a CI step, so this is the whole job:

```sh
cargo install --git https://github.com/otmof-ops/cigar-script --locked
cigar check .
```

`cigar check .` checks the `.cig` and `.cigar` files at the top of the
folder, not the folders inside it; name those too (`cigar check . scripts
tools`).

For a machine reading the result, `--json` writes one JSON object per
line on stdout: each error as a diagnostic, and a line `{"file": ..., "ok":
true}` for each file that passed.

```sh
$ cigar --json check moves.cigar
{"code":"E1101","kind":"check","message":"using records needs strength medium; this file is mild","hint":"to use it, put `strength medium` at the top of the file","line":3,"col":1,"file":"moves.cigar"}
```

Turn that into one line per failure with `jq`:

```sh
cigar --json check . | jq -r 'select(.code) | "\(.file):\(.line):\(.col) \(.code)"'
```

```
./late.cigar:3:1 E1103
./moves.cigar:3:1 E1101
./spicy.cigar:1:10 E1103
./typed.cigar:3:1 E1102
```

`--plain` drops the catchphrases for logs, and `--no-color` (or
`NO_COLOR`) turns color off; neither changes a code or an exit.

## Next

- [Commands](commands.md): `cigar check`, every flag, the exit codes and the
  JSON shape.
- [Mild](guides/mild.md): you already know it. [Medium](guides/medium.md) and
  [full](guides/full.md): what the design says, and what `cigar check` says
  about it today.
- [The language](LANGUAGE.md): the strengths, the feature table, the grammar.
