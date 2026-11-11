# CigarScript: the language

**Status: draft.** This is the reference the checker is built from. Mild,
medium and full are built, with the exceptions the [feature table](#the-feature-table)
marks. The design questions #1, #3, #4 and #5 are decided and written here
as final. What is still marked **provisional** follows the recommendation in
its own design question and says what changes if the decision goes another
way.

CigarScript is CigScript grown deeper, one **strength** at a time. Every
CigScript program is CigarScript at the mild strength, and the same rule
holds at every strength: nothing touches the world outside a `burn { }`.

## Files and strengths

| file | strength | what the checker accepts |
|---|---|---|
| `.cig` | always **mild** | exactly what `cig check` accepts, forever: no new errors, ever; the deeper checker's findings come as [hints](#a-cig-file-gets-hints-never-errors) |
| `.cigar` | the one it declares, **mild** when it declares none | the features of its strength and below |

- A `.cigar` file declares its strength with `strength mild`, `strength
  medium` or `strength full` as its **first statement**. Comments and
  blank lines may come before it. A declaration anywhere else, or a word
  other than the three, is `E1103`.
- **Turning up is additive.** Changing `strength mild` to `strength medium`
  never breaks a file: every word below is reserved in `.cigar` files at
  every strength, so no name a mild file uses can become a keyword when
  the strength goes up. *(Provisional.)*
- **Turning down is checked.** `cigar check --strength mild` on a medium
  file lists every feature it uses above mild, each with its place.
- **A `.cig` file gets hints, never errors.** The superset is exact and it
  stays exact (#1, decided): every CigScript program is valid CigarScript,
  and a `.cig` file keeps CigScript's own acceptance forever, so no release
  of cigar gives a `.cig` file a new error. What the medium checker would
  find in it, and CigScript only hits when the line runs (or never, on a
  path that does not run), is said as a **hint**, through cig's renderer,
  on stderr:

  ```text
  hint[E1216 check]: this would be an error at strength medium: `add` takes 2 argument(s), given 1
    --> latent.cig:3:11
  ```

  `cigar check` still says `checked` and exits `0`, and `cigar run` runs the
  file exactly as `cig run` runs it: the same output and the same exit, the
  failing line failing where it always did. A hint has the code the error
  would have, so `cig explain E1216` says what it means. Under `--json`,
  `cigar check` writes each hint as a diagnostic object with that message.
  A `.cig` file CigScript itself rejects gets CigScript's error and no
  hints. To have the findings as errors, rename the file to `.cigar` and
  declare `strength medium`: that is what `.cigar` is for.
- **Strengths mix across files** in one project. A module is a file at
  medium or full (`pub` is medium's word), so a medium file can use a full
  module and a full file a medium one, each checked at its own strength. A
  mild file cannot use a module: `use` is medium's. *(Provisional: #10.)*

## The feature table

Each feature belongs to the lowest strength where it cannot surprise
someone who doesn't know it exists (#4, decided). The checker's feature table is generated from the
table below at build time; a row added here is a row the checker knows.

`detect` is how the checker finds a use:
- `word:W` is the word `W` anywhere in the file;
- `annotation` is a type written on `stick`, `roll`, a `pull` parameter
  (`name: T`) or a `pull`'s result (`-> T`).

`built` says whether the feature works yet: `no` means a file at the
right strength gets `E1102` (*not built yet*) instead of a result.

<!-- features:begin -->
| feature | strength | detect | issue | built |
|---|---|---|---|---|
| type annotations | medium | annotation | #8 | yes |
| typed coughs | medium | word:coughs | #8 | yes |
| records | medium | word:record | #9 | yes |
| enums | medium | word:enum | #9 | yes |
| pattern matching | medium | word:match | #9 | yes |
| modules | medium | word:use | #10 | yes |
| exports | medium | word:pub | #10 | yes |
| interfaces | medium | word:interface | #10 | yes |
| implementations | medium | word:impl | #10 | yes |
| `into` on built-in types | medium | word:into | #13 | yes |
| `measure` on built-in types | medium | word:measure | #13 | yes |
| owned values | full | word:owned | #12 | yes |
| borrows | full | word:borrow | #12 | yes |
| moves | full | word:move | #12 | yes |
| parallel loops | full | word:parallel | #12 | yes |
| channels | full | word:channel | #12 | yes |
| unchecked blocks | full | word:unchecked | #12 | no |
| context blocks | full | word:within | #13 | yes |
| calls into Rust and C (`foreign`) | full | word:foreign | #5 | no |
| new layers | full | word:layer | #13 | yes |
<!-- features:end -->

Everything CigScript has is mild: `stick`, `roll`, `pull`, `burn`,
`unburn`, `cough`, `exhale`, loops, lists, maps, the standard library,
hops, `pack` and `touches`.

Three things above are written with no reserved word of their own, and
are checked at the strength that owns them: a type that is a sized number
(`u8` to `u64`, `i8` to `i64`), `Bytes` or `Channel<T>` is `E1101` below
full wherever it is written.

## How a file is checked and run

A mild file goes to CigScript as it is. A medium or full file is
**lowered**:

1. The file's constructs are rewritten **in place** into CigScript text that
   keeps every line on the line it was written on: annotations become
   spaces, a construct is rewritten inside its own lines, and what it needs
   (record constructors, `impl` methods, the run-time checks, the modules)
   is appended after the last line. Names the lowering makes start `_cg_` or
   `__cg_`, which a `.cigar` file may not use (`E1104`).
2. The type checker reads the result, with what lowering learned about
   the file's declarations beside it, and at full the ownership checker
   does too. Every diagnostic is placed back in the file as written.
3. What the checker decided (which layer, which `impl`, which range check)
   is applied to the text.
4. CigScript's own check reads the text exactly as `cig check` would.

`cigar run` runs that text as a cig program: plans, journals, rollback and
run records are cig's, and an error names the `.cigar` file and its line.
A **run-time** error shows the line as written, with a caret under the
statement or expression. The one exception is an error inside a module's
code, which is reported past the end of the file with the lowered text.
Every construct is defined by what it lowers to, which is what keeps the
whole language CigScript underneath.

## Errors

CigarScript's errors are cig's: the same four kinds of no, a stable code,
a kind, a span and `cig explain`. These codes are registered in cig's
error registry when CigarScript first ships, so `cig explain E1101` has a
page; until then the message carries the whole explanation.

**The gate**

| code | kind | when |
|---|---|---|
| `E1101` | check | a feature above the file's strength: names the feature, the strength that allows it, and the line to add |
| `E1102` | check | a feature the file's strength allows, which this version has not built yet: names the issue it waits on. Today: `foreign`, `unchecked`, and a `within` context other than `retry`, `parallel` and `timeout` |
| `E1103` | check | a strength declaration that is not the first statement, or names no strength |
| `E1104` | check | a name that starts `_cg_` or `__cg_`. As kind `internal`: the file lowered to CigScript cig refuses, which is a bug in `cigar` |

**Types, records and `match`** (medium)

| code | when |
|---|---|
| `E1201` | a value does not fit the type expected: a call's argument, an annotated `stick` or `roll`, a record or variant field, a list or map item. The span is on the value, with a note naming the signature or annotation |
| `E1202` | a type that is not one: no record, enum or type parameter has that name; a map keyed by anything but text; a type or a declaration written wrongly |
| `E1203` | a type given the wrong number of type arguments (`List<Int, Text>`) |
| `E1204` | a field a record does not have is read |
| `E1205` | a record literal leaves a field out. Also coughed at run time when the fields are not known until then |
| `E1206` | a record literal gives a field the record does not have. Also at run time |
| `E1207` | a variant the enum does not have |
| `E1208` | a variant given the wrong fields: too many or too few, a name it does not have, names and positions mixed, or a variant that carries fields built without them. Also at run time |
| `E1209` | `null` where the type is not optional |
| `E1211` | a `match` that does not cover every case: lists the missing ones |
| `E1212` | a `match` or an arm `cigar` cannot read: a pattern it does not know, an arm with no `=>`, a `match` nested as an arm's result, or a `match` that is neither a statement nor bound by `stick`, `roll`, an assignment or `snuff` |
| `E1213` | a block as the arm of a `match` used as a value |
| `E1214` | a record that names a field twice, an enum that names a variant twice, or a record, enum or interface name declared twice |
| `E1215` | an assignment to a `stick` |
| `E1216` | a call with the wrong number of arguments |
| `E1217` | a `snuff` whose value is not the function's declared result |
| `E1218` | a `cough` of a record the function's `coughs` list does not hold |
| `E1210` | **at run time**: a value entering a typed place does not fit it (`E1210 double: n should be Int and is Text "x"`) |

**Modules and interfaces** (medium)

| code | when |
|---|---|
| `E1301` | a module that cannot be found; a module that does not check (names its first error, at the `use` line); a `use` that names no module; a file with no folder to find modules from |
| `E1302` | modules that use each other in a cycle: names the cycle |
| `E1303` | a name a module has but does not mark `pub`: a function or value brought in with braces, or a record or enum used by name |
| `E1304` | a name a module does not have |
| `E1311` | an `impl` that leaves out a method its interface declares, or an `impl` written wrongly |
| `E1312` | an `impl` method whose signature is not the interface's |
| `E1313` | a method called on a type that has no `impl` giving it that method |

**Layers** (medium and full)

| code | when |
|---|---|
| `E1401` | a layer with two meanings for one type: names both |
| `E1402` | a layer with no meaning for the type: says what to declare. Also a layer declaration written wrongly, and a run-time cough when the type was not known until then |
| `E1403` | `break` or `continue` inside `within retry` that would leave the block (the block runs inside a loop of its own) |
| `E1404` | cough, at run time: a `within timeout` block ran past its deadline |
| `E1405` | `within timeout` written without a limit |
| `E303` | `into` a path outside a `burn`: an error at the top level and a warning inside a function. It is cig's own code, the one `fs.write_text` gets. The message says which layer made it an effect and why: "`into` means *write file* here, because `"report.csv"` is a Path" |

**Full**

| code | when |
|---|---|
| `E1501` | a name used after it was given to an `owned` parameter or to `move`: names the line it went |
| `E1502` | a `borrow` parameter returned, stored (`push`, `insert`, `set`) or given away |
| `E1510` | a number outside its sized type: at check time for a number written out (`stick a: u8 = 300`), **at run time** for one that is computed (`E1510 c is 300, outside u8 (0 to 255)`) |
| `E1520` | **at run time**: a send on a closed channel |

The run-time codes (`E1205`, `E1206`, `E1208`, `E1210`, `E1402`, `E1510` and
`E1520`) are coughs whose message starts with the code. `try` and `ashtray`
catch them, and the caught value has `code` and `message`.

```sh
$ printf 'strength mild\n\nrecord Move { from: Path, to: Path }\n' > moves.cigar
$ cigar check moves.cigar
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> moves.cigar:3:1
  |
3 | record Move { from: Path, to: Path }
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

## Grammar

This is the grammar of what `cigar` reads. The rest of a file is CigScript,
whose grammar is `cig`'s own and is read by `cig`'s parser, never a second
one: `cigar` lowers each construct below to CigScript and hands the result to
the same parse and check `cig check` runs.

Notation is ISO 14977 EBNF: `,` follows, `|` chooses, `[ ]` is optional,
`{ }` repeats, and `(* *)` is a comment. *(Provisional: every construct below
follows its design question, and changes with it.)*

```ebnf
file            = { separator } , [ strength-decl , end ] , { item } ;

strength-decl   = "strength" , strength-word ;
strength-word   = "mild" | "medium" | "full" ;

separator       = newline | ";" ;
end             = newline | ";" | end-of-file ;

item            = [ "pub" ] , declaration | statement ;   (* statement: CigScript's *)
declaration     = pull-decl | record-decl | enum-decl | interface-decl
                | stick-decl ;
```

### Types (medium; #8)

```ebnf
type            = base | type , "?" | fn-type | named ;
base            = "Any" | "Null" | "Bool" | "Int" | "Float" | "Text" | "Path"
                | "List" , [ "<" , type , ">" ] | "ListView" , "<" , type , ">"
                | "Map"  , [ "<" , [ "Text" , "," ] , type , ">" ] ;
fn-type         = "(" , [ type , { "," , type } ] , ")" , "->" , type ;
named           = name , [ "<" , type , { "," , type } , ">" ] ;
                  (* a record, an enum or a type parameter *)

(* full only *)
sized           = "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" ;
full-type       = sized | "Bytes" | "Channel" , "<" , type , ">" ;

annotated       = ( "stick" | "roll" ) , name , [ ":" , type ] , "=" , expr ;
pull-decl       = "pull" , name , [ type-params ] , "(" , [ param , { "," , param } ] , ")" ,
                  [ "->" , type ] , [ "coughs" , name , { "," , name } ] , block ;
type-params     = "<" , name , { "," , name } , ">" ;
param           = [ "owned" | "borrow" ] , name , [ ":" , type ] ;
```

A bare `List` or `Map` is `List<Any>` or `Map<Any>`. The type of a `pack`'s
parameters is written the same way (`pack(a: Int) => a + 1`). A name with no
annotation is `Any`, unless local inference knows better.

### Records and enums (medium; #9)

```ebnf
record-decl     = "record" , name , [ type-params ] , "{" , [ field , { sep , field } ] , "}" ;
field           = name , ":" , type ;
record-literal  = name , "{" , [ name , ":" , expr , { sep , name , ":" , expr } ] , "}" ;

enum-decl       = "enum" , name , [ type-params ] , "{" , variant , { sep , variant } , "}" ;
variant         = name , [ "(" , ( named-field | type ) , { "," , ( named-field | type ) } , ")" ] ;
named-field     = name , ":" , type ;
variant-value   = name , "." , name , [ "(" , args , ")" ] ;

sep             = "," | newline ;
```

A record literal is not allowed in the head of an `if`, `while` or `for`:
wrap it in parentheses. Positional variant fields are `_0`, `_1`. A variant
with named fields is built with its arguments in order or by name in any
order.

### `match` (medium; #9)

```ebnf
match           = "match" , expr , "{" , arm , { arm-sep , arm } , "}" ;
arm             = pattern , [ "if" , expr ] , "=>" , ( expr | statement | block ) ;
arm-sep         = "," | newline ;
pattern         = "_" | literal | name
                | [ name , "." ] , name , "(" , [ sub , { "," , sub } ] , ")"
                | name , "{" , field-pat , { "," , field-pat } , "}" ;
sub             = "_" | literal | name ;
field-pat       = name , [ ":" , ( "_" | literal | name ) ] ;
```

A `match` is a statement, or a value after `stick`, `roll`, an assignment or
`snuff`; in a value, each arm is an expression. Exhaustiveness: over an
enum, every variant, or `_` or a name; over `Bool`, `true` and `false`; over
`T?`, `null` and a name; otherwise `_` or a name. An arm with a guard does
not count as covering its pattern.

### Interfaces and implementations (medium; #10)

```ebnf
interface-decl  = "interface" , name , [ type-params ] , "{" , { method-sig } , "}" ;
method-sig      = "pull" , name , "(" , [ "self" , { "," , param } ] , ")" , [ "->" , type ] ;
impl-decl       = "impl" , name , "for" , type , "{" , { pull-decl } , "}" ;
```

A call `x.m(...)` uses the `impl` the type of `x` selects **when the checker
knows the type**, and a run-time choice by the value's record or enum name
or CigScript type when `x` is `Any`.

### Modules (medium; #10)

```ebnf
use-decl        = "use" , module-path , [ "." , "{" , name , { "," , name } , "}" ] ;
module-path     = name , { "." , name } ;
export          = "pub" , ( pull-decl | record-decl | enum-decl | interface-decl | stick-decl ) ;
```

`use tools.plan` reads `tools/plan.cigar` from the folder of the file being
checked or run and binds it as `plan` (its last name). `use tools.plan.{Move}`
binds the listed names. A module's body is wrapped in a function the first
time a file uses it, and a file that uses a module runs the body itself.

### `into`, `measure` and layers (medium, full; #13)

```ebnf
measure-expr    = "measure" , expr ;
into-stmt       = expr , "into" , expr ;
layer-meaning   = ( "measure" | "into" | layer-name ) , "for" , type , "=" , expr ;
layer-decl      = "layer" , name ;            (* full *)
```

`measure x` and `a into b` (and `name x` for a declared layer) are resolved
**by type at check time**, never at run time, and lowered to the function
chosen. A meaning for `measure` and a layer is called `f(value)`; for
`into` it is called `f(target, value)`. Built in: `measure` for `List`,
`Map` (length), `Text` (characters) and `Path` (`fs.size`); `into` for `List`
(append), `Channel` (`send`) and `Path` (`fs.write_text` of the value: text
as is, anything else as JSON). Two meanings is `E1401`, none is `E1402`, and an effectful one
(`into` a path) needs a burn.

**`cigar explain --layers file:line:col`** peels the `into`, `measure` or
`within` at a place (a place as an error shows it: line and column from 1,
columns in characters) back to the core, one level per line: `full` as
written, `medium` the chosen meaning as a call, `mild` the CigScript it
becomes, `core` the operation the burn rule watches. It reads what the
checker chose when it checked the file, so it never disagrees with a run. It
names the layer, the type it was chosen by and where the meaning came from
(built in, declared, or chosen at run time), and for a place inside `within`
blocks it lists them, outermost first. `--json` prints one object. A file
that does not check gets its errors instead; a `.cig` file or a mild file has
no layers, and a place with none is a usage error (`E800`) that lists where
the file's layers are.

### Ownership, concurrency and `within` (full; #12, #13)

```ebnf
move-expr       = "move" , name ;
parallel-for    = "parallel" , "(" , "threads" , ":" , expr , ")" , "for" , name , "in" , expr , block ;
channel-expr    = "channel" , "<" , type , ">" , "(" , ")" ;
within-stmt     = "within" , context , block ;
context         = "retry" , "(" , expr , [ "," , "backoff" , ":" , expr ] , ")"
                | "parallel" , "(" , "threads" , ":" , expr , ")"
                | "timeout" , "(" , ( duration | expr ) , ")" ;
```

**`within timeout(d)`** (#13; the design document names it and does not say
what it means, so this is the rule). `d` is a duration literal (`500ms`, `2s`,
`1m`) or an `Int` expression of milliseconds. The deadline is `d` after the
block starts. It is checked at the top of every `for` and `while` pass
written in the block (at any depth, not inside a function the block calls),
and once more when the block ends. Past it the block coughs `E1404`
(`within timeout(2s) ran out of time`): the burns in the block roll back with
the run, or a `within retry` around it tries again. The check is cooperative: one
call that never returns (a `net` call, a long `fs` walk) is not interrupted,
and the cough comes when control gets back to the block. It reads the clock
(`time.now_ms`) and writes nothing, so it has no effect on the world.
Nesting order is significant: `retry` around `timeout` gives every try its own
deadline, `timeout` around `retry` gives all the tries one. A context with no
limit, `within timeout() { ... }`, is `E1405`.

A channel has the methods `send(x)`, `receive()` (a `T?`: `null` when empty)
and `close()`.

### What the checker finds first

CigScript's parser does not know the words of medium and full, so the checker
scans the tokens of a `.cigar` file for them before anything else, and reports
each use at its place:

```ebnf
word-use        = reserved-word ;      (* the word, anywhere in the file *)
annotation      = ( "stick" | "roll" ) , identifier , ":" , type
                | "pull" , identifier , "(" , { param } , ")"
                  [ "->" , type ] ;
```

An annotation is found where it is written: the `:` after the name of a
`stick` or a `roll`, a `:` after a parameter name at the top level of a
`pull`'s parameter list, and the `->` that follows the closing `)`
directly. A `:` inside a map or list a default value writes is not an
annotation.

For each use the checker looks the feature up in the
[feature table](#the-feature-table): above the file's strength it is
`E1101`; at or below it, and not built, it is `E1102`; at or below it and
built, nothing. If any use is found, the checker reports those and stops
there: it does not lower or type-check a file that failed this one.

### The declaration

- **Where a declaration may appear.** Only as the first statement of the
  file. Comments and blank lines may come before it (a comment is not a
  statement, and a leading `;` opens none). `strength` and its word must be
  on the same line.
- **A declaration anywhere else** (after any statement) is `E1103`:
  "`strength` declarations must be the first statement of the file".
- **A word that is not one of the three** after `strength`, or none on the
  same line, is `E1103`.
- **`strength` as a name** anywhere in a `.cigar` file is `E1103`: it is
  reserved.
- **No declaration** means mild.
- **A `.cig` file takes no declaration.** Its text is CigScript's, so
  `strength medium` there is `cig check`'s own `E203`, as it always was.
- **The declaration is read, then set aside.** The checker replaces it with
  spaces before CigScript sees the file, so every line and column in an
  error is where you wrote it.

## Types, values and checks

*(Provisional: #2.)*

- **Assignability.** `Any` fits and is fitted by every type. `Null` fits `T?`,
  and `T` fits `T?`. `Int` fits `Float`. `Path` and `Text` fit each other.
  Records and enums fit by name and type arguments. A type parameter fits
  only itself and `Any`.
- **Mutable containers are invariant.** A `List<A>` fits only a `List<B>` of
  the same element type, and a `Map<A>` only a `Map<B>` (so does a
  `Channel<A>`): `List<Int>` is not a `List<Any>`, because a write through the
  `List<Any>` could put a text in the `List<Int>`. That is `E1201`, with a
  note that says so. The way through is `a.copy()` (a new list may be any
  wider type), a list written out where it is wanted, or a **`ListView<T>`**:
  a list that can be read and never written, covariant, so a `List<Int>` fits
  a `ListView<Any>`. `push`, `pop`, `insert`, `remove` and `v[i] = x` on a
  `ListView` are `E1215`, and a `ListView` does not fit a `List`. A value of
  type `List<Any>` still fits a `List<Int>` place; it is checked there when
  the program runs.
- **Functions are contravariant in their parameters.** `(A) -> R` fits
  `(B) -> S` when `B` fits `A` and `R` fits `S`. `(Int) -> Int` is not a
  `(Any) -> Int` (it would be handed a text), and `Any` is not a stand-in
  for a type in a parameter; `(Any) -> Int` fits `(Int) -> Int`.
- **A `T?` is not a `T`.** A method call, field, index or arithmetic on a
  `T?` is `E1209` unless the checker has narrowed it on that path:
  `if x != null { ... }`, `if x == null { snuff ... }` and the rest of the
  block, `x != null and ...`, `while x != null`, `x ?? d`, `x?.f`, or an
  assignment of a value that is not null. Narrowing ends at an assignment that
  may be null and at any call that could run a function assigning the name.
- **Every path returns.** A function whose result is not `T?`, `Null` or `Any`
  must `snuff` (or `cough`, or end in a loop that never ends) on every path.
  A path that falls off the end is `E1217` at the closing `}`, with a note on
  the branch that lacks one.
- **Inference.** `stick x = e` takes `e`'s type, and so does `roll x = e` (a
  `roll` first set to `null` is `Any`). An unannotated parameter is `Any`,
  and an unannotated result is `Any` to callers: **signatures are the
  boundary.**
- **Checked boundaries.** A value entering a typed place (an annotated
  `stick`, a typed parameter, a typed result at each `snuff`) is checked when
  the program runs, and a value that does not fit coughs `E1210`. The
  checker elides none of them yet; what it can prove wrong it reports first.
- **Generics** are checked, then erased: at run time a type parameter is
  `Any`.
- **Standard library.** Every function and method of CigScript's standard
  library has a signature the checker knows, including whether it is an
  effect. Anything it does not know is `Any`, never a guess that rejects a
  valid program.
- **Records and enums are cig maps.** A record is a map tagged with its
  name and its fields in declaration order; an enum value is a map with its
  enum, its variant and its fields. Equality and hashing are a map's, and
  they go into the journal as such.
- **Display.** `exhale`, `str` and `${...}` show a record as
  `Move { from: "a", to: "b" }` and an enum as `Status.ok` or
  `Status.failed(reason: "x")`, at any depth. Positional fields show as
  values: `Shape.circle(2.0)`.
- **JSON.** `json.stringify` writes a record as a plain object, with no tag,
  and an enum as `"ok"` or `{"failed": {"reason": "x"}}`.
- **Sized numbers** are range-checked where a value enters a sized place and
  in the result of arithmetic between sized values: outside the range is
  `E1510`. `Bytes` is checked as `Any` until cig has a bytes value.
- **Ownership** is static, per block and conservative, and erased at run
  time (`move x` is `x`).
- **Concurrency.** A `parallel` loop and `within parallel` run **in loop
  order, on one thread, today**, so the journal is the sequential run's by
  construction, which the design requires of the parallel version too.
  `within retry(n)` runs its block up to `n` tries in all.

## Memory

*(Decided: #3. The reasons and the measurements are in
[ADR 0002](decisions/0002-memory.md).)*

- **Automatic by default.** At mild and medium nothing is freed by hand:
  values are reference counted, and a value is released the moment its last
  holder lets go, so a resource closes in a known order, which the journal
  relies on. There are no collector pauses.
- **Reuse.** A list, map or record held by one holder and replaced by an
  updated copy is updated in place: the functional form costs no more than
  the mutating one. The count says when that is safe.
- **Cycles.** A cycle through a mutable container is found and released by a
  cycle collector that runs when the count traffic says garbage may exist;
  immutable data cannot form a cycle.
- **Ownership at full.** `owned`, `borrow` and `move` are checked statically
  and erased at run time. The model is deliberately smaller than Rust's: a
  **borrow is scoped to a block** and checked locally, with no lifetime
  parameters.
- **The boundary rule.** A value crossing from full to medium is **copied or
  wrapped in a counted handle**, which the compiler decides and `cig
  explain --layers` shows. Nothing a medium file holds ever points into
  storage a full file owns.

## Calling Rust and C

*(Decided: #5. The rules are final; the syntax below is what the first
implementation builds, and `foreign` is still `E1102` until it lands.)*

A foreign function is code CigarScript did not write and cannot watch, so
the rules start from that. `foreign` is a full-strength feature.

**C.** A `foreign "c"` block declares signatures; nothing else about the
library is guessed.

<!-- expect: E1102 -->
```cigar
strength full

foreign "c" {
  pure pull strlen(s: Text) -> Int
  pull remove(path: Path) -> Int
    journal with snapshot(path) undo restore(path)
}

burn {
  exhale strlen("cigar")
}
```

- **Linking is named in `cigar.toml`**, never in the source: a system
  library by name, any other by path and the SHA-256 it must have (the file
  is checked when the program is built and again when it is loaded).

  ```toml
  [foreign.c.libc]
  system = "c"

  [foreign.c.sha]
  path   = "native/libsha.so"
  sha256 = "9f2b..."
  ```

  A block names its library with `foreign "c" from "sha" { ... }`; with no
  `from` it is the system libraries listed. `cigar.toml` is the same file
  carton packages use (#15).
- **Signatures are types, and only types that cross.** `Int`, `Float`,
  `Bool`, `Text`, `Path`, the sized numbers, and `Bytes` with its length. A
  `Text` is passed as a NUL-terminated UTF-8 string that lives for the call;
  a `Text` result is copied, and a library that allocated it says how to
  release it (`free(name)`). Any other type in a signature is an error where
  it is written. A value coming back is checked at the boundary like any
  value entering a typed place (`E1210`).

**Rust.** A crate opts in, function by function, with an attribute from the
`cigar-export` crate:

```rust
#[cigar::export]
pub fn sha256_hex(data: &str) -> String { /* ... */ }

#[cigar::export(pure)]
pub fn double(n: i64) -> i64 { n * 2 }
```

The attribute generates the C-ABI shim and the CigarScript declaration file
(`<crate>.foreign.cigar`) from the Rust signature, which can only use the
types above. A panic is caught at the shim and arrives as a `cough`, never
as an unwind through the runtime. Nothing is exported that does not carry
the attribute, and an export is effectful unless it says `pure`.

**The burn rule.**

1. **Effectful unless declared `pure`.** A foreign function with no `pure` is
   assumed to touch the world.
2. **Burn-only.** An effectful foreign function is called only inside a
   `burn { }`, the same rule as every other effect; outside one it is the
   same burn error. A `pure` function may be called anywhere.
3. **Irreversible unless journaled.** Inside a burn an effectful call is
   journaled as an **irreversible** op, as a process the kernel cannot watch
   is today. The plan (`--dry-run`) lists it as irreversible and does not
   call it; rollback names it as not undone; a trial never repeats it.
4. **An adapter makes it reversible.** `journal with snapshot(args) undo
   restore(args)` names a before-state capture and its compensation, both
   CigarScript functions in the program. The call lowers to a `burn (s) {
   ... } unburn { ... }`: the snapshot runs first and its result is the
   state the compensation gets back, so the op is **compensated**, not
   irreversible. Both halves are required: one without the other is an error
   where it is written.
5. **`pure` is a promise the doctor tests.** A `pure` declaration that has
   effects is the author's bug, and the doctor catches the common ones: it
   dry-runs the program with the ghost's write check on, and a `pure` foreign
   call that writes a file, opens a socket or starts a process is reported
   with its declaration. It finds mistakes; it does not prove purity, and a
   `pure` function it cannot catch is still the author's.
6. **The guard still watches.** Foreign code runs in the program's process
   with the program's privileges. The guard (otmof-ops/cig-orchestrator#55
   and #57) watches that process as it watches any other: a declaration
   lowers no guard level, and `pure` exempts nothing from it.

## Reserved words

Every word below is reserved in `.cigar` files at every strength, wherever
it appears: as a name, a map key or a field. That is what makes turning the
strength up additive.

| word | belongs to | strength |
|---|---|---|
| `strength` | the declaration | all (`E1103` anywhere but the first statement) |
| `coughs` | typed coughs | medium |
| `record`, `enum`, `match` | records, enums, pattern matching | medium |
| `use`, `pub` | modules, exports | medium |
| `interface`, `impl` | interfaces, implementations | medium |
| `into`, `measure` | `into` and `measure` on built-in types | medium |
| `owned`, `borrow`, `move` | ownership | full |
| `parallel`, `channel` | concurrency | full |
| `within`, `layer` | context blocks, new layers | full |
| `foreign`, `unchecked` | calls into Rust and C, unchecked blocks | full (not built) |

The table and the feature table agree: a word is reserved because a row
in the feature table detects it. A `.cig` file reserves none of them.

## The design questions

| question | answer | status | where |
|---|---|---|---|
| #1 how far the superset goes | `.cig` files never get new errors, only hints; `.cigar` files at medium and above get them as errors | decided | [Files and strengths](#files-and-strengths); `tests/superset.rs` |
| #2 the type system at medium | local inference inside functions, types written at module boundaries; `T?` with `?.` and `??`; `cough` stays, typed in signatures (`coughs`) | provisional | [Types, values and checks](#types-values-and-checks) |
| #3 memory | reference counting with reuse, a cycle collector for mutable containers, ownership at full, borrows scoped to a block, copy or wrap at the boundary | decided | [Memory](#memory); [ADR 0002](decisions/0002-memory.md) |
| #4 which feature at which strength | the feature table above | decided | [The feature table](#the-feature-table) |
| #5 calling Rust and C | `foreign` blocks and `#[cigar::export]`, effectful unless declared `pure`, burn-only, irreversible unless journaled with a snapshot and an undo | decided (rules; not yet built) | [Calling Rust and C](#calling-rust-and-c) |

The medium and full constructs also follow #9 (records, enums and `match`),
#10 (modules, generics and interfaces) and #13 (layers) as their issues
state; their syntax is provisional with them. A decided question stays
decided: a change is a new ADR that supersedes the old one.

## Not built yet

- `foreign` and `#[cigar::export]` (#5: the rules are decided, the
  implementation is not built), `unchecked` blocks (#12), and user-declared
  `within` contexts (#13): each is `E1102`.
- Custom journaling adapters for a `burn` (#12) and scripts reading each
  other's results by context (#14): no syntax yet.
- `cig explain --layers` forwarding to `cigar explain --layers` (cig-script's
  side of #13), the experience warning (#12, #13), compiling
  a `.cigar` file, carton (#15) and the language server (#16).
- The semantics of each construct by reduction to CigScript's core are
  written here as each construct is built; the lowering is the working
  definition until they are.
- The memory model's implementation (#3): reuse and the cycle collector are
  the runtime's to build; today's runtime reference counts without reuse
  (ADR 0002 has the numbers).
