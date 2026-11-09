# Medium: types and records in an afternoon

> **Nothing in this guide is built yet.** Medium is designed, and `cigar
> check` already knows its words, but a file that uses any of them stops at
> `E1102`: *not built yet in this version*. This page is the design as it
> stands: what you will write, and what the checker says today. The syntax
> is **provisional** until the design questions it depends on (#2, #4) are
> decided, and each sample is marked as one that does not yet pass.

Mild is CigScript as it is: no types written, everything inferred, and
mistakes found when the line runs. Medium writes the shapes of your data
down, so the checker finds the mistake before anything runs. It is for real
programs, libraries and team code. You do not need it for a script.

Medium includes all of mild. A file turns up by changing one line:

```cigar
# expect: E1101
strength mild

stick x: Int = 1
```

```sh
$ cigar check typed.cigar
Don't see any cigarettes.
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> typed.cigar:4:8
  |
4 | stick x: Int = 1
  |        ^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

Put `strength medium` at the top and the same line is allowed by the file
and refused by this version:

```cigar
# expect: E1102
strength medium

stick x: Int = 1
```

```sh
$ cigar check typed.cigar
Don't see any cigarettes.
error[E1102 check]: using type annotations is medium-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#8)
  --> typed.cigar:4:8
  |
4 | stick x: Int = 1
  |        ^
  = explain: cig explain E1102
```

Everything below fails the same way, with the issue that will build it.

## Types where they help (#8)

Annotations go on `stick`, `roll`, a `pull`'s parameters and its result.
Inside a function the types are inferred; you write them where a function
meets the rest of the program. A `T?` is "maybe nothing", with `?.` and
`??`, and there is no null anywhere else. (`T?` does not lex yet: the
checker stops at the `?` with cig's `E103` before it reaches a feature, so
the sample below leaves it out.)

```cigar
# expect: E1102
strength medium

pull parent(p: Path) -> Path {
  snuff path.dir(p)
}
```

A function that coughs says so in its signature (`coughs`); `cough` itself
does not change. When a mild call site calls a typed function, one check is
made at the boundary and none inside typed code.

Type errors will be `E1200` to `E1399`, with a span on both sides of the
mismatch: *expected `Path` here, because of this signature.* Not built;
issue [#8](https://github.com/otmof-ops/cigar-script/issues/8).

## Records and enums (#9)

A record is a named shape. Printing, equality and hashing come with it, and
JSON and CSV map straight onto it.

```cigar
# expect: E1102
strength medium

record Move { from: Path, to: Path }
```

An enum is one of a set of named cases, each of which may carry data, and
`match` picks by shape. A `match` that misses a case is an error that lists
the cases it missed.

```cigar
# expect: E1102
strength medium

enum Status { ok, failed(reason: Text), skipped }

pull describe(s: Status) -> Text {
  match s {
    ok => snuff "fine"
    failed(reason) => snuff reason
    skipped => snuff "skipped"
  }
}
```

Records and enums are values in the burn kernel's sense: they go into the
journal, so `--dry-run` plans and run records show them. Not built; issue
[#9](https://github.com/otmof-ops/cigar-script/issues/9).

## Modules, generics and interfaces (#10)

One file is one module. `use` brings in another's names, and `pub` marks
what a module exports; modules may not import each other in a circle, and
the error names the circle. Generics take the form `List<T>`. An interface
is a contract a type can meet.

```cigar
# expect: E1102
strength medium

use tools.plan.{Move, plan_moves}

interface Sized {
  pull size(self) -> Int
}
```

A mild file can use a medium module, with a checked boundary between them.
Not built; issue [#10](https://github.com/otmof-ops/cigar-script/issues/10).
The standard library gets typed signatures in
[#11](https://github.com/otmof-ops/cigar-script/issues/11), with no change in
behavior at mild.

## The rule does not change

Medium changes what you can say about your data, not what touches the world.
A record, a type or a module is pure; the only place a file changes anything
outside it is a `burn { }`, with the journal, the plan and the rollback it
always had.

## What you can do today

- Declare `strength medium` and run `cigar check`: you get `E1102` at each
  place the file uses a feature this version has not built.
- Check a file you mean to take to medium: `cigar check --strength medium
  file.cigar` lists every feature it uses, each with its issue.
- Keep writing mild. It is a subset of medium, so nothing you write now
  needs to change when medium arrives.

See [the roadmap](../roadmap.md) for the order of work,
[the language](../LANGUAGE.md) for the feature table and the provisional
decisions, and [full](full.md) for the strength above.
