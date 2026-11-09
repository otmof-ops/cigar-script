# Full: control over how the work is done

> **Nothing in this guide is built yet.** Full is designed, and `cigar
> check` already knows its words, but a file that uses any of them stops at
> `E1102`: *not built yet in this version*. The syntax below is
> **provisional**; it will be settled as each part is built (#12, #13, #5).

This is for experienced developers. Switching a file to `strength full`
will ask first:

> *Full strength lets one expression mean different things in different
> places. It's built for experienced developers; medium covers most
> programs. Continue?*

The question is asked once per project and remembered, and a setting skips
it for people who know what they are doing. It is not built yet either; a
file at full today is held to the checker alone.

If you are coming from CigScript, read [mild](mild.md) first, and
[medium](medium.md) before this: full includes both. Most programs belong at
medium.

Declare it, and the checker says what is and is not built:

```cigar
# expect: E1102
strength full

pull checksum(owned data: Bytes) -> Hash {
  snuff hash.sha256(data)
}
```

```sh
$ cigar check checksum.cigar
Don't see any cigarettes.
error[E1102 check]: using owned values is full-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#12)
  --> checksum.cigar:4:15
  |
4 | pull checksum(owned data: Bytes) -> Hash {
  |               ^^^^^
  = explain: cig explain E1102
error[E1102 check]: using type annotations is medium-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#8)
  --> checksum.cigar:4:25
  |
4 | pull checksum(owned data: Bytes) -> Hash {
  |                         ^
  = explain: cig explain E1102
error[E1102 check]: using type annotations is medium-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#8)
  --> checksum.cigar:4:34
  |
4 | pull checksum(owned data: Bytes) -> Hash {
  |                                  ^^
  = explain: cig explain E1102
```

Full contains medium, so the file's type annotations are findings too.
Every finding is listed, in the order it sits in the file, and each names
the issue that will build it: `owned` is
[#12](https://github.com/otmof-ops/cigar-script/issues/12), the types are
[#8](https://github.com/otmof-ops/cigar-script/issues/8).

## The rule does not change

**Nothing touches the world outside a `burn { }`, at full strength too.**
Full gives control over *how* a burn does its work and how it is
journaled and compensated, never a way around it. Code that cannot be
journaled, such as a raw device write or a foreign library with side
effects, is labeled **irreversible** in the plan, exactly as an unwatched
hop is today.

## Ownership (#12)

`owned`, `borrow` and `move` say who holds a value. They are checked
locally, per block, and an error shows both the borrow and the conflicting
use (`E1500` to `E1599`).

```cigar
# expect: E1102
strength full

pull consume(owned data) {
  exhale "done"
}
```

What manages memory for mild and medium (counting references, with a cycle
collector for mutable containers) and how it meets full's ownership is
design question 3 (#3), and is **provisional**.

## Concurrency (#12)

`parallel` runs a loop several at a time, and channels pass values between
threads. The journal still records every write in program order: each
thread journals into its own segment and the kernel merges them
deterministically at the end of the burn, so a rollback undoes things in the
order a sequential run would.

```cigar
# expect: E1102
strength full

burn {
  parallel(threads: 8) for m in moves {
    fs.mv(m.from, m.to)
  }
}
```

Raw buffers (`Bytes`, slices and sized numbers) are bounds-checked by
default, and `unchecked` blocks exist only in full files and are named in
the plan. A `burn` can declare an adapter that says how a new kind of
operation captures its before-state and compensates, so a database write or
an API call with an undo becomes a reversible op in the same journal.

## Calling Rust and C (#5)

A `foreign` block declares signatures for code that did not come from
CigarScript. A foreign function is effectful unless declared `pure`; calling
an effectful one is allowed only inside a burn, where it is journaled as
irreversible unless it comes with an adapter.

```cigar
# expect: E1102
strength full

foreign "c" {
  pull strlen(s: Text) -> Int pure
}
```

## Layered depth (#13)

This is what the warning is about. One expression can mean different things
in different places, and every meaning is chosen when the program compiles,
never while it runs.

`a into b` puts `a` into `b`, and what that does depends on what `b` is:

```cigar
# expect: E1102
strength full

stick rows = [1, 2, 3]
roll seen = []
rows into seen
```

A `within` block sets a context, and the code inside adapts to it. Nesting
stacks contexts, and the order changes the behavior:

```cigar
# expect: E1102
strength full

within retry(3) {
  within parallel(threads: 8) {
    exhale "resizing"
  }
}
```

`measure` asks one question and gets the right answer for each thing, and a
`layer` declares a meaning once for a type. Four rules keep it readable:

- **Every meaning is decided when the program compiles.** If the compiler
  cannot tell which applies, that is an error naming the candidates.
- **The tools can always explain it.** `cig explain --layers` will peel an
  expression back to CigScript's core, and the editor will show the same on
  hover.
- **Context never hides an effect.** A layer can change what an expression
  does, never whether it touches the world. `rows into "report.csv"`
  outside a `burn { }` is refused, as `fs.write_text` is today.
- **Every layer reduces to CigScript.** However rich an expression gets, it
  ends as plain CigScript logic, which is what keeps it trustworthy.

Not built; issue [#13](https://github.com/otmof-ops/cigar-script/issues/13).
Scripts reading each other's results by context (`stats.total`) are
[#14](https://github.com/otmof-ops/cigar-script/issues/14).

## What you can do today

- Declare `strength full` and run `cigar check` to see exactly which words
  your file uses and which issue builds each.
- Take a file down with `cigar check --strength medium file.cigar`: it
  lists everything the file uses above medium, each with its place.
- Stay at medium, or at mild. The experience warning is meant seriously:
  nothing here is a reason to move up a strength before you need it.

See [the roadmap](../roadmap.md) for the order of work and
[the language](../LANGUAGE.md) for the feature table.
