# CigarScript and CigCompile

<!-- SPDX-License-Identifier: CC-BY-4.0 -->

**Status:** planned · **Joins:** CigScript, cig-orchestrator, the humidor, the tin, carton

Two new pieces for the cig family. **CigarScript** is the bigger language: the same rules as CigScript, compiled, and far deeper, in three strengths (mild, medium, full) you choose per file depending on how much control you want. **CigCompile** is how you build it: open it, pick what to compile, click, done.

The rule doesn't change. **Nothing touches the world outside a burn**, in a script or in a compiled program.

---

## The family

| piece | what it is | for |
|---|---|---|
| **CigScript** (`cig`) | the hand-rolled one: a small scripting language, interpreted | automation: anything that changes files, runs programs, ships builds |
| **CigarScript** (`.cigar`) | the fuller one: compiled, three strengths deep | programs: tools, services, data work, anything CigScript would hop out to |
| **CigCompile** | the compiler, behind two buttons | turning CigarScript into programs that run on any machine |
| **cig-orchestrator** (`cigo`) | every language in a project, one chain | building, testing and shipping CigarScript next to everything else |
| **carton** | packages | sharing CigarScript libraries |
| **the humidor** | every version, kept fresh | a project builds with the CigCompile it was written for |
| **the tin** | the before-bytes | a compiled program's file changes can be put back too |

---

## CigarScript

### Why a second language

CigScript is small on purpose. Its DESIGN.md says heavy data work belongs in a program the script hops to, and the 1.x line showed what happens when a scripting language grows to do everything: it got larger without getting better.

CigarScript is where that program gets written, without leaving the family.

### What it keeps from CigScript

**Everything.** CigarScript runs on exactly the same rule system and logic CigScript already has: the same burns, journal, plan, rollback, unburn, tin, humidor and errors. It doesn't get a second set of rules. It gets more ways to say things inside the same ones.

- **The burn rule.** A compiled program's effects are fenced in `burn { }` too. The same kernel journals them, so a CigarScript program gets the dry run, rollback and `cig unburn` that a script gets.
- **The words.** `stick`, `roll`, `pull`, `burn`, `unburn`, `cough`, `exhale`: anyone who knows CigScript can read CigarScript.
- **The four kinds of no.** Every compile error and runtime error has a stable code, a kind, a span and `cig explain`.
- **The checker's honesty.** It says what it knows, and when it guesses, it says it's a guess.

### What it adds

| | CigScript | CigarScript |
|---|---|---|
| runs as | interpreted | compiled to a native program |
| speed | fine for automation | fast enough for real work |
| types | checked at run time | checked before it builds |
| ships as | a `.cig` file plus `cig` | one program, nothing else to install |
| libraries | the standard library | the standard library, plus carton packages |
| calls other code | hops (processes) | hops, plus Rust and C libraries directly |
| depth | one level | three strengths, chosen per file (below) |

### Three strengths: as much control as you want

CigarScript is far deeper than CigScript, but nobody has to take all of it at once. The syntax comes in **three strengths**, like a cigar: **mild**, **medium** and **full**. Each strength includes everything below it, and you only reach for the next one when you want the control it gives.

| strength | for | what you write | what you control |
|---|---|---|---|
| **mild** | scripts, quick tools, anyone coming from CigScript | CigScript as it is: no types written, everything inferred | what happens; CigarScript decides how |
| **medium** | real programs, libraries, team code | types, records and enums, modules, pattern matching, generics, interfaces | the shapes of your data and the contracts between parts |
| **full** | performance, systems work, talking to Rust and C | explicit memory and ownership, concurrency, raw buffers, calls into Rust/C, custom journaling for a burn | how it happens, down to the memory and the threads |

**Mild**: exactly CigScript. Every `.cig` file is mild CigarScript.

```cig
stick root = args[0]
for file in fs.list(root) {
  burn {
    fs.mv(file, path.join(root, path.ext(file).lower(), path.base(file)))
  }
}
```

**Medium**: the same logic, with the shapes written down so the checker catches mistakes before it builds. *(Syntax illustrative; to be settled in CigarScript's LANGUAGE.md.)*

```cigar
strength medium

record Move { from: Path, to: Path }

pull plan_moves(root: Path) -> List<Move> {
  snuff fs.list(root)
    .filter(pack(f) => fs.is_file(f))
    .map(pack(f) => Move { from: f, to: path.join(root, path.ext(f).lower(), path.base(f)) })
}

burn {
  for m in plan_moves(args[0]) {
    fs.mkdir(path.dir(m.to))
    fs.mv(m.from, m.to)
  }
}
```

**Full**: the same burn, with control over how the work is done: here, in parallel, with an explicit buffer. *(Syntax illustrative.)*

```cigar
strength full

pull checksum(owned data: Bytes) -> Hash { ... }

burn {
  stick moves = plan_moves(args[0])
  parallel(threads: 8) for m in moves {
    fs.mv(m.from, m.to)
  }
}
```

**The rules stay the same at every strength:**
- **Nothing touches the world outside a burn**, at full strength too. Full gives control over *how* a burn does its work, and how it's journaled and compensated, never a way around it. Code that can't be journaled (raw device writes, foreign libraries with side effects) is labelled **irreversible** in the plan, exactly as an unwatched hop is today.
- **Strength is declared per file** (`strength medium`), defaulting to mild. The checker refuses a feature above the file's strength and names the strength that allows it, so nobody wanders into full by accident.
- **Strengths mix freely in one project.** A mild script calls a medium library, which calls a full module, with no glue. Types written at medium are inferred at mild call sites.
- **Turning it up is additive.** Changing `strength mild` to `strength medium` never breaks a file; it only lets you write more. Turning it down is checked: the checker lists what the file uses above the new strength.
- **The errors are the same four kinds of no** at every strength, with the strength named in the hint when that's what's in the way.

### Full strength: layered depth

Full strength is where CigarScript gets deep. The foundation stays simple, the same logic as CigScript, and depth is **layered** on top of it, layer on layer, each one reacting to the context it's used in. The same expression can mean different things in different places, including in how one script works with another.

Every layer is written in terms of the layer below it, all the way down to CigScript's core. However rich an expression gets, it always reduces to plain CigScript logic, and that's what keeps it trustworthy.

*All syntax below is illustrative, to show the idea; the real forms get settled in CigarScript's LANGUAGE.md.*

#### Example 1: one expression, many meanings: `into`

`a into b` means "put `a` into `b`", and what that does depends on what `b` is:

```cigar
strength full

roll seen = []
stick rows = csv.read("sales.csv")

rows into seen                      # b is a list: append them (pure)
rows into "report.csv"              # b is a path: write the file (a burn, so it must be inside burn { })
rows into stats.totals              # b is another script's stick: hand the value across
rows into workers                   # b is a channel: send them to the threads reading it
```

Peeled back with `cig explain --layers`, the file case reduces like this:

```
full    rows into "report.csv"
medium  csv.write(Path("report.csv"), rows)
mild    fs.write_text("report.csv", csv.encode(rows))
core    burn op: write report.csv   (journaled, reversible)
```

The meaning changes with the context. **Whether it touches the world never does without the code saying so**: `rows into "report.csv"` outside a `burn { }` is refused, exactly as `fs.write_text` is today.

#### Example 2: context blocks: the same code, layered behaviour

A `within` block sets a context, and the expressions inside adapt to it:

```cigar
strength full

within retry(3, backoff: 2s) {
  within parallel(threads: 8) {
    burn {
      for f in fs.glob("photos/*.jpg") {
        image.resize(f, 1024) into path.join("thumbs", path.base(f))
      }
    }
  }
}
```

- Inside `parallel`, the loop runs eight at a time, and the journal still records every write in order.
- Inside `retry`, a `cough` from one file retries that file, not the whole run.
- Nesting stacks them. Change the order (`parallel` outside `retry`) and the behaviour changes with it, which is the point.

Reduced to mild, it's an ordinary loop with a counter and a `burn`: nothing the core doesn't already have.

#### Example 3: scripts that read each other by context

One script can refer to another's sticks, and what it gets depends on where the other one is:

```cigar
strength full

stick total = stats.total
```

| where `stats` is | `stats.total` means |
|---|---|
| in the same chain, already lit | the value it produced this run |
| lit in an earlier run | the value recorded in that run's record |
| in a dry run | the value its own plan would produce (from the ghost filesystem) |
| not run yet | a refusal at compile time: `stats` hasn't run; light it first, or `needs: stats` |

This is cig-orchestrator's `{{stats.total}}`, built into the language: the same expression, resolved by context, with every case known before anything runs.

#### Example 4: one question, the right answer for each thing: `measure`

```cigar
strength full

measure [1, 2, 3]            # 3          (a list: its length)
measure "report.csv"         # 20481      (a path: its size in bytes, a read, so no burn needed)
measure lit(build)           # 4.2s       (a lit chain: how long it took)
measure "héllo"              # 5          (text: characters, not bytes)
```

Each meaning is its own layer, declared once (`measure for Path = fs.size`) and chosen by type when the program compiles.

#### What keeps it readable

Layering is what lets full strength go this deep without becoming a puzzle:

- **Every meaning is decided when the program compiles**, never while it runs. If the compiler can't tell which layer applies, that's an error naming the candidates, not a guess.
- **The tools can always explain it.** `cig explain --layers` peels any expression back to the core, and the editor shows the same on hover: "here, `into` means *write file*, because `"report.csv"` is a path."
- **Context never hides an effect.** A layer can change what an expression does, never whether it touches the world. The burn rule is enforced at the core, and no layer can get around something the core refuses.
- **The experience warning.** Switching a file to `strength full` asks first: *Full strength lets one expression mean different things in different places. It's built for experienced developers; medium covers most programs. Continue?* Once per project, remembered, and skippable with a setting for people who know what they're doing.

### The path: compile CigScript first, then grow it into CigarScript

**Decided:** CigarScript is a superset of CigScript. The compiler comes first, and the first thing it compiles is **CigScript as it is today**. CigarScript then grows out of it, one feature at a time, on a compiler that already works.

1. **CigCompile compiles CigScript.** Every `.cig` script that runs under `cig run` compiles to a native program that does exactly the same thing: the same burns, plan, journal and errors.
2. **The interpreter is the reference.** CigScript's whole test suite, the examples, the error corpus and cig-orchestrator's `orchestrate.cig` already say what correct output looks like. The compiled program has to match the interpreter on every one of them, byte for byte, before anything new is added. That's the compiler's test suite on day one, for free.
3. **Then CigarScript features, opt in.** Types first, then everything else in the next section. A `.cig` file stays valid CigarScript forever, and a `.cigar` file is one that uses the new features.
4. **Users never start over.** Their scripts compile the day the compiler ships. CigarScript is something they grow into, not a second language to learn.

**What the compiler is written in.** Rust, beside cig, in the same workspace: it reuses cig's lexer, parser, checker, error registry and burn kernel as crates rather than copying them. It runs at native speed from the start.

**The milestone after that: self-hosting.** Once CigarScript is mature, CigCompile is rewritten in CigarScript and compiles itself. Rust, Go and Zig all did this to prove the language could carry a real program of its own. It comes after the language is proven, not before.

### Questions to settle before CigarScript's own features

1. **How far the superset goes.** Every CigScript program is valid CigarScript (decided). The open question is whether CigarScript may ever reject something CigScript accepts (for example, a type error in code CigScript only finds at run time), and whether that's a warning or an error in `.cig` files.
2. **The type system (medium).** Types inferred where they're obvious and written where they help; a `?` for "maybe null" in place of surprises. Errors as values, or `cough` and catch?
3. **Memory (mild and medium vs full).** Mild and medium need automatic memory so nobody thinks about it: a garbage collector or reference counting. Full adds explicit ownership for code that wants it. The question is the default, and how values cross between full code and the rest.
4. **Where each feature lives.** Which features are medium and which are full (concurrency, raw buffers and foreign calls are the obvious full ones), so the line between strengths is principled, not arbitrary.
5. **How it compiles.** Cranelift (quick to build, written in Rust like cig), LLVM (fastest programs, heavier), or generating Rust and letting `rustc` do the rest (fastest way to a working compiler).
6. **Talking to Rust and C (full).** How a CigarScript program uses a Rust crate or a C library, and how much of the burn rule applies to code that didn't come from CigarScript.

---

## CigCompile

### For people: two buttons

```
┌───────────────────────────────────────┐
│  CigCompile                           │
│                                       │
│  What do you want to compile?         │
│  [ my-tool/          ▾ ]  [ Browse ]  │
│                                       │
│            [  Compile  ]              │
│                                       │
└───────────────────────────────────────┘
```

1. **Pick** a project folder (or drop one on the window). CigCompile finds the `.cig` and `.cigar` files, the manifest and the version pin by itself. A folder of plain CigScript compiles as it is.
2. **Click Compile.** It checks, builds and tests, then shows where the program went.

Everything else has a sensible default and stays out of the way:
- the target is this machine, with a drop-down for others (Linux, macOS, Windows, ARM);
- release build;
- tests run before it says done.

When it fails, it shows the error the same way cig does: what kind of no it is, where it is, and what to try next, with `cig explain` one click away. Nothing a person has to type.

### For machines: the same thing, no window

CI servers, cigo and scheduled builds can't click. Underneath the window is the same compiler as a command, so the two can never disagree:

```
cigcompile build my-tool/
```

The window is a face over this, not a separate program. cigo gets a `cigarscript` toolchain that calls it, so a CigarScript component builds, tests and rolls back in a chain like any other language.

### What it does on every build

- **Uses the right version.** The project's pin (`cigar 1.0` in the manifest, or `.cigar-version`) picks the CigCompile from the humidor. An old project builds with the compiler it was written for, and an upgrade gets the same trial CigScript gets: both versions compile, and the results are compared before the new one is trusted.
- **Fetches packages through carton**, checked by hash, pinned in a lock file.
- **Puts the output somewhere predictable**, and a rebuild that changes nothing touches nothing.
- **Journals what it writes.** A build that fails halfway leaves the folder as it was, the same promise as every burn.

---

## carton

The package manager for CigarScript (the lexicon already reserves the word for distribution):

- a manifest that names the package, its version and its dependencies;
- a lock file with every dependency's hash;
- a registry, or plain git repositories to start with;
- `carton add`, `carton update` and `carton publish`, also behind buttons in CigCompile.

---

## Order of work

**Phase one: CigCompile compiles CigScript.**

| # | lands | proves |
|---|---|---|
| 1 | the workspace: cig's lexer, parser, checker, errors and kernel split into crates the compiler can use | `cig` still builds and passes every test, unchanged |
| 2 | the first back end: CigScript to a native program, through the route chosen in question 5 | `hello.cig` compiles and runs with nothing installed |
| 3 | burns in compiled programs, through the same kernel | a compiled program's failed burn rolls back exactly like the interpreted script's |
| 4 | the differential suite: every test, example and corpus script, run interpreted and compiled, outputs compared | the compiled program matches the interpreter byte for byte |
| 5 | `orchestrate.cig` compiled | cig-orchestrator runs as a native program and passes its 58 tests |
| 6 | `cigcompile build` (the command) | cigo can build any CigScript as a program |
| 7 | the CigCompile window: pick, click, done | someone who has never seen a terminal builds a program |

**Phase two: CigarScript grows out of it, one strength at a time.**

| # | lands | proves |
|---|---|---|
| 8 | the open questions above, decided and written into CigarScript's LANGUAGE.md, with every feature assigned a strength | the language is designed before its features are built |
| 9 | **mild**: `strength` declarations, and the checker refusing features above a file's strength | every CigScript script compiles as mild, unchanged |
| 10 | **medium**: types, records, enums, modules, pattern matching, generics, interfaces | a typed file catches its mistake before it builds; mild code calls medium code with no glue |
| 11 | the standard library, grown from CigScript's, typed at medium | the examples need nothing outside it |
| 12 | **full**: explicit ownership, concurrency, raw buffers, custom journaling for a burn | a full module runs in parallel and its burns still roll back |
| 13 | calling Rust and C (full) | a `.cigar` program uses a Rust crate, and its unwatched effects are labelled irreversible |
| 14 | carton | one CigarScript package uses another, across strengths |
| 15 | the humidor and the trial for CigCompile | a project pinned to 1.0 builds with 1.0 after 1.1 is installed |
| 16 | editor support (a language server): errors as you type, go to definition, the file's strength shown | it's pleasant to write in |

**Phase three: self-hosting.**

| # | lands | proves |
|---|---|---|
| 17 | CigCompile rewritten in CigarScript | CigCompile compiles itself, and the result compiles itself again to the same bytes |

Each lands with a test named for what it proves, the same as everything else in the family.

---

## Lexicon additions

| word | meaning | the manual's name |
|---|---|---|
| **CigarScript** | the fuller, compiled language of the family | compiled general-purpose language |
| **strength** (mild, medium, full) | how much of CigarScript a file uses, declared at its top | language level / feature tier |
| **CigCompile** | the compiler, with a two-button window over a command | compiler / build tool |
| **carton** | a CigarScript package, and the tool that handles them | package / package manager |
