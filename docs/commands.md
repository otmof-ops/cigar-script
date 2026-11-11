# Commands

```
cigar check <path>... [--strength mild|medium|full] [--deny any|warnings]
cigar run <file> [--dry-run] [--no-rollback] [--max-steps <n>] [--strength mild|medium|full] [-- <args>...]
cigar --version
cigar help [command]
```

Global flags apply to every command: `--json` for machine-readable output
on stdout, `--plain` for the same information without the catchphrases
(also `CIG_PLAIN`), `--no-color` to turn color off (also `NO_COLOR`). They
go before or after the command.

`cigar check` and `cigar run` are the two commands. `cigar` does not compile,
format or package anything yet; see the [roadmap](roadmap.md) for what comes
and in what order.

## `cigar check`

Check each path, in the order given. A path is one file, or a folder.

```sh
cigar check hello.cigar              # one file
cigar check hello.cigar tidy.cig     # two files
cigar check .                        # the .cig and .cigar files at the top of this folder
cigar check . scripts tools          # three folders, each at its top level
```

A file that passes prints `checked <path>` on stdout. A file that fails
prints its errors on stderr, and the check goes on to the next file, so one
run reports every file.

### What it checks

| file | what `cigar check` does |
|---|---|
| `.cig` | exactly what `cig check` does, of the cig version `cigar` is built on: the same parse, the same check, the same errors. A `.cig` file is always mild, and never gets a new error |
| `.cigar` | reads its `strength` declaration, finds every feature above or beyond that strength, and then, at medium and full, checks its types and (at full) its ownership. Then it hands the CigScript it lowers to to the same parse and check |

For a `.cigar` file that is, in order:

1. The first statement may be `strength mild`, `strength medium` or
   `strength full`. A declaration anywhere else, or a word that is not one
   of the three, is `E1103`. No declaration means mild.
2. The checker finds each use of a feature in the
   [feature table](LANGUAGE.md#the-feature-table): a reserved word, or a type
   written on a `stick`, a `roll` or a `pull`. A feature above the file's
   strength is `E1101`; one the strength allows but this version has not
   built is `E1102`. If this step finds anything, the check stops there.
3. At mild, the file goes to CigScript's own check as it is. At medium and
   full the file is **lowered** to CigScript, keeping every line where it
   was; the modules it uses are read from the folder of the file; the
   type checker reads the result (`E1200` to `E1399`); at full the
   ownership checker does too (`E1501`, `E1502`); and last, CigScript's own
   check reads what will run, and anything it finds is `cig`'s error, with
   `cig`'s code. Every error is reported where it sits in the file as
   written.

Findings are listed in the order they sit in the file. A file that
cigar cannot lower correctly reports `E1104` with kind `internal`: that is a
bug in `cigar`, and the message gives the issues page to report it at:
<https://github.com/otmof-ops/cigar-script/issues>. A `.cigar` file may not
use a name that starts `_cg_` or `__cg_`, which are the lowering's; that is
`E1104` too.

### Flags

| flag | does |
|---|---|
| `--strength <mild\|medium\|full>` | check each `.cigar` file as if it declared this strength, whatever it declares. Ignored for `.cig` files. Any other word is a usage error (exit `3`) |
| `--deny <any\|warnings>` | make a warning an error: `any` is `W1230` (a parameter with no type) at medium, `warnings` is every warning. Repeat the flag for both. A finding keeps its code. Not applied to `.cig` files. Any other word is a usage error (exit `3`) |
| `--json` | one JSON object per line on stdout; see [the JSON shape](#the-json-shape) |
| `--plain` | no catchphrases: the same codes, hints and spans, without *Don't see any cigarettes.* Also set by `CIG_PLAIN` |
| `--no-color` | never color. Also set by `NO_COLOR`. Without either, errors are colored only when stderr is a terminal |

### Folders

A folder is read at its top level only: every `.cig` and `.cigar` file in it,
sorted by name, and nothing in the folders inside it. A folder with no such
file is an error (`E800`, exit `3`), not a pass. To check a tree, name each
folder you want:

```sh
cigar check . src tools
```

### `.cig` versus `.cigar`

| | `.cig` | `.cigar` |
|---|---|---|
| strength | always mild | the one it declares; mild with none |
| `strength` line | not allowed: `E203`, `cig check`'s own | allowed as the first statement |
| medium and full features | not a feature: CigScript reads the file as it always did | `E1101` above the strength, `E1102` if not built |
| reserved words (`record`, `match`, `move`...) | ordinary names | keywords only where their syntax begins; after `.` and as map keys they are names; as a variable name `E1105` at the strength that reserves them, `W1234` below it |
| `--strength` | ignored | applied |

Any other extension is a usage error: `cigar check a.txt` says `a.txt: not a
.cig or .cigar file` and exits `3`. A file named on the command line must
have one of the two extensions; the extension decides which of the two
checks it gets.

## `cigar run`

Check a file, then run it. The check is the one `cigar check` makes, with the
same flags (`--strength`) and the same errors; a file that fails it is
reported, exits `2`, and does not run.

```sh
cigar run tidy.cigar -- ~/Downloads     # for real: every burn journaled
cigar run --dry-run tidy.cigar          # the plan; nothing changes
```

A file that checks is run as CigScript. `cigar` builds a program from it
(cig's [program](https://github.com/otmof-ops/cig-script/blob/main/docs/PROGRAMS.md)
format: `cigar` itself, which carries cig's runtime, with the lowered script
appended) and runs that, so the run is exactly what `cig run` does:

- **Burns are journaled**, and when the script fails, rolled back, newest
  op first. The rule is cig's and does not change at any strength: nothing
  touches the world outside a `burn { }`.
- **`--dry-run`** simulates every burn and prints the plan. The script's
  reads and `exhale`s still happen.
- **Run records are cig's.** Each run is a record in cig's state folder, and
  `cig runs` lists them and `cig unburn <run-id>` puts the world back. The
  record names the `.cigar` file and the line, not the lowered text.
- **`.cig` files run too**, exactly as `cig run` runs them.

A script that fails looks like this. The `burn` moved a file, then coughed,
and the move was put back:

```cigar
strength medium

record Move { from: Path, to: Path }

stick moves = [Move { from: "inbox/a.txt", to: "inbox/docs/a.txt" }]
burn {
  for m in moves {
    fs.mkdir(path.dir(m.to))
    fs.mv(m.from, m.to)
  }
  cough "stop here"
}
```

```sh
$ mkdir inbox
$ touch inbox/a.txt
$ cigar run fail.cigar
Don't see any cigarettes.
error[E600 cough]: stop here
  --> fail.cigar:11:3
   |
11 |   cough "stop here"
   |   ^^^^^^^^^^^^^^^^^
  = explain: cig explain E600
unburn: rolling back this run's burns
  restored move inbox/a.txt -> inbox/docs/a.txt (moved back: inbox/a.txt)
  restored move inbox/a.txt -> inbox/docs/a.txt (removed inbox/docs/a.txt)
  restored mkdir inbox/docs (removed inbox/docs)
$ find inbox
inbox
inbox/a.txt
```

### Run-time errors show the line as written

A run-time error names the `.cigar` file and the line, shows the line as you
wrote it, and puts a caret under the statement or expression. The checks the
lowering inserts (`E1210`, `E1510`, `E1520`) report at the statement that
holds them: a value passed to a typed parameter is checked where it is
passed, and the parameter's own check is at its `pull` line. One limit is
known: an error inside a module's code is reported at a line past the end of
your file, with the lowered text in the snippet.

### Flags

| flag | does |
|---|---|
| `--dry-run` | simulate every burn and print the plan instead of executing it |
| `--no-rollback` | leave burns in place if the script fails |
| `--max-steps <n>` | the step budget before "your loop never ends" (`E515`); `0` disables it. Also `CIG_MAX_STEPS` |
| `--strength <mild\|medium\|full>` | check as if the file declared this strength, as `cigar check` does |
| `--json`, `--plain`, `--no-color` | as for `check`. With `--json`, the script's output is followed by one JSON object, cig's run record |
| `-- <args>...` | everything after `--` is the script's `args`, as text |

```sh
cigar run args.cigar -- one two     # args is ["one", "two"]
```

### Where programs are kept

A built program is `cigar` with the script appended. It is kept in
`$XDG_CACHE_HOME/cigar/run` (`~/.cache/cigar/run` without it), named by the
script and the `cigar` that built it, so running the same file again starts at
once. The newest 12 are kept and older ones are removed. If the folder cannot be written,
`cigar run` says so with `E802` and exits `3`.

## `cigar explain`

`cigar explain --layers FILE:LINE:COL` peels the `into`, `measure` or `within`
at a place back to CigScript's core: which layer was chosen, for which type,
and the reduction it becomes. The place is written as an error shows it.

```sh
$ cigar explain --layers sales.cigar:10:3
sales.cigar:10:3
Here, `into` means *write file*, because `"report.csv"` is a Path.

full    rows into "report.csv"
medium  _cg_into_path(rows, "report.csv")
mild    fs.write_text("report.csv", json.stringify(rows))
core    burn op: write "report.csv"   (journaled, reversible)
```

Inside `within` blocks it also lists them, outermost first. `--json` prints
one object with `what`, `layer`, `type`, `source` (`built in`, `declared` or
`chosen at run time`), `effect`, `within`, `headline` and `reduction` (a list
of `{level, text}`). The file is checked first: if it does not check, its
errors are printed and the exit is `2`. A `.cig` file, a mild file, a bad
position and a place with no layer exit `3` (`E800`), the last listing where
the file's layers are. [The language](LANGUAGE.md) has the rules.

## Exit codes

| code | `cigar check` | `cigar run` |
|---|---|---|
| `0` | every file checked | the script ran to the end |
| `1` | not used | the script failed: a cough, a run-time error, an exceeded step budget. Burns were rolled back, unless `--no-rollback` |
| `2` | at least one file failed its check | the check failed: nothing ran |
| `3` | `cigar` could not check: no path given, a path that does not exist or cannot be read, a file that is not `.cig` or `.cigar`, a folder with none, or a bad `--strength` | `cigar` could not run: no file, a file that does not exist or cannot be read, a file that is not `.cig` or `.cigar`, a bad `--strength`, or a program that could not be built or started |

A `check` run that has both a failed check and a usage problem exits `3`.
Argument errors from the command line itself exit `3` as well; `--help` and
`--version` exit `0`. The check codes are cigcompile's codes too, so a CI
step reads them the same way.

## Errors

| code | kind | when |
|---|---|---|
| `E1101` | check | a feature above the file's strength |
| `E1102` | check | a feature at or below the file's strength that is not built yet in this version |
| `E1103` | check | a `strength` declaration that is not the first statement, or names no strength |
| `E1104` | check, or internal | a name that starts `_cg_` or `__cg_`; or, as kind `internal`, a file that lowers to CigScript cig refuses (a bug in `cigar`) |
| `E1105` | check | a reserved word where a name goes in a file whose strength reserves it; the message offers `r#word` |
| `W1234` | warning | a name that shadows a keyword a higher strength reserves |
| `W1230` | warning, error at full | a parameter with no type that the body does not settle; an error at full and under `--deny any` |
| `W1231` | warning | code after a `snuff`, `cough`, `break`, `continue`, or a `match` or `if` whose every branch ends |
| `W1232` | warning | a `stick`, `roll`, parameter or `use` that nothing reads; `_` silences it |
| `W1233` | warning | the result of a pure method or a `-> T` function dropped; `_ = f()` says it is meant |
| `E502`, `E1510` | check | a divisor, or a sized number, that the checker works out before the program runs (`10 / 0`, `stick c: u8 = b + 10`, `[1, 2, 300]` into `List<u8>`); they stop the run as well when the number is only known then |
| `E1200` to `E1399` | check | medium's type, record, `match`, module and interface errors |
| `E1401`, `E1402` | check | layers: two meanings for a type, or none |
| `E1403` | check | `break` or `continue` that would leave a `within retry` block |
| `E1404` | cough, at run time | a `within timeout` block ran past its deadline |
| `E1405` | check | `within timeout` written without a limit |
| `E1501`, `E1502`, `E1503` | check | ownership, at full: a use after a move, a borrow that escapes, a capture or a branch that moved a name |
| `E1521`, `E1522` | check | a parallel loop's turn that writes a name, list, map or record from outside the loop; a `parallel … collect` or `reduce` whose last line is not a value, or a `reduce` operator other than `+`, `*`, `min` or `max` |
| `E303` | check | `into` a path outside a burn (a warning inside a function), as `fs.write_text` is |
| `E1210`, `E1510`, `E1205`, `E1206`, `E1208`, `E1520` | cough, at run time | a boundary (for a list or map, with the path to the first value that does not fit: `xs[2] is 300, outside u8`), a sized number, a record's or a variant's fields given at run time, and a send on a closed channel; the message starts with the code |
| `E800` | usage | no path given, a file that is not `.cig` or `.cigar`, a folder with no such files, a path that is neither a file nor a folder, or a bad `--strength` |
| `E801` | usage | a path that cannot be read, including one that does not exist |
| `E802` | usage | `cigar run` could not build or start its program |

[The language](LANGUAGE.md#errors) says when each of the `E11xx` to `E15xx`
codes comes. Anything else is cig's: `cigar check` reports what `cig check`
would, and `cigar run` what `cig run` would. `E800` and `E801` are cig's own,
and `cig explain E800` and `cig explain E801` have pages. The `E11xx` to
`E15xx` codes are registered in cig's error registry when CigarScript ships,
so `cig explain E1101` has no page yet; the message says all of it.

## The JSON shape

`--json` writes one JSON object per line on stdout. Nothing else is
written to stdout by `cigar check`. The one exception is an argument the
command line itself rejects before `cigar` reads it (a flag it does not
know, a command that does not exist): that is the argument parser's own
text on stderr, with exit `3`. A file that passed is one object:

```json
{"file":"hello.cigar","ok":true}
```

A file that failed is one object per error, the same shape `cig check
--json` uses, with `file`, `line` and `col` where the error has a place:

```json
{"code":"E1101","kind":"check","message":"using records needs strength medium; this file is mild","hint":"to use it, put `strength medium` at the top of the file","line":3,"col":1,"file":"moves.cigar"}
```

| field | is |
|---|---|
| `code` | the error code, stable |
| `kind` | `check` for the codes `cigar` adds; `internal` for a lowering bug; `usage` for `E800` to `E802`; cig's own kinds for what it reports |
| `message` | the one-line message |
| `hint` | the line to type next; absent when there is none |
| `line`, `col` | where the error is, 1-based; absent for a usage error |
| `file` | the path as given (a folder's files carry the folder as a prefix, `./late.cigar`); absent for a usage error |

The exit code is the same with and without `--json`. With `cigar run
--json`, a check that fails writes these objects; a check that passes writes
the script's output and then cig's run record as one JSON object.

## Environment

| variable | does |
|---|---|
| `NO_COLOR` | color off, as `--no-color` |
| `CIG_PLAIN` | plain output, as `--plain` |
| `CIG_MAX_STEPS` | the step budget of `cigar run`, as `--max-steps` |
| `XDG_CACHE_HOME` | where `cigar run` keeps its built programs |
| `CIGSCRIPT_HOME` | cig's state folder, where run records are kept |

## `cigar --version`

Prints `cigar <version>` and exits `0`:

```sh
$ cigar --version
cigar 0.1.0
```

## `cigar help`

`cigar help` and `cigar --help` list the commands and global flags;
`cigar help check`, `cigar help run` and `cigar check --help` list the flags
above. All exit `0`. With no command, `cigar` prints that help and exits `0`.
