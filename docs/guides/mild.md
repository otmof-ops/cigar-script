# Mild: you already know it

Mild is CigScript. Every word, every rule and every error you know from
`cig` is the same here: `stick`, `roll`, `pull`, `burn`, `unburn`, `cough`,
`exhale`, loops, lists, maps, the standard library, hops, `pack` and
`touches`. Nothing in this guide is new to you except where the files live
and one line at the top. Everything in it works today.

If you have never written CigScript, start with
[CigScript's own guide](https://github.com/otmof-ops/cig-script) and come
back; mild adds nothing to it.

## What changes: the file and one line

A mild program is either of two files:

| file | strength | the `strength` line |
|---|---|---|
| `tidy.cig` | always mild | not allowed: `cigar check` gives the error `cig check` gives (`E203`) |
| `tidy.cigar` | mild, unless it declares another | optional; `strength mild` says it out loud |

A `.cig` file is checked exactly as `cig check` checks it, and always will
be. A `.cigar` file may start with a strength declaration, as its first
statement. Comments and blank lines may come before it. With no declaration
the file is mild.

```cigar
strength mild

stick names = ["a.txt", "b.txt"]
roll count = 0
for n in names {
  count += 1
}
exhale "count: ${count}"
```

```sh
$ cigar check plan.cigar
checked plan.cigar
```

`strength mild` changes nothing about the file's meaning. It is there so
the next reader knows the file was written to stay at CigScript's level,
and so the line is in place on the day you want to turn it up.

## A real script, unchanged

This is the shape of CigScript's `tidy-downloads.cig` example, with the
rule on show: the world changes only inside the `burn`. It is checked as
`.cigar`, at mild, with no change but the file name.

```cigar
strength mild

if args.len() == 0 {
  cough "usage: tidy.cigar -- <folder>"
}
stick root = args[0]

pull bucket_for(file) {
  stick ext = path.ext(file).lower()
  if ext == "jpg" or ext == "png" {
    snuff "image"
  }
  if ext == "pdf" or ext == "txt" {
    snuff "doc"
  }
  snuff "other"
}

roll plan = {}
for file in fs.list(root) {
  if fs.is_dir(file) {
    continue
  }
  plan[file] = path.join(root, bucket_for(file), path.base(file))
}

burn {
  for file in plan {
    fs.mkdir(path.dir(plan[file]))
    fs.mv(file, plan[file])
  }
}
exhale "moved ${plan.len()} files"
```

Reads (`fs.list`, `fs.is_dir`, `path.join`) are free, so the plan is built
outside the burn. The two writes, `fs.mkdir` and `fs.mv`, are inside it,
where the kernel snapshots, journals and can put them back.

## Coughs, chains and the rest

Errors work as they always did: `cough` stops, and `try { } ashtray e { }`
catches.

```cigar
strength mild

try {
  cough "bad build"
} ashtray e {
  exhale "caught: ${e}"
}
```

Chains are the same too: sticks that light from each other, fail together
and roll back together.

```cigar
strength mild

stick fetch = pack() {
  exhale "fetching"
}
stick build = pack() {
  exhale "building"
}
chain release { fetch, build }
```

## Running it

`cigar run` checks a file and then runs it, with every `burn` journaled and
rolled back if the script fails. Everything after `--` is the script's `args`:

```sh
cigar run --dry-run tidy.cigar -- ~/Downloads     # the plan; nothing changes
cigar run tidy.cigar -- ~/Downloads               # for real, every move journaled
cig unburn <run-id>                               # changed your mind
```

The run is `cig run`'s: plans, journals, rollback and run records are cig's, and
`cig runs` lists them. A `.cig` file runs the same way (`cigar run tidy.cig`),
or under `cig` directly. Compiling a `.cigar` file to a program is
[CigCompile's](https://github.com/otmof-ops/cig-compile) job, and it is not
built yet; see the [roadmap](../roadmap.md).

## Where mild stops

Mild stops where CigScript stops. The moment a file uses a word or a form
that belongs to a higher strength, `cigar check` says which strength allows
it and the line to add:

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

A type written on a `stick`, a `roll`, a `pull` parameter or a `pull` result
is medium too (`stick x: Int = 1`), and so is every word in the
[feature table](../LANGUAGE.md#the-feature-table).

Those words are reserved in `.cigar` files at every strength, wherever they
appear: as a name, as a map key, as a field. A mild file that names a stick
`move` gets `E1101` for it today. That is the price of the promise that
changing `strength mild` to `strength medium` (or `full`) later never breaks
a file: no name a mild file uses can become a keyword when the strength goes
up. If a `.cigar` file needs one of those names, pick another. A `.cig` file
is not affected; it keeps every name it has. The
[language reference](../LANGUAGE.md#reserved-words) lists the words.

## Next

- [Medium](medium.md): types, records, enums, `match` and modules.
- [Commands](../commands.md): `cigar check`, `cigar run` and every flag.
- [The language](../LANGUAGE.md): the strengths, the feature table and the
  grammar.
