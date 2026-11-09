# The CigarScript Lexicon

> Explain a thing by what it feels like, then attach the real name so nobody's
> embarrassed later.

These are CigarScript's additions to [the CigScript lexicon](https://github.com/otmof-ops/cig-script/blob/main/docs/LEXICON.md).
Every word CigScript has (`stick`, `roll`, `pull`, `burn`, `cough`, `exhale`,
the kernel, the journal, the four kinds of no) means in CigarScript what it
means there. This page is only what is new.

Every entry has four parts: **your word**, **what it means**, **the name the
manual uses**, and **where it stands**. CigarScript is built a strength at a
time, so most of the words below are designed before they are built. The
last column says which:

- **built** works in this version of `cigar`.
- **reserved** is a word the checker knows and reserves today. Using it is
  `E1101` above the file's strength and `E1102` at it, because the feature
  behind it is **not built**.
- **not built** is designed, and the issue that builds it is named.

---

## The family

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **CigarScript** | The fuller language of the family: CigScript's rules and words, grown deeper, in three strengths you choose per file. A `.cigar` file is CigarScript; a `.cig` file is mild CigarScript, always. The rule does not change: nothing touches the world outside a burn. | a superset language; a language with opt-in feature levels | **built** in part: `cigar check` (0.1.0, not released). The language beyond mild is not built |
| **`.cigar`** | A CigarScript file. It may begin with a strength declaration. | source file extension; compilation unit | **built** |
| **`.cig`** | A CigScript file. Checked exactly as `cig check` checks it, and always will be: never a new error. | the superset's base case | **built** |
| **CigCompile** | The compiler, behind two buttons: open it, pick what to compile, click. Today it turns a CigScript script into a program that runs with nothing installed; CigarScript will compile through it later. Its own repository: [cig-compile](https://github.com/otmof-ops/cig-compile). | compiler; build tool; packager | **built** for CigScript; CigarScript through it is **not built** |
| **carton** | Distribution: a versioned set of packs with a manifest, and the package manager for CigarScript. No install scripts, by design: installing one never runs anything. | package; package manager; registry client | **not built** (#15) |
| **the humidor** | Every version of the tools, kept fresh, so a project builds with the CigCompile it was written for. | toolchain manager; version pinning | **not built** |
| **cigo** | cig-orchestrator: every language in a project, one chain. It will build, test and ship CigarScript next to everything else. | build orchestrator; polyglot task runner | **built** for CigScript; CigarScript **not built** |

---

## Strength

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **strength** | How deep a file goes, like a cigar: **mild**, **medium** or **full**. Chosen per file, and each includes everything below it. You only reach for the next one when you want the control it gives. | language level; feature tier; dialect | **built**: the checker holds a file to its strength |
| **mild** | CigScript as it is: no types written, everything inferred. Anyone who knows CigScript already knows it. | the base level; the scripting subset | **built** |
| **medium** | The shapes of your data written down, and the contracts between parts: types, records and enums, modules, pattern matching, generics, interfaces. For real programs, libraries, team code. | a statically typed level; gradual typing | **not built** (#8 to #11) |
| **full** | Control over how the work is done, down to the memory and the threads: ownership, concurrency, raw buffers, calls into Rust and C, custom journaling for a burn, and layered meanings. For performance and systems work. | a systems level; explicit memory and concurrency | **not built** (#12, #13, #5) |
| **`strength mild`**, **`strength medium`**, **`strength full`** | The declaration: the first statement of a `.cigar` file. With none, the file is mild. | a language level pragma; a file-level `use strict` | **built** |
| **turning it up** | Changing `strength mild` to `strength medium`. Never breaks a file: it only lets you write more. It works because every keyword is reserved at every strength, so no name a mild file uses can become one later. | forward compatibility; additive feature gating | **built** (the reservation); **provisional** |
| **turning it down** | Checking a file at a lower strength than it declares, `cigar check --strength mild`. It lists everything the file uses above that strength, each with its place. | feature audit; downlevel check | **built** |
| **the feature table** | The one table in `docs/LANGUAGE.md` that says which feature belongs to which strength. The checker is built from it, so a row added there is a row the checker knows. | feature matrix; capability table | **built** |
| **the feature gate** | The check that refuses a feature above the file's strength, and names the strength that allows it. Nobody wanders into full by accident. | feature flag check; language-level gate | **built** |
| **mixing strengths** | A mild file calling a medium module, in one project, with a checked boundary between them. | interop between language levels | **not built** (#10) |
| **checked boundary** | The one check where untyped code meets typed code, made at the edge and never inside typed code. | gradual typing boundary; contract check | **not built** (#8) |
| **the experience warning** | What asks before a file goes to `strength full`: *Full strength lets one expression mean different things in different places. It's built for experienced developers; medium covers most programs. Continue?* Once per project, remembered, and skippable with a setting. | a first-use confirmation; an opt-in prompt | **not built** (#12, #13) |

---

## The words of medium and full

Each of these is **reserved** in a `.cigar` file at every strength: as a name,
a map key or a field, wherever it appears. A use at the wrong strength is
`E1101`, and at the right one is `E1102` until the feature is built. A
`.cig` file reserves none of them.

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **record** | A named shape of fields: `record Move { from: Path, to: Path }`. Printing, equality and hashing come with it, and JSON and CSV map straight onto it. Medium. | struct; record; product type | **reserved**; feature **not built** (#9) |
| **enum** | A value that is one of a fixed set of named cases, each of which may carry data. Medium. | enumeration; sum type; tagged union | **reserved**; feature **not built** (#9) |
| **match** | Pick what to do by the shape of a value, and be told when a case is missing. Medium. | pattern matching; `switch` with destructuring | **reserved**; feature **not built** (#9) |
| **use** | Bring in another file's module: one file is one module. Medium. | import; `use`; module system | **reserved**; feature **not built** (#10) |
| **interface** | A contract a type can meet: the calls it must answer to. Medium. | trait; protocol; abstract class | **reserved**; feature **not built** (#10) |
| **coughs** | The errors a function may cough, written in its signature and lowered to results. `cough` itself stays. Medium. | checked exceptions; typed errors; `throws` | **reserved**; feature **not built** (#8) |
| **into** | `a into b`: put `a` into `b`, and what that does depends on what `b` is: a list takes it, a path is written (a burn), another script's stick is handed the value, a channel is sent it. On built-in types it is medium; the layered form is full. | overloaded operator; double dispatch; the sink operator | **reserved**; feature **not built** (#13) |
| **measure** | One question, the right answer for each thing: a list's length, a path's size, a lit chain's time, text's characters. Declared once per type. | polymorphic `len`/`size`; protocol method | **reserved**; feature **not built** (#13) |
| **within** | A context block: the expressions inside adapt to it. `within retry(3) { }`; `within parallel(threads: 8) { }`. Nesting stacks them, and the order matters. Full. | context manager; scoped effect handler; dynamic scope | **reserved**; feature **not built** (#13) |
| **layer** | A meaning declared once for a type and chosen when the program compiles: `measure for Path = fs.size`. Every layer is written in terms of the layer below, down to CigScript's core. Full. | trait implementation; type class instance; overload | **reserved**; feature **not built** (#13) |
| **parallel** | A loop that runs several at a time while the journal still records every write in order. Full. | parallel for; data parallelism; thread pool | **reserved**; feature **not built** (#12) |
| **channel** | A pipe values go through between threads: one side sends, the other reads. Full. | channel; message passing; CSP | **reserved**; feature **not built** (#12) |
| **owned** | A value with exactly one holder, which is responsible for it: `pull checksum(owned data: Bytes)`. Full. | ownership; unique reference; linear value | **reserved**; feature **not built** (#12) |
| **borrow** | Use a value without taking it, and give it back untouched. Full. | borrowing; shared reference | **reserved**; feature **not built** (#12) |
| **move** | Hand a value over, so the old name no longer holds it. Full. | move semantics; transfer of ownership | **reserved**; feature **not built** (#12) |
| **foreign** | A block that calls Rust or C directly, with no process between. Effectful unless declared `pure`, and journaled as irreversible unless it comes with an adapter. Full. | FFI; foreign function interface; `extern` | **reserved**; feature **not built** (#5) |

The rules for the three ownership words, and for memory, wait on design
question 3 (#3). The answers in [the language](LANGUAGE.md) are provisional
until the questions are decided.

---

## Errors the checker adds

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **E1101** | A feature above the file's strength. The message names the feature, the strength that allows it, and the line to add. | feature not enabled; language level error | **built** |
| **E1102** | A feature the file's strength allows that this version has not built yet. The message names the issue it waits on. | not implemented; unsupported feature | **built** |
| **E1103** | A `strength` declaration that is not the first statement, or that names no strength. | invalid pragma; misplaced directive | **built** |
| **E1200 to E1399** | Kept for medium's type errors, with spans on both sides of a mismatch: "expected `Path` here, because of this signature". | type errors | **not built** (#8) |

`E1101` to `E1103` are registered in cig's error registry when CigarScript
ships, so `cig explain E1101` has no page yet. The message says all of it.

---

## Depth

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **layered depth** | Full strength's idea: the foundation stays CigScript's, and depth is layered on it, each layer reacting to the context it is used in. However rich an expression gets, it reduces to plain CigScript logic. | context-dependent semantics; overloading by type and context | **not built** (#13) |
| **context never hides an effect** | A layer can change what an expression does, never whether it touches the world. The burn rule is enforced at the core, and no layer can get around it. | effect discipline; capability safety | **not built** (#13), a rule of the design |
| **`cig explain --layers`** | Peels any expression back, layer by layer, to the core, so you can see what `into` means here and why. | desugaring view; expansion trace | **not built** (#13) |
| **scripts that read each other** | One script reading another's stick by context: `stats.total` is the value this run produced, the last run recorded, or the dry run's plan would, depending on where `stats` is. | cross-task data flow; build graph outputs | **not built** (#14) |
| **superset** | Every CigScript program is valid CigarScript. A `.cig` file stays valid forever; a `.cigar` file is one that uses the new features. | backward compatibility; a language superset | **built** (the `.cig` rule) |
