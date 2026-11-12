# Getting started

Ten minutes: install `cigar`, check and run your first `.cigar`
file, declare a strength, write a small medium program, read the errors the
checker adds, turn a file down with `--strength`, and put the check in CI.

`cigar check` checks a file and `cigar run` runs it. `cigar` does not compile a
program yet; the [roadmap](roadmap.md) says when that comes.

## 1. Install cigar

Three ways. Pick one.

**From the release archive.** Each 0.1.0 release carries one
`cigar-v0.1.0-<os>-<arch>.tar.gz` per machine, holding `bin/cigar` and
`bin/cigar-lsp`. Verify the sha256 and the build attestation before you
run it:

```sh
ver=0.1.0
asset=cigar-v$ver-linux-x86_64.tar.gz           # or your machine
gh release download v$ver -R otmof-ops/cigar-script -p "$asset" -p "$asset.sha256"
sha256sum -c "$asset.sha256"
gh attestation verify "$asset" -R otmof-ops/cigar-script
tar -xzf "$asset"
./bin/cigar --version
```

**With `ciginstaller`.** Once carton 1.0.0 is published, the installer
takes care of the download and the sha256:

```sh
ciginstaller install carton
```

(or a pack or stick with `cigar` in it, once the installer's manifest lists
one). No `ciginstaller` yet? It is at
<https://github.com/otmof-ops/cig-installer#install>. To remove `cigar`, run
`ciginstaller uninstall cigar`; that also clears its run cache, the programs
`cigar run` built, which it makes again when needed.

**From source**, with Rust 1.82 or newer:

```sh
git clone https://github.com/otmof-ops/cigar-script
cd cigar-script
cargo build --release
```

That writes `target/release/cigar` and `target/release/cigar-lsp`; put them
on your `PATH`.

```sh
$ cigar --version
cigar 0.1.0
```

## 2. Check and run your first file

A `.cigar` file is CigScript with, optionally, a first line that says how
deep it goes. Make one:

```cigar
strength mild

exhale "hello from CigarScript"
```

Save that as `hello.cigar`, then:

```sh
$ cigar check hello.cigar
checked hello.cigar
$ cigar run hello.cigar
hello from CigarScript
```

`cigar check` exits `0`: the file is what it says it is. It reads the file the
way `cig check` does, so a misspelled name or a missing `burn` fails here
with the error `cig` would give. `cigar run` checks the file and then runs
it, with every `burn` journaled and rolled back if the script fails.

## 3. Declare a strength

CigarScript comes in three strengths: **mild** (CigScript as it is),
**medium** (types, records, modules) and **full** (ownership, concurrency,
sized numbers, layers). A `.cigar` file declares its own as its first
statement:

```
strength mild
strength medium
strength full
```

Comments and blank lines may come before it. With no declaration the file
is mild. A `.cig` file takes no declaration: it is always mild, and
`cigar check` checks it exactly as `cig check` does.

The checker's job is to hold a file to its strength. The [language](LANGUAGE.md)
has the full table of what belongs where, and which parts are not built.

## 4. Write a medium program

Medium writes the shapes of your data down. This file sorts files into
folders; it has a record, typed functions, and one `burn` where the world
changes:

```cigar
strength medium

record Move { from: Path, to: Path }

pull bucket(file: Path) -> Text {
  stick ext = path.ext(file).lower()
  if ext == "png" { snuff "images" }
  snuff "docs"
}

pull plan(files: List<Text>) -> List<Move> {
  roll moves: List<Move> = []
  for file in files {
    moves.push(Move { from: file, to: path.join(bucket(file), path.base(file)) })
  }
  snuff moves
}

stick moves = plan(args)
burn {
  for m in moves {
    fs.mkdir(path.dir(m.to))
    fs.mv(m.from, m.to)
  }
}
exhale "moved ${moves.len()} files"
```

Everything after `--` is the script's `args`. `--dry-run` prints the plan and
changes nothing:

```sh
$ touch a.txt b.png
$ cigar run --dry-run tidy.cigar -- a.txt b.png
moved 2 files
dry-run plan: 4 ops
     1  reversible    mkdir docs
     2  reversible    move a.txt -> docs/a.txt
     3  reversible    mkdir images
     4  reversible    move b.png -> images/b.png
nothing was changed
```

Run it for real, and it is journaled, so `cig unburn` can put it back:

```sh
$ cigar run tidy.cigar -- a.txt b.png
moved 2 files
burned 4 ops (0 irreversible), run 20261109T153733-038931
$ find docs images
docs
docs/a.txt
images
images/b.png
```

## 5. Read the errors the checker adds

`cigar` adds codes to cig's. Each names what is wrong, where, and what to do.

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
something you asked for isn't allowed here. The hint is the fix: put
`strength medium` at the top, and the same file checks.

### E1102: at the file's strength, not built yet

A few features are designed and not built: `foreign`, `unchecked` blocks,
and `within` contexts other than `retry` and `parallel`. Using one stops at
`E1102`, which names the issue:

<!-- expect: E1102 -->
```cigar
strength full

foreign "c" {
  pull strlen(s: Text) -> Int pure
}
```

```sh
$ cigar check foreign.cigar
Don't see any cigarettes.
error[E1102 check]: using calls into Rust and C (`foreign`) is full-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#5)
  --> foreign.cigar:3:1
  |
3 | foreign "c" {
  | ^^^^^^^
  = explain: cig explain E1102
```

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

### E1201 and the rest: type errors

At medium and full the checker finds wrong types before anything runs:

<!-- expect: E1201 -->
```cigar
strength medium

pull add(a: Int, b: Int) -> Int { snuff a + b }

exhale add(1, "2")
```

```sh
$ cigar check wrong.cigar
Don't see any cigarettes.
error[E1201 check]: expected `Int`, found `Text`
  --> wrong.cigar:5:15
  |
5 | exhale add(1, "2")
  |               ^^^
  = hint: expected `Int` here, because of this signature of `add`
  = explain: cig explain E1201
```

The codes from `E1201` on are listed, each with when it comes, in
[the language](LANGUAGE.md#errors).

`cig explain E1101` does not know these codes yet; they are registered in
cig when CigarScript ships. The message carries the whole explanation until
then.

## 6. Turn a file down with `--strength`

`--strength` checks a file as if it declared another strength. It does not
change the file. Turning a file down is how you find what it uses above a
strength:

```sh
$ cigar check --strength mild tidy.cigar
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> tidy.cigar:3:1
  |
3 | record Move { from: Path, to: Path }
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> tidy.cigar:5:17
  |
5 | pull bucket(file: Path) -> Text {
  |                 ^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> tidy.cigar:5:25
  |
5 | pull bucket(file: Path) -> Text {
  |                         ^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> tidy.cigar:11:16
   |
11 | pull plan(files: List<Text>) -> List<Move> {
   |                ^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> tidy.cigar:11:30
   |
11 | pull plan(files: List<Text>) -> List<Move> {
   |                              ^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> tidy.cigar:12:13
   |
12 |   roll moves: List<Move> = []
   |             ^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

`tidy.cigar` declares medium, and the check was at mild, so every medium
feature it uses is listed, each with its place. `--strength` is ignored for
`.cig` files, which are always mild.

## 7. Use it in CI

`cigar check` exits `0` when every file checked, `2` when a check failed,
and `3` when it could not check (no path, a path that does not exist, a
file that is not `.cig` or `.cigar`, a bad `--strength`: each `E800` or
`E801`). Any nonzero exit fails a CI step, so this is the whole job:

```sh
cigar check .
```

The job needs `cigar` on its `PATH` first. The lightest way is to download
the release archive for the runner's platform and verify its sha256; once
carton 1.0.0 is published, `ciginstaller install carton` does the same with
one command, which the installer's page shows how to fetch on a runner:
<https://github.com/otmof-ops/cig-installer#install>.

`cigar check .` checks the `.cig` and `.cigar` files at the top of the
folder, not the folders inside it; name those too (`cigar check . scripts
tools`).

For a machine reading the result, `--json` writes one JSON object per
line on stdout: each error as a diagnostic, and a line `{"file": ..., "ok":
true}` for each file that passed.

```sh
$ mkdir ci
$ cp moves.cigar late.cigar spicy.cigar foreign.cigar ci
$ cigar --json check ci/moves.cigar
{"code":"E1101","kind":"check","message":"using records needs strength medium; this file is mild","hint":"to use it, put `strength medium` at the top of the file","line":3,"col":1,"file":"ci/moves.cigar"}
```

Turn that into one line per failure with `jq`:

```sh
$ cigar --json check ci | jq -r 'select(.code) | "\(.file):\(.line):\(.col) \(.code)"'
ci/foreign.cigar:3:1 E1102
ci/late.cigar:3:1 E1103
ci/moves.cigar:3:1 E1101
ci/spicy.cigar:1:10 E1103
```

`--plain` drops the catchphrases for logs, and `--no-color` (or
`NO_COLOR`) turns color off; neither changes a code or an exit.

## Next

- [Commands](commands.md): `cigar check`, `cigar run`, every flag, the exit
  codes and the JSON shape.
- [Mild](guides/mild.md): you already know it. [Medium](guides/medium.md) and
  [full](guides/full.md): types, records, modules, ownership and the rest.
- [The language](LANGUAGE.md): the strengths, the feature table, the grammar.
