# CigarScript: the language

**Status: draft.** This is the reference the checker is built from. Mild,
medium and full are built, with the exceptions the [feature table](#the-feature-table)
marks. The parts marked **provisional** follow the recommendation in their
design question (#1 to #5); each says what changes if the decision goes
another way. Nothing here is final until its design question is decided.

CigarScript is CigScript grown deeper, one **strength** at a time. Every
CigScript program is CigarScript at the mild strength, and the same rule
holds at every strength: nothing touches the world outside a `burn { }`.

## Files and strengths

| file | strength | what the checker accepts |
|---|---|---|
| `.cig` | always **mild** | exactly what `cig check` accepts, forever: no new errors, ever (#1) |
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
- **Strengths mix across files** in one project. A module is a file at
  medium or full (`pub` is medium's word), so a medium file can use a full
  module and a full file a medium one, each checked at its own strength. A
  mild file cannot use a module: `use` is medium's. *(Provisional: #10.)*

## The feature table

Each feature belongs to the lowest strength where it cannot surprise
someone who doesn't know it exists (#4). *(Provisional: this is #4's
proposed assignment.)* The checker's feature table is generated from the
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
| `E1102` | check | a feature the file's strength allows, which this version has not built yet: names the issue it waits on. Today: `foreign`, `unchecked`, and a `within` context other than `retry` and `parallel` |
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
| `E303` | `into` a path outside a `burn`: an error at the top level and a warning inside a function. It is cig's own code, the one `fs.write_text` gets |

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
                | "List" , [ "<" , type , ">" ]
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
(append) and `Path` (`fs.write_text` of the value: text as is, anything else
as JSON). Two meanings is `E1401`, none is `E1402`, and an effectful one
(`into` a path) needs a burn.

### Ownership, concurrency and `within` (full; #12, #13)

```ebnf
move-expr       = "move" , name ;
parallel-for    = "parallel" , "(" , "threads" , ":" , expr , ")" , "for" , name , "in" , expr , block ;
channel-expr    = "channel" , "<" , type , ">" , "(" , ")" ;
within-stmt     = "within" , context , block ;
context         = "retry" , "(" , expr , [ "," , "backoff" , ":" , expr ] , ")"
                | "parallel" , "(" , "threads" , ":" , expr , ")" ;
```

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
  `List<A>` fits `List<B>` and `Map<A>` fits `Map<B>` when `A` fits `B`.
  Records and enums fit by name and type arguments, functions by parameters
  and result. A type parameter fits only itself and `Any`.
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

| question | provisional answer | if it goes another way |
|---|---|---|
| #1 how far the superset goes | `.cig` files never get new errors, only hints; `.cigar` files at medium and above get them as errors | the `.cig` row of the files table changes, and the superset test with it |
| #2 the type system at medium | local inference inside functions, types written at module boundaries; `T?` with `?.` and `??`; `cough` stays, typed in signatures (`coughs`) | the checker's rules and the run-time checks change with it; files written to the provisional rules may need edits |
| #3 memory | reference counting with reuse (Perceus-style) and a cycle collector for mutable containers; ownership at full | ownership is erased at run time, so the checker's rules would change and the run would not; stage C's runtime (otmof-ops/cig-compile#32) is built to the other choice |
| #4 which feature at which strength | the feature table above | rows move between strengths in the table; the checker follows |
| #5 calling Rust and C | `foreign` blocks, effectful unless declared `pure`, journaled as irreversible unless they come with an adapter | `foreign` is not built, so nothing built depends on it |

The medium and full constructs also follow #9 (records, enums and `match`),
#10 (modules, generics and interfaces) and #13 (layers) as their issues
state; their syntax is provisional with them.

## Not built yet

- `foreign` (#5), `unchecked` blocks (#12), and `within` contexts other than
  `retry` and `parallel` (#13): each is `E1102`.
- Custom journaling adapters for a `burn` (#12) and scripts reading each
  other's results by context (#14): no syntax yet.
- `cig explain --layers` (#13), the experience warning (#12, #13), compiling
  a `.cigar` file, carton (#15) and the language server (#16).
- The semantics of each construct by reduction to CigScript's core are
  written here as each construct is built; the lowering is the working
  definition until they are.
- The decisions from #1 to #5 in their final form: the answers above stay
  **provisional** until the questions are decided.
