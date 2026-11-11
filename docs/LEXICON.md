# The CigarScript Lexicon

> Explain a thing by what it feels like, then attach the real name so nobody's
> embarrassed later.

These are CigarScript's additions to [the CigScript lexicon](https://github.com/otmof-ops/cig-script/blob/main/docs/LEXICON.md).
Every word CigScript has (`stick`, `roll`, `pull`, `burn`, `cough`, `exhale`,
the kernel, the journal, the four kinds of no) means in CigarScript what it
means there. This page is only what is new.

Every entry has four parts: **your word**, **what it means**, **the name the
manual uses**, and **where it stands**. CigarScript is built a strength at a
time. The last column says where each word stands:

- **built** works in this version of `cigar`. Where the syntax follows a
  design question that is still open, it is also **provisional**.
- **reserved** is a word the checker knows and reserves. Using it is `E1101`
  above the file's strength, and at the file's strength it works, unless it is
  marked **not built**, when it is `E1102`.
- **not built** is designed, and the issue that builds it is named.

---

## The family

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **CigarScript** | The fuller language of the family: CigScript's rules and words, grown deeper, in three strengths you choose per file. A `.cigar` file is CigarScript; a `.cig` file is mild CigarScript, always. The rule does not change: nothing touches the world outside a burn. | a superset language; a language with opt-in feature levels | **built**: `cigar check` and `cigar run` (0.1.0, not released), at all three strengths, with the parts the feature table marks not built |
| **`.cigar`** | A CigarScript file. It may begin with a strength declaration. | source file extension; compilation unit | **built** |
| **`.cig`** | A CigScript file. Checked exactly as `cig check` checks it, and always will be: never a new error. | the superset's base case | **built** |
| **CigCompile** | The compiler, behind two buttons: open it, pick what to compile, click. Today it turns a CigScript script into a program that runs with nothing installed; CigarScript will compile through it later. Its own repository: [cig-compile](https://github.com/otmof-ops/cig-compile). | compiler; build tool; packager | **built** for CigScript; CigarScript through it is **not built** (`cigar run` runs a `.cigar` file today) |
| **carton** | Distribution: a versioned set of packs with a manifest, and the package manager for CigarScript. No install scripts, by design: installing one never runs anything. | package; package manager; registry client | **not built** (#15) |
| **the humidor** | Every version of the tools, kept fresh, so a project builds with the CigCompile it was written for. | toolchain manager; version pinning | **not built** |
| **cigo** | cig-orchestrator: every language in a project, one chain. It will build, test and ship CigarScript next to everything else. | build orchestrator; polyglot task runner | **built** for CigScript; CigarScript **not built** |

---

## Strength

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **strength** | How deep a file goes, like a cigar: **mild**, **medium** or **full**. Chosen per file, and each includes everything below it. You only reach for the next one when you want the control it gives. | language level; feature tier; dialect | **built**: the checker holds a file to its strength |
| **mild** | CigScript as it is: no types written, everything inferred. Anyone who knows CigScript already knows it. | the base level; the scripting subset | **built** |
| **medium** | The shapes of your data written down, and the contracts between parts: types, records and enums, modules, pattern matching, generics, interfaces. For real programs, libraries, team code. | a statically typed level; gradual typing | **built**, provisional (#8 to #11) |
| **full** | Control over how the work is done, down to the memory and the threads: ownership, concurrency, raw buffers, calls into Rust and C, custom journaling for a burn, and layered meanings. For performance and systems work. | a systems level; explicit memory and concurrency | **built**, provisional, except `foreign`, `unchecked` and custom journaling (#12, #13, #5) |
| **`strength mild`**, **`strength medium`**, **`strength full`** | The declaration: the first statement of a `.cigar` file. With none, the file is mild. | a language level pragma; a file-level `use strict` | **built** |
| **turning it up** | Changing `strength mild` to `strength medium`. It lets you write more, and a file with no `W1234` warning keeps working. Keywords are contextual (#22 D1): a name after `.` or as a key is never a keyword. A name that shadows a keyword is `W1234` below the strength that reserves the word and `E1105` at it, so turning up can stop such a name, and the message says to rename it or write `r#name`. | forward compatibility; additive feature gating | **built** (the reservation); **provisional** |
| **turning it down** | Checking a file at a lower strength than it declares, `cigar check --strength mild`. It lists everything the file uses above that strength, each with its place. | feature audit; downlevel check | **built** |
| **the feature table** | The one table in `docs/LANGUAGE.md` that says which feature belongs to which strength. The checker is built from it, so a row added there is a row the checker knows. | feature matrix; capability table | **built** |
| **the feature gate** | The check that refuses a feature above the file's strength, and names the strength that allows it. Nobody wanders into full by accident. | feature flag check; language-level gate | **built** |
| **mixing strengths** | A medium file using a full module, or a full file using a medium one, each checked at its own strength, with a checked boundary between them. A module is medium or full: a mild file cannot `use`. | interop between language levels | **built**, provisional (#10) |
| **checked boundary** | The one check where untyped code meets typed code: a value entering a typed place is checked when the program runs, and fails with `E1210`. | gradual typing boundary; contract check | **built**, provisional (#8) |
| **the experience warning** | What asks before a file goes to `strength full`: *Full strength lets one expression mean different things in different places. It's built for experienced developers; medium covers most programs. Continue?* Once per project, remembered, and skippable with a setting. | a first-use confirmation; an opt-in prompt | **not built** (#12, #13) |

---

## The words of medium and full

Each of these is **reserved** in a `.cigar` file at every strength: as a name,
a map key or a field, wherever it appears. A use at the wrong strength is
`E1101`, and at the right one is `E1102` until the feature is built. A
`.cig` file reserves none of them.

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **record** | A named shape of fields: `record Move { from: Path, to: Path }`. Printing, equality and hashing come with it, and JSON and CSV map straight onto it. Medium. | struct; record; product type | **built**, provisional (#9) |
| **enum** | A value that is one of a fixed set of named cases, each of which may carry data. Medium. | enumeration; sum type; tagged union | **built**, provisional (#9) |
| **match** | Pick what to do by the shape of a value, and be told when a case is missing. Medium. | pattern matching; `switch` with destructuring | **built**, provisional (#9) |
| **use** | Bring in another file's module: one file is one module. Medium. | import; `use`; module system | **built**, provisional (#10) |
| **interface** | A contract a type can meet: the calls it must answer to. Medium. | trait; protocol; abstract class | **built**, provisional (#10) |
| **coughs** | The errors a function may cough, written in its signature and lowered to results. `cough` itself stays. Medium. | checked exceptions; typed errors; `throws` | **built**, provisional (#8) |
| **into** | `a into b`: put `a` into `b`, and what that does depends on what `b` is: a list takes it, a path is written (a burn), On built-in types it is medium (a list takes it, a path is written inside a burn), and a type can be given its own meaning with `into for T = f`. A channel is sent with `send`; handing a value to another script's stick is not built. | overloaded operator; double dispatch; the sink operator | **built**, provisional (#13) |
| **measure** | One question, the right answer for each thing: a list's length, a path's size, text's characters. Declared once per type with `measure for T = f`. | polymorphic `len`/`size`; protocol method | **built**, provisional (#13) |
| **within** | A context block: the expressions inside adapt to it. `within retry(3) { }`; `within parallel(threads: 8) { }`. Nesting stacks them, and the order matters. Full. | context manager; scoped effect handler; dynamic scope | **built**, provisional (#13) |
| **layer** | A new layer name, declared with `layer weight` and given a meaning per type with `weight for Path = fs.size`; chosen when the file is checked and used like `measure`. `measure` and `into` are the built-in layers. Full. | trait implementation; type class instance; overload | **built**, provisional (#13) |
| **parallel** | `parallel(threads: N) for`: a loop written to run several at a time while the journal still records every write in order. Today it runs in loop order on one thread, so the journal is the sequential run's by construction. Full. | parallel for; data parallelism; thread pool | **built**, provisional (#12) |
| **channel** | `channel<T>()`: a pipe values go through; `send`, `receive` (`null` when empty) and `close`. Full. | channel; message passing; CSP | **built**, provisional (#12) |
| **owned** | A value with exactly one holder, which is responsible for it: `pull checksum(owned data: Bytes)`. Full. | ownership; unique reference; linear value | **built**, provisional (#12) |
| **borrow** | Use a value without taking it, and give it back untouched. Full. | borrowing; shared reference | **built**, provisional (#12) |
| **move** | Hand a value over, so the old name no longer holds it. Full. | move semantics; transfer of ownership | **built**, provisional (#12) |
| **foreign** | A block that calls Rust or C directly, with no process between. Effectful unless declared `pure`, and journaled as irreversible unless it comes with an adapter. Full. | FFI; foreign function interface; `extern` | **reserved**; feature **not built** (#5): `E1102` |
| **pub** | Marks what a module exports: `pub pull`, `pub record`, `pub enum`, `pub stick`, `pub interface`. A name not marked `pub` is `E1303` from outside. Medium. | export; visibility | **built**, provisional (#10) |
| **impl** | `impl Sized for Move { ... }`: says a type meets an interface, with the interface's methods. Medium. | trait implementation; instance | **built**, provisional (#10) |
| **unchecked** | A block with the sized-number checks off, named in the plan. Full. | unsafe block | **reserved**; feature **not built** (#12): `E1102` |

The rules for the three ownership words, and for memory, follow design
question 3 (#3). The answers in [the language](LANGUAGE.md) are provisional
until the questions are decided.

---

## Errors the checker adds

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **E1101** | A feature above the file's strength. The message names the feature, the strength that allows it, and the line to add. | feature not enabled; language level error | **built** |
| **E1102** | A feature the file's strength allows that this version has not built yet (`foreign`, `unchecked`, a `within` context other than `retry` and `parallel`). The message names the issue it waits on. | not implemented; unsupported feature | **built** |
| **E1103** | A `strength` declaration that is not the first statement, or that names no strength. | invalid pragma; misplaced directive | **built** |
| **E1104** | A name that starts `_cg_` or `__cg_`, which the lowering keeps for itself; or a file that lowers to CigScript cig refuses (a bug in `cigar`). | reserved identifier | **built** |
| **E1201 to E1399** | Medium's type, record, `match`, module and interface errors, with spans on both sides of a mismatch: "expected `Path` here, because of this signature". `E1210` is found when the program runs. | type errors | **built** (#8 to #10) |
| **E1401, E1402** | A layer with two meanings for a type, or none. | ambiguous or missing overload | **built** (#13) |
| **E1501, E1502, E1503** | A name used after it was given away; a borrowed value returned, stored or given away; a name a closure borrowed and that was given away, or one moved on some paths and used after they join. | use after move; borrow escape; possibly moved | **built** (#12, #22) |
| **E1510, E1520** | A number outside its sized type; a send on a closed channel. Both are found when the program runs. | overflow; closed channel | **built** (#12) |

`E1101` to `E1104` and the codes above are registered in cig's error registry when CigarScript
ships, so `cig explain E1101` has no page yet. The message says all of it.

---

## Depth

| your word | what it means | the manual's name | state |
|---|---|---|---|
| **layered depth** | Full strength's idea: the foundation stays CigScript's, and depth is layered on it, each layer reacting to the context it is used in. However rich an expression gets, it reduces to plain CigScript logic. | context-dependent semantics; overloading by type and context | **built**, provisional (#13) |
| **context never hides an effect** | A layer can change what an expression does, never whether it touches the world. The burn rule is enforced at the core, and no layer can get around it. | effect discipline; capability safety | **built** (#13): `into` a path outside a burn is `E303` |
| **`cig explain --layers`** | Peels any expression back, layer by layer, to the core, so you can see what `into` means here and why. | desugaring view; expansion trace | **not built** (#13) |
| **scripts that read each other** | One script reading another's stick by context: `stats.total` is the value this run produced, the last run recorded, or the dry run's plan would, depending on where `stats` is. | cross-task data flow; build graph outputs | **not built** (#14) |
| **superset** | Every CigScript program is valid CigarScript. A `.cig` file stays valid forever; a `.cigar` file is one that uses the new features. | backward compatibility; a language superset | **built** (the `.cig` rule) |
| **cigar run** | Checks a file, then runs it as a cig program: plans, journals, rollback and run records are cig's. | run; execute | **built** |
| **lowering** | How a medium or full file becomes CigScript: every construct is rewritten in place, keeping its line, and `cigar` hands the result to cig. It is why every meaning reduces to CigScript. | desugaring; transpilation | **built** |
| **sized numbers** | `u8` to `u64` and `i8` to `i64`: integers with a range, checked when a value enters one and in arithmetic between them; outside it is `E1510`. Full. | fixed-width integers | **built**, provisional (#12) |
| **Bytes** | The type of a raw buffer. Checked as `Any` until cig has a bytes value. Full. | byte array | **built** as a type, checked as `Any` |
