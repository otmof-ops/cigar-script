<!-- SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script) -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0002: Memory

- Status: accepted
- Date: 2026-11-11
- Issue: otmof-ops/cigar-script#3

## Context

What manages memory at mild and medium, and how does full strength's explicit
ownership cross into it? The burn kernel closes resources in a known order
and the journal depends on that, so release has to be deterministic.

## Decision

1. **Automatic by default.** Mild and medium free nothing by hand.
2. **Reference counting with reuse** (the Perceus approach of Koka and Lean 4).
   A value held once and replaced by an updated copy is updated in place.
3. **A cycle collector for mutable containers.** Immutable data cannot form a
   cycle. The collector runs when count traffic says garbage may exist.
4. **Ownership at full is static and erased at run time.** `owned`, `borrow`
   and `move` are checked locally; a **borrow is scoped to a block**, with no
   lifetime parameters.
5. **The boundary rule.** A value crossing from full to medium is copied or
   wrapped in a counted handle, decided by the compiler and shown by
   `cig explain --layers`. Nothing at medium points into storage a full file
   owns.

## Options considered

| option | pros | cons |
|---|---|---|
| tracing collector | handles cycles, fast allocation | pauses, finalization at no known time (breaks the journal's order), a large runtime piece |
| plain reference counting (today) | deterministic, simple | cycles leak; every update copies when it could reuse |
| **reference counting with reuse** | deterministic, no pauses, functional updates in place | needs a cycle backup |
| plain ownership everywhere | fastest, no counts | is the full-strength model; cannot be the default, since it asks the author to annotate |

## Measurement

Three programs from cig-compile's bench set (trees, nbody, lists) ported to Rust
in `bench/memory-options`, one variant per strategy: `own` (plain ownership,
`Box`/`Vec`), `rc` (`Rc`, reuse by `Rc::make_mut` where one holder remains),
`rc-copy` (`Rc` with no reuse: today's behavior; lists only) and `gc` (a tracing
collector: **gc-arena 0.5.3, MIT licensed**). Every variant prints exactly the
CigarScript program's output, checked against its `.expect` file. The CigarScript
runtime today runs the same programs through `bench/bench.toml` (the `memory`
groups). All through `cig bench` (median of 9 samples of 5 runs, peak resident
set from `wait4`); the `x100` rows run the workload 100 times in one process,
5 samples of 1.

Taken 2026-11-11 on one laptop (Intel Core Ultra 7 255H, 16 cores, Linux 6.17,
governor `powersave`), with other builds running, Rust 1.99, release profile with
LTO, `cig bench` from cig 1.20.0. Times are medians in milliseconds; CV was under
7 % on every Rust row and 3 to 23 % on the runtime rows.

| program | `own` | `rc` | `rc-copy` | `gc` (gc-arena) | CigarScript runtime today |
|---|---|---|---|---|---|
| trees, 1 run | 7.5 | 8.6 | n/a | 19.8 | 347 |
| trees, x100 | 645 | 785 | n/a | 1714 | n/a |
| nbody, 1 run | 1.5 | 1.8 | n/a | 1.9 | 492 |
| nbody, x100 | 129 | 81 | n/a | 77 | n/a |
| lists, 1 run | 1.3 | 1.4 | 1.7 | 1.6 | 36 |
| lists, x100 | 58.0 | 57.3 | 59.3 | 81.9 | n/a |

Peak resident set, in KB (best of three, `/usr/bin/time`, one run; `cig bench`'s
own column is a floor of about 6.9 MB for every Rust row, set by the runner, so it
is not used for them):

| program | `own` | `rc` | `rc-copy` | `gc` | runtime today (`cig bench`) |
|---|---|---|---|---|---|
| trees | 2332 | 2692 | n/a | 3824 | 31496 |
| nbody | 1908 | 2024 | n/a | 1940 | 31704 |
| lists | 2608 | 2604 | 3192 | 3204 | 31276 |

What it shows:

- **Reference counting costs about 15 % over plain ownership on the
  allocation-heavy program** (trees: 785 against 645 at x100) and 15 % more
  memory. The tracing collector is 2.7 times slower there with 64 % more
  memory, and its cost is in marking and the write barrier, not in pauses we
  measured.
- **Reuse is small here.** On lists, `rc` against `rc-copy` is 57.3 against 59.3
  (3 %) and 2604 against 3192 KB (18 % less memory). The sort dominates; reuse
  matters most where a program updates large values in a loop, which none of
  these three does. It is kept for the memory and for the functional-update form,
  not for the speed measured here.
- **`nbody` does not separate the strategies.** It allocates nothing after set-up;
  the `own` x100 figure is slower because the port copies a body on every read
  and write, a fault of the port, not of ownership.
- **Today's runtime is 30 to 330 times slower than any Rust variant on these
  programs and holds about 31 MB.** The gap is the interpreter, not the memory
  strategy: no choice here recovers it, and none is ruled out by it.

The raw runner output is not kept in the repository; `bench/memory-options/bench.toml`
and `bench/bench.toml` reproduce it.

## Consequences

- The runtime's reuse and the cycle collector are still to build; today's
  runtime counts without reuse. The numbers above bound what they buy.
- Limits of the measurement: the Rust variants are hand-written ports, not
  output of a compiler doing reuse analysis, so `rc` is a ceiling for reuse;
  gc-arena counts a `Vec`'s header, not its buffer, so the `gc` lists variant
  collects after each step instead of on debt; the machine was shared with
  other builds, so quote ratios, not absolute times.
