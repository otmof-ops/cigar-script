# Commands

```
cigar check <path>... [--strength mild|medium|full]
cigar --version
cigar help [command]
```

Global flags apply to every command: `--json` for machine-readable output
on stdout, `--plain` for the same information without the catchphrases
(also `CIG_PLAIN`), `--no-color` to turn color off (also `NO_COLOR`). They
go before or after `check`.

`cigar check` is the only command. `cigar` does not run, compile, format or
package anything yet; see the [roadmap](roadmap.md) for what comes and in
what order.

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
| `.cigar` | reads its `strength` declaration, finds every feature above or beyond that strength, and only if there is none hands the file to the same parse and check |

For a `.cigar` file that is, in order:

1. The first statement may be `strength mild`, `strength medium` or
   `strength full`. A declaration anywhere else, or a word that is not one
   of the three, is `E1103`. No declaration means mild.
2. The checker finds each use of a feature in the
   [feature table](LANGUAGE.md#the-feature-table): a reserved word, or a type
   written on a `stick`, a `roll` or a `pull`. A feature above the file's
   strength is `E1101`; one the strength allows but this version has not
   built is `E1102`.
3. If steps 1 and 2 found nothing, CigScript's own check runs on the file,
   and anything it finds is `cig`'s error, with `cig`'s code.

Findings are listed in the order they sit in the file.

### Flags

| flag | does |
|---|---|
| `--strength <mild\|medium\|full>` | check each `.cigar` file as if it declared this strength, whatever it declares. Ignored for `.cig` files. Any other word is a usage error (exit `3`) |
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
| reserved words (`record`, `match`, `move`...) | ordinary names | reserved at every strength |
| `--strength` | ignored | applied |

Any other extension is a usage error: `cigar check a.txt` says `a.txt: not a
.cig or .cigar file` and exits `3`. A file named on the command line must
have one of the two extensions; the extension decides which of the two
checks it gets.

### Exit codes

| code | meaning |
|---|---|
| `0` | every file checked |
| `2` | at least one file failed its check |
| `3` | `cigar` could not check: no path given, a path that does not exist or cannot be read, a file that is not `.cig` or `.cigar`, a folder with none, or a bad `--strength` |

A run that has both a failed check and a usage problem exits `3`.
Argument errors from the command line itself exit `3` as well; `--help` and
`--version` exit `0`.

These are cigcompile's codes too, so a CI step reads them the same way.

### Errors

| code | kind | when |
|---|---|---|
| `E1101` | check | a feature above the file's strength |
| `E1102` | check | a feature at or below the file's strength that is not built yet in this version |
| `E1103` | check | a `strength` declaration that is not the first statement, or names no strength |
| `E800` | usage | no path given, a file that is not `.cig` or `.cigar`, a folder with no such files, a path that is neither a file nor a folder, or a bad `--strength` |
| `E801` | usage | a path that cannot be read, including one that does not exist |

Anything else is cig's: `cigar check` reports what `cig check` would. `E800` and `E801` are cig's own, and `cig explain E800` and `cig explain E801`
have pages. The three `E110x` codes are registered in cig's error registry
when CigarScript ships, so `cig explain E1101` has no page yet; the message
says all of it.

### The JSON shape

`--json` writes one JSON object per line on stdout. Nothing else is
written to stdout. The one exception is an argument the command line itself
rejects before `cigar` reads it (a flag it does not know, a command that does
not exist): that is the argument parser's own text on stderr, with exit `3`. A file that passed is one object:

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
| `kind` | `check` for `E1101` to `E1103`; `usage` for `E800` and `E801`; cig's own kinds for what it reports |
| `message` | the one-line message |
| `hint` | the line to type next; absent when there is none |
| `line`, `col` | where the error is, 1-based; absent for a usage error |
| `file` | the path as given (a folder's files carry the folder as a prefix, `./late.cigar`); absent for a usage error |

The exit code is the same with and without `--json`.

### Environment

| variable | does |
|---|---|
| `NO_COLOR` | color off, as `--no-color` |
| `CIG_PLAIN` | plain output, as `--plain` |

## `cigar --version`

Prints `cigar <version>` and exits `0`:

```
cigar 0.1.0
```

## `cigar help`

`cigar help` and `cigar --help` list the commands and global flags;
`cigar help check` and `cigar check --help` list the flags above. Both exit
`0`. With no command, `cigar` prints that help and exits `0`.
