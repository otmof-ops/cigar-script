# CigarScript: the language

**Status: draft.** This is the reference the checker is built from. The
parts marked **provisional** follow the recommendation in their design
question (#1 to #5); each says what changes if the decision goes another
way. Nothing here is final until its design question is decided.

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
- **Strengths mix freely** across files in one project: a mild file can
  use a medium module, with a checked boundary between them (#8). *(Not
  built yet: modules come with medium.)*

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

## Errors

CigarScript's errors are cig's: the same four kinds of no, a stable code,
a kind, a span and `cig explain`. The checker's own:

| code | kind | when |
|---|---|---|
| `E1101` | check | a feature above the file's strength: names the feature, the strength that allows it, and the line to add |
| `E1102` | check | a feature the file's strength allows, which this version has not built yet: names the issue it waits on |
| `E1103` | check | a strength declaration that is not the first statement, or names no strength |

`E1200` to `E1399` are kept for medium's type errors (#8). These codes are
registered in cig's error registry when CigarScript first ships, so `cig
explain E1101` has a page; until then the message carries the whole
explanation.

```
Don't see any cigarettes.
error[E1101 check]: using records needs strength medium; this file is mild
  --> moves.cigar:3:1
  |
3 | record Move {
  | ^^^^^^
  = hint: to use it, put `strength medium` at the top of the file
  = explain: cig explain E1101
```

## Grammar

This is the grammar of what the checker reads today. It is small, because
the checker adds one construct to CigScript: the strength declaration. The
rest of a file is CigScript, whose grammar is `cig`'s own and is read by
`cig`'s parser, never a second one: after the declaration is set aside,
`cigar check` hands the file to the same parse and check `cig check` runs.

Notation is ISO 14977 EBNF: `,` follows, `|` chooses, `[ ]` is optional,
`{ }` repeats, and `(* *)` is a comment.

```ebnf
file            = { separator } , [ strength-decl , end ] , cigscript-rest ;

strength-decl   = "strength" , strength-word ;
strength-word   = "mild" | "medium" | "full" ;

separator       = newline | ";" ;
end             = newline | ";" | end-of-file ;

cigscript-rest  = (* CigScript: the grammar of `cig`, unchanged *) ;
```

What the rules say, and what the checker does with what they leave out:

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

### What the checker finds in the rest

CigScript's parser does not know the words of medium and full, and the
checker does not teach it. It scans the tokens of a `.cigar` file for them
before CigScript reads it, and reports each use at its place. Two kinds
of use are found:

```ebnf
word-use        = reserved-word ;      (* the word, anywhere in the file *)

annotation      = ( "stick" | "roll" ) , identifier , ":" , type
                | "pull" , identifier , "(" , { param } , ")"
                  [ "->" , type ] ;
param           = identifier , ":" , type | identifier ;
```

An annotation is found where it is written: the `:` after the name of a
`stick` or a `roll`, a `:` after a parameter name at the top level of a
`pull`'s parameter list, and the `->` that follows the closing `)`
directly. A `:` inside a map or list a default value writes
(`pull f(a = {x: 1})`) is not an annotation. `type` is not parsed yet; it
comes with medium (#8).

For each use the checker looks the feature up in the
[feature table](#the-feature-table): above the file's strength it is
`E1101`; at or below it, and not built, it is `E1102`; at or below it and
built, nothing. If any use is found, the checker reports those and stops
there: it does not run CigScript's check on a file that failed this one.

### Reserved words

Every word below is reserved in `.cigar` files at every strength, wherever
it appears: as a name, a map key or a field. That is what makes turning the
strength up additive.

| word | belongs to | strength |
|---|---|---|
| `strength` | the declaration | all (`E1103` anywhere but the first statement) |
| `coughs` | typed coughs | medium |
| `record`, `enum`, `match` | records, enums, pattern matching | medium |
| `use`, `interface` | modules, interfaces | medium |
| `into`, `measure` | `into` and `measure` on built-in types | medium |
| `owned`, `borrow`, `move` | ownership | full |
| `parallel`, `channel` | concurrency | full |
| `within`, `layer` | context blocks, new layers | full |
| `foreign` | calls into Rust and C | full |

The table and the feature table agree: a word is reserved because a row
in the feature table detects it. A `.cig` file reserves none of them.

The grammar of the features themselves (a record's fields, a `match`'s arms,
a module's `use` list) is written as each is built. Until then a use is
`E1102`, and the checker does not read past the keyword.

## The design questions

| question | provisional answer | if it goes another way |
|---|---|---|
| #1 how far the superset goes | `.cig` files never get new errors, only hints; `.cigar` files at medium and above get them as errors | the `.cig` row of the files table changes, and the superset test with it |
| #2 the type system at medium | local inference inside functions, types written at module boundaries; `T?` with `?.` and `??`; `cough` stays, typed in signatures, lowered to results | #8's rules change before it is built; nothing built depends on it yet |
| #3 memory | reference counting with reuse (Perceus-style) and a cycle collector for mutable containers; ownership at full | stage C's runtime (otmof-ops/cig-compile#32) is built to the other choice |
| #4 which feature at which strength | the feature table above | rows move between strengths in the table; the checker follows |
| #5 calling Rust and C | `foreign` blocks, effectful unless declared `pure`, journaled as irreversible unless they come with an adapter | full's foreign calls (#5) are built to the other choice |

## Not written yet

- The grammar of every feature in the table: each comes with the work that
  builds it (#8 to #13), and is tested against the same fixtures the
  checker is.
- The semantics of each construct by reduction to CigScript's core (the
  layered-depth rule), written as each construct is built.
- The decisions from #1 to #5 in their final form: the answers above stay
  **provisional** until the questions are decided.
