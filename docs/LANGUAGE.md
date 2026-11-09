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
| type annotations | medium | annotation | #8 | no |
| typed coughs | medium | word:coughs | #8 | no |
| records | medium | word:record | #9 | no |
| enums | medium | word:enum | #9 | no |
| pattern matching | medium | word:match | #9 | no |
| modules | medium | word:use | #10 | no |
| interfaces | medium | word:interface | #10 | no |
| `into` on built-in types | medium | word:into | #13 | no |
| `measure` on built-in types | medium | word:measure | #13 | no |
| owned values | full | word:owned | #12 | no |
| borrows | full | word:borrow | #12 | no |
| moves | full | word:move | #12 | no |
| parallel loops | full | word:parallel | #12 | no |
| channels | full | word:channel | #12 | no |
| context blocks | full | word:within | #13 | no |
| calls into Rust and C | full | word:foreign | #5 | no |
| new layers | full | word:layer | #13 | no |
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
error[E1101 check]: using records needs strength medium; this file is mild
  --> plan.cigar:3:1
  = hint: to use it, put `strength medium` at the top of the file
```

## The design questions

| question | provisional answer | if it goes another way |
|---|---|---|
| #1 how far the superset goes | `.cig` files never get new errors, only hints; `.cigar` files at medium and above get them as errors | the `.cig` row of the files table changes, and the superset test with it |
| #2 the type system at medium | local inference inside functions, types written at module boundaries; `T?` with `?.` and `??`; `cough` stays, typed in signatures, lowered to results | #8's rules change before it is built; nothing built depends on it yet |
| #3 memory | reference counting with reuse (Perceus-style) and a cycle collector for mutable containers; ownership at full | stage C's runtime (otmof-ops/cig-compile#32) is built to the other choice |
| #4 which feature at which strength | the feature table above | rows move between strengths in the table; the checker follows |
| #5 calling Rust and C | `foreign` blocks, effectful unless declared `pure`, journaled as irreversible unless they come with an adapter | full's foreign calls (#5) are built to the other choice |

## Not written yet

The grammar (an EBNF the parser is tested against), the semantics of each
construct by reduction to CigScript's core, the lexicon additions, and the
three strength guides (#6) come as each strength is built.
