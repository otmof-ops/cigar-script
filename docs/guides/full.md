# Full: control over how the work is done

> **Most of full is built and runs; four parts are not.** Ownership,
> `parallel` loops, channels, `within retry` and `within parallel`, sized
> numbers, `Bytes` and `Channel<T>` types, and `layer` declarations all work.
> `foreign`, `unchecked` blocks, `within` contexts other than `retry` and
> `parallel`, and custom journaling adapters are not built, and a file that
> uses one of the first three stops at `E1102` naming the issue. The syntax
> is **provisional**: it follows the answers in
> [the language](../LANGUAGE.md#the-design-questions) to design questions #3,
> #4 and #5, which are still open.

This is for experienced developers. The design asks before it switches a
file to `strength full`:

> *Full strength lets one expression mean different things in different
> places. It's built for experienced developers; medium covers most
> programs. Continue?*

The question, asked once per project and remembered, is **not built**: a
file at full today is held to the checker alone. Most programs belong at
medium, and full includes everything in [medium](medium.md), so read that
first. If you are coming from CigScript, read [mild](mild.md) before both.

## The rule does not change

**Nothing touches the world outside a `burn { }`, at full strength too.**
Full gives control over *how* the work is done, never a way around the burn.
Code that cannot be journaled, such as a raw device write or a foreign
library with side effects, is to be labeled **irreversible** in the plan,
exactly as an unwatched hop is today. (Calls into foreign code are not
built.)

## Ownership (#12)

*(Provisional: #3.)* `owned`, `borrow` and `move` say who holds a value:

- A name given to an `owned` parameter, or to `move`, is gone: using it again
  in that block is `E1501`, and the message names the line where it went.
- A `borrow` parameter is the caller's. Returning it, storing it (`push`,
  `insert`, `set`) or giving it away is `E1502`.
- A parameter with neither word is shared, as in medium.

The check is **static, per block, and conservative**: a move inside a branch
or a loop body ends with that block, so it never reports a use that might be
fine. At run time ownership is erased: `move x` is just `x`, and a program
that checks runs the same with or without the words.

```cigar
strength full

pull total(owned xs: List<Int>) -> Int { snuff xs.sum() }
pull peek(borrow xs: List<Int>) -> Int { snuff xs.len() }

stick data = [1, 2, 3]
exhale peek(data)
exhale peek(data)
stick moved = move data
exhale total(moved)
```

```sh
$ cigar run ownership.cigar
3
3
6
```

Using a value after it is given away is the mistake the check is for:

<!-- expect: E1501 -->
```cigar
strength full

pull consume(owned xs: List<Int>) -> Int { snuff xs.len() }

stick data = [1, 2]
exhale consume(data)
exhale data
```

```sh
$ cigar check gone.cigar
Don't see any cigarettes.
error[E1501 check]: `data` was given away on line 6 and cannot be used after
  --> gone.cigar:7:8
  |
7 | exhale data
  |        ^^^^
  = hint: use it before it is moved, or move a copy (`data.copy()`)
  = explain: cig explain E1501
```

A copy is always available: `data.copy()` gives away a copy and keeps the
original. A borrowed value cannot be given away or returned:

<!-- expect: E1502 -->
```cigar
strength full

pull keep(borrow xs: List<Int>) -> List<Int> {
  snuff xs
}
exhale keep([1])
```

```sh
$ cigar check borrowed.cigar
Don't see any cigarettes.
error[E1502 check]: `xs` is borrowed, so it cannot be returned
  --> borrowed.cigar:4:9
  |
4 |   snuff xs
  |         ^^
  = hint: return a copy (`xs.copy()`), or take it `owned`
  = explain: cig explain E1502
```

## Concurrency (#12)

*(Provisional: #3, #4.)* `parallel(threads: N) for` is a loop written to run
several at a time, and `channel<T>()` makes a channel that values pass
through.

**Today a parallel loop runs in loop order, on one thread.** The design
requires the journal to be the sequential run's, with each thread journaling
into its own segment and the kernel merging them by loop index at the end of
the burn; running in order is that guarantee by construction, and the
threads come with the kernel's per-thread segments (cig). A program written
now does not change when they arrive, because its journal and its results
are already the sequential ones.

```cigar
strength full

roll results = []
parallel(threads: 4) for n in 1..6 {
  results.push(n * n)
}
exhale results

stick ch = channel<Int>()
parallel(threads: 4) for n in 1..5 {
  ch.send(n * 10)
}
roll got = []
roll v = ch.receive()
while v != null {
  got.push(v)
  v = ch.receive()
}
ch.close()
exhale got
```

```sh
$ cigar run parallel.cigar
[1, 4, 9, 16, 25]
[10, 20, 30, 40]
```

A channel has three methods. `send(x)` adds a value. `receive()` gives the
oldest value, or `null` when there is none. `close()` stops further sends:
a send after that is a coughed `E1520`, on the line of the send. The type of
the channel is `Channel<T>`: `send` takes a `T` (anything else is `E1201`),
and what `receive` gives is a `T?`.

A parallel loop works inside a `burn` the same way, and the loop body is a
plain block:

```cigar
strength full

pull make_all(names: List<Text>) {
  burn {
    parallel(threads: 8) for n in names {
      fs.mkdir(path.join("out", n))
    }
  }
}
make_all(args)
```

The plan lists the writes in loop order, whatever the thread count:

```sh
$ cigar run --dry-run parallel-burn.cigar -- a b c
dry-run plan: 3 ops
     1  reversible    mkdir out/a
     2  reversible    mkdir out/b
     3  reversible    mkdir out/c
nothing was changed
```

### `within`

A `within` block sets a context, and the code inside adapts to it. Two
contexts are built.

`within retry(n)` runs the block, and when it coughs runs it again, up to
`n` tries in all; the last cough is raised if none succeeds.
`within retry(n, backoff: 2s)` waits the backoff between tries.
`within parallel(...)` runs its block, with the same guarantee as a
`parallel` loop: in order, on one thread, today.

```cigar
strength full

roll tries = 0
within retry(3) {
  tries += 1
  if tries < 2 {
    cough "flaky"
  }
  exhale "ok after ${tries}"
}
exhale tries
```

```sh
$ cigar run retry.cigar
ok after 2
2
```

Any other context is not built, and says so:

<!-- expect: E1102 -->
```cigar
strength full

within timeout(5) {
  exhale 1
}
```

```sh
$ cigar check timeout.cigar
Don't see any cigarettes.
error[E1102 check]: `within timeout` is full-strength CigarScript, not built yet in this version (otmof-ops/cigar-script#13)
  --> timeout.cigar:3:8
  |
3 | within timeout(5) {
  |        ^^^^^^^
  = hint: `within retry(n)` and `within parallel(...)` are built
  = explain: cig explain E1102
```

## Sized numbers, `Bytes` and the checks on them (#12)

*(Provisional: #4.)* `u8 u16 u32 u64` and `i8 i16 i32 i64` are types. A
value entering a sized place, and the result of arithmetic between sized
values, is **range-checked when the program runs**: a number outside the
range is a coughed `E1510`. Without `unchecked` there is no way to turn the
check off (`unchecked` is not built).

```cigar
strength full

stick a: u8 = 200
stick b: u8 = 55
stick c: u8 = a + b
exhale a, b, c

stick x: i8 = -128
stick y: u16 = 65535
stick z: i64 = 9000000000
exhale x, y, z
```

```sh
$ cigar run sized.cigar
200 55 255
-128 65535 9000000000
```

One more and it does not fit:

```cigar
strength full

stick a: u8 = 200
stick b: u8 = 100
stick c = a + b
exhale c
```

```sh
$ cigar check overflow.cigar
checked overflow.cigar
$ cigar run overflow.cigar
Don't see any cigarettes.
error[E600 cough]: E1510 c is 300, outside u8 (0 to 255)
  --> overflow.cigar:5:1
  |
5 | stick c = a + b
  | ^^^^^^^^^^^^^^^
  = explain: cig explain E600
```

The check passes and the run stops: the checker does not evaluate
arithmetic, and a coughed `E1510` is catchable with `try` and `ashtray`.
The error points at the statement that made the number, as written. An
`Int` fits a sized place when it is in range; a number written out of range
is found by the check (`stick a: u8 = 300` is `E1510` before anything runs).

`Bytes` is a type, and **is checked as `Any` today**, because cig has no
bytes value yet. Sized numbers, `Bytes` and `Channel<T>` are full's: in a
medium file each is `E1101`.

## Layers: `layer`, `measure` and `into` (#13)

*(Provisional: #4, #13.)* This is what the warning is about: one expression
can mean different things in different places, and every meaning is chosen
**when the file is checked**, never while it runs. At medium, `into` and
`measure` have their built-in meanings and you can give a type another
(`measure for T = f`). At full, `layer name` declares a new layer, and
`name for T = f` gives it a meaning for a type:

```cigar
strength full

layer weight
weight for Path = fs.size

pull weigh(p: Path) -> Int {
  snuff weight p
}
exhale weigh("note.txt")
```

```sh
$ printf 'hello' > note.txt
$ cigar run layer.cigar
5
```

`weight p` is resolved as `measure p` is: by the type of `p`. If the checker
cannot tell which meaning applies it says so and names the candidates
(`E1401`), and if none applies that is `E1402`.

Four rules keep it readable:

- **Every meaning is decided when the file is checked.** Ambiguity is an
  error naming the candidates.
- **Context never hides an effect.** A layer can change what an expression
  does, never whether it touches the world. `rows into "report.csv"` outside
  a `burn { }` is refused (`E303`), as `fs.write_text` is.
- **Every layer reduces to CigScript.** However rich an expression gets, it
  is lowered to plain CigScript before it runs, which is what keeps it
  trustworthy. `cigar run` shows your own line on a run-time error.
- **The tools can always explain it.** `cig explain --layers` is to peel an
  expression back layer by layer. It is **not built** ([#13](https://github.com/otmof-ops/cigar-script/issues/13)).

## What is not built

Four things are named in the design and not built. A file that uses one of
the first three gets `E1102`, which names the issue:

| use | stops at | waits on |
|---|---|---|
| `foreign "c" { ... }`, calls into Rust and C | `E1102` | [#5](https://github.com/otmof-ops/cigar-script/issues/5): the kernel loading a library, and how the burn rule applies |
| `unchecked { ... }`, sized-number checks off | `E1102` | [#12](https://github.com/otmof-ops/cigar-script/issues/12) |
| `within` with a context other than `retry` and `parallel` | `E1102` | [#13](https://github.com/otmof-ops/cigar-script/issues/13) |
| a custom journaling adapter for a `burn` | no syntax yet | [#12](https://github.com/otmof-ops/cigar-script/issues/12) |

Scripts that read each other's results by context (`stats.total`) are
[#14](https://github.com/otmof-ops/cigar-script/issues/14), and also have no
syntax yet.

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

## What you can do next

- Take a file down with `cigar check --strength medium file.cigar`: it lists
  everything the file uses above medium, each with its place.
- Stay at medium, or at mild. The experience warning is meant seriously:
  nothing here is a reason to move up a strength before you need it.

See [the roadmap](../roadmap.md) for the order of work,
[the language](../LANGUAGE.md) for the feature table and the grammar, and
[commands](../commands.md) for `cigar run`.
