# Medium: types and records in an afternoon

> **Everything in this guide is built and runs.** Medium is checked by
> `cigar check` and run by `cigar run`. The syntax is **provisional**:
> it follows the answers in [the language](../LANGUAGE.md#the-design-questions)
> to design questions #2, #4, #9 and #10, which are still open, and each
> provisional part says what it waits on.

Mild is CigScript as it is: no types written, everything inferred, and
mistakes found when the line runs. Medium writes the shapes of your data
down, so the checker finds the mistake before anything runs. It is for real
programs, libraries and team code. You do not need it for a script.

Medium includes all of mild. A file turns up by changing one line. This
file is mild, and a type on a `stick` is medium:

<!-- expect: E1101 -->
```cigar
strength mild

stick x: Int = 1
exhale x + 1
```

```sh
$ cigar check typed.cigar
Don't see any cigarettes.
error[E1101 check]: using type annotations needs strength medium; this file is mild
  --> typed.cigar:3:8
  |
3 | stick x: Int = 1
  |        ^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

Put `strength medium` at the top and the same line is allowed:

```cigar
strength medium

stick x: Int = 1
exhale x + 1
```

```sh
$ cigar check typed.cigar
checked typed.cigar
$ cigar run typed.cigar
2
```

`cigar run` checks the file, then runs it. The rest of this guide is what
you can write once the strength is medium.

## Types where they help (#8)

*(Provisional: #2.)* Annotations go on `stick`, `roll`, a `pull`'s
parameters and its result, and on a `pack`'s parameters. A type is one of:

| type | is |
|---|---|
| `Any` | anything. A name with no annotation is `Any`, unless the checker can tell better from the value it was given |
| `Null`, `Bool`, `Int`, `Float`, `Text` | the CigScript values of those names |
| `Path` | a path. `Path` and `Text` are the same kind of value, and either fits where the other is wanted |
| `List<T>` | a list whose items are all `T`. A bare `List` is `List<Any>` |
| `Map<V>` | a map whose values are all `V`. Keys are text in CigScript, so `Map<Text, V>` is the same type as `Map<V>`. A bare `Map` is `Map<Any>` |
| `T?` | a `T`, or `null` |
| `(A, B) -> R` | a function that takes an `A` and a `B` and gives an `R` |
| a record or enum name | that record or enum, with type arguments when it has them: `Pair<Int, Text>` |

An `Int` fits where a `Float` is wanted. Nothing else converts. The
sized numbers (`u8` and the rest), `Bytes` and `Channel<T>` are
[full](full.md): at medium they are `E1101`.

Inside a function the types are inferred. You write them where a function
meets the rest of the program.

```cigar
strength medium

pull add(a: Int, b: Int) -> Int { snuff a + b }
pull shout(t: Text) -> Text { snuff t.upper() }
stick f = pack(a: Int, b: Int) => a + b

exhale add(2, 3), shout("hi"), f(4, 5)

stick names: List<Text> = ["a", "bcd"]
roll total: Int = 0
for n in names {
  total += n.len()
}
exhale total
```

```sh
$ cigar run typed-functions.cigar
5 HI 9
4
```

A `T?` is "maybe nothing". `?.` stops at a `null` and gives `null`, and
`??` gives its right side when the left is `null`:

```cigar
strength medium

record User { name: Text, nick: Text? }

stick none: User? = null
stick ann: User? = User { name: "ann", nick: null }
exhale none?.name ?? "anon"
exhale ann?.name ?? "anon"
exhale ann?.nick ?? "no nick"
```

```sh
$ cigar run optional.cigar
anon
ann
no nick
```

### What the checker finds

Wrong types are found before anything runs, with the place of the value and
a note naming where the type came from:

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

`null` where the type is not optional is `E1209`, a result that is not the
declared type is `E1217`, a wrong number of arguments is `E1216`, a type
that is not a type is `E1202`, and an assignment to a `stick` is `E1215`.
[The language](../LANGUAGE.md#errors) lists every code and when it comes.

### Boundaries are checked when the program runs

Typed code is checked statically where the checker can tell, and **at run
time wherever a value enters a typed place**: a `stick x: Int = ...`, a
typed parameter at the start of its function, a typed result at each
`snuff`. Every such boundary is checked on every run; the checker does not
elide any yet. A value the checker cannot see into (an untyped function's
result, `json.parse`) is `Any` to it, so the mistake is the run's to find,
and it stops with `E1210`:

```cigar
strength medium

pull double(n: Int) -> Int { snuff n * 2 }

stick v = json.parse('"x"')
exhale double(v)
```

```sh
$ cigar check boundary.cigar
checked boundary.cigar
$ cigar run boundary.cigar
Don't see any cigarettes.
error[E600 cough]: E1210 double: n should be Int and is Text "x"
  --> boundary.cigar:3:1
  |
3 | pull double(n: Int) -> Int { snuff n * 2 }
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  = explain: cig explain E600
```

The check passes, because `v` is `Any` there. The run stops at the
boundary, and the message says which place and what the value was. The
error shows your line as you wrote it. A parameter's check sits at the start
of its function, so it points at the `pull` line, not at the call.

A run-time failure is an ordinary cough whose message starts with its code.
`try` and `ashtray` catch it, and the caught value has `code` and
`message`:

```cigar
strength medium

pull double(n: Int) -> Int { snuff n * 2 }

stick v = json.parse('"x"')
try {
  exhale double(v)
} ashtray err {
  exhale err.code
  exhale err.message
}
```

```sh
$ cigar run caught.cigar
E1210
E1210 double: n should be Int and is Text "x"
```

### Typed coughs

A function that coughs says so in its signature with `coughs`; `cough`
itself does not change. What it coughs is a record, and a `cough` of a
record the signature does not list is `E1218`.

```cigar
strength medium

record ParseError { message: Text }

pull parse(t: Text) -> Int coughs ParseError {
  if t == "" {
    cough ParseError { message: "empty" }
  }
  snuff t.to_int()
}

exhale parse("42")
try {
  parse("")
} ashtray err {
  exhale "caught", err.message
}
```

```sh
$ cigar run typed-cough.cigar
42
caught empty
```

## Records and enums (#9)

*(Provisional: #9.)* A record is a named shape. Fields are separated by
commas or newlines, and a record can take type parameters:

```cigar
strength medium

record Move { from: Path, to: Path }
record Pair<A, B> { first: A, second: B }

stick m = Move { from: "a.txt", to: "b/a.txt" }
stick p: Pair<Int, Text> = Pair { first: 1, second: "x" }
exhale m.from
exhale m
exhale p, p.first + 1
```

```sh
$ cigar run records.cigar
a.txt
Move { from: "a.txt", to: "b/a.txt" }
Pair { first: 1, second: "x" } 2
```

A record's literal must give every field, once each, in any order: a field
left out is `E1205`, and a field that is not in the record is `E1206`. A
record is a value: two records with the same fields are equal, and it
prints as `Move { from: "a.txt", to: "b/a.txt" }`, at any depth. A record
literal cannot be written in the head of an `if`, `while` or `for`; wrap it
in parentheses.

`json.stringify` writes a record as a plain object, with no tag:

```cigar
strength medium

record Move { from: Path, to: Path }
record Job { id: Int, step: Move }

stick j = Job { id: 1, step: Move { from: "a", to: "b" } }
exhale json.stringify(j)
exhale j
```

```sh
$ cigar run record-json.cigar
{"id":1,"step":{"from":"a","to":"b"}}
Job { id: 1, step: Move { from: "a", to: "b" } }
```

An enum is one of a set of named cases, each of which may carry data.
Fields are named, or positional (`circle(Float)`); a variant is built with
its fields in order, or, when they are named, by name in any order:

```cigar
strength medium

enum Status { ok, failed(reason: Text), skipped }
enum Shape { circle(Float), rect(w: Float, h: Float) }

stick a = Status.ok
stick b = Status.failed(reason: "disk full")
exhale a, b
exhale [a, b]
exhale Shape.circle(2.0), Shape.rect(h: 4.0, w: 3.0)
exhale json.stringify(b)
exhale json.stringify(a)
```

```sh
$ cigar run enums.cigar
Status.ok Status.failed(reason: "disk full")
[Status.ok, Status.failed(reason: "disk full")]
Shape.circle(2.0) Shape.rect(w: 3.0, h: 4.0)
{"failed":{"reason":"disk full"}}
"ok"
```

A variant that does not exist is `E1207`, and the wrong fields are `E1208`.

Records and enums are values in the burn kernel's sense: a record is a map
with its fields in declaration order, and an enum is a map with its
variant, so they go into the journal, and `--dry-run` plans and run
records show them.

## match (#9)

`match` picks by shape. An arm is `pattern => result`; arms are separated by
newlines or commas. As a statement, each arm's result is a statement or a
block; as a value (after `stick`, `roll`, an assignment or `snuff`), each
arm is an expression.

```cigar
strength medium

enum Status { ok, failed(reason: Text), skipped }

pull label(s: Status) -> Text {
  match s {
    failed(reason) if reason != "" => snuff "failed: ${reason}"
    failed(_) => snuff "failed"
    ok => snuff "ok"
    skipped => snuff "skipped"
  }
}

for s in [Status.skipped, Status.ok, Status.failed("x"), Status.failed("")] {
  stick n = match s { ok => 0, failed(r) => 1, skipped => 2 }
  exhale label(s), n
}
```

```sh
$ cigar run match.cigar
skipped 2
ok 0
failed: x 1
failed 1
```

The patterns are `_`; a literal (`1`, `"x"`, `true`, `null`); a name, which
binds the value; a variant, `failed(p, ...)` or `Status.failed(p, ...)`,
whose parts are names, `_` or literals; and a record,
`Move { from: "a", to }`, where `to` alone binds the field `to`. A guard is
`if <expression>` after the pattern, and an arm with a guard does not count
as covering its pattern.

**A `match` that misses a case is an error that lists the cases it
missed** (`E1211`). A match over an enum must cover every variant, or end
in `_` or a name. A match over a `Bool` needs `true` and `false`, over a
`T?` needs `null` and a name, and over anything else needs an arm of `_` or
a name.

<!-- expect: E1211 -->
```cigar
strength medium

enum Status { ok, failed(reason: Text), skipped }

pull f(s: Status) -> Int {
  match s {
    ok => snuff 1
  }
}
exhale f(Status.ok)
```

```sh
$ cigar check missing.cigar
Don't see any cigarettes.
error[E1211 check]: this match does not cover every `Status`: missing failed(...), skipped
  --> missing.cigar:6:3
  |
6 |   match s {
  |   ^^^^^
  = hint: add an arm for each, or `_ => ...` for the rest
  = explain: cig explain E1211
```

A block as the arm of a match used as a value is `E1213`. Pattern or arm
syntax `cigar` cannot read, and a `match` nested as a bare arm result, are
`E1212`: bind the inner one first.

## Generics, interfaces and implementations (#10)

*(Provisional: #10.)* Generics take the form `List<T>`. They are checked
and then erased: at run time a type parameter is checked as `Any`.

```cigar
strength medium

pull first<T>(xs: List<T>) -> T? {
  if xs.len() == 0 { snuff null }
  snuff xs[0]
}

exhale first([1, 2, 3]) ?? 0
exhale first([]) ?? 0
exhale first(["a"]) ?? "none"
```

```sh
$ cigar run generic.cigar
1
0
a
```

An interface is a contract a type can meet. An `impl` says that a type
meets it, and must give every method with the interface's signature
(`E1311` for a method left out, `E1312` for a different signature). `self`
is the value. A call is resolved by the receiver's type **when the checker
knows it**, and by a run-time dispatcher, on the value's record or enum
name or its CigScript type, when the receiver is `Any`. A method on a type
with no `impl` is `E1313`.

```cigar
strength medium

interface Sized {
  pull size(self) -> Int
}

record Move { from: Path, to: Path }

impl Sized for Move {
  pull size(self) -> Int { snuff 2 }
}

impl Sized for Int {
  pull size(self) -> Int { snuff self }
}

pull total(x) {
  snuff x.size()
}

stick m = Move { from: "a", to: "b" }
exhale m.size(), 7.size()
exhale total(m), total(9)
```

```sh
$ cigar run interface.cigar
2 7
2 9
```

## Modules (#10)

*(Provisional: #10.)* One file is one module. `use tools.plan` reads
`tools/plan.cigar`, and `pub` marks what a module exports: `pub pull`,
`pub record`, `pub enum`, `pub stick` and `pub interface`. `use
tools.plan` binds the module as `plan`, so `plan.plan_moves(...)`;
`use tools.plan.{Move, plan_moves}` binds those names directly.

```cigar tools/plan.cigar
strength medium

pub record Move { from: Path, to: Path }

pub pull plan_moves(names: List<Text>) -> List<Move> {
  roll out: List<Move> = []
  for n in names {
    out.push(Move { from: n, to: "out/${n}" })
  }
  snuff out
}

pull private_helper() -> Int { snuff 0 }
```

```cigar
strength medium

use tools.plan.{Move, plan_moves}

for m in plan_moves(["a.txt", "b.txt"]) {
  exhale m
}
```

```sh
$ cigar run main.cigar
Move { from: "a.txt", to: "out/a.txt" }
Move { from: "b.txt", to: "out/b.txt" }
```

A module is found from **the folder of the file being checked or run**, for
that file and for every module it reaches. A module has to be medium or
full itself: `pub` is medium's word, so a mild file is not a module. A
medium file can use a full module and the reverse.

- A name that is not `pub` is `E1303`, and a name the module does not have
  is `E1304`.
- A module that is not there is `E1301`, and a cycle is `E1302`, naming it.
- A module that does not check is `E1301` at the `use` line, naming its first
  error. Check the module itself to see where: `cigar check tools/plan.cigar`.

Two limits are known. A run-time error inside a module's code is reported
at a line past the end of the main file, because the module's text is
appended to it. And each file that uses a module runs the module's body
separately, so a module's top-level statements run once per file that
uses it.

## `into` and `measure` on built-in types (#13)

*(Provisional: #4, #13.)* `measure x` asks one question and gets the right
answer for each thing: the length of a list, a map or a text, and the size
of a path. `a into b` puts `a` into `b`: a list takes it as a new last item,
and a path is written. Which one applies is decided **when the file is
checked**, from the type, never while it runs.

```cigar
strength medium

roll seen = []
1 into seen
"x" into seen
exhale seen
exhale measure seen
exhale measure ({a: 1, b: 2})
exhale measure "hello"
```

```sh
$ cigar run layers.cigar
[1, "x"]
2
2
5
```

A type can be given its own meaning, once, at the top level:
`measure for Move = move_cost` makes `measure m` call `move_cost(m)`, and
`into for T = f` makes `a into b` call `f(b, a)` for a `b` of type `T`.

```cigar
strength medium

record Move { from: Text, to: Text }

pull move_cost(m: Move) -> Int {
  snuff m.from.len() + m.to.len()
}

measure for Move = move_cost

stick m = Move { from: "ab", to: "cde" }
exhale measure m
exhale measure [m, m]
```

```sh
$ cigar run measure.cigar
5
2
```

Two meanings for one type are `E1401` and name both. No meaning is `E1402`,
which says what to declare.

**Context never hides an effect.** `into` a path writes the file, so it
needs a burn, exactly as `fs.write_text` does: outside a burn it is `E303`,
an error at the top level and a warning inside a function (the function may
be called from a burn).

<!-- expect: E303 -->
```cigar
strength medium

stick rows = [1, 2]
rows into "out.txt"
```

```sh
$ cigar check into-path.cigar
Don't see any cigarettes.
error[E303 check]: `into` a path writes the file, so it must be inside a burn block
  --> into-path.cigar:4:1
  |
4 | rows into "out.txt"
  | ^^^^^^^^^^^^^^^^^^^
  = hint: wrap it: burn { ... }, or call this function from inside one
  = explain: cig explain E303
```

## The standard library is typed (#11)

Every function in CigScript's standard library, and every method on text,
lists and maps, has a signature the checker knows: its parameters, its
result, and whether it is an effect that needs a burn. What it does not know
is `Any`, never a guess that rejects a valid program. Behavior at mild is
unchanged.

## The rule does not change

Medium changes what you can say about your data, not what touches the world.
A record, a type or a module is pure; the only place a file changes anything
outside it is a `burn { }`, with the journal, the plan and the rollback it
always had. This is a medium file whose `burn` moves two files:

```cigar
strength medium

record Move { from: Path, to: Path }

pull plan(names: List<Text>) -> List<Move> {
  roll out: List<Move> = []
  for n in names {
    out.push(Move { from: n, to: "docs/${n}" })
  }
  snuff out
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

`--dry-run` prints the plan and changes nothing. A run for real is journaled
and, if the script fails, rolled back:

```sh
$ touch a.txt b.txt
$ cigar run --dry-run tidy.cigar -- a.txt b.txt
moved 2 files
dry-run plan: 3 ops
     1  reversible    mkdir docs
     2  reversible    move a.txt -> docs/a.txt
     3  reversible    move b.txt -> docs/b.txt
nothing was changed
$ ls *.txt
a.txt
b.txt
$ cigar run tidy.cigar -- a.txt b.txt
moved 2 files
burned 3 ops (0 irreversible), run 20261109T153740-6398dc
$ ls docs
a.txt
b.txt
```

The script's own `exhale` still prints in a dry run: reads and prints run,
and only the burn is simulated.

## What you can do next

- Read [full](full.md) for ownership, parallel loops, channels, `within`
  and sized numbers.
- Turn a file down to see what it uses above a strength: `cigar check
  --strength mild file.cigar` lists every medium feature it uses, each with
  its place.
- Keep writing mild where it is enough. It is a subset of medium, so nothing
  you write now needs to change when you turn a file up.

See [the roadmap](../roadmap.md) for what is next, [the language](../LANGUAGE.md)
for the feature table, the grammar and the provisional decisions, and
[commands](../commands.md) for `cigar check` and `cigar run`.
